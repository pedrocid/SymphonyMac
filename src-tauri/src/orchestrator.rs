use crate::github;
use crate::SharedState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Preparing,
    Running,
    Completed,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    Implement,
    CodeReview,
    Testing,
    Done,
}

impl std::fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineStage::Implement => write!(f, "implement"),
            PipelineStage::CodeReview => write!(f, "code_review"),
            PipelineStage::Testing => write!(f, "testing"),
            PipelineStage::Done => write!(f, "done"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub id: String,
    pub repo: String,
    pub issue_number: u64,
    pub issue_title: String,
    pub status: AgentStatus,
    pub stage: PipelineStage,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub logs: Vec<String>,
    pub workspace_path: String,
    pub error: Option<String>,
    pub attempt: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub agent_type: String,
    pub auto_approve: bool,
    pub max_concurrent: usize,
    pub poll_interval_secs: u64,
    pub issue_label: Option<String>,
    pub max_turns: u32,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            agent_type: "claude".to_string(),
            auto_approve: true,
            max_concurrent: 3,
            poll_interval_secs: 60,
            issue_label: None,
            max_turns: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorStatus {
    pub is_running: bool,
    pub repo: Option<String>,
    pub runs: Vec<AgentRun>,
    pub config: RunConfig,
    pub total_completed: usize,
    pub total_failed: usize,
    pub active_count: usize,
}

pub struct OrchestratorState {
    pub is_running: bool,
    pub repo: Option<String>,
    pub runs: HashMap<String, AgentRun>,
    pub config: RunConfig,
    pub agent_pids: HashMap<String, u32>,
    pub stop_flag: bool,
}

impl OrchestratorState {
    pub fn new() -> Self {
        Self {
            is_running: false,
            repo: None,
            runs: HashMap::new(),
            config: RunConfig::default(),
            agent_pids: HashMap::new(),
            stop_flag: false,
        }
    }

    /// Get the latest run for a given issue number
    pub fn latest_run_for_issue(&self, issue_number: u64) -> Option<&AgentRun> {
        self.runs.values()
            .filter(|r| r.issue_number == issue_number)
            .max_by_key(|r| r.started_at.clone())
    }
}

#[tauri::command]
pub async fn get_status(state: tauri::State<'_, SharedState>) -> Result<OrchestratorStatus, String> {
    let s = state.lock().await;
    let runs: Vec<AgentRun> = s.runs.values().cloned().collect();
    let total_completed = runs.iter().filter(|r| r.stage == PipelineStage::Done).count();
    let total_failed = runs.iter().filter(|r| r.status == AgentStatus::Failed).count();
    let active_count = runs.iter().filter(|r| r.status == AgentStatus::Running || r.status == AgentStatus::Preparing).count();

    Ok(OrchestratorStatus {
        is_running: s.is_running,
        repo: s.repo.clone(),
        runs,
        config: s.config.clone(),
        total_completed,
        total_failed,
        active_count,
    })
}

#[tauri::command]
pub async fn update_config(
    state: tauri::State<'_, SharedState>,
    config: RunConfig,
) -> Result<(), String> {
    let mut s = state.lock().await;
    s.config = config;
    Ok(())
}

#[tauri::command]
pub async fn start_orchestrator(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    repo: String,
) -> Result<(), String> {
    {
        let mut s = state.lock().await;
        if s.is_running {
            return Err("Orchestrator already running".to_string());
        }
        s.is_running = true;
        s.repo = Some(repo.clone());
        s.stop_flag = false;
    }

    let _ = app.emit("orchestrator-status", serde_json::json!({
        "running": true,
        "repo": &repo,
    }));

    let state_clone = state.inner().clone();
    let app_clone = app.clone();

    tokio::spawn(async move {
        poll_loop(app_clone, state_clone, repo).await;
    });

    Ok(())
}

#[tauri::command]
pub async fn stop_orchestrator(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
) -> Result<(), String> {
    let mut s = state.lock().await;
    s.stop_flag = true;
    s.is_running = false;
    drop(s);

    let _ = app.emit("orchestrator-status", serde_json::json!({
        "running": false,
    }));

    Ok(())
}

#[tauri::command]
pub async fn get_agent_logs(
    state: tauri::State<'_, SharedState>,
    run_id: String,
) -> Result<Vec<String>, String> {
    let s = state.lock().await;
    if let Some(run) = s.runs.get(&run_id) {
        Ok(run.logs.clone())
    } else {
        Err("Run not found".to_string())
    }
}

async fn poll_loop(app: AppHandle, state: SharedState, repo: String) {
    loop {
        let (should_stop, poll_interval, max_concurrent, label) = {
            let s = state.lock().await;
            (
                s.stop_flag,
                s.config.poll_interval_secs,
                s.config.max_concurrent,
                s.config.issue_label.clone(),
            )
        };

        if should_stop {
            break;
        }

        let _ = app.emit("orchestrator-poll", serde_json::json!({
            "timestamp": Utc::now().to_rfc3339(),
        }));

        // Fetch open issues
        let issues = match github::list_issues(repo.clone(), Some("open".to_string()), label.clone()).await {
            Ok(issues) => issues,
            Err(e) => {
                let _ = app.emit("orchestrator-error", serde_json::json!({
                    "error": format!("Failed to fetch issues: {}", e),
                }));
                tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval)).await;
                continue;
            }
        };

        // Count active agents
        let active_count = {
            let s = state.lock().await;
            s.runs.values()
                .filter(|r| r.status == AgentStatus::Running || r.status == AgentStatus::Preparing)
                .count()
        };

        let available_slots = max_concurrent.saturating_sub(active_count);

        // Find issues not already being worked on (no active run for this issue)
        let already_working: Vec<u64> = {
            let s = state.lock().await;
            s.runs.values()
                .filter(|r| r.status == AgentStatus::Running || r.status == AgentStatus::Preparing)
                .map(|r| r.issue_number)
                .collect()
        };

        // Also skip issues that have completed the full pipeline
        let fully_done: Vec<u64> = {
            let s = state.lock().await;
            s.runs.values()
                .filter(|r| r.stage == PipelineStage::Done && r.status == AgentStatus::Completed)
                .map(|r| r.issue_number)
                .collect()
        };

        let candidates: Vec<&github::Issue> = issues.iter()
            .filter(|i| !already_working.contains(&i.number) && !fully_done.contains(&i.number))
            .collect();

        // Also skip issues that already have a completed implement stage (they'll be auto-advanced)
        let has_any_run: Vec<u64> = {
            let s = state.lock().await;
            s.runs.values()
                .map(|r| r.issue_number)
                .collect()
        };

        let new_candidates: Vec<&github::Issue> = candidates.into_iter()
            .filter(|i| !has_any_run.contains(&i.number))
            .collect();

        // Dispatch up to available_slots
        for issue in new_candidates.into_iter().take(available_slots) {
            let app_c = app.clone();
            let state_c = state.clone();
            let repo_c = repo.clone();
            let issue_title = issue.title.clone();
            let issue_body = issue.body.clone().unwrap_or_default();
            let issue_number = issue.number;

            if let Err(e) = crate::agent::launch_agent(
                app_c,
                state_c,
                repo_c,
                issue_number,
                issue_title,
                issue_body,
                PipelineStage::Implement,
            ).await {
                let _ = app.emit("orchestrator-error", serde_json::json!({
                    "error": format!("Failed to launch agent for #{}: {}", issue_number, e),
                }));
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval)).await;
    }

    let mut s = state.lock().await;
    s.is_running = false;
}
