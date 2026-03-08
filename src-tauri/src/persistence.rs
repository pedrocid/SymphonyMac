use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::orchestrator::{AgentRun, AgentStatus, PipelineStage};

/// Returns the persistence directory: ~/Library/Application Support/SymphonyMac/
fn persistence_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join("Library")
        .join("Application Support")
        .join("SymphonyMac")
}

/// Returns the path to the persisted state file.
fn state_file_path() -> PathBuf {
    let dir = persistence_dir();
    let _ = fs::create_dir_all(&dir);
    dir.join("orchestrator_state.json")
}

/// Subset of OrchestratorState that we persist to disk.
/// Excludes runtime-only fields like agent_pids and stop_flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    pub repo: Option<String>,
    pub runs: HashMap<String, AgentRun>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub total_runtime_secs: f64,
}

/// Save the current orchestrator state to disk.
/// Called on every status transition (stage change, completion, failure).
pub fn save_state(state: &crate::orchestrator::OrchestratorState) {
    let persisted = PersistedState {
        repo: state.repo.clone(),
        runs: state.runs.clone(),
        total_input_tokens: state.total_input_tokens,
        total_output_tokens: state.total_output_tokens,
        total_cost_usd: state.total_cost_usd,
        total_runtime_secs: state.total_runtime_secs,
    };

    let path = state_file_path();
    match serde_json::to_string_pretty(&persisted) {
        Ok(json) => {
            // Write to temp file first, then rename for atomicity
            let tmp_path = path.with_extension("json.tmp");
            if fs::write(&tmp_path, &json).is_ok() {
                let _ = fs::rename(&tmp_path, &path);
            }
        }
        Err(e) => {
            eprintln!("[persistence] Failed to serialize state: {}", e);
        }
    }
}

/// Load persisted state from disk.
/// Any runs that were Running or Preparing are marked as Interrupted.
pub fn load_state() -> Option<PersistedState> {
    let path = state_file_path();
    let data = fs::read_to_string(&path).ok()?;
    let mut persisted: PersistedState = serde_json::from_str(&data).ok()?;

    // Mark any in-progress runs as Interrupted since the app restarted
    let mut modified = false;
    for run in persisted.runs.values_mut() {
        if run.status == AgentStatus::Running || run.status == AgentStatus::Preparing {
            run.status = AgentStatus::Interrupted;
            run.error = Some("App was restarted while this run was in progress".to_string());
            if run.finished_at.is_none() {
                run.finished_at = Some(chrono::Utc::now().to_rfc3339());
            }
            modified = true;
        }
    }

    // Persist the updated state so Interrupted status is saved to disk
    if modified {
        let path = state_file_path();
        if let Ok(json) = serde_json::to_string_pretty(&persisted) {
            let tmp_path = path.with_extension("json.tmp");
            if fs::write(&tmp_path, &json).is_ok() {
                let _ = fs::rename(&tmp_path, &path);
            }
        }
    }

    Some(persisted)
}

/// Get a list of interrupted runs that can be resumed.
/// Returns (run_id, repo, issue_number, issue_title, stage) tuples.
pub fn get_interrupted_runs(
    state: &crate::orchestrator::OrchestratorState,
) -> Vec<InterruptedRunInfo> {
    state
        .runs
        .values()
        .filter(|r| r.status == AgentStatus::Interrupted)
        .map(|r| {
            let resume_stage = next_resumable_stage(r);
            InterruptedRunInfo {
                run_id: r.id.clone(),
                repo: r.repo.clone(),
                issue_number: r.issue_number,
                issue_title: r.issue_title.clone(),
                interrupted_stage: r.stage.clone(),
                resume_stage,
            }
        })
        .collect()
}

/// Determine which stage to resume from based on the interrupted run.
/// If the run was in-progress at a stage, we restart that same stage.
fn next_resumable_stage(run: &AgentRun) -> PipelineStage {
    // Resume from the same stage that was interrupted
    run.stage.clone()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterruptedRunInfo {
    pub run_id: String,
    pub repo: String,
    pub issue_number: u64,
    pub issue_title: String,
    pub interrupted_stage: PipelineStage,
    pub resume_stage: PipelineStage,
}
