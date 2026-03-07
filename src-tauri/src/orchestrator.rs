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
    Merge,
    Done,
}

impl std::fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineStage::Implement => write!(f, "implement"),
            PipelineStage::CodeReview => write!(f, "code_review"),
            PipelineStage::Testing => write!(f, "testing"),
            PipelineStage::Merge => write!(f, "merge"),
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
    pub max_retries: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<crate::report::PipelineReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub agent_type: String,
    pub auto_approve: bool,
    pub max_concurrent: usize,
    pub poll_interval_secs: u64,
    pub issue_label: Option<String>,
    pub max_turns: u32,
    pub notifications_enabled: bool,
    pub notification_sound: bool,
    pub max_retries: u32,
    pub retry_backoff_secs: u64,
    pub cleanup_on_failure: bool,
    pub cleanup_on_stop: bool,
    pub workspace_ttl_days: u32,
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
            notifications_enabled: true,
            notification_sound: true,
            max_retries: 1,
            retry_backoff_secs: 10,
            cleanup_on_failure: false,
            cleanup_on_stop: false,
            workspace_ttl_days: 7,
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
        self.runs
            .values()
            .filter(|r| r.issue_number == issue_number)
            .max_by_key(|r| r.started_at.clone())
    }
}

#[tauri::command]
pub async fn get_status(
    state: tauri::State<'_, SharedState>,
) -> Result<OrchestratorStatus, String> {
    let s = state.lock().await;
    let runs: Vec<AgentRun> = s.runs.values().cloned().collect();
    let total_completed = runs
        .iter()
        .filter(|r| r.stage == PipelineStage::Done)
        .count();
    let total_failed = runs
        .iter()
        .filter(|r| r.status == AgentStatus::Failed)
        .count();
    let active_count = runs
        .iter()
        .filter(|r| r.status == AgentStatus::Running || r.status == AgentStatus::Preparing)
        .count();

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

    let _ = app.emit(
        "orchestrator-status",
        serde_json::json!({
            "running": true,
            "repo": &repo,
        }),
    );

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

    let _ = app.emit(
        "orchestrator-status",
        serde_json::json!({
            "running": false,
        }),
    );

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
        // Fall back to disk logs for historical runs
        let lines = crate::logs::read_log_lines(&run_id);
        if lines.is_empty() {
            Err("Run not found".to_string())
        } else {
            Ok(lines)
        }
    }
}

#[tauri::command]
pub async fn search_agent_logs(
    run_id: String,
    query: String,
) -> Result<Vec<String>, String> {
    let results = crate::logs::search_logs(&run_id, &query);
    Ok(results)
}

#[tauri::command]
pub async fn export_logs_text(
    run_id: String,
) -> Result<String, String> {
    let text = crate::logs::export_as_text(&run_id);
    if text.is_empty() {
        Err("No logs found for this run".to_string())
    } else {
        Ok(text)
    }
}

#[tauri::command]
pub async fn export_logs_json(
    run_id: String,
) -> Result<String, String> {
    let json = crate::logs::export_as_json(&run_id);
    if json.is_empty() {
        Err("No logs found for this run".to_string())
    } else {
        Ok(json)
    }
}

#[tauri::command]
pub async fn list_log_history() -> Result<Vec<crate::logs::LogMeta>, String> {
    Ok(crate::logs::list_all_runs())
}

#[tauri::command]
pub async fn get_pipeline_report(
    state: tauri::State<'_, SharedState>,
    run_id: String,
) -> Result<Option<crate::report::PipelineReport>, String> {
    let s = state.lock().await;
    if let Some(run) = s.runs.get(&run_id) {
        Ok(run.report.clone())
    } else {
        Err("Run not found".to_string())
    }
}

async fn poll_loop(app: AppHandle, state: SharedState, repo: String) {
    let mut all_processed_notified = false;

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

        let _ = app.emit(
            "orchestrator-poll",
            serde_json::json!({
                "timestamp": Utc::now().to_rfc3339(),
            }),
        );

        // ---- STEP 1: Fetch issues and PRs in parallel ----
        let issues = match github::list_issues(
            repo.clone(),
            Some("open".to_string()),
            label.clone(),
        )
        .await
        {
            Ok(issues) => issues,
            Err(e) => {
                let _ = app.emit(
                    "orchestrator-error",
                    serde_json::json!({
                        "error": format!("Failed to fetch issues: {}", e),
                    }),
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval)).await;
                continue;
            }
        };

        let open_prs = github::list_open_prs(repo.clone())
            .await
            .unwrap_or_default();

        // Build a map: issue_number -> PR exists
        let mut issues_with_pr: std::collections::HashMap<u64, &github::PullRequest> =
            std::collections::HashMap::new();
        for pr in &open_prs {
            let issue_num = pr
                .closes_issue
                .or_else(|| github::parse_issue_from_title(&pr.title));
            if let Some(n) = issue_num {
                issues_with_pr.insert(n, pr);
            }
        }

        // ---- STEP 2: Determine available slots ----
        let active_count = {
            let s = state.lock().await;
            s.runs
                .values()
                .filter(|r| r.status == AgentStatus::Running || r.status == AgentStatus::Preparing)
                .count()
        };
        let available_slots = max_concurrent.saturating_sub(active_count);
        if available_slots == 0 {
            tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval)).await;
            continue;
        }

        // ---- STEP 3: Filter issues that need work ----
        let (already_working, fully_done, has_any_run) = {
            let s = state.lock().await;
            let working: Vec<u64> = s
                .runs
                .values()
                .filter(|r| r.status == AgentStatus::Running || r.status == AgentStatus::Preparing)
                .map(|r| r.issue_number)
                .collect();
            let done: Vec<u64> = s
                .runs
                .values()
                .filter(|r| r.stage == PipelineStage::Done && r.status == AgentStatus::Completed)
                .map(|r| r.issue_number)
                .collect();
            let any: Vec<u64> = s.runs.values().map(|r| r.issue_number).collect();
            (working, done, any)
        };

        // ---- Check if all issues are processed ----
        {
            let s = state.lock().await;
            let all_done = !issues.is_empty()
                && issues.iter().all(|issue| {
                    already_working.contains(&issue.number)
                        || fully_done.contains(&issue.number)
                        || has_any_run.contains(&issue.number)
                });
            let no_active = active_count == 0;
            if all_done && no_active && !all_processed_notified {
                if s.config.notifications_enabled {
                    crate::notification::notify_all_processed(&app, s.config.notification_sound);
                }
                all_processed_notified = true;
            } else if !all_done || !no_active {
                all_processed_notified = false;
            }
        }

        // ---- STEP 4: Dispatch issues ----
        let mut used_slots = 0usize;

        for issue in &issues {
            if used_slots >= available_slots {
                break;
            }

            // Skip already active, fully done, or already has a run
            if already_working.contains(&issue.number)
                || fully_done.contains(&issue.number)
                || has_any_run.contains(&issue.number)
            {
                continue;
            }

            // Decide which stage to start at
            let (stage, title, body) = if let Some(pr) = issues_with_pr.get(&issue.number) {
                // This issue already has a PR → start at Code Review
                (
                    PipelineStage::CodeReview,
                    pr.title.clone(),
                    pr.body.clone().unwrap_or_default(),
                )
            } else {
                // No PR → start at Implement
                (
                    PipelineStage::Implement,
                    issue.title.clone(),
                    issue.body.clone().unwrap_or_default(),
                )
            };

            if let Err(e) = crate::agent::launch_agent(
                app.clone(),
                state.clone(),
                repo.clone(),
                issue.number,
                title,
                body,
                stage.clone(),
            )
            .await
            {
                let _ = app.emit("orchestrator-error", serde_json::json!({
                    "error": format!("Failed to launch {} agent for #{}: {}", stage, issue.number, e),
                }));
            } else {
                used_slots += 1;
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval)).await;
    }

    let mut s = state.lock().await;
    s.is_running = false;
}
