pub mod agent;
pub mod github;
pub mod pi_rpc;
pub mod webhook;
pub mod worktree;

use agent::{
    AgentManager, list_agent_sessions, send_agent_command, spawn_agent_session, stop_agent_session,
};

use github::{
    gh_issue_list, gh_json, gh_pr_checks, gh_pr_list, gh_pr_view, gh_run_list, gh_status,
};
use webhook::{
    WebhookManager, get_webhook_forwarder, start_webhook_forwarder, stop_webhook_forwarder,
};

use pi_rpc::{PiRpcManager, get_pi_status, send_pi_command, start_pi_session, stop_pi_session};
use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(PiRpcManager::new())
        .manage(AgentManager::new())
        .manage(WebhookManager::new())
        .invoke_handler(tauri::generate_handler![
            start_pi_session,
            stop_pi_session,
            send_pi_command,
            get_pi_status,
            spawn_agent_session,
            send_agent_command,
            stop_agent_session,
            list_agent_sessions,
            gh_status,
            gh_json,
            gh_pr_view,
            gh_pr_list,
            gh_pr_checks,
            gh_issue_list,
            gh_run_list,
            start_webhook_forwarder,
            stop_webhook_forwarder,
            get_webhook_forwarder
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            let manager = app_handle.state::<PiRpcManager>().inner().clone();
            let agents = app_handle.state::<AgentManager>().inner().clone();
            let webhooks = app_handle.state::<WebhookManager>().inner().clone();
            tauri::async_runtime::block_on(async move {
                manager.shutdown().await;
                agents.shutdown_all().await;
                webhooks.shutdown().await;
            });
        }
    });
}
