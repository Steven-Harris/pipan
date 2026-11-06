use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
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

    async fn shutdown_existing(&self) {
        let existing = {
            let mut guard = self.inner.lock().await;
            guard.generation += 1;
            guard.active.take()
        };

        if let Some(mut active) = existing {
            // 1. Drop ChildStdin first so Pi receives EOF on stdin per RPC spec
            drop(active.stdin.take());

            // 2. Wait up to 2 seconds for graceful shutdown, then force kill if needed
            let mut child_guard = active.child.lock().await;
            match tokio::time::timeout(Duration::from_secs(2), child_guard.wait()).await {
                Ok(_) => {}
                Err(_) => {
                    let _ = child_guard.kill().await;
                    let _ = child_guard.wait().await;
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
        let stdin = child.stdin.take().ok_or_else(|| {
            "Failed to capture stdin pipe for Pi RPC child process".to_string()
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            "Failed to capture stdout pipe for Pi RPC child process".to_string()
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            "Failed to capture stderr pipe for Pi RPC child process".to_string()
        })?;

        let child_arc = Arc::new(Mutex::new(child));

        let running_status = {
            let mut guard = self.inner.lock().await;
            guard.active = Some(ActiveChild {
                stdin: Some(stdin),
                child: child_arc.clone(),
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

fn build_pi_command(working_dir: &Path) -> Command {
    #[cfg(target_os = "windows")]
    {
        // Check if a native `pi.exe` exists on PATH first; otherwise invoke via cmd.exe
        // so `.cmd` shims created by npm/pnpm work seamlessly without flashing a console window.
        let mut cmd = if let Some(pi_exe) = find_on_path("pi.exe") {
            let mut c = Command::new(pi_exe);
            c.args(["--mode", "rpc", "--no-session"]);
            c
        } else {
            let mut c = Command::new("cmd.exe");
            c.args(["/d", "/s", "/c", "pi --mode rpc --no-session"]);
            c
        };
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
        let mut cmd = Command::new("pi");
        cmd.args(["--mode", "rpc", "--no-session"])
            .current_dir(working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        cmd
    }
}

#[cfg(target_os = "windows")]
fn find_on_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(binary_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
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
