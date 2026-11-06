//! GitHub CLI (`gh`) integration.
//!
//! All GitHub access goes through direct `gh` execution — no MCP servers and no
//! REST polling. Read-only commands return parsed JSON to the frontend; live
//! updates arrive through [`crate::webhook`].

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

use crate::pi_rpc::get_augmented_path;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Name of the official extension providing `gh webhook forward`.
pub const WEBHOOK_EXTENSION: &str = "cli/gh-webhook";

/// Builds a `gh` command with a non-interactive, hidden-window environment.
pub(crate) fn gh_command(args: &[String], cwd: Option<&Path>) -> Command {
    let mut cmd = Command::new("gh");
    cmd.args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let path = get_augmented_path();
    if !path.is_empty() {
        cmd.env("PATH", path);
    }
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

fn to_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

fn resolve_cwd(cwd: Option<&str>) -> Result<Option<PathBuf>, String> {
    match cwd {
        None | Some("") => Ok(None),
        Some(dir) => {
            let p = PathBuf::from(dir);
            if p.is_dir() {
                Ok(Some(p))
            } else {
                Err(format!("Not a directory: {}", dir))
            }
        }
    }
}

/// Runs `gh <args>` and returns trimmed stdout.
///
/// `gh pr checks` exits with code 8 while checks are pending; that is not an
/// error as long as it produced output.
pub async fn run_gh(args: &[String], cwd: Option<&Path>) -> Result<String, String> {
    let output = gh_command(args, cwd).output().await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "GitHub CLI (`gh`) was not found on PATH. Install it from https://cli.github.com"
                .to_string()
        } else {
            format!("Failed to run `gh {}`: {}", args.join(" "), e)
        }
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let pending_checks = output.status.code() == Some(8) && !stdout.is_empty();
    if !output.status.success() && !pending_checks {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(format!("`gh {}` failed: {}", args.join(" "), detail));
    }
    Ok(stdout)
}

/// Runs `gh <args>` and parses stdout as JSON.
pub async fn run_gh_json(args: &[String], cwd: Option<&Path>) -> Result<Value, String> {
    let out = run_gh(args, cwd).await?;
    if out.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&out)
        .map_err(|e| format!("`gh {}` did not return valid JSON: {}", args.join(" "), e))
}

/// Whether a `gh` invocation is on the read-only allowlist for [`gh_json`].
///
/// The generic command deliberately excludes mutating verbs and `gh api`.
pub fn is_allowed_gh_invocation(args: &[String]) -> bool {
    const ALLOWED: &[(&str, &[&str])] = &[
        ("pr", &["view", "list", "checks", "status"]),
        ("issue", &["view", "list", "status"]),
        ("run", &["view", "list"]),
        ("workflow", &["view", "list"]),
        ("repo", &["view", "list"]),
        ("release", &["view", "list"]),
    ];
    let mut positional = args.iter().filter(|a| !a.starts_with('-'));
    let (Some(group), Some(action)) = (positional.next(), positional.next()) else {
        return false;
    };
    // Leading flags (e.g. `-R owner/repo pr view`) are not supported: the group
    // must be the first argument so the allowlist cannot be sidestepped.
    if args.first().map(String::as_str) != Some(group.as_str()) {
        return false;
    }
    // `gh run view --log` etc. are not JSON; `--web` would open a browser.
    if args.iter().any(|a| a == "--web" || a == "-w") {
        return false;
    }
    ALLOWED
        .iter()
        .any(|(g, actions)| g == group && actions.contains(&action.as_str()))
}

/// Validates an `owner/name` repository slug.
pub fn is_valid_repo_slug(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let (Some(owner), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let ok = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    ok(owner) && ok(name)
}

/// Whether the `gh webhook` extension is installed.
pub async fn webhook_extension_installed() -> bool {
    match run_gh(&to_args(&["extension", "list"]), None).await {
        Ok(out) => out.lines().any(|l| l.contains("gh-webhook")),
        Err(_) => false,
    }
}

/// Installs the `gh webhook` extension.
pub async fn install_webhook_extension() -> Result<(), String> {
    run_gh(&to_args(&["extension", "install", WEBHOOK_EXTENSION]), None)
        .await
        .map(|_| ())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GhStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub authenticated: bool,
    pub account: Option<String>,
    pub webhook_extension: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn parse_version(out: &str) -> Option<String> {
    out.lines()
        .next()
        .and_then(|l| l.strip_prefix("gh version "))
        .map(|v| v.split_whitespace().next().unwrap_or(v).to_string())
}

#[tauri::command]
pub async fn gh_status() -> Result<GhStatus, String> {
    let version = match run_gh(&to_args(&["--version"]), None).await {
        Ok(out) => parse_version(&out),
        Err(e) => {
            return Ok(GhStatus {
                installed: false,
                version: None,
                authenticated: false,
                account: None,
                webhook_extension: false,
                error: Some(e),
            });
        }
    };

    let (authenticated, account, error) =
        match run_gh(&to_args(&["api", "user", "--jq", ".login"]), None).await {
            Ok(login) if !login.is_empty() => (true, Some(login), None),
            Ok(_) => (false, None, None),
            Err(e) => (false, None, Some(e)),
        };

    Ok(GhStatus {
        installed: true,
        version,
        authenticated,
        account,
        webhook_extension: webhook_extension_installed().await,
        error,
    })
}

/// Generic read-only `gh` JSON command, e.g. `["pr", "view", "12", "--json", "title"]`.
#[tauri::command]
pub async fn gh_json(args: Vec<String>, cwd: Option<String>) -> Result<Value, String> {
    if !is_allowed_gh_invocation(&args) {
        return Err(format!(
            "`gh {}` is not an allowed read-only command",
            args.join(" ")
        ));
    }
    let cwd = resolve_cwd(cwd.as_deref())?;
    run_gh_json(&args, cwd.as_deref()).await
}

const PR_VIEW_FIELDS: &str = "number,title,state,url,headRefName,baseRefName,author,isDraft,\
mergeable,reviewDecision,statusCheckRollup,updatedAt,body";
const PR_LIST_FIELDS: &str = "number,title,state,url,headRefName,baseRefName,author,isDraft,\
reviewDecision,updatedAt";
const PR_CHECK_FIELDS: &str = "name,state,bucket,workflow,link,event,startedAt,completedAt";
const ISSUE_LIST_FIELDS: &str = "number,title,state,url,author,labels,updatedAt";
const RUN_LIST_FIELDS: &str = "databaseId,displayTitle,status,conclusion,workflowName,\
headBranch,event,url,createdAt";

fn fields_arg(fields: Option<Vec<String>>, default: &str) -> Result<String, String> {
    match fields {
        Some(f) if !f.is_empty() => {
            if f.iter()
                .all(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_alphanumeric()))
            {
                Ok(f.join(","))
            } else {
                Err("Invalid JSON field name".to_string())
            }
        }
        _ => Ok(default.to_string()),
    }
}

fn selector_arg(sel: &str) -> Result<(), String> {
    // PR/issue number, URL, or branch name; never a flag.
    if sel.is_empty() || sel.starts_with('-') {
        Err(format!("Invalid selector: {}", sel))
    } else {
        Ok(())
    }
}

fn repo_args(repo: Option<String>, args: &mut Vec<String>) -> Result<(), String> {
    if let Some(r) = repo.filter(|r| !r.is_empty()) {
        if !is_valid_repo_slug(&r) {
            return Err(format!("Invalid repository: {}", r));
        }
        args.push("--repo".into());
        args.push(r);
    }
    Ok(())
}

/// `gh pr view [selector] --json …`. Without a selector, uses the PR for the
/// current branch of `cwd`.
#[tauri::command]
pub async fn gh_pr_view(
    cwd: Option<String>,
    repo: Option<String>,
    selector: Option<String>,
    fields: Option<Vec<String>>,
) -> Result<Value, String> {
    let mut args = to_args(&["pr", "view"]);
    if let Some(sel) = selector.filter(|s| !s.is_empty()) {
        selector_arg(&sel)?;
        args.push(sel);
    }
    repo_args(repo, &mut args)?;
    args.push("--json".into());
    args.push(fields_arg(fields, PR_VIEW_FIELDS)?);
    run_gh_json(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await
}

/// `gh pr list --json …`.
#[tauri::command]
pub async fn gh_pr_list(
    cwd: Option<String>,
    repo: Option<String>,
    state: Option<String>,
    limit: Option<u32>,
    fields: Option<Vec<String>>,
) -> Result<Value, String> {
    let mut args = to_args(&["pr", "list"]);
    repo_args(repo, &mut args)?;
    push_state_limit(&mut args, state, limit)?;
    args.push("--json".into());
    args.push(fields_arg(fields, PR_LIST_FIELDS)?);
    run_gh_json(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await
}

/// `gh pr checks [selector] --json …`.
#[tauri::command]
pub async fn gh_pr_checks(
    cwd: Option<String>,
    repo: Option<String>,
    selector: Option<String>,
    fields: Option<Vec<String>>,
) -> Result<Value, String> {
    let mut args = to_args(&["pr", "checks"]);
    if let Some(sel) = selector.filter(|s| !s.is_empty()) {
        selector_arg(&sel)?;
        args.push(sel);
    }
    repo_args(repo, &mut args)?;
    args.push("--json".into());
    args.push(fields_arg(fields, PR_CHECK_FIELDS)?);
    run_gh_json(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await
}

/// `gh issue list --json …`.
#[tauri::command]
pub async fn gh_issue_list(
    cwd: Option<String>,
    repo: Option<String>,
    state: Option<String>,
    limit: Option<u32>,
    fields: Option<Vec<String>>,
) -> Result<Value, String> {
    let mut args = to_args(&["issue", "list"]);
    repo_args(repo, &mut args)?;
    push_state_limit(&mut args, state, limit)?;
    args.push("--json".into());
    args.push(fields_arg(fields, ISSUE_LIST_FIELDS)?);
    run_gh_json(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await
}

/// `gh run list --json …` (GitHub Actions workflow runs).
#[tauri::command]
pub async fn gh_run_list(
    cwd: Option<String>,
    repo: Option<String>,
    branch: Option<String>,
    limit: Option<u32>,
    fields: Option<Vec<String>>,
) -> Result<Value, String> {
    let mut args = to_args(&["run", "list"]);
    repo_args(repo, &mut args)?;
    if let Some(b) = branch.filter(|b| !b.is_empty()) {
        selector_arg(&b)?;
        args.push("--branch".into());
        args.push(b);
    }
    if let Some(n) = limit {
        args.push("--limit".into());
        args.push(n.clamp(1, 100).to_string());
    }
    args.push("--json".into());
    args.push(fields_arg(fields, RUN_LIST_FIELDS)?);
    run_gh_json(&args, resolve_cwd(cwd.as_deref())?.as_deref()).await
}

fn push_state_limit(
    args: &mut Vec<String>,
    state: Option<String>,
    limit: Option<u32>,
) -> Result<(), String> {
    if let Some(s) = state.filter(|s| !s.is_empty()) {
        if !matches!(s.as_str(), "open" | "closed" | "merged" | "all") {
            return Err(format!("Invalid state: {}", s));
        }
        args.push("--state".into());
        args.push(s);
    }
    if let Some(n) = limit {
        args.push("--limit".into());
        args.push(n.clamp(1, 100).to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        to_args(args)
    }

    #[test]
    fn allowlist_accepts_read_only_commands() {
        assert!(is_allowed_gh_invocation(&v(&[
            "pr", "view", "1", "--json", "title"
        ])));
        assert!(is_allowed_gh_invocation(&v(&[
            "run", "list", "--json", "status"
        ])));
        assert!(is_allowed_gh_invocation(&v(&[
            "pr", "checks", "--json", "name"
        ])));
    }

    #[test]
    fn allowlist_rejects_mutations_and_bypasses() {
        assert!(!is_allowed_gh_invocation(&v(&["pr", "merge", "1"])));
        assert!(!is_allowed_gh_invocation(&v(&[
            "api",
            "-X",
            "DELETE",
            "/repos/a/b"
        ])));
        assert!(!is_allowed_gh_invocation(&v(&["issue", "close", "3"])));
        assert!(!is_allowed_gh_invocation(&v(&[
            "-R", "a/b", "pr", "merge", "1"
        ])));
        assert!(!is_allowed_gh_invocation(&v(&["pr", "view", "--web"])));
        assert!(!is_allowed_gh_invocation(&v(&["pr"])));
        assert!(!is_allowed_gh_invocation(&[]));
    }

    #[test]
    fn repo_slug_validation() {
        assert!(is_valid_repo_slug("Steven-Harris/pipan"));
        assert!(is_valid_repo_slug("a/b.c_d"));
        assert!(!is_valid_repo_slug("pipan"));
        assert!(!is_valid_repo_slug("a/b/c"));
        assert!(!is_valid_repo_slug("a/--flag b"));
        assert!(!is_valid_repo_slug("../x"));
        assert!(!is_valid_repo_slug("/x"));
    }

    #[test]
    fn field_and_selector_validation() {
        assert_eq!(fields_arg(None, "a,b").unwrap(), "a,b");
        assert_eq!(
            fields_arg(Some(vec!["title".into(), "url".into()]), "a").unwrap(),
            "title,url"
        );
        assert!(fields_arg(Some(vec!["a;b".into()]), "x").is_err());
        assert!(selector_arg("--web").is_err());
        assert!(selector_arg("123").is_ok());
    }

    #[test]
    fn parses_version() {
        assert_eq!(
            parse_version("gh version 2.103.3 (2026-10-26)\nhttps://x"),
            Some("2.103.3".to_string())
        );
        assert_eq!(parse_version("garbage"), None);
    }
}
