mod agent;
mod github;
mod orchestrator;
mod paths;
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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
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
            orchestrator::get_pipeline_report,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
