//! Multi-session agent runtime: one isolated git worktree + one `pi --mode rpc`
//! child process per session. Pi's JSONL event stream is classified and routed to
//! the frontend over Tauri events.
//!
//! Events emitted:
//! * `agent:status` – [`AgentSessionInfo`] whenever a session changes state.
//! * `agent:event`  – [`AgentEventPayload`] for every JSONL line from Pi's stdout.
//!   `kind` is `thought`, `tool_call`, `task_complete`, or `event` (everything else).
//! * `agent:stderr` – [`AgentStderrPayload`] for stderr / unparsable stdout lines.

use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::Mutex;

use crate::approval::{
    ApprovalDecision, ApprovalRegistry, ApprovalRequestPayload, ApprovalResolvedPayload,
    build_extension_ui_response, ensure_plan_gate_extension, parse_approval_request,
};
use crate::db::Database;
use crate::pi_rpc::{
    build_pi_command_with_args, build_pi_session_args, terminate_process_tree, trim_line_endings,
};
use crate::storage::{SaveSessionMetadataInput, default_session_name, resolve_session_file_path};
use crate::worktree::{self, WorktreeInfo};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionInfo {
    pub session_id: String,
    /// "running" | "exited" | "error" | "stopped"
    pub state: String,
    pub worktree: WorktreeInfo,
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEventPayload {
    pub session_id: String,
    /// "thought" | "tool_call" | "task_complete" | "event"
    pub kind: String,
    pub timestamp: u64,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStderrPayload {
    pub session_id: String,
    pub line: String,
    pub timestamp: u64,
}

struct AgentChild {
    stdin: Option<ChildStdin>,
    child: Arc<Mutex<Child>>,
    pid: Option<u32>,
}

struct AgentEntry {
    info: AgentSessionInfo,
    active: Option<AgentChild>,
}

#[derive(Clone, Default)]
pub struct AgentManager {
    sessions: Arc<Mutex<HashMap<String, AgentEntry>>>,
    approvals: ApprovalRegistry,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Classifies one Pi JSONL event into a frontend-facing `(kind, data)` pair.
pub fn classify_event(event: &Value) -> (&'static str, Value) {
    let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);

    match event.get("type").and_then(Value::as_str) {
        Some("message_update") => {
            if let Some(ame) = event.get("assistantMessageEvent") {
                match ame.get("type").and_then(Value::as_str) {
                    Some("thinking_delta") => {
                        return (
                            "thought",
                            json!({
                                "phase": "delta",
                                "contentIndex": ame.get("contentIndex"),
                                "text": str_of(ame, "delta").unwrap_or_default(),
                            }),
                        );
                    }
                    Some("thinking_end") => {
                        return (
                            "thought",
                            json!({
                                "phase": "end",
                                "contentIndex": ame.get("contentIndex"),
                                "text": str_of(ame, "content").unwrap_or_default(),
                            }),
                        );
                    }
                    _ => {}
                }
            }
            ("event", event.clone())
        }
        Some(t @ ("tool_execution_start" | "tool_execution_update" | "tool_execution_end")) => {
            let phase = match t {
                "tool_execution_start" => "start",
                "tool_execution_update" => "update",
                _ => "end",
            };
            (
                "tool_call",
                json!({
                    "phase": phase,
                    "toolCallId": event.get("toolCallId"),
                    "toolName": event.get("toolName"),
                    "args": event.get("args"),
                    "partialResult": event.get("partialResult"),
                    "result": event.get("result"),
                    "isError": event.get("isError"),
                }),
            )
        }
        Some("agent_settled") => (
            "task_complete",
            json!({ "aborted": event.get("aborted").and_then(Value::as_bool).unwrap_or(false) }),
        ),
        _ => ("event", event.clone()),
    }
}

fn worktrees_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map(|p| p.join("worktrees"))
        .map_err(|e| format!("Failed to resolve app data directory: {}", e))
}

impl AgentManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn list(&self) -> Vec<AgentSessionInfo> {
        let guard = self.sessions.lock().await;
        let mut out: Vec<_> = guard.values().map(|e| e.info.clone()).collect();
        out.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        out
    }

    /// Creates a worktree for `repo_path`, spawns Pi inside it and starts routing events.
    pub async fn spawn(
        &self,
        app: &AppHandle,
        repo_path: &Path,
        base_ref: Option<&str>,
        prompt: Option<String>,
    ) -> Result<AgentSessionInfo, String> {
        let root = worktrees_root(app)?;
        let session_id = worktree::new_session_id();
        let wt = worktree::create_worktree(repo_path, &root, &session_id, base_ref).await?;

        let sessions_dir = app
            .path()
            .app_local_data_dir()
            .map(|d| d.join("sessions"))
            .unwrap_or_else(|_| std::env::temp_dir().join("pipan-sessions"));
        let _ = std::fs::create_dir_all(&sessions_dir);
        let session_file = resolve_session_file_path(&sessions_dir, &session_id);
        let session_label = default_session_name("worktree", Some(&wt.repo_root), Some(&wt.branch));

        // Plan Mode: load the bundled interceptor extension into this session so destructive
        // commands pause for user approval before execution.
        let mut ext_args: Vec<std::ffi::OsString> = Vec::new();
        if let Ok(ext_path) = ensure_plan_gate_extension(app) {
            ext_args.push("--extension".into());
            ext_args.push(ext_path.into_os_string());
        }
        let extra_args =
            build_pi_session_args(Some(&session_file), Some(&session_label), false, &ext_args);

        let mut cmd = build_pi_command_with_args(Path::new(&wt.path), &extra_args);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = worktree::remove_worktree(&root, Path::new(&wt.path)).await;
                return Err(format!("Failed to spawn `pi --mode rpc`: {}", e));
            }
        };

        let pid = child.id();
        let (stdin, stdout, stderr) =
            match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
                (Some(i), Some(o), Some(e)) => (i, o, e),
                _ => {
                    let _ = child.kill().await;
                    let _ = worktree::remove_worktree(&root, Path::new(&wt.path)).await;
                    return Err("Failed to capture Pi stdio pipes".to_string());
                }
            };

        let child_arc = Arc::new(Mutex::new(child));
        let info = AgentSessionInfo {
            session_id: session_id.clone(),
            state: "running".to_string(),
            worktree: wt,
            pid,
            error: None,
        };
        {
            let mut guard = self.sessions.lock().await;
            guard.insert(
                session_id.clone(),
                AgentEntry {
                    info: info.clone(),
                    active: Some(AgentChild {
                        stdin: Some(stdin),
                        child: child_arc.clone(),
                        pid,
                    }),
                },
            );
        }
        if let Some(db) = app.try_state::<Database>().map(|s| s.inner().clone()) {
            let meta = SaveSessionMetadataInput {
                id: session_id.clone(),
                name: Some(session_label),
                status: Some("running".to_string()),
                mode: Some("worktree".to_string()),
                cwd: Some(info.worktree.path.clone()),
                session_file_path: Some(session_file.to_string_lossy().to_string()),
                branch: Some(info.worktree.branch.clone()),
                worktree_path: Some(info.worktree.path.clone()),
                ..Default::default()
            };
            let _ = tokio::task::spawn_blocking(move || db.save_session_metadata(&meta)).await;
        }
        let _ = app.emit("agent:status", &info);

        // stderr drain (prevents pipe backpressure)
        {
            let app = app.clone();
            let sid = session_id.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut buf = Vec::with_capacity(1024);
                loop {
                    buf.clear();
                    match reader.read_until(b'\n', &mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let bytes = trim_line_endings(&buf);
                            if bytes.is_empty() {
                                continue;
                            }
                            let _ = app.emit(
                                "agent:stderr",
                                AgentStderrPayload {
                                    session_id: sid.clone(),
                                    line: String::from_utf8_lossy(bytes).to_string(),
                                    timestamp: now_ms(),
                                },
                            );
                        }
                    }
                }
            });
        }

        // stdout JSONL loop (split strictly on '\n')
        {
            let app = app.clone();
            let sid = session_id.clone();
            let sessions = self.sessions.clone();
            let approvals = self.approvals.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                let mut buf = Vec::with_capacity(8192);
                loop {
                    buf.clear();
                    match reader.read_until(b'\n', &mut buf).await {
                        Ok(0) => break,
                        Ok(_) => {
                            let bytes = trim_line_endings(&buf);
                            if bytes.is_empty() {
                                continue;
                            }
                            match serde_json::from_slice::<Value>(bytes) {
                                Ok(event) => {
                                    // Plan Mode gate: check if this is a dialog request asking
                                    // for approval of a destructive action.
                                    if let Some((req_id, action)) = parse_approval_request(&event) {
                                        let payload = ApprovalRequestPayload {
                                            request_id: req_id.clone(),
                                            session_id: sid.clone(),
                                            tool_name: action.tool_name.clone(),
                                            tool_call_id: action.tool_call_id.clone(),
                                            kind: action.kind.clone(),
                                            reason: action.reason.clone(),
                                            action: action.action.clone(),
                                            timestamp: now_ms(),
                                        };
                                        let _ = app.emit("agent:approval_request", &payload);

                                        // Register the gate in Rust. When the user approves/modifies/rejects,
                                        // write the extension_ui_response back to Pi's stdin.
                                        let rx = approvals
                                            .register(req_id.clone(), sid.clone(), action)
                                            .await;
                                        let app_c = app.clone();
                                        let sid_c = sid.clone();
                                        let sessions_c = sessions.clone();
                                        tokio::spawn(async move {
                                            if let Ok(decision) = rx.await {
                                                let resp =
                                                    build_extension_ui_response(&req_id, &decision);
                                                let mut payload = match serde_json::to_vec(&resp) {
                                                    Ok(p) => p,
                                                    Err(_) => return,
                                                };
                                                payload.push(b'\n');

                                                let mut guard = sessions_c.lock().await;
                                                if let Some(entry) = guard.get_mut(&sid_c)
                                                    && let Some(active) = entry.active.as_mut()
                                                    && let Some(stdin) = active.stdin.as_mut()
                                                {
                                                    let _ = stdin.write_all(&payload).await;
                                                    let _ = stdin.flush().await;
                                                }

                                                let (res_str, repl) = match decision {
                                                    ApprovalDecision::Approve => {
                                                        ("approved".to_string(), None)
                                                    }
                                                    ApprovalDecision::Reject => {
                                                        ("rejected".to_string(), None)
                                                    }
                                                    ApprovalDecision::Modify(m) => {
                                                        ("modified".to_string(), Some(m))
                                                    }
                                                };
                                                let _ = app_c.emit(
                                                    "agent:approval_resolved",
                                                    ApprovalResolvedPayload {
                                                        request_id: req_id,
                                                        session_id: sid_c,
                                                        resolution: res_str,
                                                        replacement: repl,
                                                        timestamp: now_ms(),
                                                    },
                                                );
                                            }
                                        });
                                        continue;
                                    }

                                    let (kind, data) = classify_event(&event);
                                    let _ = app.emit(
                                        "agent:event",
                                        AgentEventPayload {
                                            session_id: sid.clone(),
                                            kind: kind.to_string(),
                                            timestamp: now_ms(),
                                            data,
                                        },
                                    );
                                }
                                Err(err) => {
                                    let _ = app.emit(
                                        "agent:stderr",
                                        AgentStderrPayload {
                                            session_id: sid.clone(),
                                            line: format!(
                                                "[RPC JSONL parse warning: {}] {}",
                                                err,
                                                String::from_utf8_lossy(bytes)
                                            ),
                                            timestamp: now_ms(),
                                        },
                                    );
                                }
                            }
                        }
                        Err(err) => {
                            let _ = app.emit(
                                "agent:stderr",
                                AgentStderrPayload {
                                    session_id: sid.clone(),
                                    line: format!("[RPC stdout read error] {}", err),
                                    timestamp: now_ms(),
                                },
                            );
                            break;
                        }
                    }
                }

                let exit_status = child_arc.lock().await.wait().await.ok();

                // Only report if `stop` has not already taken ownership of the child.
                let mut guard = sessions.lock().await;
                if let Some(entry) = guard.get_mut(&sid)
                    && entry.active.is_some()
                {
                    entry.active = None;
                    entry.info.pid = None;
                    match exit_status {
                        Some(s) if !s.success() => {
                            entry.info.state = "error".to_string();
                            entry.info.error =
                                Some(format!("Pi process exited with status: {}", s));
                        }
                        _ => entry.info.state = "exited".to_string(),
                    }
                    let info_snapshot = entry.info.clone();
                    let next_state = info_snapshot.state.clone();
                    let sid_for_db = sid.clone();
                    drop(guard);

                    if let Some(db) = app.try_state::<Database>().map(|s| s.inner().clone()) {
                        let _ = tokio::task::spawn_blocking(move || {
                            db.set_session_status(&sid_for_db, &next_state)
                        })
                        .await;
                    }
                    let _ = app.emit("agent:status", &info_snapshot);
                }
            });
        }

        if let Some(text) = prompt
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
        {
            self.send_command(&session_id, json!({ "type": "prompt", "message": text }))
                .await?;
        }

        Ok(info)
    }

    pub async fn send_command(&self, session_id: &str, command: Value) -> Result<(), String> {
        let mut payload = serde_json::to_vec(&command)
            .map_err(|e| format!("Failed to serialize RPC command: {}", e))?;
        payload.push(b'\n');

        let mut guard = self.sessions.lock().await;
        let stdin = guard
            .get_mut(session_id)
            .ok_or_else(|| format!("Unknown agent session: {}", session_id))?
            .active
            .as_mut()
            .ok_or_else(|| "Agent session is not running".to_string())?
            .stdin
            .as_mut()
            .ok_or_else(|| "Agent stdin is closed".to_string())?;
        stdin
            .write_all(&payload)
            .await
            .map_err(|e| format!("Failed to write to agent stdin: {}", e))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush agent stdin: {}", e))
    }

    async fn terminate(active: AgentChild) {
        let mut active = active;
        drop(active.stdin.take()); // EOF → graceful shutdown per RPC spec
        let mut child = active.child.lock().await;
        if tokio::time::timeout(Duration::from_secs(2), child.wait())
            .await
            .is_err()
        {
            terminate_process_tree(&mut child, active.pid).await;
        }
    }

    /// Stops the Pi process. If `remove_worktree` is true the worktree is deleted and the
    /// session forgotten; the `pipan/<id>` branch is always preserved.
    pub async fn stop(
        &self,
        app: &AppHandle,
        session_id: &str,
        remove_worktree: bool,
    ) -> Result<AgentSessionInfo, String> {
        let active = {
            let mut guard = self.sessions.lock().await;
            let entry = guard
                .get_mut(session_id)
                .ok_or_else(|| format!("Unknown agent session: {}", session_id))?;
            entry.active.take()
        };
        if let Some(active) = active {
            Self::terminate(active).await;
        }
        self.approvals.cancel_session(session_id).await;

        let info = {
            let mut guard = self.sessions.lock().await;
            let entry = guard
                .get_mut(session_id)
                .ok_or_else(|| format!("Unknown agent session: {}", session_id))?;
            entry.info.state = "stopped".to_string();
            entry.info.pid = None;
            entry.info.clone()
        };

        if remove_worktree {
            let root = worktrees_root(app)?;
            worktree::remove_worktree(&root, Path::new(&info.worktree.path)).await?;
            self.sessions.lock().await.remove(session_id);
        }

        if let Some(db) = app.try_state::<Database>().map(|s| s.inner().clone()) {
            let sid = session_id.to_string();
            let _ =
                tokio::task::spawn_blocking(move || db.set_session_status(&sid, "stopped")).await;
        }

        let _ = app.emit("agent:status", &info);
        Ok(info)
    }

    /// Terminates every running agent (used on app exit). Worktrees are left in place.
    pub async fn shutdown_all(&self) {
        let actives: Vec<AgentChild> = {
            let mut guard = self.sessions.lock().await;
            guard.values_mut().filter_map(|e| e.active.take()).collect()
        };
        for active in actives {
            Self::terminate(active).await;
        }
    }

    pub fn approvals(&self) -> &ApprovalRegistry {
        &self.approvals
    }
}

#[tauri::command]
pub async fn resolve_approval(
    manager: State<'_, AgentManager>,
    request_id: String,
    resolution: String,
    replacement: Option<String>,
) -> Result<(), String> {
    let decision = match resolution.trim().to_lowercase().as_str() {
        "approve" | "approved" => ApprovalDecision::Approve,
        "reject" | "rejected" | "cancel" | "cancelled" => ApprovalDecision::Reject,
        "modify" | "modified" => {
            let repl = replacement
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty())
                .ok_or_else(|| {
                    "Replacement command required when modifying an action".to_string()
                })?;
            ApprovalDecision::Modify(repl)
        }
        other => return Err(format!("Unknown resolution: {}", other)),
    };
    manager
        .approvals()
        .resolve(&request_id, decision)
        .await
        .map(|_| ())
        .ok_or_else(|| {
            format!(
                "Approval request not found or already resolved: {}",
                request_id
            )
        })
}

#[tauri::command]
pub async fn list_pending_approvals(
    manager: State<'_, AgentManager>,
) -> Result<Vec<ApprovalRequestPayload>, String> {
    Ok(manager.approvals().list().await)
}

#[tauri::command]
pub async fn spawn_agent_session(
    app_handle: AppHandle,
    manager: State<'_, AgentManager>,
    repo_path: String,
    base_ref: Option<String>,
    prompt: Option<String>,
) -> Result<AgentSessionInfo, String> {
    manager
        .spawn(
            &app_handle,
            Path::new(repo_path.trim()),
            base_ref.as_deref(),
            prompt,
        )
        .await
}

#[tauri::command]
pub async fn send_agent_command(
    manager: State<'_, AgentManager>,
    session_id: String,
    command: Value,
) -> Result<(), String> {
    manager.send_command(&session_id, command).await
}

#[tauri::command]
pub async fn stop_agent_session(
    app_handle: AppHandle,
    manager: State<'_, AgentManager>,
    session_id: String,
    remove_worktree: Option<bool>,
) -> Result<AgentSessionInfo, String> {
    manager
        .stop(&app_handle, &session_id, remove_worktree.unwrap_or(false))
        .await
}

#[tauri::command]
pub async fn list_agent_sessions(
    manager: State<'_, AgentManager>,
) -> Result<Vec<AgentSessionInfo>, String> {
    Ok(manager.list().await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_thinking_delta_as_thought() {
        let ev = json!({"type":"message_update","assistantMessageEvent":{"type":"thinking_delta","contentIndex":0,"delta":"hmm"}});
        let (kind, data) = classify_event(&ev);
        assert_eq!(kind, "thought");
        assert_eq!(data["text"], "hmm");
        assert_eq!(data["phase"], "delta");
    }

    #[test]
    fn classifies_tool_execution() {
        let ev = json!({"type":"tool_execution_start","toolCallId":"c1","toolName":"bash","args":{"command":"ls"}});
        let (kind, data) = classify_event(&ev);
        assert_eq!(kind, "tool_call");
        assert_eq!(data["phase"], "start");
        assert_eq!(data["toolName"], "bash");

        let end = json!({"type":"tool_execution_end","toolCallId":"c1","toolName":"bash","isError":false});
        assert_eq!(classify_event(&end).1["phase"], "end");
    }

    #[test]
    fn classifies_agent_settled_as_task_complete() {
        let (kind, data) = classify_event(&json!({"type":"agent_settled","aborted":true}));
        assert_eq!(kind, "task_complete");
        assert_eq!(data["aborted"], true);
    }

    #[test]
    fn passes_through_other_events() {
        let text = json!({"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"x"}});
        assert_eq!(classify_event(&text).0, "event");
        assert_eq!(classify_event(&json!({"type":"response"})).0, "event");
    }
}
