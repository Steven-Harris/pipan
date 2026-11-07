//! Persistent interactive shell session manager (`terminal.rs`).
//!
//! Manages per-session interactive shell processes (`powershell.exe` / `cmd.exe` on Windows,
//! `$SHELL` / `zsh` / `bash` / `sh` on macOS and Linux) rooted in the active session's `cwd`
//! or worktree. Streams output chunks over `terminal:output` and exit status over `terminal:exit`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;

use crate::pi_rpc::{get_augmented_path, terminate_process_tree};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub const DEFAULT_COLS: u16 = 80;
pub const DEFAULT_ROWS: u16 = 24;
pub const MIN_COLS: u16 = 20;
pub const MAX_COLS: u16 = 500;
pub const MIN_ROWS: u16 = 5;
pub const MAX_ROWS: u16 = 200;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSessionInfo {
    pub session_id: String,
    pub cwd: String,
    pub shell: String,
    pub pid: Option<u32>,
    pub cols: u16,
    pub rows: u16,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputPayload {
    pub session_id: String,
    /// `"stdout"` | `"stderr"` | `"system"`
    pub stream: String,
    pub data: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExitPayload {
    pub session_id: String,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub timestamp: u64,
}

struct ActiveTerminal {
    info: TerminalSessionInfo,
    stdin: Option<ChildStdin>,
    child: Arc<Mutex<Child>>,
    generation: u64,
}

struct TerminalManagerInner {
    sessions: HashMap<String, ActiveTerminal>,
    next_generation: u64,
}

#[derive(Clone)]
pub struct TerminalManager {
    inner: Arc<Mutex<TerminalManagerInner>>,
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn clamp_terminal_dimensions(cols: Option<u16>, rows: Option<u16>) -> (u16, u16) {
    let c = cols.unwrap_or(DEFAULT_COLS).clamp(MIN_COLS, MAX_COLS);
    let r = rows.unwrap_or(DEFAULT_ROWS).clamp(MIN_ROWS, MAX_ROWS);
    (c, r)
}

pub fn sanitize_terminal_session_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Resolves the working directory for a terminal session.
/// If `cwd` is `None` or empty, creates and returns the headless workspace directory.
pub fn resolve_terminal_cwd(cwd: Option<&str>) -> Result<PathBuf, String> {
    match cwd.map(str::trim).filter(|s| !s.is_empty()) {
        Some(dir_str) => {
            let path = PathBuf::from(dir_str);
            if !path.exists() || !path.is_dir() {
                return Err(format!(
                    "Terminal working directory does not exist: {}",
                    dir_str
                ));
            }
            Ok(path)
        }
        None => {
            let headless = std::env::temp_dir().join("pipan-headless");
            std::fs::create_dir_all(&headless).map_err(|e| {
                format!(
                    "Failed to create headless terminal directory ({}): {}",
                    headless.display(),
                    e
                )
            })?;
            Ok(headless)
        }
    }
}

/// Resolves the interactive shell executable and arguments for the current platform.
pub fn resolve_shell_program(preferred: Option<&str>) -> (String, Vec<String>) {
    let pref = preferred.map(str::trim).filter(|s| !s.is_empty());

    #[cfg(target_os = "windows")]
    {
        if let Some(p) = pref {
            let lower = p.to_ascii_lowercase();
            if lower == "cmd" || lower == "cmd.exe" || lower.ends_with("\\cmd.exe") {
                return ("cmd.exe".to_string(), vec!["/Q".to_string()]);
            }
            if lower == "pwsh" || lower == "pwsh.exe" {
                return (
                    "pwsh.exe".to_string(),
                    vec![
                        "-NoLogo".to_string(),
                        "-NoProfile".to_string(),
                        "-NoExit".to_string(),
                        "-Command".to_string(),
                        "-".to_string(),
                    ],
                );
            }
            if lower == "powershell"
                || lower == "powershell.exe"
                || lower.ends_with("\\powershell.exe")
            {
                return (
                    "powershell.exe".to_string(),
                    vec![
                        "-NoLogo".to_string(),
                        "-NoProfile".to_string(),
                        "-NoExit".to_string(),
                        "-Command".to_string(),
                        "-".to_string(),
                    ],
                );
            }
            return (p.to_string(), Vec::new());
        }

        // Default to PowerShell on Windows, with `-Command -` so piped stdin executes commands cleanly
        (
            "powershell.exe".to_string(),
            vec![
                "-NoLogo".to_string(),
                "-NoProfile".to_string(),
                "-NoExit".to_string(),
                "-Command".to_string(),
                "-".to_string(),
            ],
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(p) = pref {
            return (p.to_string(), Vec::new());
        }
        if let Ok(env_shell) = std::env::var("SHELL") {
            let trimmed = env_shell.trim();
            if !trimmed.is_empty() && Path::new(trimmed).exists() {
                return (trimmed.to_string(), Vec::new());
            }
        }
        for candidate in ["/bin/zsh", "/bin/bash", "/usr/bin/zsh", "/usr/bin/bash"] {
            if Path::new(candidate).exists() {
                return (candidate.to_string(), Vec::new());
            }
        }
        ("/bin/sh".to_string(), Vec::new())
    }
}

/// Normalizes input sent to the shell's stdin pipe.
/// Ensures commands end with a newline so line-oriented shells execute immediately,
/// while preserving raw Ctrl+C (`\x03`).
pub fn normalize_terminal_input(data: &str) -> Vec<u8> {
    if data == "\u{3}" {
        return b"\x03\n".to_vec();
    }
    if data.ends_with('\n') {
        data.as_bytes().to_vec()
    } else {
        let mut bytes = data.as_bytes().to_vec();
        bytes.push(b'\n');
        bytes
    }
}

pub(crate) fn build_shell_command(
    program: &str,
    args: &[String],
    working_dir: &Path,
    cols: u16,
    rows: u16,
) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(working_dir)
        .env("TERM", "xterm-256color")
        .env("COLORTERM", "truecolor")
        .env("COLUMNS", cols.to_string())
        .env("LINES", rows.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let augmented_path = get_augmented_path();
    if !augmented_path.is_empty() {
        cmd.env("PATH", augmented_path);
    }

    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    cmd
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(TerminalManagerInner {
                sessions: HashMap::new(),
                next_generation: 0,
            })),
        }
    }

    pub async fn start_session(
        &self,
        app_handle: &AppHandle,
        session_id: String,
        cwd: Option<String>,
        shell: Option<String>,
        cols: Option<u16>,
        rows: Option<u16>,
    ) -> Result<TerminalSessionInfo, String> {
        let sid = sanitize_terminal_session_id(&session_id);
        let working_dir = resolve_terminal_cwd(cwd.as_deref())?;
        let cwd_display = working_dir.to_string_lossy().to_string();
        let (cols_val, rows_val) = clamp_terminal_dimensions(cols, rows);

        // If an active shell is already running for this session_id in the same cwd (and no explicit shell change), reuse it
        let existing_to_stop = {
            let mut guard = self.inner.lock().await;
            if let Some(existing) = guard.sessions.get_mut(&sid) {
                let shell_matches = shell
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .is_none_or(|req| existing.info.shell == req);
                if existing.info.running && existing.info.cwd == cwd_display && shell_matches {
                    existing.info.cols = cols_val;
                    existing.info.rows = rows_val;
                    return Ok(existing.info.clone());
                }
            }
            guard.sessions.remove(&sid)
        };

        if let Some(mut old) = existing_to_stop {
            drop(old.stdin.take());
            let mut child_guard = old.child.lock().await;
            terminate_process_tree(&mut child_guard, old.info.pid).await;
        }

        let (program, args) = resolve_shell_program(shell.as_deref());
        let mut cmd = build_shell_command(&program, &args, &working_dir, cols_val, rows_val);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(primary_err) => {
                // Fallback on Windows to cmd.exe if powershell.exe failed to spawn
                #[cfg(target_os = "windows")]
                {
                    if program != "cmd.exe" {
                        let mut fallback = build_shell_command(
                            "cmd.exe",
                            &["/Q".to_string()],
                            &working_dir,
                            cols_val,
                            rows_val,
                        );
                        fallback.spawn().map_err(|e| {
                            format!(
                                "Failed to spawn terminal shell '{}' ({}) or fallback cmd.exe: {}",
                                program, primary_err, e
                            )
                        })?
                    } else {
                        return Err(format!(
                            "Failed to spawn terminal shell '{}': {}",
                            program, primary_err
                        ));
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    return Err(format!(
                        "Failed to spawn terminal shell '{}': {}",
                        program, primary_err
                    ));
                }
            }
        };

        let pid = child.id();
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to capture terminal stdin pipe".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture terminal stdout pipe".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Failed to capture terminal stderr pipe".to_string())?;

        let child_arc = Arc::new(Mutex::new(child));

        let info = TerminalSessionInfo {
            session_id: sid.clone(),
            cwd: cwd_display,
            shell: program,
            pid,
            cols: cols_val,
            rows: rows_val,
            running: true,
        };

        let generation = {
            let mut guard = self.inner.lock().await;
            guard.next_generation += 1;
            let gen_id = guard.next_generation;
            guard.sessions.insert(
                sid.clone(),
                ActiveTerminal {
                    info: info.clone(),
                    stdin: Some(stdin),
                    child: child_arc.clone(),
                    generation: gen_id,
                },
            );
            gen_id
        };

        // Stream stderr chunks over `terminal:output`
        let app_stderr = app_handle.clone();
        let sid_stderr = sid.clone();
        tokio::spawn(async move {
            let mut reader = stderr;
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = app_stderr.emit(
                            "terminal:output",
                            TerminalOutputPayload {
                                session_id: sid_stderr.clone(),
                                stream: "stderr".to_string(),
                                data,
                                timestamp: now_millis(),
                            },
                        );
                    }
                    Err(_) => break,
                }
            }
        });

        // Stream stdout chunks over `terminal:output` and emit `terminal:exit` when process finishes
        let app_stdout = app_handle.clone();
        let sid_stdout = sid.clone();
        let inner_clone = self.inner.clone();
        tokio::spawn(async move {
            let mut reader = stdout;
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buf[..n]).to_string();
                        let _ = app_stdout.emit(
                            "terminal:output",
                            TerminalOutputPayload {
                                session_id: sid_stdout.clone(),
                                stream: "stdout".to_string(),
                                data,
                                timestamp: now_millis(),
                            },
                        );
                    }
                    Err(_) => break,
                }
            }

            let exit_status = {
                let mut child_guard = child_arc.lock().await;
                child_guard.wait().await.ok()
            };

            let should_emit = {
                let mut guard = inner_clone.lock().await;
                if let Some(active) = guard.sessions.get_mut(&sid_stdout)
                    && active.generation == generation
                {
                    active.info.running = false;
                    active.info.pid = None;
                    active.stdin = None;
                    true
                } else {
                    false
                }
            };

            if should_emit {
                let exit_code = exit_status.and_then(|s| s.code());
                let success = exit_status.is_some_and(|s| s.success());
                let _ = app_stdout.emit(
                    "terminal:exit",
                    TerminalExitPayload {
                        session_id: sid_stdout,
                        exit_code,
                        success,
                        timestamp: now_millis(),
                    },
                );
            }
        });

        Ok(info)
    }

    pub async fn send_input(&self, session_id: &str, data: &str) -> Result<(), String> {
        let sid = sanitize_terminal_session_id(session_id);
        let bytes = normalize_terminal_input(data);

        let mut guard = self.inner.lock().await;
        let active = guard
            .sessions
            .get_mut(&sid)
            .ok_or_else(|| format!("No active terminal session for '{}'", sid))?;

        if !active.info.running {
            return Err(format!("Terminal session '{}' is no longer running", sid));
        }

        let stdin = active
            .stdin
            .as_mut()
            .ok_or_else(|| format!("Terminal stdin for session '{}' is closed", sid))?;

        stdin
            .write_all(&bytes)
            .await
            .map_err(|e| format!("Failed to write to terminal stdin: {}", e))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush terminal stdin: {}", e))?;

        Ok(())
    }

    pub async fn resize(
        &self,
        session_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<TerminalSessionInfo, String> {
        let sid = sanitize_terminal_session_id(session_id);
        let (cols_val, rows_val) = clamp_terminal_dimensions(Some(cols), Some(rows));

        let mut guard = self.inner.lock().await;
        let active = guard
            .sessions
            .get_mut(&sid)
            .ok_or_else(|| format!("No active terminal session for '{}'", sid))?;

        active.info.cols = cols_val;
        active.info.rows = rows_val;
        Ok(active.info.clone())
    }

    pub async fn stop_session(
        &self,
        app_handle: &AppHandle,
        session_id: &str,
    ) -> Result<bool, String> {
        let sid = sanitize_terminal_session_id(session_id);
        let removed = {
            let mut guard = self.inner.lock().await;
            guard.sessions.remove(&sid)
        };

        if let Some(mut active) = removed {
            drop(active.stdin.take());
            let mut child_guard = active.child.lock().await;
            terminate_process_tree(&mut child_guard, active.info.pid).await;
            let _ = app_handle.emit(
                "terminal:exit",
                TerminalExitPayload {
                    session_id: sid,
                    exit_code: None,
                    success: false,
                    timestamp: now_millis(),
                },
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn shutdown_all(&self) {
        let drained: Vec<ActiveTerminal> = {
            let mut guard = self.inner.lock().await;
            guard.sessions.drain().map(|(_, v)| v).collect()
        };

        for mut active in drained {
            drop(active.stdin.take());
            let mut child_guard = active.child.lock().await;
            terminate_process_tree(&mut child_guard, active.info.pid).await;
        }
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[tauri::command]
pub async fn start_terminal_session(
    app_handle: AppHandle,
    manager: State<'_, TerminalManager>,
    session_id: String,
    cwd: Option<String>,
    shell: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
) -> Result<TerminalSessionInfo, String> {
    manager
        .start_session(&app_handle, session_id, cwd, shell, cols, rows)
        .await
}

#[tauri::command]
pub async fn send_terminal_input(
    manager: State<'_, TerminalManager>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    manager.send_input(&session_id, &data).await
}

#[tauri::command]
pub async fn resize_terminal(
    manager: State<'_, TerminalManager>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<TerminalSessionInfo, String> {
    manager.resize(&session_id, cols, rows).await
}

#[tauri::command]
pub async fn stop_terminal_session(
    app_handle: AppHandle,
    manager: State<'_, TerminalManager>,
    session_id: String,
) -> Result<bool, String> {
    manager.stop_session(&app_handle, &session_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration, timeout};

    #[test]
    fn test_normalize_terminal_input_appends_newline_and_handles_ctrl_c() {
        assert_eq!(normalize_terminal_input("echo hello"), b"echo hello\n");
        assert_eq!(normalize_terminal_input("echo hello\n"), b"echo hello\n");
        assert_eq!(normalize_terminal_input("\u{3}"), b"\x03\n");
    }

    #[test]
    fn test_clamp_terminal_dimensions() {
        assert_eq!(
            clamp_terminal_dimensions(None, None),
            (DEFAULT_COLS, DEFAULT_ROWS)
        );
        assert_eq!(
            clamp_terminal_dimensions(Some(5), Some(2)),
            (MIN_COLS, MIN_ROWS)
        );
        assert_eq!(
            clamp_terminal_dimensions(Some(9999), Some(9999)),
            (MAX_COLS, MAX_ROWS)
        );
        assert_eq!(clamp_terminal_dimensions(Some(120), Some(40)), (120, 40));
    }

    #[test]
    fn test_resolve_shell_program_defaults_and_overrides() {
        let (default_shell, _) = resolve_shell_program(None);
        assert!(!default_shell.is_empty());

        #[cfg(target_os = "windows")]
        {
            let (cmd_shell, cmd_args) = resolve_shell_program(Some("cmd.exe"));
            assert_eq!(cmd_shell, "cmd.exe");
            assert_eq!(cmd_args, vec!["/Q".to_string()]);
        }

        #[cfg(not(target_os = "windows"))]
        {
            let (sh_shell, sh_args) = resolve_shell_program(Some("/bin/sh"));
            assert_eq!(sh_shell, "/bin/sh");
            assert!(sh_args.is_empty());
        }
    }

    #[test]
    fn test_resolve_terminal_cwd_creates_headless_fallback_and_rejects_missing() {
        let headless = resolve_terminal_cwd(None).expect("headless cwd");
        assert!(headless.exists());
        assert!(headless.is_dir());

        let missing = resolve_terminal_cwd(Some("/nonexistent/pipan/path/xyz987"));
        assert!(missing.is_err());
    }

    #[tokio::test]
    async fn test_spawn_and_interact_with_shell_process() {
        let cwd = resolve_terminal_cwd(None).expect("headless cwd");
        let (program, args) = resolve_shell_program(None);
        let mut cmd = build_shell_command(&program, &args, &cwd, 80, 24);
        let mut child = cmd.spawn().expect("spawn shell");
        let pid = child.id();

        let mut stdin = child.stdin.take().expect("stdin");
        let mut stdout = child.stdout.take().expect("stdout");

        let payload = normalize_terminal_input("echo pipan_shell_test_marker");
        stdin.write_all(&payload).await.expect("write command");
        stdin.flush().await.expect("flush stdin");

        let mut collected = String::new();
        let mut buf = [0u8; 1024];
        let read_result = timeout(Duration::from_secs(10), async {
            loop {
                let n = stdout.read(&mut buf).await.expect("read stdout");
                if n == 0 {
                    break;
                }
                collected.push_str(&String::from_utf8_lossy(&buf[..n]));
                if collected.contains("pipan_shell_test_marker") {
                    break;
                }
            }
        })
        .await;

        terminate_process_tree(&mut child, pid).await;
        assert!(
            read_result.is_ok(),
            "Timed out waiting for shell output; got: {collected}"
        );
        assert!(collected.contains("pipan_shell_test_marker"));
    }
}
