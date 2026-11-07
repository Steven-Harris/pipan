pub mod agent;
pub mod approval;
pub mod db;
pub mod github;
pub mod pi_rpc;
pub mod webhook;
pub mod worktree;

use agent::{
    AgentManager, list_agent_sessions, list_pending_approvals, resolve_approval,
    send_agent_command, spawn_agent_session, stop_agent_session,
};

use db::{
    Database, clear_node_positions, get_graph_direction, load_node_positions, save_node_positions,
    set_graph_direction,
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
        .setup(|app| {
            let db = app
                .path()
                .app_local_data_dir()
                .map_err(|e| e.to_string())
                .and_then(|dir| {
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    Database::open(&dir.join("pipan.db"))
                })
                .unwrap_or_else(|err| {
                    // Keep the app usable; graph layout just won't survive a restart.
                    eprintln!("pipan: could not open pipan.db ({err}); using an in-memory store");
                    Database::open_in_memory().expect("in-memory sqlite")
                });
            app.manage(db);
            Ok(())
        })
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
            resolve_approval,
            list_pending_approvals,
            gh_status,
            gh_json,
            gh_pr_view,
            gh_pr_list,
            gh_pr_checks,
            gh_issue_list,
            gh_run_list,
            start_webhook_forwarder,
            stop_webhook_forwarder,
            get_webhook_forwarder,
            get_graph_direction,
            set_graph_direction,
            load_node_positions,
            save_node_positions,
            clear_node_positions
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
