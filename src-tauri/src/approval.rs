//! Interceptor and approval management for Pipan's "Plan Mode" (Phase 5).
//!
//! Destructive operations (file deletion, `git push`, hard resets, ...) are intercepted
//! before execution. When the Pi process requests an approval dialog via RPC
//! (`type: "extension_ui_request"` with `title: "PIPAN_APPROVAL::<json>"`), this module:
//! 1. Identifies the blocked action and registers it as an in-flight [`ApprovalRequest`].
//! 2. Emits an `agent:approval_request` event over Tauri IPC so the Svelte Flow dashboard
//!    renders an [`ApprovalNode`].
//! 3. Pauses the tool call in Pi by leaving the dialog request unresolved.
//! 4. Injects the user's decision (`approve` | `modify` | `reject`) back into Pi's stdin
//!    as an `extension_ui_response` once the user clicks a button in the UI.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, oneshot};

pub const APPROVAL_TITLE_PREFIX: &str = "PIPAN_APPROVAL::";

/// Bundled extension source. Written to app-local data so `pi -e <path>` can always load it.
pub const PLAN_GATE_EXTENSION_SOURCE: &str = include_str!("../resources/pipan-plan-gate.ts");

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Description of the blocked action intercepted by the Plan Mode gate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalAction {
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Classification, e.g. "delete", "git-push", "git-destructive".
    pub kind: String,
    /// Human-friendly reason, e.g. "Deletes files".
    pub reason: String,
    /// The full command / invocation blocked (e.g. `rm -rf src/`).
    pub action: String,
}

/// Payload emitted over `agent:approval_request`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequestPayload {
    pub request_id: String,
    pub session_id: String,
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    pub kind: String,
    pub reason: String,
    pub action: String,
    pub timestamp: u64,
}

/// Payload emitted over `agent:approval_resolved`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalResolvedPayload {
    pub request_id: String,
    pub session_id: String,
    pub resolution: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    pub timestamp: u64,
}

/// In-flight gate awaiting the user's decision in the frontend.
struct PendingApproval {
    session_id: String,
    action: ApprovalAction,
    tx: oneshot::Sender<ApprovalDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approve,
    Modify(String),
    Reject,
}

impl ApprovalDecision {
    /// Formats the string value expected by the `pipan-plan-gate.ts` extension.
    pub fn to_wire_value(&self) -> String {
        match self {
            ApprovalDecision::Approve => "approve".to_string(),
            ApprovalDecision::Reject => "reject".to_string(),
            ApprovalDecision::Modify(cmd) => format!("modify:{}", cmd),
        }
    }
}

/// Shared registry of pending approval requests across all agent sessions.
#[derive(Clone, Default)]
pub struct ApprovalRegistry {
    pending: Arc<Mutex<HashMap<String, PendingApproval>>>,
}

impl ApprovalRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a pending approval for the given dialog request. Returns a receiver that
    /// resolves when the user submits an approval or modification.
    pub async fn register(
        &self,
        request_id: String,
        session_id: String,
        action: ApprovalAction,
    ) -> oneshot::Receiver<ApprovalDecision> {
        let (tx, rx) = oneshot::channel();
        let mut guard = self.pending.lock().await;
        guard.insert(
            request_id,
            PendingApproval {
                session_id,
                action,
                tx,
            },
        );
        rx
    }

    /// Resolves a pending approval. Returns `(session_id, decision, action)` if found.
    pub async fn resolve(
        &self,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Option<(String, ApprovalDecision, ApprovalAction)> {
        let entry = {
            let mut guard = self.pending.lock().await;
            guard.remove(request_id)
        }?;
        let _ = entry.tx.send(decision.clone());
        Some((entry.session_id, decision, entry.action))
    }

    /// Cancels every pending approval for `session_id` (e.g. when stopped or killed).
    pub async fn cancel_session(&self, session_id: &str) {
        let mut guard = self.pending.lock().await;
        let mut drop_keys = Vec::new();
        for (k, v) in guard.iter() {
            if v.session_id == session_id {
                drop_keys.push(k.clone());
            }
        }
        for k in drop_keys {
            if let Some(entry) = guard.remove(&k) {
                let _ = entry.tx.send(ApprovalDecision::Reject);
            }
        }
    }

    /// Lists snapshots of all pending approvals.
    pub async fn list(&self) -> Vec<ApprovalRequestPayload> {
        let guard = self.pending.lock().await;
        guard
            .iter()
            .map(|(req_id, p)| ApprovalRequestPayload {
                request_id: req_id.clone(),
                session_id: p.session_id.clone(),
                tool_name: p.action.tool_name.clone(),
                tool_call_id: p.action.tool_call_id.clone(),
                kind: p.action.kind.clone(),
                reason: p.action.reason.clone(),
                action: p.action.action.clone(),
                timestamp: now_ms(),
            })
            .collect()
    }
}

/// Fallback heuristic: checks whether a raw bash command is destructive. Used for tests and
/// as a defence-in-depth classification if an un-gated session emits a tool call.
/// Implemented with pure string matching so no extra regex dependency is required.
pub fn is_destructive_command(command: &str) -> Option<(&'static str, &'static str)> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Split on pipeline / statement boundaries so we test the command heads individually.
    for segment in trimmed.split([';', '|', '&', '\n']) {
        let seg = segment.trim();
        if seg.is_empty() {
            continue;
        }

        // Tokenize words, stripping leading wrappers like sudo / xargs / nohup / exec / time
        let mut words: Vec<&str> = seg.split_whitespace().collect();
        while let Some(w) = words.first() {
            if matches!(*w, "sudo" | "xargs" | "nohup" | "exec" | "time" | "command")
                || (w.starts_with('-') && words.len() > 1)
            {
                words.remove(0);
            } else {
                break;
            }
        }
        if words.is_empty() {
            continue;
        }

        let cmd0 = words[0];
        // File deletions
        if matches!(
            cmd0,
            "rm" | "rmdir" | "unlink" | "shred" | "del" | "erase" | "rd"
        ) || cmd0.eq_ignore_ascii_case("Remove-Item")
            || cmd0.eq_ignore_ascii_case("ri")
        {
            return Some(("delete", "Deletes files or directories"));
        }

        if cmd0 == "git" {
            // Find first non-flag git argument
            let sub = words.iter().skip(1).find(|w| !w.starts_with('-')).copied();
            match sub {
                Some("rm") => return Some(("delete", "Deletes files or directories")),
                Some("push") => return Some(("git-push", "Pushes commits to a remote repository")),
                Some("reset") if words.iter().any(|w| *w == "--hard") => {
                    return Some((
                        "git-destructive",
                        "Discards uncommitted changes or git history",
                    ));
                }
                Some("clean") if words.iter().any(|w| w.starts_with('-') && (w.contains('f') || w.contains('d') || w.contains('x'))) => {
                    return Some((
                        "git-destructive",
                        "Discards uncommitted changes or git history",
                    ));
                }
                Some("branch") if words.iter().any(|w| *w == "-D" || *w == "-d") => {
                    return Some((
                        "git-destructive",
                        "Discards uncommitted changes or git history",
                    ));
                }
                Some("tag") if words.iter().any(|w| *w == "-d") => {
                    return Some((
                        "git-destructive",
                        "Discards uncommitted changes or git history",
                    ));
                }
                Some("stash") if words.iter().any(|w| *w == "drop" || *w == "clear") => {
                    return Some((
                        "git-destructive",
                        "Discards uncommitted changes or git history",
                    ));
                }
                Some("checkout") | Some("restore") if words.iter().any(|w| *w == ".") => {
                    return Some((
                        "git-destructive",
                        "Discards uncommitted changes or git history",
                    ));
                }
                _ => {}
            }
        }

        if cmd0 == "find" && words.iter().any(|w| *w == "-delete") {
            return Some(("delete", "Deletes files or directories"));
        }
    }

    None
}

/// Parses an `extension_ui_request` emitted by Pi's RPC mode. If the title starts with
/// [`APPROVAL_TITLE_PREFIX`], decodes the contained [`ApprovalAction`].
pub fn parse_approval_request(event: &Value) -> Option<(String, ApprovalAction)> {
    if event.get("type").and_then(Value::as_str) != Some("extension_ui_request") {
        return None;
    }
    let req_id = event.get("id").and_then(Value::as_str)?.to_string();
    let title = event.get("title").and_then(Value::as_str)?;
    let encoded = title.strip_prefix(APPROVAL_TITLE_PREFIX)?;
    let action: ApprovalAction = serde_json::from_str(encoded).ok()?;
    Some((req_id, action))
}

/// Builds an `extension_ui_response` RPC message to inject into Pi's stdin stream.
pub fn build_extension_ui_response(request_id: &str, decision: &ApprovalDecision) -> Value {
    match decision {
        ApprovalDecision::Reject => json!({
            "type": "extension_ui_response",
            "id": request_id,
            "value": "reject",
        }),
        ApprovalDecision::Approve => json!({
            "type": "extension_ui_response",
            "id": request_id,
            "value": "approve",
        }),
        ApprovalDecision::Modify(replacement) => json!({
            "type": "extension_ui_response",
            "id": request_id,
            "value": format!("modify:{}", replacement),
        }),
    }
}

/// Ensures the bundled `pipan-plan-gate.ts` extension exists in the app data directory
/// and returns its absolute path.
pub fn ensure_plan_gate_extension(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| format!("Failed to resolve app data directory: {}", e))?
        .join("extensions");
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create extension dir: {}", e))?;
    let path = dir.join("pipan-plan-gate.ts");
    // Write unconditionally to guarantee hot updates on newer Pipan builds.
    fs::write(&path, PLAN_GATE_EXTENSION_SOURCE)
        .map_err(|e| format!("Failed to write plan gate extension: {}", e))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_approval_request_from_extension_ui_json() {
        let raw = json!({
            "type": "extension_ui_request",
            "id": "gate-123",
            "method": "select",
            "title": format!("{}::{{\"toolName\":\"bash\",\"toolCallId\":\"call-1\",\"kind\":\"delete\",\"reason\":\"Deletes files\",\"action\":\"rm -rf dist/\"}}", "PIPAN_APPROVAL"),
            "options": ["approve", "modify", "reject"]
        });

        let (req_id, action) = parse_approval_request(&raw).expect("must parse");
        assert_eq!(req_id, "gate-123");
        assert_eq!(action.tool_name, "bash");
        assert_eq!(action.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(action.kind, "delete");
        assert_eq!(action.action, "rm -rf dist/");
    }

    #[test]
    fn ignores_non_matching_extension_ui_requests() {
        let raw = json!({
            "type": "extension_ui_request",
            "id": "ui-1",
            "method": "select",
            "title": "Which file do you want to edit?",
            "options": ["a", "b"]
        });
        assert!(parse_approval_request(&raw).is_none());
    }

    #[test]
    fn builds_correct_extension_ui_responses() {
        let approve = build_extension_ui_response("r1", &ApprovalDecision::Approve);
        assert_eq!(approve["type"], "extension_ui_response");
        assert_eq!(approve["id"], "r1");
        assert_eq!(approve["value"], "approve");

        let reject = build_extension_ui_response("r2", &ApprovalDecision::Reject);
        assert_eq!(reject["value"], "reject");

        let modify =
            build_extension_ui_response("r3", &ApprovalDecision::Modify("rm -i file".into()));
        assert_eq!(modify["value"], "modify:rm -i file");
    }

    #[test]
    fn classifies_destructive_commands() {
        assert!(is_destructive_command("rm -rf node_modules").is_some());
        assert!(is_destructive_command("git rm src/app.ts").is_some());
        assert!(is_destructive_command("git push origin main").is_some());
        assert!(is_destructive_command("git push --force").is_some());
        assert!(is_destructive_command("git reset --hard HEAD~1").is_some());
        assert!(is_destructive_command("git clean -fd").is_some());
        assert!(is_destructive_command("git branch -D feat").is_some());
        assert!(is_destructive_command("git stash drop").is_some());

        // Safe commands must not match
        assert!(is_destructive_command("git status").is_none());
        assert!(is_destructive_command("git log -n 5").is_none());
        assert!(is_destructive_command("pnpm build").is_none());
        assert!(is_destructive_command("cargo test").is_none());
        assert!(is_destructive_command("cat README.md").is_none());
    }

    #[tokio::test]
    async fn registry_registers_and_resolves() {
        let registry = ApprovalRegistry::new();
        let rx = registry
            .register(
                "req-1".into(),
                "session-1".into(),
                ApprovalAction {
                    tool_name: "bash".into(),
                    tool_call_id: None,
                    kind: "delete".into(),
                    reason: "Deletes files".into(),
                    action: "rm test.txt".into(),
                },
            )
            .await;

        assert_eq!(registry.list().await.len(), 1);

        let res = registry
            .resolve(
                "req-1",
                ApprovalDecision::Modify("rm -i test.txt".to_string()),
            )
            .await;
        assert!(res.is_some());
        let (sid, dec, act) = res.unwrap();
        assert_eq!(sid, "session-1");
        assert_eq!(dec, ApprovalDecision::Modify("rm -i test.txt".to_string()));
        assert_eq!(act.action, "rm test.txt");

        let decision = rx.await.expect("channel must deliver");
        assert_eq!(decision, ApprovalDecision::Modify("rm -i test.txt".to_string()));
        assert_eq!(registry.list().await.len(), 0);
    }
}
