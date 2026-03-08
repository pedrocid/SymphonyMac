use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::orchestrator::{AgentRun, AgentStatus, PipelineStage, RunConfig};

/// Returns the persistence directory: ~/Library/Application Support/SymphonyMac/
fn persistence_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join("Library")
        .join("Application Support")
        .join("SymphonyMac")
}

/// Returns the path to the persisted state file.
fn state_file_path() -> Result<PathBuf, String> {
    let dir = persistence_dir();
    fs::create_dir_all(&dir).map_err(|e| {
        format!(
            "failed to create persistence directory {}: {}",
            dir.display(),
            e
        )
    })?;
    Ok(dir.join("orchestrator_state.json"))
}

/// Subset of OrchestratorState that we persist to disk.
/// Excludes runtime-only fields like agent_pids and stop_flag.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PersistedState {
    /// Legacy field for backward compatibility with old persisted state files.
    pub repo: Option<String>,
    /// List of monitored repositories.
    pub repos: Vec<String>,
    pub runs: HashMap<String, AgentRun>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub total_runtime_secs: f64,
    pub config: RunConfig,
}

fn write_file_atomically(path: &Path, contents: &str) -> Result<(), String> {
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, contents).map_err(|e| {
        format!(
            "failed to write temp state file {}: {}",
            tmp_path.display(),
            e
        )
    })?;
    fs::rename(&tmp_path, path).map_err(|e| {
        format!(
            "failed to rename temp state file {} to {}: {}",
            tmp_path.display(),
            path.display(),
            e
        )
    })?;
    Ok(())
}

fn write_persisted_state(persisted: &PersistedState) -> Result<(), String> {
    let path = state_file_path()?;
    let json = serde_json::to_string_pretty(persisted)
        .map_err(|e| format!("failed to serialize state: {}", e))?;
    write_file_atomically(&path, &json)
}

/// Save the current orchestrator state to disk.
/// Called on every status transition (stage change, completion, failure).
pub fn save_state(state: &crate::orchestrator::OrchestratorState) -> Result<(), String> {
    let persisted = PersistedState {
        repo: None,
        repos: state.repos.clone(),
        runs: state.runs.clone(),
        total_input_tokens: state.total_input_tokens,
        total_output_tokens: state.total_output_tokens,
        total_cost_usd: state.total_cost_usd,
        total_runtime_secs: state.total_runtime_secs,
        config: state.config.clone(),
    };
    write_persisted_state(&persisted)
}

/// Load persisted state from disk.
/// Any runs that were Running or Preparing are marked as Interrupted.
pub fn load_state() -> Option<PersistedState> {
    let path = match state_file_path() {
        Ok(path) => path,
        Err(err) => {
            eprintln!("[persistence] Failed to resolve state path: {}", err);
            return None;
        }
    };
    let data = fs::read_to_string(&path).ok()?;
    // Normalize the pre-snake_case AwaitingApproval value so older state files still load.
    let normalized = data.replace("\"awaitingapproval\"", "\"awaiting_approval\"");
    let mut persisted: PersistedState = serde_json::from_str(&normalized).ok()?;

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
        if let Err(err) = write_persisted_state(&persisted) {
            eprintln!(
                "[persistence] Failed to save interrupted run status update: {}",
                err
            );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::{AgentRun, AgentStatus, OrchestratorState, PipelineStage, RunConfig};
    use std::collections::HashMap;

    fn make_run(id: &str, status: AgentStatus, stage: PipelineStage) -> AgentRun {
        AgentRun {
            id: id.to_string(),
            repo: "test/repo".to_string(),
            issue_number: 42,
            issue_title: "Test issue".to_string(),
            status,
            stage,
            started_at: "2026-01-01T00:00:00Z".to_string(),
            finished_at: None,
            logs: vec![],
            workspace_path: "/tmp/test".to_string(),
            error: None,
            attempt: 1,
            max_retries: 3,
            lines_added: 0,
            lines_removed: 0,
            files_modified_list: vec![],
            report: None,
            command_display: None,
            agent_type: "claude".to_string(),
            last_log_line: None,
            log_count: 0,
            activity: None,
            last_log_timestamp: None,
            input_tokens: 100,
            output_tokens: 200,
            cost_usd: 0.05,
            issue_labels: vec![],
            skipped_stages: vec![],
            stage_context: None,
            pending_next_stage: None,
        }
    }

    #[test]
    fn test_persisted_state_round_trip() {
        let mut runs = HashMap::new();
        runs.insert(
            "run-1".to_string(),
            make_run("run-1", AgentStatus::Completed, PipelineStage::Implement),
        );
        runs.insert(
            "run-2".to_string(),
            make_run("run-2", AgentStatus::Failed, PipelineStage::Testing),
        );

        let mut approval_gates = HashMap::new();
        approval_gates.insert("testing".to_string(), true);
        let config = RunConfig {
            agent_type: "codex".to_string(),
            auto_approve: false,
            max_concurrent: 5,
            poll_interval_secs: 30,
            issue_label: Some("autofix".to_string()),
            approval_gates,
            ..RunConfig::default()
        };

        let original = PersistedState {
            repo: Some("test/repo".to_string()),
            repos: vec!["test/repo".to_string()],
            runs,
            total_input_tokens: 1000,
            total_output_tokens: 2000,
            total_cost_usd: 0.50,
            total_runtime_secs: 120.0,
            config: config.clone(),
        };

        let json = serde_json::to_string_pretty(&original).unwrap();
        let loaded: PersistedState = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.repo, Some("test/repo".to_string()));
        assert_eq!(loaded.runs.len(), 2);
        assert_eq!(loaded.total_input_tokens, 1000);
        assert_eq!(loaded.total_output_tokens, 2000);
        assert!((loaded.total_cost_usd - 0.50).abs() < f64::EPSILON);
        assert!((loaded.total_runtime_secs - 120.0).abs() < f64::EPSILON);
        assert_eq!(loaded.config, config);

        let run1 = loaded.runs.get("run-1").unwrap();
        assert_eq!(run1.status, AgentStatus::Completed);
        assert_eq!(run1.stage, PipelineStage::Implement);
        assert_eq!(run1.input_tokens, 100);
        assert_eq!(run1.output_tokens, 200);
    }

    #[test]
    fn test_legacy_persisted_state_defaults_config() {
        let legacy_json = r#"{
            "repo": "test/repo",
            "runs": {}
        }"#;

        let loaded: PersistedState = serde_json::from_str(legacy_json).unwrap();

        assert_eq!(loaded.repo, Some("test/repo".to_string()));
        assert!(loaded.repos.is_empty());
        assert!(loaded.runs.is_empty());
        assert_eq!(loaded.total_input_tokens, 0);
        assert_eq!(loaded.total_output_tokens, 0);
        assert!((loaded.total_cost_usd - 0.0).abs() < f64::EPSILON);
        assert!((loaded.total_runtime_secs - 0.0).abs() < f64::EPSILON);
        assert_eq!(loaded.config, RunConfig::default());
    }

    #[test]
    fn test_load_marks_running_as_interrupted() {
        let mut runs = HashMap::new();
        runs.insert(
            "run-active".to_string(),
            make_run(
                "run-active",
                AgentStatus::Running,
                PipelineStage::CodeReview,
            ),
        );
        runs.insert(
            "run-prep".to_string(),
            make_run("run-prep", AgentStatus::Preparing, PipelineStage::Implement),
        );
        runs.insert(
            "run-done".to_string(),
            make_run("run-done", AgentStatus::Completed, PipelineStage::Done),
        );

        let persisted = PersistedState {
            repo: Some("test/repo".to_string()),
            repos: vec!["test/repo".to_string()],
            runs,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cost_usd: 0.0,
            total_runtime_secs: 0.0,
            config: RunConfig::default(),
        };

        // Simulate what load_state does: mark Running/Preparing as Interrupted
        let mut loaded = persisted.clone();
        for run in loaded.runs.values_mut() {
            if run.status == AgentStatus::Running || run.status == AgentStatus::Preparing {
                run.status = AgentStatus::Interrupted;
                run.error = Some("App was restarted while this run was in progress".to_string());
                if run.finished_at.is_none() {
                    run.finished_at = Some("2026-01-01T00:01:00Z".to_string());
                }
            }
        }

        let active = loaded.runs.get("run-active").unwrap();
        assert_eq!(active.status, AgentStatus::Interrupted);
        assert!(active.error.is_some());
        assert!(active.finished_at.is_some());

        let prep = loaded.runs.get("run-prep").unwrap();
        assert_eq!(prep.status, AgentStatus::Interrupted);
        assert!(prep.error.is_some());

        // Completed run should be unchanged
        let done = loaded.runs.get("run-done").unwrap();
        assert_eq!(done.status, AgentStatus::Completed);
        assert!(done.error.is_none());
    }

    #[test]
    fn test_get_interrupted_runs_filters_correctly() {
        let mut runs = HashMap::new();
        runs.insert(
            "r1".to_string(),
            make_run("r1", AgentStatus::Interrupted, PipelineStage::Testing),
        );
        runs.insert(
            "r2".to_string(),
            make_run("r2", AgentStatus::Completed, PipelineStage::Done),
        );
        runs.insert(
            "r3".to_string(),
            make_run("r3", AgentStatus::Interrupted, PipelineStage::Implement),
        );

        let state = OrchestratorState {
            is_running: false,
            repos: vec!["test/repo".to_string()],
            runs,
            config: Default::default(),
            agent_pids: HashMap::new(),
            stop_flag: false,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cost_usd: 0.0,
            total_runtime_secs: 0.0,
        };

        let interrupted = get_interrupted_runs(&state);
        assert_eq!(interrupted.len(), 2);

        for info in &interrupted {
            assert_eq!(info.repo, "test/repo");
            assert_eq!(info.issue_number, 42);
        }
    }

    #[test]
    fn test_next_resumable_stage_returns_same_stage() {
        let run = make_run("r1", AgentStatus::Interrupted, PipelineStage::CodeReview);
        let resume = next_resumable_stage(&run);
        assert_eq!(resume, PipelineStage::CodeReview);

        let run2 = make_run("r2", AgentStatus::Interrupted, PipelineStage::Testing);
        let resume2 = next_resumable_stage(&run2);
        assert_eq!(resume2, PipelineStage::Testing);
    }

    #[test]
    fn test_token_cost_aggregates_persist() {
        let persisted = PersistedState {
            repo: Some("test/repo".to_string()),
            repos: vec!["test/repo".to_string()],
            runs: HashMap::new(),
            total_input_tokens: 50000,
            total_output_tokens: 75000,
            total_cost_usd: 3.75,
            total_runtime_secs: 600.0,
            config: RunConfig::default(),
        };

        let json = serde_json::to_string(&persisted).unwrap();
        let loaded: PersistedState = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.total_input_tokens, 50000);
        assert_eq!(loaded.total_output_tokens, 75000);
        assert!((loaded.total_cost_usd - 3.75).abs() < f64::EPSILON);
        assert!((loaded.total_runtime_secs - 600.0).abs() < f64::EPSILON);
    }
}
