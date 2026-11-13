use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::Mutex;

use crate::db::Database;
use crate::storage::{
    SaveSessionMetadataInput, default_session_name, resolve_session_file_path, sanitize_session_id,
};

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
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "sessionFilePath", skip_serializing_if = "Option::is_none")]
    pub session_file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Default for PiSessionStatus {
    fn default() -> Self {
        Self {
            state: "disconnected".to_string(),
            mode: "headless".to_string(),
            cwd: None,
            pid: None,
            error: None,
            session_id: None,
            session_file_path: None,
            name: None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPiOptions {
    pub cwd: Option<String>,
    pub session_id: Option<String>,
    pub session_file_path: Option<String>,
    pub name: Option<String>,
    pub ephemeral: Option<bool>,
    pub model: Option<String>,
    pub thinking_level: Option<String>,
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

    pub async fn update_active_session_name(&self, session_id: &str, name: &str) {
        let mut guard = self.inner.lock().await;
        if guard.status.session_id.as_deref() == Some(session_id) {
            guard.status.name = Some(name.to_string());
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
                session_id: guard.status.session_id.clone(),
                session_file_path: guard.status.session_file_path.clone(),
                name: guard.status.name.clone(),
            };
            guard.status.clone()
        };

        if let (Some(sid), Some(db)) = (
            status.session_id.clone(),
            app_handle
                .try_state::<Database>()
                .map(|s| s.inner().clone()),
        ) {
            let _ =
                tokio::task::spawn_blocking(move || db.set_session_status(&sid, "disconnected"))
                    .await;
        }

        let _ = app_handle.emit("pi:status", &status);
        status
    }

    pub async fn start_session(
        &self,
        app_handle: &AppHandle,
        options: StartPiOptions,
    ) -> Result<PiSessionStatus, String> {
        // Cleanly stop any existing child process first
        self.shutdown_existing().await;

        // Resolve working directory and mode
        let trimmed_cwd = options
            .cwd
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
                        session_id: None,
                        session_file_path: None,
                        name: None,
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
                        session_id: None,
                        session_file_path: None,
                        name: None,
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
        let ephemeral = options.ephemeral.unwrap_or(false);

        let session_id = options
            .session_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(sanitize_session_id)
            .unwrap_or_else(crate::worktree::new_session_id);

        let session_name = options
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_session_name(&mode, Some(&cwd_display), None));

        let session_file_path_buf: Option<PathBuf> = if ephemeral {
            None
        } else if let Some(explicit) = options
            .session_file_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let p = PathBuf::from(explicit);
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            Some(p)
        } else {
            let sessions_dir = app_handle
                .path()
                .app_local_data_dir()
                .map(|d| d.join("sessions"))
                .unwrap_or_else(|_| std::env::temp_dir().join("pipan-sessions"));
            let _ = std::fs::create_dir_all(&sessions_dir);
            Some(resolve_session_file_path(&sessions_dir, &session_id))
        };

        let session_file_str = session_file_path_buf
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());

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
                session_id: Some(session_id.clone()),
                session_file_path: session_file_str.clone(),
                name: Some(session_name.clone()),
            };
            let _ = app_handle.emit("pi:status", &guard.status);
            guard.generation
        };

        // Build and spawn the Pi RPC process
        let mut extra_args: Vec<OsString> = Vec::new();
        if let Some(model_arg) = options
            .model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            extra_args.push(OsString::from("--model"));
            extra_args.push(OsString::from(model_arg));
        }
        if let Some(thinking_arg) = options
            .thinking_level
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            extra_args.push(OsString::from("--thinking"));
            extra_args.push(OsString::from(thinking_arg));
        }

        // Extension channel: load the shim so other Pi extensions can drive PiPan UI. A failure
        // to write it only disables the channel.
        match crate::channel::ensure_channel_shim(app_handle) {
            Ok(shim_path) => extra_args.extend(crate::channel::channel_shim_args(&shim_path)),
            Err(e) => eprintln!("PiPan: extension channel disabled: {e}"),
        }

        let cli_args = build_pi_session_args(
            session_file_path_buf.as_deref(),
            Some(session_name.as_str()),
            ephemeral,
            &extra_args,
        );
        let mut cmd = build_pi_command_with_args(&working_dir, &cli_args);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let err_msg = format!("Failed to spawn `pi --mode rpc`: {}", e);
                let err_status = PiSessionStatus {
                    state: "error".to_string(),
                    mode,
                    cwd: Some(cwd_display),
                    pid: None,
                    error: Some(err_msg.clone()),
                    session_id: Some(session_id),
                    session_file_path: session_file_str,
                    name: Some(session_name),
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
                session_id: Some(session_id.clone()),
                session_file_path: session_file_str.clone(),
                name: Some(session_name.clone()),
            };
            guard.status.clone()
        };

        if !ephemeral
            && let Some(db) = app_handle
                .try_state::<Database>()
                .map(|s| s.inner().clone())
        {
            let (opt_provider, opt_model_id) = match options
                .model
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                Some(raw) => match raw.split_once('/') {
                    Some((p, m)) => (Some(p.to_string()), Some(m.to_string())),
                    None => (None, Some(raw.to_string())),
                },
                None => (None, None),
            };
            let meta = SaveSessionMetadataInput {
                id: session_id.clone(),
                name: options.name.clone().or(Some(session_name.clone())),
                status: Some("running".to_string()),
                mode: Some(mode.clone()),
                cwd: Some(cwd_display.clone()),
                session_file_path: session_file_str.clone(),
                model_id: opt_model_id,
                provider: opt_provider,
                ..Default::default()
            };
            let _ = tokio::task::spawn_blocking(move || db.save_session_metadata(&meta)).await;
        }

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
                let exited_sid = guard.status.session_id.clone();
                let next_state_for_db = next_state.clone();
                guard.status = PiSessionStatus {
                    state: next_state,
                    mode: guard.status.mode.clone(),
                    cwd: guard.status.cwd.clone(),
                    pid: None,
                    error: err_msg,
                    session_id: guard.status.session_id.clone(),
                    session_file_path: guard.status.session_file_path.clone(),
                    name: guard.status.name.clone(),
                };
                let status_snapshot = guard.status.clone();
                drop(guard);

                if let (Some(sid), Some(db)) = (
                    exited_sid,
                    app_stdout
                        .try_state::<Database>()
                        .map(|s| s.inner().clone()),
                ) {
                    let _ = tokio::task::spawn_blocking(move || {
                        db.set_session_status(&sid, &next_state_for_db)
                    })
                    .await;
                }

                let _ = app_stdout.emit("pi:status", &status_snapshot);
            }
        });

        Ok(running_status)
    }

    pub async fn send_command(&self, command: Value) -> Result<(), String> {
        let mut payload = serde_json::to_vec(&command)
            .map_err(|e| format!("Failed to serialize RPC command: {}", e))?;
        payload.push(b'\n');

        let mut guard = self.inner.lock().await;
        if command.get("type").and_then(Value::as_str) == Some("switch_session")
            && let Some(path) = command.get("sessionPath").and_then(Value::as_str)
        {
            guard.status.session_file_path = Some(path.to_string());
            if let Some(sid) = command.get("sessionId").and_then(Value::as_str) {
                guard.status.session_id = Some(sid.to_string());
            }
        } else if command.get("type").and_then(Value::as_str) == Some("set_session_name")
            && let Some(name) = command.get("name").and_then(Value::as_str)
        {
            guard.status.name = Some(name.to_string());
        }

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

pub(crate) fn trim_line_endings(buf: &[u8]) -> &[u8] {
    let mut end = buf.len();
    if end > 0 && buf[end - 1] == b'\n' {
        end -= 1;
    }
    if end > 0 && buf[end - 1] == b'\r' {
        end -= 1;
    }
    &buf[..end]
}

pub(crate) async fn terminate_process_tree(child: &mut Child, pid: Option<u32>) {
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

/// Builds the session persistence CLI arguments for `pi --mode rpc`.
///
/// * When `ephemeral` is `false` (default): omits `--no-session`, passes `--session <path>`
///   when `session_file` is provided, and passes `--name <label>` when `session_name` is non-empty.
/// * When `ephemeral` is `true`: passes `--no-session`.
pub(crate) fn build_pi_session_args(
    session_file: Option<&Path>,
    session_name: Option<&str>,
    ephemeral: bool,
    extra_args: &[OsString],
) -> Vec<OsString> {
    let mut args = Vec::new();
    if ephemeral {
        args.push(OsString::from("--no-session"));
    } else {
        if let Some(path) = session_file {
            args.push(OsString::from("--session"));
            args.push(path.as_os_str().to_os_string());
        }
        if let Some(label) = session_name.map(str::trim).filter(|s| !s.is_empty()) {
            args.push(OsString::from("--name"));
            args.push(OsString::from(label));
        }
    }
    args.extend_from_slice(extra_args);
    args
}

#[allow(dead_code)]
pub(crate) fn build_pi_command(working_dir: &Path) -> Command {
    build_pi_command_with_args(working_dir, &[])
}

/// Spawns `pi --mode rpc` in `working_dir` and appends `extra_args` (such as `--session <path>`,
/// `--name <label>`, `--extension <path>`, or `--no-session` when ephemeral).
pub(crate) fn build_pi_command_with_args(working_dir: &Path, extra_args: &[OsString]) -> Command {
    let resolved_path = get_augmented_path();

    #[cfg(target_os = "windows")]
    {
        // Check if a native `pi.exe` exists first; otherwise check `pi.cmd` / `pi` on augmented PATH
        // and invoke via cmd.exe so `.cmd` shims created by npm/pnpm work seamlessly without flashing a console window.
        let mut cmd = if let Some(pi_exe) = find_executable_in_path("pi.exe", &resolved_path) {
            let mut c = Command::new(pi_exe);
            c.args(["--mode", "rpc"]);
            c
        } else if let Some(pi_cmd) = find_executable_in_path("pi.cmd", &resolved_path) {
            let mut c = Command::new(pi_cmd);
            c.args(["--mode", "rpc"]);
            c
        } else {
            let mut c = Command::new("cmd.exe");
            c.args(["/d", "/c", "pi", "--mode", "rpc"]);
            c
        };
        if !resolved_path.is_empty() {
            cmd.env("PATH", &resolved_path);
        }
        cmd.args(extra_args);
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
        cmd.args(["--mode", "rpc"])
            .args(extra_args)
            .current_dir(working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        cmd
    }
}

pub(crate) fn find_executable_in_path(binary_name: &str, path_var: &OsString) -> Option<PathBuf> {
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

static AUGMENTED_PATH: RwLock<Option<OsString>> = RwLock::new(None);

pub(crate) fn get_augmented_path() -> OsString {
    if let Ok(guard) = AUGMENTED_PATH.read()
        && let Some(cached) = guard.as_ref()
    {
        return cached.clone();
    }
    refresh_augmented_path()
}

pub(crate) fn refresh_augmented_path() -> OsString {
    let computed = compute_augmented_path();
    if let Ok(mut guard) = AUGMENTED_PATH.write() {
        *guard = Some(computed.clone());
    }
    computed
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
#[allow(clippy::too_many_arguments)]
pub async fn start_pi_session(
    app_handle: AppHandle,
    manager: State<'_, PiRpcManager>,
    cwd: Option<String>,
    session_id: Option<String>,
    session_file_path: Option<String>,
    name: Option<String>,
    ephemeral: Option<bool>,
    model: Option<String>,
    thinking_level: Option<String>,
) -> Result<PiSessionStatus, String> {
    manager
        .start_session(
            &app_handle,
            StartPiOptions {
                cwd,
                session_id,
                session_file_path,
                name,
                ephemeral,
                model,
                thinking_level,
            },
        )
        .await
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

    #[test]
    fn test_build_pi_session_args_persistent_and_ephemeral() {
        let path = Path::new("/tmp/sessions/sess-1.jsonl");
        let persistent = build_pi_session_args(Some(path), Some("My Session"), false, &[]);
        let persistent_strs: Vec<String> = persistent
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert!(!persistent_strs.contains(&"--no-session".to_string()));
        assert_eq!(
            persistent_strs,
            vec![
                "--session".to_string(),
                path.to_string_lossy().to_string(),
                "--name".to_string(),
                "My Session".to_string()
            ]
        );

        let ephemeral = build_pi_session_args(Some(path), Some("Ignored"), true, &[]);
        let ephemeral_strs: Vec<String> = ephemeral
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(ephemeral_strs, vec!["--no-session".to_string()]);

        let with_model = build_pi_session_args(
            Some(path),
            Some("Model Session"),
            false,
            &[
                OsString::from("--model"),
                OsString::from("anthropic/claude-sonnet-4-20250514"),
            ],
        );
        let with_model_strs: Vec<String> = with_model
            .iter()
            .map(|a| a.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            with_model_strs,
            vec![
                "--session".to_string(),
                path.to_string_lossy().to_string(),
                "--name".to_string(),
                "Model Session".to_string(),
                "--model".to_string(),
                "anthropic/claude-sonnet-4-20250514".to_string(),
            ]
        );
    }
}
