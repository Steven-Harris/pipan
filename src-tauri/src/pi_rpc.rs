use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSessionStatus {
    pub state: String, // "disconnected" | "starting" | "running" | "error"
    pub mode: String,  // "headless" | "directory"
    pub cwd: Option<String>,
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Default for PiSessionStatus {
    fn default() -> Self {
        Self {
            state: "disconnected".to_string(),
            mode: "headless".to_string(),
            cwd: None,
            pid: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiStderrPayload {
    pub line: String,
    pub timestamp: u64,
}

struct ActiveChild {
    stdin: Option<ChildStdin>,
    child: Arc<Mutex<Child>>,
    pid: Option<u32>,
}

struct PiSessionInner {
    status: PiSessionStatus,
    active: Option<ActiveChild>,
    generation: u64,
}

#[derive(Clone)]
pub struct PiRpcManager {
    inner: Arc<Mutex<PiSessionInner>>,
}

impl Default for PiRpcManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PiRpcManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PiSessionInner {
                status: PiSessionStatus::default(),
                active: None,
                generation: 0,
            })),
        }
    }

    pub async fn get_status(&self) -> PiSessionStatus {
        let guard = self.inner.lock().await;
        guard.status.clone()
    }

    pub async fn shutdown(&self) {
        self.shutdown_existing().await;
    }

    async fn shutdown_existing(&self) {
        let existing = {
            let mut guard = self.inner.lock().await;
            guard.generation += 1;
            guard.active.take()
        };

        if let Some(mut active) = existing {
            // 1. Drop ChildStdin first so Pi receives EOF on stdin per RPC spec
            drop(active.stdin.take());

            // 2. Wait up to 2 seconds for graceful shutdown, then force kill the process tree if needed
            let mut child_guard = active.child.lock().await;
            match tokio::time::timeout(Duration::from_secs(2), child_guard.wait()).await {
                Ok(_) => {}
                Err(_) => {
                    terminate_process_tree(&mut child_guard, active.pid).await;
                }
            }
        }
    }

    pub async fn stop_session(&self, app_handle: &AppHandle) -> PiSessionStatus {
        self.shutdown_existing().await;

        let status = {
            let mut guard = self.inner.lock().await;
            guard.status = PiSessionStatus {
                state: "disconnected".to_string(),
                mode: guard.status.mode.clone(),
                cwd: guard.status.cwd.clone(),
                pid: None,
                error: None,
            };
            guard.status.clone()
        };

        let _ = app_handle.emit("pi:status", &status);
        status
    }

    pub async fn start_session(
        &self,
        app_handle: &AppHandle,
        cwd: Option<String>,
    ) -> Result<PiSessionStatus, String> {
        // Cleanly stop any existing child process first
        self.shutdown_existing().await;

        // Resolve working directory and mode
        let trimmed_cwd = cwd
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let (mode, working_dir): (String, PathBuf) = match trimmed_cwd {
            Some(path_str) => {
                let path = PathBuf::from(&path_str);
                if !path.exists() || !path.is_dir() {
                    let err_msg = format!("Selected directory does not exist: {}", path_str);
                    let err_status = PiSessionStatus {
                        state: "error".to_string(),
                        mode: "directory".to_string(),
                        cwd: Some(path_str),
                        pid: None,
                        error: Some(err_msg.clone()),
                    };
                    let mut guard = self.inner.lock().await;
                    guard.status = err_status.clone();
                    let _ = app_handle.emit("pi:status", &err_status);
                    return Err(err_msg);
                }
                ("directory".to_string(), path)
            }
            None => {
                let headless_dir = std::env::temp_dir().join("pipan-headless");
                if let Err(e) = std::fs::create_dir_all(&headless_dir) {
                    let err_msg = format!(
                        "Failed to create headless temp workspace ({}): {}",
                        headless_dir.display(),
                        e
                    );
                    let err_status = PiSessionStatus {
                        state: "error".to_string(),
                        mode: "headless".to_string(),
                        cwd: None,
                        pid: None,
                        error: Some(err_msg.clone()),
                    };
                    let mut guard = self.inner.lock().await;
                    guard.status = err_status.clone();
                    let _ = app_handle.emit("pi:status", &err_status);
                    return Err(err_msg);
                }
                ("headless".to_string(), headless_dir)
            }
        };

        let cwd_display = working_dir.to_string_lossy().to_string();

        // Emit starting status
        let current_generation = {
            let mut guard = self.inner.lock().await;
            guard.generation += 1;
            guard.status = PiSessionStatus {
                state: "starting".to_string(),
                mode: mode.clone(),
                cwd: Some(cwd_display.clone()),
                pid: None,
                error: None,
            };
            let _ = app_handle.emit("pi:status", &guard.status);
            guard.generation
        };

        // Build and spawn the Pi RPC process
        let mut cmd = build_pi_command(&working_dir);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("Failed to spawn `pi --mode rpc --no-session`: {}", e);
                let err_status = PiSessionStatus {
                    state: "error".to_string(),
                    mode,
                    cwd: Some(cwd_display),
                    pid: None,
                    error: Some(err_msg.clone()),
                };
                let mut guard = self.inner.lock().await;
                guard.status = err_status.clone();
                let _ = app_handle.emit("pi:status", &err_status);
                return Err(err_msg);
            }
        };

        let pid = child.id();
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to capture stdin pipe for Pi RPC child process".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture stdout pipe for Pi RPC child process".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Failed to capture stderr pipe for Pi RPC child process".to_string())?;

        let child_arc = Arc::new(Mutex::new(child));

        let running_status = {
            let mut guard = self.inner.lock().await;
            guard.active = Some(ActiveChild {
                stdin: Some(stdin),
                child: child_arc.clone(),
                pid,
            });
            guard.status = PiSessionStatus {
                state: "running".to_string(),
                mode: mode.clone(),
                cwd: Some(cwd_display.clone()),
                pid,
                error: None,
            };
            guard.status.clone()
        };

        let _ = app_handle.emit("pi:status", &running_status);

        // Spawn stderr reader loop so pipe backpressure never blocks Pi
        let app_stderr = app_handle.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buf = Vec::with_capacity(1024);
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let line_bytes = trim_line_endings(&buf);
                        if line_bytes.is_empty() {
                            continue;
                        }
                        let line = String::from_utf8_lossy(line_bytes).to_string();
                        let timestamp = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        let _ = app_stderr.emit("pi:stderr", PiStderrPayload { line, timestamp });
                    }
                    Err(_) => break,
                }
            }
        });

        // Spawn strict JSONL stdout reader loop (split strictly on '\n', never U+2028/U+2029)
        let app_stdout = app_handle.clone();
        let inner_clone = self.inner.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut buf = Vec::with_capacity(8192);
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let line_bytes = trim_line_endings(&buf);
                        if line_bytes.is_empty() {
                            continue;
                        }
                        match serde_json::from_slice::<Value>(line_bytes) {
                            Ok(json_val) => {
                                let _ = app_stdout.emit("pi:event", json_val);
                            }
                            Err(parse_err) => {
                                let raw = String::from_utf8_lossy(line_bytes).to_string();
                                let timestamp = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .map(|d| d.as_millis() as u64)
                                    .unwrap_or(0);
                                let _ = app_stdout.emit(
                                    "pi:stderr",
                                    PiStderrPayload {
                                        line: format!(
                                            "[RPC JSONL parse warning: {}] {}",
                                            parse_err, raw
                                        ),
                                        timestamp,
                                    },
                                );
                            }
                        }
                    }
                    Err(read_err) => {
                        let timestamp = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        let _ = app_stdout.emit(
                            "pi:stderr",
                            PiStderrPayload {
                                line: format!("[RPC stdout read error] {}", read_err),
                                timestamp,
                            },
                        );
                        break;
                    }
                }
            }

            // Child stdout reached EOF; check exit status if this generation is still active
            let exit_status = {
                let mut child_guard = child_arc.lock().await;
                child_guard.wait().await.ok()
            };

            let mut guard = inner_clone.lock().await;
            if guard.generation == current_generation {
                guard.active = None;
                let (next_state, err_msg) = match exit_status {
                    Some(status) if !status.success() => (
                        "error".to_string(),
                        Some(format!("Pi process exited with status: {}", status)),
                    ),
                    _ => ("disconnected".to_string(), None),
                };
                guard.status = PiSessionStatus {
                    state: next_state,
                    mode: guard.status.mode.clone(),
                    cwd: guard.status.cwd.clone(),
                    pid: None,
                    error: err_msg,
                };
                let _ = app_stdout.emit("pi:status", &guard.status);
            }
        });

        Ok(running_status)
    }

    pub async fn send_command(&self, command: Value) -> Result<(), String> {
        let mut payload = serde_json::to_vec(&command)
            .map_err(|e| format!("Failed to serialize RPC command: {}", e))?;
        payload.push(b'\n');

        let mut guard = self.inner.lock().await;
        let active = guard
            .active
            .as_mut()
            .ok_or_else(|| "No active Pi RPC session is running".to_string())?;
        let stdin = active
            .stdin
            .as_mut()
            .ok_or_else(|| "Pi RPC stdin is closed".to_string())?;

        stdin
            .write_all(&payload)
            .await
            .map_err(|e| format!("Failed to write command to Pi stdin: {}", e))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush Pi stdin: {}", e))?;

        Ok(())
    }
}

fn trim_line_endings(buf: &[u8]) -> &[u8] {
    let mut end = buf.len();
    if end > 0 && buf[end - 1] == b'\n' {
        end -= 1;
    }
    if end > 0 && buf[end - 1] == b'\r' {
        end -= 1;
    }
    &buf[..end]
}

async fn terminate_process_tree(child: &mut Child, pid: Option<u32>) {
    #[cfg(target_os = "windows")]
    {
        if let Some(pid) = pid {
            let mut tk = Command::new("taskkill");
            tk.args(["/F", "/T", "/PID", &pid.to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW);
            let _ = tk.status().await;
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(pid) = pid {
            let mut term = Command::new("kill");
            term.args(["-TERM", &pid.to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let _ = term.status().await;

            if tokio::time::timeout(Duration::from_millis(500), child.wait())
                .await
                .is_ok()
            {
                return;
            }
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
}

fn build_pi_command(working_dir: &Path) -> Command {
    let resolved_path = get_augmented_path();

    #[cfg(target_os = "windows")]
    {
        // Check if a native `pi.exe` exists first; otherwise check `pi.cmd` / `pi` on augmented PATH
        // and invoke via cmd.exe so `.cmd` shims created by npm/pnpm work seamlessly without flashing a console window.
        let mut cmd = if let Some(pi_exe) = find_executable_in_path("pi.exe", &resolved_path) {
            let mut c = Command::new(pi_exe);
            c.args(["--mode", "rpc", "--no-session"]);
            c
        } else if let Some(pi_cmd) = find_executable_in_path("pi.cmd", &resolved_path) {
            let mut c = Command::new(pi_cmd);
            c.args(["--mode", "rpc", "--no-session"]);
            c
        } else {
            let mut c = Command::new("cmd.exe");
            c.args(["/d", "/c", "pi", "--mode", "rpc", "--no-session"]);
            c
        };
        if !resolved_path.is_empty() {
            cmd.env("PATH", &resolved_path);
        }
        cmd.current_dir(working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .creation_flags(CREATE_NO_WINDOW);
        cmd
    }

    #[cfg(not(target_os = "windows"))]
    {
        let program =
            find_executable_in_path("pi", &resolved_path).unwrap_or_else(|| PathBuf::from("pi"));
        let mut cmd = Command::new(program);
        if !resolved_path.is_empty() {
            cmd.env("PATH", &resolved_path);
        }
        cmd.args(["--mode", "rpc", "--no-session"])
            .current_dir(working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        cmd
    }
}

fn find_executable_in_path(binary_name: &str, path_var: &OsString) -> Option<PathBuf> {
    for dir in std::env::split_paths(path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(binary_name);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            return meta.permissions().mode() & 0o111 != 0;
        }
        false
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn get_augmented_path() -> OsString {
    static AUGMENTED_PATH: OnceLock<OsString> = OnceLock::new();
    AUGMENTED_PATH.get_or_init(compute_augmented_path).clone()
}

fn compute_augmented_path() -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    let mut push_dir = |p: PathBuf| {
        if !p.as_os_str().is_empty() && seen.insert(p.clone()) {
            dirs.push(p);
        }
    };

    // 1. Start with current process PATH
    if let Some(current_path) = std::env::var_os("PATH") {
        for p in std::env::split_paths(&current_path) {
            push_dir(p);
        }
    }

    // 2. On Unix (macOS / Linux), GUI apps launched from Finder/Dock/Desktop often have a minimal PATH.
    // Query the user's login shell once for their full PATH.
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(shell_path) = query_unix_login_shell_path() {
            for p in std::env::split_paths(&shell_path) {
                push_dir(p);
            }
        }
    }

    // 3. Append well-known CLI & Node/Rust/Homebrew installation paths across platforms
    for p in candidate_cli_dirs() {
        push_dir(p);
    }

    std::env::join_paths(dirs).unwrap_or_else(|_| std::env::var_os("PATH").unwrap_or_default())
}

#[cfg(not(target_os = "windows"))]
fn query_unix_login_shell_path() -> Option<OsString> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh".to_string()
            } else {
                "/bin/sh".to_string()
            }
        });

    let output = std::process::Command::new(&shell)
        .args(["-l", "-c", "printf '%s' \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path_str.is_empty() {
        None
    } else {
        Some(OsString::from(path_str))
    }
}

fn candidate_cli_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
            dirs.push(appdata.join("npm"));
            dirs.push(appdata.join("pnpm"));
        }
        if let Some(local_appdata) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            dirs.push(local_appdata.join("pnpm"));
            dirs.push(local_appdata.join("Volta").join("bin"));
            dirs.push(local_appdata.join("Microsoft").join("WinGet").join("Links"));
            dirs.push(local_appdata.join("Programs").join("nodejs"));
        }
        if let Some(userprofile) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
            dirs.push(userprofile.join(".pi").join("agent").join("bin"));
            dirs.push(userprofile.join(".local").join("bin"));
            dirs.push(userprofile.join(".volta").join("bin"));
            dirs.push(userprofile.join(".bun").join("bin"));
            dirs.push(userprofile.join(".cargo").join("bin"));
            dirs.push(userprofile.join("scoop").join("shims"));
        }
        if let Some(program_files) = std::env::var_os("ProgramFiles").map(PathBuf::from) {
            dirs.push(program_files.join("nodejs"));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            dirs.push(home.join(".pi").join("agent").join("bin"));
            dirs.push(home.join(".local").join("bin"));
            dirs.push(home.join(".local").join("share").join("pnpm"));
            dirs.push(home.join("Library").join("pnpm"));
            dirs.push(home.join(".npm-global").join("bin"));
            dirs.push(home.join(".volta").join("bin"));
            dirs.push(home.join(".bun").join("bin"));
            dirs.push(home.join(".cargo").join("bin"));
            dirs.push(home.join(".local").join("share").join("mise").join("shims"));
            dirs.push(home.join(".asdf").join("shims"));
            dirs.push(home.join(".fnm").join("current").join("bin"));
        }
        dirs.push(PathBuf::from("/opt/homebrew/bin"));
        dirs.push(PathBuf::from("/opt/homebrew/sbin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/usr/bin"));
        dirs.push(PathBuf::from("/bin"));
        dirs.push(PathBuf::from("/usr/sbin"));
        dirs.push(PathBuf::from("/sbin"));
    }

    dirs
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[tauri::command]
pub async fn start_pi_session(
    app_handle: AppHandle,
    manager: State<'_, PiRpcManager>,
    cwd: Option<String>,
) -> Result<PiSessionStatus, String> {
    manager.start_session(&app_handle, cwd).await
}

#[tauri::command]
pub async fn stop_pi_session(
    app_handle: AppHandle,
    manager: State<'_, PiRpcManager>,
) -> Result<PiSessionStatus, String> {
    Ok(manager.stop_session(&app_handle).await)
}

#[tauri::command]
pub async fn send_pi_command(
    manager: State<'_, PiRpcManager>,
    command: Value,
) -> Result<(), String> {
    manager.send_command(command).await
}

#[tauri::command]
pub async fn get_pi_status(manager: State<'_, PiRpcManager>) -> Result<PiSessionStatus, String> {
    Ok(manager.get_status().await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim_line_endings_handles_lf_and_crlf() {
        assert_eq!(
            trim_line_endings(b"{\"type\":\"ready\"}\n"),
            b"{\"type\":\"ready\"}"
        );
        assert_eq!(
            trim_line_endings(b"{\"type\":\"ready\"}\r\n"),
            b"{\"type\":\"ready\"}"
        );
        assert_eq!(
            trim_line_endings(b"{\"type\":\"ready\"}\r"),
            b"{\"type\":\"ready\"}"
        );
        assert_eq!(
            trim_line_endings(b"{\"type\":\"ready\"}"),
            b"{\"type\":\"ready\"}"
        );
        assert_eq!(trim_line_endings(b"\r\n"), b"");
        assert_eq!(trim_line_endings(b"\n"), b"");
        assert_eq!(trim_line_endings(b""), b"");
    }

    #[test]
    fn test_augmented_path_is_non_empty_and_contains_candidates() {
        let augmented = compute_augmented_path();
        assert!(!augmented.is_empty());
        let split: Vec<PathBuf> = std::env::split_paths(&augmented).collect();
        assert!(!split.is_empty());
    }

    #[test]
    fn test_find_executable_in_path_discovers_file() {
        let temp_dir = std::env::temp_dir().join(format!(
            "pipan-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp test dir");

        #[cfg(target_os = "windows")]
        let bin_name = "mock-pi.cmd";
        #[cfg(not(target_os = "windows"))]
        let bin_name = "mock-pi";

        let bin_path = temp_dir.join(bin_name);
        std::fs::write(&bin_path, b"echo test\n").expect("write mock binary");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin_path, perms).unwrap();
        }

        let joined_path = std::env::join_paths([&temp_dir]).unwrap();
        let found = find_executable_in_path(bin_name, &joined_path);
        assert_eq!(found.as_deref(), Some(bin_path.as_path()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_windows_cmd_shim_spawns_without_escaped_quote_error() {
        let temp_dir = std::env::temp_dir().join(format!(
            "pipan test {}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp test dir");

        let bin_path = temp_dir.join("pi.cmd");
        std::fs::write(&bin_path, "@ECHO off\r\nECHO args:%*\r\n").expect("write mock pi.cmd");

        let output = Command::new(&bin_path)
            .args(["--mode", "rpc", "--no-session"])
            .current_dir(&temp_dir)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .await
            .expect("spawn pi.cmd");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "expected success, got status {:?}, stderr: {}",
            output.status,
            stderr
        );
        assert_eq!(stdout.trim(), "args:--mode rpc --no-session");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
