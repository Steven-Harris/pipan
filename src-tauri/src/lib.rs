pub mod agent;
pub mod pi_rpc;
pub mod worktree;

use agent::{
    AgentManager, list_agent_sessions, send_agent_command, spawn_agent_session, stop_agent_session,
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
        .invoke_handler(tauri::generate_handler![
            start_pi_session,
            stop_pi_session,
            send_pi_command,
            get_pi_status,
            spawn_agent_session,
            send_agent_command,
            stop_agent_session,
            list_agent_sessions
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            let manager = app_handle.state::<PiRpcManager>().inner().clone();
            let agents = app_handle.state::<AgentManager>().inner().clone();
            tauri::async_runtime::block_on(async move {
                manager.shutdown().await;
                agents.shutdown_all().await;
            });
        }
    });
}
