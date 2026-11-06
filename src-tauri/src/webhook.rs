//! `gh webhook forward` tunnel + local webhook receiver.
//!
//! Instead of polling GitHub, a loopback-only HTTP server (axum) receives the
//! events that `gh webhook forward` tunnels to this machine. Every verified
//! delivery is immediately broadcast to the frontend.
//!
//! Events emitted:
//! * `github:webhook`       – [`WebhookEvent`] for each verified delivery.
//! * `github:forwarder`     – [`ForwarderInfo`] whenever the forwarder changes state
//!   (`starting | running | exited | error | stopped`).
//! * `github:forwarder-log` – [`ForwarderLog`] for each stdout/stderr line of `gh`.

use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use hmac::{Hmac, Mac};
use serde::Serialize;
use serde_json::Value;
use sha2::Sha256;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State as TauriState};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Child;
use tokio::sync::{Mutex, oneshot, watch};
use tokio::task::JoinHandle;

use crate::github::{
    gh_command, install_webhook_extension, is_valid_repo_slug, webhook_extension_installed,
};

/// Events forwarded when the caller does not specify any.
pub const DEFAULT_EVENTS: &[&str] = &[
    "issue_comment",
    "pull_request",
    "pull_request_review",
    "pull_request_review_comment",
    "check_run",
    "check_suite",
    "workflow_run",
    "status",
];

/// GitHub allows payloads up to 25 MB.
const MAX_PAYLOAD_BYTES: usize = 26 * 1024 * 1024;
const START_TIMEOUT: Duration = Duration::from_secs(15);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwarderInfo {
    pub state: String,
    pub repo: String,
    pub events: Vec<String>,
    /// Loopback port the local receiver listens on.
    pub port: u16,
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForwarderLog {
    pub stream: String,
    pub line: String,
    pub timestamp: u64,
}

/// Normalised, frontend-friendly view of a delivery. The full GitHub payload is
/// still available in [`WebhookEvent::payload`].
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebhookSummary {
    /// `comment | review | ci | pull_request | issue | ping | other`
    pub category: String,
    pub action: Option<String>,
    pub number: Option<u64>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub sender: Option<String>,
    /// Review state, check conclusion/status, or commit status.
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookEvent {
    pub delivery_id: Option<String>,
    /// Value of the `X-GitHub-Event` header (e.g. `issue_comment`).
    pub event: String,
    pub repo: Option<String>,
    pub timestamp: u64,
    pub summary: WebhookSummary,
    pub payload: Value,
}

// ---------------------------------------------------------------------------
// Payload parsing
// ---------------------------------------------------------------------------

fn s(v: &Value, ptr: &str) -> Option<String> {
    v.pointer(ptr)
        .and_then(Value::as_str)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
}

fn n(v: &Value, ptr: &str) -> Option<u64> {
    v.pointer(ptr).and_then(Value::as_u64)
}

fn first_some<T>(items: impl IntoIterator<Item = Option<T>>) -> Option<T> {
    items.into_iter().flatten().next()
}

pub fn summarize(event: &str, p: &Value) -> WebhookSummary {
    let mut out = WebhookSummary {
        category: "other".into(),
        action: s(p, "/action"),
        sender: s(p, "/sender/login"),
        ..Default::default()
    };
    match event {
        "ping" => out.category = "ping".into(),
        "issue_comment" => {
            out.category = "comment".into();
            out.number = n(p, "/issue/number");
            out.title = s(p, "/issue/title");
            out.url = first_some([s(p, "/comment/html_url"), s(p, "/issue/html_url")]);
        }
        "pull_request_review_comment" => {
            out.category = "comment".into();
            out.number = n(p, "/pull_request/number");
            out.title = s(p, "/pull_request/title");
            out.url = first_some([s(p, "/comment/html_url"), s(p, "/pull_request/html_url")]);
        }
        "pull_request_review" => {
            out.category = "review".into();
            out.number = n(p, "/pull_request/number");
            out.title = s(p, "/pull_request/title");
            out.state = s(p, "/review/state");
            out.url = first_some([s(p, "/review/html_url"), s(p, "/pull_request/html_url")]);
        }
        "pull_request" => {
            out.category = "pull_request".into();
            out.number = n(p, "/pull_request/number");
            out.title = s(p, "/pull_request/title");
            out.state = s(p, "/pull_request/state");
            out.url = s(p, "/pull_request/html_url");
        }
        "issues" => {
            out.category = "issue".into();
            out.number = n(p, "/issue/number");
            out.title = s(p, "/issue/title");
            out.state = s(p, "/issue/state");
            out.url = s(p, "/issue/html_url");
        }
        "check_run" => {
            out.category = "ci".into();
            out.number = n(p, "/check_run/pull_requests/0/number");
            out.title = s(p, "/check_run/name");
            out.state = first_some([s(p, "/check_run/conclusion"), s(p, "/check_run/status")]);
            out.url = first_some([s(p, "/check_run/html_url"), s(p, "/check_run/details_url")]);
        }
        "check_suite" => {
            out.category = "ci".into();
            out.number = n(p, "/check_suite/pull_requests/0/number");
            out.title = s(p, "/check_suite/app/name");
            out.state = first_some([s(p, "/check_suite/conclusion"), s(p, "/check_suite/status")]);
            out.url = s(p, "/check_suite/url");
        }
        "workflow_run" => {
            out.category = "ci".into();
            out.number = n(p, "/workflow_run/pull_requests/0/number");
            out.title = first_some([s(p, "/workflow_run/name"), s(p, "/workflow/name")]);
            out.state = first_some([
                s(p, "/workflow_run/conclusion"),
                s(p, "/workflow_run/status"),
            ]);
            out.url = s(p, "/workflow_run/html_url");
        }
        "status" => {
            out.category = "ci".into();
            out.title = s(p, "/context");
            out.state = s(p, "/state");
            out.url = s(p, "/target_url");
        }
        _ => {}
    }
    out
}

/// Constant-time verification of GitHub's `X-Hub-Signature-256` header.
pub fn verify_signature(secret: &[u8], body: &[u8], header: &str) -> bool {
    let Some(sig_hex) = header.trim().strip_prefix("sha256=") else {
        return false;
    };
    let Ok(sig) = hex::decode(sig_hex) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&sig).is_ok()
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

fn new_secret() -> Result<String, String> {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).map_err(|e| format!("Failed to generate webhook secret: {}", e))?;
    Ok(hex::encode(buf))
}

fn validate_events(events: &[String]) -> Result<(), String> {
    if events.is_empty() {
        return Err("At least one webhook event is required".into());
    }
    for e in events {
        if e.is_empty() || !e.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("Invalid webhook event name: {}", e));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP receiver
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ReceiverState {
    app: AppHandle,
    secret: Arc<String>,
    repo: Arc<String>,
}

async fn receive_webhook(
    State(st): State<ReceiverState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Some(sig) = header_str(&headers, "x-hub-signature-256") else {
        return StatusCode::UNAUTHORIZED;
    };
    if !verify_signature(st.secret.as_bytes(), &body, &sig) {
        return StatusCode::UNAUTHORIZED;
    }
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return StatusCode::BAD_REQUEST;
    };
    let event = header_str(&headers, "x-github-event").unwrap_or_else(|| "unknown".into());
    let repo = s(&payload, "/repository/full_name").or_else(|| Some((*st.repo).clone()));
    let _ = st.app.emit(
        "github:webhook",
        WebhookEvent {
            delivery_id: header_str(&headers, "x-github-delivery"),
            summary: summarize(&event, &payload),
            event,
            repo,
            timestamp: now_ms(),
            payload,
        },
    );
    StatusCode::ACCEPTED
}

fn router(state: ReceiverState) -> Router {
    Router::new()
        .route("/webhook", post(receive_webhook))
        .layer(DefaultBodyLimit::max(MAX_PAYLOAD_BYTES))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Forwarder lifecycle
// ---------------------------------------------------------------------------

struct Active {
    id: u64,
    info: ForwarderInfo,
    stop_tx: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

#[derive(Default)]
struct Inner {
    active: Mutex<Option<Active>>,
    /// Serialises `start` calls so two forwarders cannot race.
    start_guard: Mutex<()>,
    next_id: AtomicU64,
}

#[derive(Clone, Default)]
pub struct WebhookManager {
    inner: Arc<Inner>,
}

type TailBuf = Arc<std::sync::Mutex<Vec<String>>>;

fn tail_text(tail: &TailBuf) -> String {
    tail.lock().map(|t| t.join("\n")).unwrap_or_default()
}

fn pump_lines<R>(
    reader: R,
    stream: &'static str,
    app: AppHandle,
    tail: TailBuf,
    state_tx: Arc<watch::Sender<&'static str>>,
    base: ForwarderInfo,
) -> JoinHandle<()>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim_end().to_string();
            if line.is_empty() {
                continue;
            }
            if let Ok(mut t) = tail.lock() {
                t.push(line.clone());
                if t.len() > 20 {
                    t.remove(0);
                }
            }
            let _ = app.emit(
                "github:forwarder-log",
                ForwarderLog {
                    stream: stream.into(),
                    line: line.clone(),
                    timestamp: now_ms(),
                },
            );
            if line.contains("Forwarding Webhook events") {
                let promoted = state_tx.send_if_modified(|st| {
                    if *st == "starting" {
                        *st = "running";
                        true
                    } else {
                        false
                    }
                });
                if promoted {
                    let mut info = base.clone();
                    info.state = "running".into();
                    let _ = app.emit("github:forwarder", &info);
                }
            }
        }
    })
}

impl WebhookManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn current(&self) -> Option<ForwarderInfo> {
        self.inner
            .active
            .lock()
            .await
            .as_ref()
            .map(|a| a.info.clone())
    }

    pub async fn start(
        &self,
        app: &AppHandle,
        repo: String,
        events: Vec<String>,
        install_extension: bool,
    ) -> Result<ForwarderInfo, String> {
        let _guard = self.inner.start_guard.lock().await;

        if self.inner.active.lock().await.is_some() {
            return Err("A webhook forwarder is already running; stop it first".into());
        }
        if !is_valid_repo_slug(&repo) {
            return Err(format!(
                "Invalid repository (expected owner/name): {}",
                repo
            ));
        }
        validate_events(&events)?;

        if !webhook_extension_installed().await {
            if install_extension {
                install_webhook_extension().await?;
            } else {
                return Err("The `gh webhook` extension is not installed. Run \
                     `gh extension install cli/gh-webhook` or retry with installExtension: true"
                    .into());
            }
        }

        // Local receiver on an ephemeral loopback port.
        let secret = new_secret()?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("Failed to bind local webhook server: {}", e))?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("Failed to read webhook server address: {}", e))?
            .port();
        let app_router = router(ReceiverState {
            app: app.clone(),
            secret: Arc::new(secret.clone()),
            repo: Arc::new(repo.clone()),
        });
        let (server_shutdown_tx, server_shutdown_rx) = oneshot::channel::<()>();
        let server_task = tokio::spawn(async move {
            let _ = axum::serve(listener, app_router)
                .with_graceful_shutdown(async move {
                    let _ = server_shutdown_rx.await;
                })
                .await;
        });

        // `gh webhook forward` -> http://127.0.0.1:<port>/webhook
        let args: Vec<String> = vec![
            "webhook".into(),
            "forward".into(),
            "--repo".into(),
            repo.clone(),
            "--events".into(),
            events.join(","),
            "--url".into(),
            format!("http://127.0.0.1:{}/webhook", port),
            "--secret".into(),
            secret,
        ];
        let mut child = match gh_command(&args, None).spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = server_shutdown_tx.send(());
                let _ = server_task.await;
                return Err(format!("Failed to spawn `gh webhook forward`: {}", e));
            }
        };

        let base = ForwarderInfo {
            state: "starting".into(),
            repo: repo.clone(),
            events: events.clone(),
            port,
            pid: child.id(),
            error: None,
        };
        let (state_tx, mut state_rx) = watch::channel::<&'static str>("starting");
        let state_tx = Arc::new(state_tx);
        let tail: TailBuf = Arc::default();

        let mut readers = Vec::new();
        if let Some(out) = child.stdout.take() {
            readers.push(pump_lines(
                out,
                "stdout",
                app.clone(),
                tail.clone(),
                state_tx.clone(),
                base.clone(),
            ));
        }
        if let Some(err) = child.stderr.take() {
            readers.push(pump_lines(
                err,
                "stderr",
                app.clone(),
                tail.clone(),
                state_tx.clone(),
                base.clone(),
            ));
        }

        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let task = tokio::spawn(supervise(
            app.clone(),
            self.inner.clone(),
            id,
            base.clone(),
            child,
            stop_rx,
            server_shutdown_tx,
            server_task,
            readers,
            state_tx.clone(),
            tail.clone(),
        ));

        *self.inner.active.lock().await = Some(Active {
            id,
            info: base.clone(),
            stop_tx,
            task,
        });
        let _ = app.emit("github:forwarder", &base);

        // Wait for `gh` to confirm it is forwarding, or to fail fast.
        let outcome = tokio::time::timeout(START_TIMEOUT, async {
            loop {
                let st = *state_rx.borrow_and_update();
                if st != "starting" {
                    return st;
                }
                if state_rx.changed().await.is_err() {
                    return "error";
                }
            }
        })
        .await;

        match outcome {
            Ok("running") => {
                let mut info = base;
                info.state = "running".into();
                if let Some(a) = self.inner.active.lock().await.as_mut()
                    && a.id == id
                {
                    a.info = info.clone();
                }
                Ok(info)
            }
            Ok(_) => {
                let detail = tail_text(&tail);
                Err(if detail.is_empty() {
                    "`gh webhook forward` exited immediately".to_string()
                } else {
                    format!("`gh webhook forward` failed: {}", detail)
                })
            }
            // Still starting (gh printed nothing recognisable); keep it running.
            Err(_) => Ok(base),
        }
    }

    pub async fn stop(&self) -> Option<ForwarderInfo> {
        let active = self.inner.active.lock().await.take()?;
        let mut info = active.info.clone();
        let _ = active.stop_tx.send(());
        let _ = active.task.await;
        info.state = "stopped".into();
        Some(info)
    }

    pub async fn shutdown(&self) {
        let _ = self.stop().await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn supervise(
    app: AppHandle,
    inner: Arc<Inner>,
    id: u64,
    base: ForwarderInfo,
    mut child: Child,
    mut stop_rx: oneshot::Receiver<()>,
    server_shutdown: oneshot::Sender<()>,
    server_task: JoinHandle<()>,
    readers: Vec<JoinHandle<()>>,
    state_tx: Arc<watch::Sender<&'static str>>,
    tail: TailBuf,
) {
    let (state, error, user_stopped): (&'static str, Option<String>, bool) = tokio::select! {
        _ = &mut stop_rx => {
            let _ = child.kill().await;
            ("stopped", None, true)
        }
        status = child.wait() => match status {
            Ok(st) if st.success() => ("exited", None, false),
            Ok(st) => {
                // Give the readers a moment to capture the final stderr lines.
                tokio::time::sleep(Duration::from_millis(100)).await;
                let detail = tail_text(&tail);
                let msg = if detail.is_empty() {
                    format!("`gh webhook forward` exited with {}", st)
                } else {
                    format!("`gh webhook forward` exited with {}: {}", st, detail)
                };
                ("error", Some(msg), false)
            }
            Err(e) => ("error", Some(format!("Failed to wait on `gh webhook forward`: {}", e)), false),
        }
    };

    let _ = server_shutdown.send(());
    let _ = server_task.await;
    for r in readers {
        let _ = tokio::time::timeout(Duration::from_secs(2), r).await;
    }
    state_tx.send_replace(state);

    // On a user stop, `stop()` already removed the entry.
    if !user_stopped {
        let mut guard = inner.active.lock().await;
        if guard.as_ref().is_some_and(|a| a.id == id) {
            *guard = None;
        }
    }

    let mut info = base;
    info.state = state.into();
    info.error = error;
    let _ = app.emit("github:forwarder", &info);
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn start_webhook_forwarder(
    app_handle: AppHandle,
    manager: TauriState<'_, WebhookManager>,
    repo: String,
    events: Option<Vec<String>>,
    install_extension: Option<bool>,
) -> Result<ForwarderInfo, String> {
    let events = events
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| DEFAULT_EVENTS.iter().map(|e| e.to_string()).collect());
    manager
        .start(
            &app_handle,
            repo,
            events,
            install_extension.unwrap_or(false),
        )
        .await
}

#[tauri::command]
pub async fn stop_webhook_forwarder(
    manager: TauriState<'_, WebhookManager>,
) -> Result<Option<ForwarderInfo>, String> {
    // The supervisor task emits the final `github:forwarder` (stopped) event.
    Ok(manager.stop().await)
}

#[tauri::command]
pub async fn get_webhook_forwarder(
    manager: TauriState<'_, WebhookManager>,
) -> Result<Option<ForwarderInfo>, String> {
    Ok(manager.current().await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn verifies_valid_signature() {
        let body = br#"{"a":1}"#;
        assert!(verify_signature(b"s3cret", body, &sign("s3cret", body)));
    }

    #[test]
    fn rejects_bad_signatures() {
        let body = br#"{"a":1}"#;
        assert!(!verify_signature(b"s3cret", body, &sign("other", body)));
        assert!(!verify_signature(
            b"s3cret",
            b"tampered",
            &sign("s3cret", body)
        ));
        assert!(!verify_signature(b"s3cret", body, "sha1=abcd"));
        assert!(!verify_signature(b"s3cret", body, "sha256=zz"));
        assert!(!verify_signature(b"s3cret", body, ""));
    }

    #[test]
    fn summarizes_issue_comment() {
        let p = json!({
            "action": "created",
            "issue": {"number": 7, "title": "Bug", "html_url": "https://x/issues/7"},
            "comment": {"html_url": "https://x/issues/7#c1"},
            "sender": {"login": "octo"}
        });
        let sum = summarize("issue_comment", &p);
        assert_eq!(sum.category, "comment");
        assert_eq!(sum.number, Some(7));
        assert_eq!(sum.url.as_deref(), Some("https://x/issues/7#c1"));
        assert_eq!(sum.action.as_deref(), Some("created"));
        assert_eq!(sum.sender.as_deref(), Some("octo"));
    }

    #[test]
    fn summarizes_review_and_ci() {
        let review = json!({
            "action": "submitted",
            "review": {"state": "approved", "html_url": "https://x/r"},
            "pull_request": {"number": 3, "title": "Add"}
        });
        let sum = summarize("pull_request_review", &review);
        assert_eq!(sum.category, "review");
        assert_eq!(sum.state.as_deref(), Some("approved"));
        assert_eq!(sum.number, Some(3));

        let run = json!({
            "action": "completed",
            "check_run": {
                "name": "build", "status": "completed", "conclusion": "failure",
                "html_url": "https://x/c", "pull_requests": [{"number": 9}]
            }
        });
        let sum = summarize("check_run", &run);
        assert_eq!(sum.category, "ci");
        assert_eq!(sum.state.as_deref(), Some("failure"));
        assert_eq!(sum.number, Some(9));
        assert_eq!(sum.title.as_deref(), Some("build"));

        assert_eq!(summarize("ping", &json!({})).category, "ping");
        assert_eq!(summarize("whatever", &json!({})).category, "other");
    }

    #[test]
    fn validates_events() {
        assert!(validate_events(&["issue_comment".into()]).is_ok());
        assert!(validate_events(&[]).is_err());
        assert!(validate_events(&["a,b".into()]).is_err());
        assert!(validate_events(&["--x".into()]).is_err());
    }

    #[test]
    fn secrets_are_unique_hex() {
        let a = new_secret().unwrap();
        let b = new_secret().unwrap();
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }
}
