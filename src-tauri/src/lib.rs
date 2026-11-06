pub mod pi_rpc;

use pi_rpc::{PiRpcManager, get_pi_status, send_pi_command, start_pi_session, stop_pi_session};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(PiRpcManager::new())
        .invoke_handler(tauri::generate_handler![
            start_pi_session,
            stop_pi_session,
            send_pi_command,
            get_pi_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
