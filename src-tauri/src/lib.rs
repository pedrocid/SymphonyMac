mod agent;
mod dock;
mod github;
mod logs;
mod notification;
mod orchestrator;
mod paths;
mod persistence;
mod report;
mod workspace;

use orchestrator::OrchestratorState;
use std::sync::Arc;

use tokio::sync::Mutex;

pub type SharedState = Arc<Mutex<OrchestratorState>>;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = Arc::new(Mutex::new(OrchestratorState::new()));

    tauri::Builder::default()
        .setup({
            let startup_state = state.clone();
            move |_app| {
                tauri::async_runtime::spawn(async move {
                    let ttl_days = {
                        let s = startup_state.lock().await;
                        s.config.workspace_ttl_days
                    };
                    if ttl_days > 0 {
                        let _ = workspace::cleanup_old_workspaces(ttl_days as f64).await;
                    }
                });
                Ok(())
            }
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            github::list_repos,
            github::list_issues,
            github::get_issue_detail,
            orchestrator::get_status,
            orchestrator::start_orchestrator,
            orchestrator::stop_orchestrator,
            orchestrator::update_config,
            orchestrator::get_agent_logs,
            agent::start_single_issue,
            agent::stop_agent,
            agent::retry_agent,
            agent::retry_agent_from_stage,
            agent::approve_stage,
            agent::reject_stage,
            agent::get_default_prompts,
            orchestrator::get_pipeline_report,
            orchestrator::search_agent_logs,
            orchestrator::export_logs_text,
            orchestrator::export_logs_json,
            orchestrator::list_log_history,
            orchestrator::get_interrupted_runs,
            orchestrator::resume_pipeline,
            workspace::list_workspaces,
            workspace::cleanup_old_workspaces,
            workspace::cleanup_single_workspace,
            workspace::cleanup_all_workspaces,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
