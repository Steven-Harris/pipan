pub mod agent;
pub mod approval;
pub mod auth;
pub mod channel;
pub mod db;
pub mod doctor;
pub mod extensions;
pub mod files;
pub mod fs_explorer;
pub mod github;
pub mod models;
pub mod pi_rpc;
pub mod skills;
pub mod storage;
pub mod terminal;
pub mod usage;
#[cfg(test)]
mod verification;
pub mod webhook;
pub mod worktree;

use agent::{
    AgentManager, list_agent_sessions, list_pending_approvals, resolve_approval,
    send_agent_command, spawn_agent_session, stop_agent_session,
};
use auth::{
    delete_custom_model_endpoint, get_github_auth_details, get_pi_auth_and_settings,
    remove_pi_provider_auth, save_custom_model_endpoint, save_pi_provider_auth, save_pi_settings,
};
use db::{
    Database, clear_channel_node_positions, clear_node_positions, delete_github_view,
    get_graph_direction, list_github_views, load_channel_node_positions, load_node_positions,
    save_channel_node_positions, save_github_view, save_node_positions, set_graph_direction,
};
use doctor::{fix_doctor_issue, run_doctor};
use extensions::{
    install_extension_from_folder, install_extension_from_git, list_external_extensions,
    uninstall_extension,
};
use files::read_file_attachment;
use fs_explorer::{get_git_file_statuses, list_workspace_files, read_workspace_file};
use github::{
    gh_auth_login_token, gh_auth_login_web, gh_auth_logout, gh_issue_list, gh_json, gh_pr_checks,
    gh_pr_comment, gh_pr_detail, gh_pr_diff, gh_pr_list, gh_pr_merge, gh_pr_review_comments,
    gh_pr_submit_review, gh_pr_view, gh_run_list, gh_status,
};
use models::list_configured_models;
use skills::list_installed_skills;
use storage::{
    archive_session, delete_session, list_saved_sessions, rename_session, save_session_metadata,
};
use terminal::{
    TerminalManager, resize_terminal, send_terminal_input, start_terminal_session,
    stop_terminal_session,
};
use usage::{get_usage_analytics, record_usage_entry, rescan_usage_files};
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
                    let db = Database::open(&dir.join("pipan.db"))?;
                    if let Err(err) =
                        db.import_legacy_metadata_json(&dir.join("pipan-metadata.json"))
                    {
                        eprintln!("pipan: failed to import legacy pipan-metadata.json: {err}");
                    }
                    Ok(db)
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
        .manage(TerminalManager::new())
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
            gh_auth_login_web,
            gh_auth_login_token,
            gh_auth_logout,
            gh_json,
            gh_pr_view,
            gh_pr_list,
            gh_pr_detail,
            gh_pr_diff,
            gh_pr_review_comments,
            gh_pr_comment,
            gh_pr_submit_review,
            gh_pr_merge,
            gh_pr_checks,
            gh_issue_list,
            gh_run_list,
            start_webhook_forwarder,
            stop_webhook_forwarder,
            get_webhook_forwarder,
            get_graph_direction,
            set_graph_direction,
            load_node_positions,
            load_channel_node_positions,
            save_channel_node_positions,
            clear_channel_node_positions,
            save_node_positions,
            clear_node_positions,
            list_github_views,
            save_github_view,
            delete_github_view,
            list_saved_sessions,
            save_session_metadata,
            rename_session,
            archive_session,
            delete_session,
            list_configured_models,
            list_installed_skills,
            read_file_attachment,
            start_terminal_session,
            send_terminal_input,
            resize_terminal,
            stop_terminal_session,
            list_workspace_files,
            get_git_file_statuses,
            read_workspace_file,
            get_usage_analytics,
            rescan_usage_files,
            record_usage_entry,
            get_pi_auth_and_settings,
            save_pi_provider_auth,
            remove_pi_provider_auth,
            save_pi_settings,
            save_custom_model_endpoint,
            delete_custom_model_endpoint,
            get_github_auth_details,
            run_doctor,
            fix_doctor_issue,
            list_external_extensions,
            install_extension_from_folder,
            install_extension_from_git,
            uninstall_extension
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            let manager = app_handle.state::<PiRpcManager>().inner().clone();
            let agents = app_handle.state::<AgentManager>().inner().clone();
            let webhooks = app_handle.state::<WebhookManager>().inner().clone();
            let terminals = app_handle.state::<TerminalManager>().inner().clone();
            tauri::async_runtime::block_on(async move {
                manager.shutdown().await;
                agents.shutdown_all().await;
                webhooks.shutdown().await;
                terminals.shutdown_all().await;
            });
        }
    });
}
