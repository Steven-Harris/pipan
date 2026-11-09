//! System Dependency Doctor & Auto-Fixer (`run_doctor`, `fix_doctor_issue`).
//!
//! Performs 7 diagnostic checks across the augmented PATH and `~/.pi/agent`:
//! 1. `git`        — `git --version` and `user.name` / `user.email`
//! 2. `node_npm`   — `node --version` and `npm` / `pnpm` availability
//! 3. `pi_cli`     — `pi --version` on the augmented PATH
//! 4. `pi_auth`    — Provider API key or OAuth credential in `~/.pi/agent/auth.json` or env
//! 5. `gh_cli`     — `gh --version` on the augmented PATH
//! 6. `gh_auth`    — `gh auth status` authentication state
//! 7. `gh_webhook` — `gh extension list` for `cli/gh-webhook`

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

use crate::auth::{has_any_pi_credential_in_dir, parse_gh_auth_status_output};
use crate::github::WEBHOOK_EXTENSION;
use crate::models::resolve_pi_agent_dir;
use crate::pi_rpc::{find_executable_in_path, get_augmented_path, refresh_augmented_path};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub const CHECK_IDS: &[&str] = &[
    "git",
    "node_npm",
    "pi_cli",
    "pi_auth",
    "gh_cli",
    "gh_auth",
    "gh_webhook",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DoctorCheckStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorCheckResult {
    pub id: String,
    pub title: String,
    pub status: DoctorCheckStatus,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    pub manual_instructions: Vec<String>,
    /// `"auto_command"` | `"open_pi_auth"` | `"open_gh_auth"` | `"git_config"`
    pub fix_action: String,
    pub fix_label: String,
    pub can_auto_fix: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub overall_status: DoctorCheckStatus,
    pub pass_count: usize,
    pub warn_count: usize,
    pub fail_count: usize,
    pub checked_at: u64,
    pub checks: Vec<DoctorCheckResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorFixLogEvent {
    pub check_id: String,
    pub stream: String,
    pub line: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorFixResult {
    pub check_id: String,
    pub success: bool,
    pub message: String,
    pub output_lines: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_flow: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_report: Option<DoctorReport>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Builds a command that resolves binaries (including `.cmd` shims on Windows) across the augmented PATH.
pub(crate) fn build_augmented_command(program: &str, args: &[&str]) -> Command {
    let path_var = get_augmented_path();

    #[cfg(target_os = "windows")]
    {
        let exe_name = format!("{program}.exe");
        let cmd_name = format!("{program}.cmd");
        let mut cmd = if let Some(exe) = find_executable_in_path(&exe_name, &path_var) {
            let mut c = Command::new(exe);
            c.args(args);
            c
        } else if let Some(shim) = find_executable_in_path(&cmd_name, &path_var) {
            let mut c = Command::new("cmd.exe");
            c.arg("/d").arg("/c").arg(shim).args(args);
            c
        } else if let Some(bin) = find_executable_in_path(program, &path_var) {
            let mut c = Command::new(bin);
            c.args(args);
            c
        } else {
            let mut c = Command::new("cmd.exe");
            c.arg("/d").arg("/c").arg(program).args(args);
            c
        };

        if !path_var.is_empty() {
            cmd.env("PATH", &path_var);
        }
        cmd.env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .creation_flags(CREATE_NO_WINDOW);
        cmd
    }

    #[cfg(not(target_os = "windows"))]
    {
        let resolved =
            find_executable_in_path(program, &path_var).unwrap_or_else(|| program.into());
        let mut cmd = Command::new(resolved);
        if !path_var.is_empty() {
            cmd.env("PATH", &path_var);
        }
        cmd.args(args)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        cmd
    }
}

async fn run_cmd_capture(program: &str, args: &[&str]) -> Result<String, String> {
    let output = build_augmented_command(program, args)
        .output()
        .await
        .map_err(|e| format!("Failed to run `{program}`: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(if stdout.is_empty() { stderr } else { stdout })
    } else {
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

pub fn summarize_report(checks: Vec<DoctorCheckResult>) -> DoctorReport {
    let mut pass_count = 0;
    let mut warn_count = 0;
    let mut fail_count = 0;

    for c in &checks {
        match c.status {
            DoctorCheckStatus::Pass => pass_count += 1,
            DoctorCheckStatus::Warn => warn_count += 1,
            DoctorCheckStatus::Fail => fail_count += 1,
        }
    }

    let overall_status = if fail_count > 0 {
        DoctorCheckStatus::Fail
    } else if warn_count > 0 {
        DoctorCheckStatus::Warn
    } else {
        DoctorCheckStatus::Pass
    };

    DoctorReport {
        overall_status,
        pass_count,
        warn_count,
        fail_count,
        checked_at: now_ms(),
        checks,
    }
}

pub fn evaluate_git_check(
    version_out: Result<&str, &str>,
    user_name: Option<&str>,
    user_email: Option<&str>,
) -> DoctorCheckResult {
    match version_out {
        Ok(ver) => {
            let clean_ver = ver.lines().next().unwrap_or(ver).trim();
            let name = user_name.map(str::trim).filter(|s| !s.is_empty());
            let email = user_email.map(str::trim).filter(|s| !s.is_empty());
            match (name, email) {
                (Some(n), Some(e)) => DoctorCheckResult {
                    id: "git".to_string(),
                    title: "Git CLI & Identity".to_string(),
                    status: DoctorCheckStatus::Pass,
                    summary: format!("{clean_ver} ({n} <{e}>)"),
                    details: None,
                    manual_instructions: Vec::new(),
                    fix_action: "git_config".to_string(),
                    fix_label: "Configure Git Identity".to_string(),
                    can_auto_fix: false,
                },
                _ => DoctorCheckResult {
                    id: "git".to_string(),
                    title: "Git CLI & Identity".to_string(),
                    status: DoctorCheckStatus::Warn,
                    summary: format!("{clean_ver} — missing user.name or user.email"),
                    details: Some(
                        "Git commits and worktree operations require global user.name and user.email."
                            .to_string(),
                    ),
                    manual_instructions: vec![
                        "Run `git config --global user.name \"Your Name\"`".to_string(),
                        "Run `git config --global user.email \"you@example.com\"`".to_string(),
                    ],
                    fix_action: "git_config".to_string(),
                    fix_label: "Fix for Me (Set Git Identity)".to_string(),
                    can_auto_fix: true,
                },
            }
        }
        Err(err) => DoctorCheckResult {
            id: "git".to_string(),
            title: "Git CLI & Identity".to_string(),
            status: DoctorCheckStatus::Fail,
            summary: "Git is not installed or not on PATH".to_string(),
            details: if err.is_empty() {
                None
            } else {
                Some(err.to_string())
            },
            manual_instructions: vec![
                "Windows: `winget install --id Git.Git -e` or `scoop install git`".to_string(),
                "macOS: `brew install git` or `xcode-select --install`".to_string(),
                "Linux: `sudo apt install git` or `sudo dnf install git`".to_string(),
            ],
            fix_action: "auto_command".to_string(),
            fix_label: "Fix for Me (Install Git)".to_string(),
            can_auto_fix: true,
        },
    }
}

pub fn evaluate_node_npm_check(
    node_ver: Result<&str, &str>,
    npm_ver: Option<&str>,
    pnpm_ver: Option<&str>,
) -> DoctorCheckResult {
    match node_ver {
        Ok(n_ver) => {
            let clean_node = n_ver.lines().next().unwrap_or(n_ver).trim();
            let mut managers = Vec::new();
            if let Some(npm) = npm_ver.map(str::trim).filter(|s| !s.is_empty()) {
                managers.push(format!("npm {}", npm.lines().next().unwrap_or(npm)));
            }
            if let Some(pnpm) = pnpm_ver.map(str::trim).filter(|s| !s.is_empty()) {
                managers.push(format!("pnpm {}", pnpm.lines().next().unwrap_or(pnpm)));
            }
            if !managers.is_empty() {
                DoctorCheckResult {
                    id: "node_npm".to_string(),
                    title: "Node.js & Package Manager".to_string(),
                    status: DoctorCheckStatus::Pass,
                    summary: format!("Node {clean_node} ({})", managers.join(", ")),
                    details: None,
                    manual_instructions: Vec::new(),
                    fix_action: "auto_command".to_string(),
                    fix_label: "Install Node.js".to_string(),
                    can_auto_fix: false,
                }
            } else {
                DoctorCheckResult {
                    id: "node_npm".to_string(),
                    title: "Node.js & Package Manager".to_string(),
                    status: DoctorCheckStatus::Warn,
                    summary: format!("Node {clean_node} found, but neither npm nor pnpm is on PATH"),
                    details: Some(
                        "npm or pnpm is required to install or update `@earendil-works/pi-coding-agent`."
                            .to_string(),
                    ),
                    manual_instructions: vec![
                        "Enable Corepack: `corepack enable`".to_string(),
                        "Or reinstall Node.js LTS from https://nodejs.org".to_string(),
                    ],
                    fix_action: "auto_command".to_string(),
                    fix_label: "Fix for Me (Enable pnpm / Install Node)".to_string(),
                    can_auto_fix: true,
                }
            }
        }
        Err(err) => DoctorCheckResult {
            id: "node_npm".to_string(),
            title: "Node.js & Package Manager".to_string(),
            status: DoctorCheckStatus::Fail,
            summary: "Node.js was not found on the augmented PATH".to_string(),
            details: if err.is_empty() {
                None
            } else {
                Some(err.to_string())
            },
            manual_instructions: vec![
                "Windows: `winget install --id OpenJS.NodeJS.LTS -e`".to_string(),
                "macOS: `brew install node`".to_string(),
                "Or download Node.js 22+ from https://nodejs.org".to_string(),
            ],
            fix_action: "auto_command".to_string(),
            fix_label: "Fix for Me (Install Node.js LTS)".to_string(),
            can_auto_fix: true,
        },
    }
}

pub fn evaluate_pi_cli_check(pi_ver: Result<&str, &str>) -> DoctorCheckResult {
    match pi_ver {
        Ok(ver) => {
            let clean = ver.lines().next().unwrap_or(ver).trim();
            DoctorCheckResult {
                id: "pi_cli".to_string(),
                title: "Pi Coding Agent CLI (`pi`)".to_string(),
                status: DoctorCheckStatus::Pass,
                summary: format!("pi {clean}"),
                details: None,
                manual_instructions: Vec::new(),
                fix_action: "auto_command".to_string(),
                fix_label: "Reinstall Pi CLI".to_string(),
                can_auto_fix: false,
            }
        }
        Err(err) => DoctorCheckResult {
            id: "pi_cli".to_string(),
            title: "Pi Coding Agent CLI (`pi`)".to_string(),
            status: DoctorCheckStatus::Fail,
            summary: "`pi` CLI was not found on the augmented PATH".to_string(),
            details: if err.is_empty() {
                None
            } else {
                Some(err.to_string())
            },
            manual_instructions: vec![
                "Install globally with npm: `npm install -g @earendil-works/pi-coding-agent`"
                    .to_string(),
                "Or install with pnpm: `pnpm add -g @earendil-works/pi-coding-agent`".to_string(),
            ],
            fix_action: "auto_command".to_string(),
            fix_label: "Fix for Me (Install Pi CLI)".to_string(),
            can_auto_fix: true,
        },
    }
}

pub fn evaluate_pi_auth_check(agent_dir: Option<&Path>) -> DoctorCheckResult {
    let (has_cred, sources) = has_any_pi_credential_in_dir(agent_dir);
    if has_cred {
        DoctorCheckResult {
            id: "pi_auth".to_string(),
            title: "Pi Provider Credentials".to_string(),
            status: DoctorCheckStatus::Pass,
            summary: format!("Configured: {}", sources.join(", ")),
            details: None,
            manual_instructions: Vec::new(),
            fix_action: "open_pi_auth".to_string(),
            fix_label: "Manage Pi Credentials".to_string(),
            can_auto_fix: false,
        }
    } else {
        DoctorCheckResult {
            id: "pi_auth".to_string(),
            title: "Pi Provider Credentials".to_string(),
            status: DoctorCheckStatus::Fail,
            summary: "No provider API keys or OAuth credentials found".to_string(),
            details: Some(
                "Configure at least one provider key in `~/.pi/agent/auth.json` or environment variables."
                    .to_string(),
            ),
            manual_instructions: vec![
                "Open PiPan Settings & Login -> Pi Authentication tab and enter an API key (Anthropic, OpenAI, Gemini, OpenRouter, etc.).".to_string(),
                "Or run `pi` in an interactive terminal to complete OAuth login.".to_string(),
            ],
            fix_action: "open_pi_auth".to_string(),
            fix_label: "Fix for Me (Configure Pi Login)".to_string(),
            can_auto_fix: true,
        }
    }
}

pub fn evaluate_gh_cli_check(gh_ver: Result<&str, &str>) -> DoctorCheckResult {
    match gh_ver {
        Ok(ver) => {
            let clean = ver.lines().next().unwrap_or(ver).trim();
            DoctorCheckResult {
                id: "gh_cli".to_string(),
                title: "GitHub CLI (`gh`)".to_string(),
                status: DoctorCheckStatus::Pass,
                summary: clean.to_string(),
                details: None,
                manual_instructions: Vec::new(),
                fix_action: "auto_command".to_string(),
                fix_label: "Install GitHub CLI".to_string(),
                can_auto_fix: false,
            }
        }
        Err(err) => DoctorCheckResult {
            id: "gh_cli".to_string(),
            title: "GitHub CLI (`gh`)".to_string(),
            status: DoctorCheckStatus::Fail,
            summary: "GitHub CLI (`gh`) was not found on the augmented PATH".to_string(),
            details: if err.is_empty() {
                None
            } else {
                Some(err.to_string())
            },
            manual_instructions: vec![
                "Windows: `winget install --id GitHub.cli -e` or `scoop install gh`".to_string(),
                "macOS: `brew install gh`".to_string(),
                "Linux: see https://cli.github.com".to_string(),
            ],
            fix_action: "auto_command".to_string(),
            fix_label: "Fix for Me (Install GitHub CLI)".to_string(),
            can_auto_fix: true,
        },
    }
}

pub fn evaluate_gh_auth_check(
    gh_installed: bool,
    auth_status_out: Result<&str, &str>,
) -> DoctorCheckResult {
    if !gh_installed {
        return DoctorCheckResult {
            id: "gh_auth".to_string(),
            title: "GitHub Authentication".to_string(),
            status: DoctorCheckStatus::Fail,
            summary: "Requires GitHub CLI (`gh`) to be installed first".to_string(),
            details: None,
            manual_instructions: vec![
                "Install `gh` first, then authenticate via `gh auth login --web`.".to_string(),
            ],
            fix_action: "open_gh_auth".to_string(),
            fix_label: "Fix for Me (Sign in to GitHub)".to_string(),
            can_auto_fix: true,
        };
    }

    match auth_status_out {
        Ok(raw) => {
            let parsed = parse_gh_auth_status_output(raw);
            if parsed.authenticated || raw.contains("Logged in to ") {
                let acct = parsed
                    .account
                    .unwrap_or_else(|| "authenticated".to_string());
                let host = parsed.hostname.unwrap_or_else(|| "github.com".to_string());
                DoctorCheckResult {
                    id: "gh_auth".to_string(),
                    title: "GitHub Authentication".to_string(),
                    status: DoctorCheckStatus::Pass,
                    summary: format!("Signed in as @{acct} on {host}"),
                    details: None,
                    manual_instructions: Vec::new(),
                    fix_action: "open_gh_auth".to_string(),
                    fix_label: "Manage GitHub Login".to_string(),
                    can_auto_fix: false,
                }
            } else {
                DoctorCheckResult {
                    id: "gh_auth".to_string(),
                    title: "GitHub Authentication".to_string(),
                    status: DoctorCheckStatus::Warn,
                    summary: "Not signed in to GitHub CLI".to_string(),
                    details: Some(raw.to_string()),
                    manual_instructions: vec![
                        "Click 'Fix for Me' to sign in with a browser device code or Personal Access Token."
                            .to_string(),
                        "Or run `gh auth login --web` in your terminal.".to_string(),
                    ],
                    fix_action: "open_gh_auth".to_string(),
                    fix_label: "Fix for Me (Sign in to GitHub)".to_string(),
                    can_auto_fix: true,
                }
            }
        }
        Err(err) => {
            let parsed = parse_gh_auth_status_output(err);
            if parsed.authenticated {
                let acct = parsed
                    .account
                    .unwrap_or_else(|| "authenticated".to_string());
                DoctorCheckResult {
                    id: "gh_auth".to_string(),
                    title: "GitHub Authentication".to_string(),
                    status: DoctorCheckStatus::Pass,
                    summary: format!("Signed in as @{acct}"),
                    details: None,
                    manual_instructions: Vec::new(),
                    fix_action: "open_gh_auth".to_string(),
                    fix_label: "Manage GitHub Login".to_string(),
                    can_auto_fix: false,
                }
            } else {
                DoctorCheckResult {
                    id: "gh_auth".to_string(),
                    title: "GitHub Authentication".to_string(),
                    status: DoctorCheckStatus::Warn,
                    summary: "GitHub CLI is not authenticated".to_string(),
                    details: if err.is_empty() {
                        None
                    } else {
                        Some(err.to_string())
                    },
                    manual_instructions: vec![
                        "Click 'Fix for Me' to open the guided GitHub login flow.".to_string(),
                        "Or run `gh auth login --web` in your terminal.".to_string(),
                    ],
                    fix_action: "open_gh_auth".to_string(),
                    fix_label: "Fix for Me (Sign in to GitHub)".to_string(),
                    can_auto_fix: true,
                }
            }
        }
    }
}

pub fn evaluate_gh_webhook_check(
    gh_installed: bool,
    ext_list_out: Result<&str, &str>,
) -> DoctorCheckResult {
    if !gh_installed {
        return DoctorCheckResult {
            id: "gh_webhook".to_string(),
            title: "GitHub Webhook Extension (`cli/gh-webhook`)".to_string(),
            status: DoctorCheckStatus::Warn,
            summary: "Requires GitHub CLI (`gh`) to be installed first".to_string(),
            details: None,
            manual_instructions: vec![
                "Install `gh` and sign in, then run `gh extension install cli/gh-webhook`."
                    .to_string(),
            ],
            fix_action: "auto_command".to_string(),
            fix_label: "Fix for Me (Install Extension)".to_string(),
            can_auto_fix: true,
        };
    }

    match ext_list_out {
        Ok(out) if out.lines().any(|l| l.contains("gh-webhook")) => DoctorCheckResult {
            id: "gh_webhook".to_string(),
            title: "GitHub Webhook Extension (`cli/gh-webhook`)".to_string(),
            status: DoctorCheckStatus::Pass,
            summary: "cli/gh-webhook extension installed".to_string(),
            details: None,
            manual_instructions: Vec::new(),
            fix_action: "auto_command".to_string(),
            fix_label: "Reinstall Extension".to_string(),
            can_auto_fix: false,
        },
        Ok(_) | Err(_) => DoctorCheckResult {
            id: "gh_webhook".to_string(),
            title: "GitHub Webhook Extension (`cli/gh-webhook`)".to_string(),
            status: DoctorCheckStatus::Warn,
            summary: "`cli/gh-webhook` extension is not installed".to_string(),
            details: Some(
                "Used for real-time PR review, comment, and CI check streaming via `gh webhook forward`."
                    .to_string(),
            ),
            manual_instructions: vec![
                "Run `gh extension install cli/gh-webhook` in your terminal.".to_string(),
            ],
            fix_action: "auto_command".to_string(),
            fix_label: "Fix for Me (Install cli/gh-webhook)".to_string(),
            can_auto_fix: true,
        },
    }
}

async fn execute_doctor_checks() -> DoctorReport {
    let git_ver = run_cmd_capture("git", &["--version"]).await;
    let (git_name, git_email) = if git_ver.is_ok() {
        let name = run_cmd_capture("git", &["config", "--get", "user.name"])
            .await
            .ok();
        let email = run_cmd_capture("git", &["config", "--get", "user.email"])
            .await
            .ok();
        (name, email)
    } else {
        (None, None)
    };
    let check_git = evaluate_git_check(
        git_ver.as_deref().map_err(|e| e.as_str()),
        git_name.as_deref(),
        git_email.as_deref(),
    );

    let node_ver = run_cmd_capture("node", &["--version"]).await;
    let npm_ver = run_cmd_capture("npm", &["--version"]).await.ok();
    let pnpm_ver = run_cmd_capture("pnpm", &["--version"]).await.ok();
    let check_node = evaluate_node_npm_check(
        node_ver.as_deref().map_err(|e| e.as_str()),
        npm_ver.as_deref(),
        pnpm_ver.as_deref(),
    );

    let pi_ver = run_cmd_capture("pi", &["--version"]).await;
    let check_pi_cli = evaluate_pi_cli_check(pi_ver.as_deref().map_err(|e| e.as_str()));

    let agent_dir = resolve_pi_agent_dir();
    let check_pi_auth = evaluate_pi_auth_check(agent_dir.as_deref());

    let gh_ver = run_cmd_capture("gh", &["--version"]).await;
    let gh_installed = gh_ver.is_ok();
    let check_gh_cli = evaluate_gh_cli_check(gh_ver.as_deref().map_err(|e| e.as_str()));

    let gh_auth_out = if gh_installed {
        run_cmd_capture("gh", &["auth", "status"]).await
    } else {
        Err("gh not installed".to_string())
    };
    let check_gh_auth =
        evaluate_gh_auth_check(gh_installed, gh_auth_out.as_deref().map_err(|e| e.as_str()));

    let gh_ext_out = if gh_installed {
        run_cmd_capture("gh", &["extension", "list"]).await
    } else {
        Err("gh not installed".to_string())
    };
    let check_gh_webhook =
        evaluate_gh_webhook_check(gh_installed, gh_ext_out.as_deref().map_err(|e| e.as_str()));

    summarize_report(vec![
        check_git,
        check_node,
        check_pi_cli,
        check_pi_auth,
        check_gh_cli,
        check_gh_auth,
        check_gh_webhook,
    ])
}

fn spawn_log_reader<R>(
    reader: R,
    app: Option<AppHandle>,
    check_id: String,
    stream: &'static str,
    collected: Arc<std::sync::Mutex<Vec<String>>>,
) -> tokio::task::JoinHandle<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let trimmed = line.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(mut guard) = collected.lock() {
                guard.push(trimmed.clone());
            }
            if let Some(ref handle) = app {
                let _ = handle.emit(
                    "doctor:fix-log",
                    DoctorFixLogEvent {
                        check_id: check_id.clone(),
                        stream: stream.to_string(),
                        line: trimmed,
                        timestamp: now_ms(),
                    },
                );
            }
        }
    })
}

async fn run_streaming_fix_command(
    app: Option<AppHandle>,
    check_id: &str,
    program: &str,
    args: &[&str],
) -> Result<Vec<String>, (String, Vec<String>)> {
    let mut cmd = build_augmented_command(program, args);
    let display_cmd = format!("{} {}", program, args.join(" "));

    if let Some(ref handle) = app {
        let _ = handle.emit(
            "doctor:fix-log",
            DoctorFixLogEvent {
                check_id: check_id.to_string(),
                stream: "stdout".to_string(),
                line: format!("$ {display_cmd}"),
                timestamp: now_ms(),
            },
        );
    }

    let mut child = cmd.spawn().map_err(|e| {
        (
            format!("Failed to start `{display_cmd}`: {e}"),
            vec![format!("$ {display_cmd}")],
        )
    })?;

    let collected = Arc::new(std::sync::Mutex::new(vec![format!("$ {display_cmd}")]));
    let mut readers = Vec::new();

    if let Some(stdout) = child.stdout.take() {
        readers.push(spawn_log_reader(
            stdout,
            app.clone(),
            check_id.to_string(),
            "stdout",
            collected.clone(),
        ));
    }
    if let Some(stderr) = child.stderr.take() {
        readers.push(spawn_log_reader(
            stderr,
            app,
            check_id.to_string(),
            "stderr",
            collected.clone(),
        ));
    }

    let status = child.wait().await;
    for r in readers {
        let _ = r.await;
    }

    let lines = collected
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| Vec::new());

    match status {
        Ok(st) if st.success() => Ok(lines),
        Ok(st) => Err((format!("`{display_cmd}` exited with status {st}"), lines)),
        Err(e) => Err((format!("Error waiting for `{display_cmd}`: {e}"), lines)),
    }
}

/// Selects the ordered list of `(program, args)` candidate auto-fix commands for a check ID.
pub fn candidate_fix_commands_for_check(
    check_id: &str,
    is_windows: bool,
    is_macos: bool,
) -> Vec<(&'static str, Vec<&'static str>)> {
    match check_id {
        "pi_cli" => vec![
            (
                "npm",
                vec!["install", "-g", "@earendil-works/pi-coding-agent"],
            ),
            ("pnpm", vec!["add", "-g", "@earendil-works/pi-coding-agent"]),
        ],
        "gh_webhook" => vec![("gh", vec!["extension", "install", WEBHOOK_EXTENSION])],
        "gh_cli" => {
            if is_windows {
                vec![
                    (
                        "winget",
                        vec![
                            "install",
                            "--id",
                            "GitHub.cli",
                            "-e",
                            "--accept-source-agreements",
                            "--accept-package-agreements",
                        ],
                    ),
                    ("scoop", vec!["install", "gh"]),
                ]
            } else if is_macos {
                vec![("brew", vec!["install", "gh"])]
            } else {
                vec![
                    ("brew", vec!["install", "gh"]),
                    ("sudo", vec!["apt-get", "install", "-y", "gh"]),
                ]
            }
        }
        "git" => {
            if is_windows {
                vec![
                    (
                        "winget",
                        vec![
                            "install",
                            "--id",
                            "Git.Git",
                            "-e",
                            "--accept-source-agreements",
                            "--accept-package-agreements",
                        ],
                    ),
                    ("scoop", vec!["install", "git"]),
                ]
            } else if is_macos {
                vec![("brew", vec!["install", "git"])]
            } else {
                vec![
                    ("brew", vec!["install", "git"]),
                    ("sudo", vec!["apt-get", "install", "-y", "git"]),
                ]
            }
        }
        "node_npm" => {
            if is_windows {
                vec![
                    (
                        "winget",
                        vec![
                            "install",
                            "--id",
                            "OpenJS.NodeJS.LTS",
                            "-e",
                            "--accept-source-agreements",
                            "--accept-package-agreements",
                        ],
                    ),
                    ("scoop", vec!["install", "nodejs-lts"]),
                ]
            } else if is_macos {
                vec![("brew", vec!["install", "node"])]
            } else {
                vec![
                    ("brew", vec!["install", "node"]),
                    ("sudo", vec!["apt-get", "install", "-y", "nodejs", "npm"]),
                ]
            }
        }
        _ => Vec::new(),
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[tauri::command]
pub async fn run_doctor() -> Result<DoctorReport, String> {
    refresh_augmented_path();
    Ok(execute_doctor_checks().await)
}

#[tauri::command]
pub async fn fix_doctor_issue(
    app: AppHandle,
    check_id: String,
    git_user_name: Option<String>,
    git_user_email: Option<String>,
) -> Result<DoctorFixResult, String> {
    let trimmed_id = check_id.trim();
    if !CHECK_IDS.contains(&trimmed_id) {
        return Err(format!("Unknown doctor check ID: `{trimmed_id}`"));
    }

    // Guided modal flows for authentication checks
    if trimmed_id == "pi_auth" {
        return Ok(DoctorFixResult {
            check_id: trimmed_id.to_string(),
            success: true,
            message: "Opening Pi authentication & provider settings…".to_string(),
            output_lines: Vec::new(),
            open_flow: Some("pi_auth".to_string()),
            updated_report: None,
        });
    }

    if trimmed_id == "gh_auth" {
        return Ok(DoctorFixResult {
            check_id: trimmed_id.to_string(),
            success: true,
            message: "Opening guided GitHub login flow…".to_string(),
            output_lines: Vec::new(),
            open_flow: Some("gh_auth".to_string()),
            updated_report: None,
        });
    }

    // Special case for `git`: if `git --version` already works, configure `user.name` and `user.email`.
    if trimmed_id == "git" && run_cmd_capture("git", &["--version"]).await.is_ok() {
        let name = git_user_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("PiPan Developer");
        let email = git_user_email
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("developer@users.noreply.github.com");

        let mut lines = Vec::new();
        match run_streaming_fix_command(
            Some(app.clone()),
            trimmed_id,
            "git",
            &["config", "--global", "user.name", name],
        )
        .await
        {
            Ok(out) => lines.extend(out),
            Err((err, out)) => {
                lines.extend(out);
                return Ok(DoctorFixResult {
                    check_id: trimmed_id.to_string(),
                    success: false,
                    message: err,
                    output_lines: lines,
                    open_flow: None,
                    updated_report: Some(execute_doctor_checks().await),
                });
            }
        }

        match run_streaming_fix_command(
            Some(app),
            trimmed_id,
            "git",
            &["config", "--global", "user.email", email],
        )
        .await
        {
            Ok(out) => lines.extend(out),
            Err((err, out)) => {
                lines.extend(out);
                return Ok(DoctorFixResult {
                    check_id: trimmed_id.to_string(),
                    success: false,
                    message: err,
                    output_lines: lines,
                    open_flow: None,
                    updated_report: Some(execute_doctor_checks().await),
                });
            }
        }

        let report = execute_doctor_checks().await;
        return Ok(DoctorFixResult {
            check_id: trimmed_id.to_string(),
            success: true,
            message: format!("Configured Git global identity ({name} <{email}>)"),
            output_lines: lines,
            open_flow: None,
            updated_report: Some(report),
        });
    }

    let candidates = candidate_fix_commands_for_check(
        trimmed_id,
        cfg!(target_os = "windows"),
        cfg!(target_os = "macos"),
    );

    let mut all_lines = Vec::new();
    let mut last_err = String::from("No auto-fix command available for this check");

    for (program, args) in candidates {
        match run_streaming_fix_command(Some(app.clone()), trimmed_id, program, &args).await {
            Ok(lines) => {
                all_lines.extend(lines);
                refresh_augmented_path();
                let report = execute_doctor_checks().await;
                return Ok(DoctorFixResult {
                    check_id: trimmed_id.to_string(),
                    success: true,
                    message: format!("Successfully ran `{program} {}`", args.join(" ")),
                    output_lines: all_lines,
                    open_flow: None,
                    updated_report: Some(report),
                });
            }
            Err((err, lines)) => {
                last_err = err;
                all_lines.extend(lines);
            }
        }
    }

    refresh_augmented_path();
    let report = execute_doctor_checks().await;
    Ok(DoctorFixResult {
        check_id: trimmed_id.to_string(),
        success: false,
        message: last_err,
        output_lines: all_lines,
        open_flow: None,
        updated_report: Some(report),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_git_check_states_correctly() {
        let pass = evaluate_git_check(
            Ok("git version 2.45.2"),
            Some("Alice"),
            Some("alice@example.com"),
        );
        assert_eq!(pass.status, DoctorCheckStatus::Pass);
        assert!(!pass.can_auto_fix);
        assert!(pass.summary.contains("Alice <alice@example.com>"));

        let warn = evaluate_git_check(Ok("git version 2.45.2"), Some("Alice"), None);
        assert_eq!(warn.status, DoctorCheckStatus::Warn);
        assert!(warn.can_auto_fix);
        assert_eq!(warn.fix_action, "git_config");

        let fail = evaluate_git_check(Err("not found"), None, None);
        assert_eq!(fail.status, DoctorCheckStatus::Fail);
        assert!(fail.can_auto_fix);
        assert_eq!(fail.fix_action, "auto_command");
    }

    #[test]
    fn evaluates_node_pi_gh_checks_and_overall_summary() {
        let node_pass = evaluate_node_npm_check(Ok("v22.14.0"), Some("10.9.0"), Some("10.5.0"));
        assert_eq!(node_pass.status, DoctorCheckStatus::Pass);
        assert!(node_pass.summary.contains("npm 10.9.0"));
        assert!(node_pass.summary.contains("pnpm 10.5.0"));

        let node_warn = evaluate_node_npm_check(Ok("v22.14.0"), None, None);
        assert_eq!(node_warn.status, DoctorCheckStatus::Warn);

        let pi_fail = evaluate_pi_cli_check(Err("pi not found"));
        assert_eq!(pi_fail.status, DoctorCheckStatus::Fail);
        assert!(pi_fail.can_auto_fix);

        let gh_auth_pass = evaluate_gh_auth_check(
            true,
            Ok("github.com\n  ✓ Logged in to github.com account octocat (keyring)"),
        );
        assert_eq!(gh_auth_pass.status, DoctorCheckStatus::Pass);
        assert!(gh_auth_pass.summary.contains("@octocat"));

        let webhook_warn = evaluate_gh_webhook_check(true, Ok("gh copilot\tgithub/gh-copilot"));
        assert_eq!(webhook_warn.status, DoctorCheckStatus::Warn);

        let webhook_pass =
            evaluate_gh_webhook_check(true, Ok("gh webhook\tcli/gh-webhook\tv0.1.0"));
        assert_eq!(webhook_pass.status, DoctorCheckStatus::Pass);

        let report = summarize_report(vec![node_pass, node_warn, pi_fail]);
        assert_eq!(report.overall_status, DoctorCheckStatus::Fail);
        assert_eq!(report.pass_count, 1);
        assert_eq!(report.warn_count, 1);
        assert_eq!(report.fail_count, 1);
    }

    #[test]
    fn candidate_fix_commands_cover_all_fixable_checks() {
        let pi_cmds = candidate_fix_commands_for_check("pi_cli", true, false);
        assert_eq!(pi_cmds[0].0, "npm");
        assert!(pi_cmds[0].1.contains(&"@earendil-works/pi-coding-agent"));

        let gh_win = candidate_fix_commands_for_check("gh_cli", true, false);
        assert_eq!(gh_win[0].0, "winget");
        assert!(gh_win[0].1.contains(&"GitHub.cli"));

        let gh_mac = candidate_fix_commands_for_check("gh_cli", false, true);
        assert_eq!(gh_mac[0].0, "brew");

        let wh = candidate_fix_commands_for_check("gh_webhook", true, false);
        assert_eq!(wh[0].0, "gh");
        assert!(wh[0].1.contains(&"cli/gh-webhook"));
    }
}
