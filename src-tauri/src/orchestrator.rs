use crate::SharedState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

mod reconciliation;
mod scan;
mod scheduler;

pub use scheduler::{
    can_launch_stage, compute_skipped_stages, is_gate_enabled, next_pipeline_stage,
};

#[derive(Debug, Clone, Serialize, PartialEq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "contracts.ts")]
pub enum AgentStatus {
    Preparing,
    Running,
    Completed,
    Failed,
    Stopped,
    Interrupted,
    #[serde(alias = "awaitingapproval")]
    AwaitingApproval,
}

impl<'de> Deserialize<'de> for AgentStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "preparing" => Ok(Self::Preparing),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "stopped" => Ok(Self::Stopped),
            "interrupted" => Ok(Self::Interrupted),
            "awaiting_approval" | "awaitingapproval" => Ok(Self::AwaitingApproval),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &[
                    "preparing",
                    "running",
                    "completed",
                    "failed",
                    "stopped",
                    "interrupted",
                    "awaiting_approval",
                ],
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "contracts.ts")]
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

/// Structured context generated at the end of each pipeline stage,
/// injected into the next stage's prompt to provide continuity.
#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
#[ts(export, export_to = "contracts.ts")]
pub struct StageContext {
    /// Which stage produced this context
    pub from_stage: String,
    /// Files that were modified (from git diff)
    pub files_changed: Vec<String>,
    /// Lines added in this stage
    pub lines_added: u32,
    /// Lines removed in this stage
    pub lines_removed: u32,
    /// PR number if one was created or exists
    pub pr_number: Option<u64>,
    /// Branch name for the PR
    pub branch_name: Option<String>,
    /// Key summary extracted from agent logs (review comments, test results, etc.)
    pub summary: String,
}

impl StageContext {
    /// Format this context as a concise section to append to a prompt.
    /// Kept under ~500 tokens to avoid bloating the prompt.
    pub fn to_prompt_section(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!("## Context from {} stage", self.from_stage));

        if !self.files_changed.is_empty() {
            let files_list: String = self
                .files_changed
                .iter()
                .take(20) // cap at 20 files to keep concise
                .map(|f| format!("  - {}", f))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!(
                "Files changed ({} added, {} removed):\n{}",
                self.lines_added, self.lines_removed, files_list
            ));
            if self.files_changed.len() > 20 {
                parts.push(format!(
                    "  ... and {} more files",
                    self.files_changed.len() - 20
                ));
            }
        }

        if let Some(pr) = self.pr_number {
            parts.push(format!("PR number: #{}", pr));
        }
        if let Some(ref branch) = self.branch_name {
            parts.push(format!("Branch: {}", branch));
        }

        if !self.summary.is_empty() {
            parts.push(format!("Summary: {}", self.summary));
        }

        parts.join("\n")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "contracts.ts")]
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
    pub lines_added: u32,
    pub lines_removed: u32,
    pub files_modified_list: Vec<String>,
    pub report: Option<crate::report::PipelineReport>,
    /// The CLI command invoked (e.g. "claude --print ...")
    pub command_display: Option<String>,
    /// Agent type used: "claude" or "codex"
    pub agent_type: String,
    /// Last line of output received
    pub last_log_line: Option<String>,
    /// Total number of log lines produced so far
    pub log_count: u32,
    /// Detected activity state from log content
    pub activity: Option<String>,
    /// Timestamp of the last log output (for stall detection)
    pub last_log_timestamp: Option<String>,
    /// Token usage from Claude result events
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    /// Labels from the GitHub issue, used for stage-skip logic
    #[serde(default)]
    pub issue_labels: Vec<String>,
    /// Stages that were skipped for this issue based on label rules
    #[serde(default)]
    pub skipped_stages: Vec<String>,
    /// Structured context from the previous pipeline stage
    pub stage_context: Option<StageContext>,
    /// The next stage to advance to when approval is granted (only set when status is AwaitingApproval)
    #[serde(default)]
    pub pending_next_stage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunSummary {
    pub id: String,
    pub repo: String,
    pub issue_number: u64,
    pub issue_title: String,
    pub status: AgentStatus,
    pub stage: PipelineStage,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub workspace_path: String,
    pub error: Option<String>,
    pub attempt: u32,
    pub max_retries: u32,
    pub command_display: Option<String>,
    pub agent_type: String,
    pub last_log_line: Option<String>,
    pub log_count: u32,
    pub activity: Option<String>,
    pub last_log_timestamp: Option<String>,
    #[serde(default)]
    pub skipped_stages: Vec<String>,
    #[serde(default)]
    pub pending_next_stage: Option<String>,
}

impl From<&AgentRun> for RunSummary {
    fn from(run: &AgentRun) -> Self {
        Self {
            id: run.id.clone(),
            repo: run.repo.clone(),
            issue_number: run.issue_number,
            issue_title: run.issue_title.clone(),
            status: run.status.clone(),
            stage: run.stage.clone(),
            started_at: run.started_at.clone(),
            finished_at: run.finished_at.clone(),
            workspace_path: run.workspace_path.clone(),
            error: run.error.clone(),
            attempt: run.attempt,
            max_retries: run.max_retries,
            command_display: run.command_display.clone(),
            agent_type: run.agent_type.clone(),
            last_log_line: run.last_log_line.clone(),
            log_count: run.log_count,
            activity: run.activity.clone(),
            last_log_timestamp: run.last_log_timestamp.clone(),
            skipped_stages: run.skipped_stages.clone(),
            pending_next_stage: run.pending_next_stage.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(default)]
#[ts(export, export_to = "contracts.ts")]
pub struct LifecycleHooks {
    /// Runs after a new workspace is created (e.g., npm install). Failure aborts.
    pub after_create: Option<String>,
    /// Runs before each agent attempt (e.g., git pull). Failure aborts.
    pub before_run: Option<String>,
    /// Runs after each agent attempt, success or failure. Failure is logged but ignored.
    pub after_run: Option<String>,
    /// Runs before workspace deletion. Failure is logged but ignored.
    pub before_remove: Option<String>,
    /// Timeout in seconds for each hook (default 60).
    pub timeout_secs: u64,
}

impl Default for LifecycleHooks {
    fn default() -> Self {
        Self {
            after_create: None,
            before_run: None,
            after_run: None,
            before_remove: None,
            timeout_secs: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[serde(default)]
#[ts(export, export_to = "contracts.ts")]
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
    #[serde(default = "default_retry_base_delay")]
    pub retry_base_delay_secs: u64,
    #[serde(default = "default_retry_max_backoff")]
    pub retry_max_backoff_secs: u64,
    pub cleanup_on_failure: bool,
    pub cleanup_on_stop: bool,
    pub workspace_ttl_days: u32,
    #[serde(default)]
    pub max_concurrent_by_stage: HashMap<String, usize>,
    #[serde(default)]
    pub stage_prompts: HashMap<String, String>,
    #[serde(default)]
    pub hooks: LifecycleHooks,
    /// Priority label ordering for dispatch sorting.
    /// Labels listed first have higher priority (dispatched first).
    /// Issues without any priority label are dispatched last.
    /// Default: ["priority:critical", "priority:high", "priority:medium", "priority:low"]
    #[serde(default = "default_priority_labels")]
    pub priority_labels: Vec<String>,
    /// Stall detection timeout in seconds. If an agent produces no output for
    /// this duration, it is killed and marked as failed. Set to 0 to disable.
    /// Default: 300 (5 minutes).
    #[serde(default = "default_stall_timeout")]
    pub stall_timeout_secs: u64,
    /// Label-to-stage skip mappings. When an issue has a label matching a key,
    /// the listed stages are skipped during auto-chaining.
    /// Only CodeReview and Testing can be skipped; Implement and Merge are always required.
    /// Default: {"skip:code-review": ["code_review"], "skip:testing": ["testing"], "docs-only": ["code_review", "testing"]}
    #[serde(default = "default_stage_skip_labels")]
    pub stage_skip_labels: HashMap<String, Vec<String>>,
    /// Per-stage approval gates. When a gate is enabled for a stage, the pipeline
    /// pauses after that stage completes and waits for explicit user approval before
    /// advancing to the next stage. Keys are stage names (implement, code_review,
    /// testing, merge). Default: all false (fully automatic).
    #[serde(default)]
    pub approval_gates: HashMap<String, bool>,
}

fn default_priority_labels() -> Vec<String> {
    vec![
        "priority:critical".to_string(),
        "priority:high".to_string(),
        "priority:medium".to_string(),
        "priority:low".to_string(),
    ]
}

fn default_stall_timeout() -> u64 {
    300
}

fn default_stage_skip_labels() -> HashMap<String, Vec<String>> {
    let mut m = HashMap::new();
    m.insert(
        "skip:code-review".to_string(),
        vec!["code_review".to_string()],
    );
    m.insert("skip:testing".to_string(), vec!["testing".to_string()]);
    m.insert(
        "docs-only".to_string(),
        vec!["code_review".to_string(), "testing".to_string()],
    );
    m
}

fn default_retry_base_delay() -> u64 {
    10
}

fn default_retry_max_backoff() -> u64 {
    300
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
            retry_base_delay_secs: default_retry_base_delay(),
            retry_max_backoff_secs: default_retry_max_backoff(),
            cleanup_on_failure: false,
            cleanup_on_stop: false,
            workspace_ttl_days: 7,
            max_concurrent_by_stage: HashMap::new(),
            stage_prompts: HashMap::new(),
            hooks: LifecycleHooks::default(),
            priority_labels: default_priority_labels(),
            stall_timeout_secs: default_stall_timeout(),
            stage_skip_labels: default_stage_skip_labels(),
            approval_gates: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "contracts.ts")]
pub struct OrchestratorOverview {
    pub is_running: bool,
    pub repos: Vec<String>,
    pub runs: Vec<RunSummary>,
    pub config: RunConfig,
    pub total_completed: usize,
    pub total_failed: usize,
    pub active_count: usize,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub total_runtime_secs: f64,
}

fn build_overview(state: &OrchestratorState) -> OrchestratorOverview {
    let mut runs = Vec::with_capacity(state.runs.len());
    let mut total_completed = 0;
    let mut total_failed = 0;
    let mut active_count = 0;

    for run in state.runs.values() {
        if run.stage == PipelineStage::Done {
            total_completed += 1;
        }
        if run.status == AgentStatus::Failed {
            total_failed += 1;
        }
        if run.status == AgentStatus::Running || run.status == AgentStatus::Preparing {
            active_count += 1;
        }

        runs.push(RunSummary::from(run));
    }

    OrchestratorOverview {
        is_running: state.is_running,
        repos: state.repos.clone(),
        runs,
        config: state.config.clone(),
        total_completed,
        total_failed,
        active_count,
        total_input_tokens: state.total_input_tokens,
        total_output_tokens: state.total_output_tokens,
        total_cost_usd: state.total_cost_usd,
        total_runtime_secs: state.total_runtime_secs,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "contracts.ts")]
pub struct BlockedIssue {
    pub repo: String,
    pub issue_number: u64,
    pub blocked_by: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "contracts.ts")]
pub struct BlockedIssueListEvent {
    pub blocked: Vec<BlockedIssue>,
}

pub struct OrchestratorState {
    pub is_running: bool,
    pub repos: Vec<String>,
    pub runs: HashMap<String, AgentRun>,
    pub config: RunConfig,
    pub agent_pids: HashMap<String, u32>,
    pub stop_flag: bool,
    /// Cumulative token and cost totals across all runs
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub total_runtime_secs: f64,
}

impl OrchestratorState {
    pub fn new() -> Self {
        // Try to load persisted state from disk
        if let Some(persisted) = crate::persistence::load_state() {
            return Self::from_persisted(persisted);
        }

        Self {
            is_running: false,
            repos: Vec::new(),
            runs: HashMap::new(),
            config: RunConfig::default(),
            agent_pids: HashMap::new(),
            stop_flag: false,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cost_usd: 0.0,
            total_runtime_secs: 0.0,
        }
    }

    fn from_persisted(persisted: crate::persistence::PersistedState) -> Self {
        // Backward compat: migrate old single `repo` to `repos` list
        let repos = if persisted.repos.is_empty() {
            persisted.repo.into_iter().collect()
        } else {
            persisted.repos
        };

        Self {
            is_running: false,
            repos,
            runs: persisted.runs,
            config: persisted.config,
            agent_pids: HashMap::new(),
            stop_flag: false,
            total_input_tokens: persisted.total_input_tokens,
            total_output_tokens: persisted.total_output_tokens,
            total_cost_usd: persisted.total_cost_usd,
            total_runtime_secs: persisted.total_runtime_secs,
        }
    }

    pub fn try_persist(&self) -> Result<(), String> {
        crate::persistence::save_state(self)
    }

    /// Persist current state to disk. Call after every state transition.
    pub fn persist(&self) {
        if let Err(err) = self.try_persist() {
            eprintln!("[persistence] Failed to save orchestrator state: {}", err);
        }
    }

    /// Get the latest run for a given repo and issue number
    pub fn latest_run_for_issue(&self, repo: &str, issue_number: u64) -> Option<&AgentRun> {
        self.runs
            .values()
            .filter(|r| r.repo == repo && r.issue_number == issue_number)
            .max_by_key(|r| r.started_at.clone())
    }
}

#[tauri::command]
pub async fn get_status(
    state: tauri::State<'_, SharedState>,
) -> Result<OrchestratorOverview, String> {
    let s = state.lock().await;
    Ok(build_overview(&s))
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
    repos: Vec<String>,
) -> Result<(), String> {
    if repos.is_empty() {
        return Err("At least one repository must be selected".to_string());
    }

    {
        let mut s = state.lock().await;
        if s.is_running {
            return Err("Orchestrator already running".to_string());
        }
        s.is_running = true;
        s.repos = repos.clone();
        s.stop_flag = false;
    }

    let _ = app.emit(
        "orchestrator-status",
        serde_json::json!({
            "running": true,
            "repos": &repos,
        }),
    );

    let state_clone = state.inner().clone();
    let app_clone = app.clone();

    tokio::spawn(async move {
        poll_loop(app_clone, state_clone, repos).await;
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
    s.persist();
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
pub async fn search_agent_logs(run_id: String, query: String) -> Result<Vec<String>, String> {
    let results = crate::logs::search_logs(&run_id, &query);
    Ok(results)
}

#[tauri::command]
pub async fn export_logs_text(run_id: String) -> Result<String, String> {
    let text = crate::logs::export_as_text(&run_id);
    if text.is_empty() {
        Err("No logs found for this run".to_string())
    } else {
        Ok(text)
    }
}

#[tauri::command]
pub async fn export_logs_json(run_id: String) -> Result<String, String> {
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

#[tauri::command]
pub async fn get_interrupted_runs(
    state: tauri::State<'_, SharedState>,
) -> Result<Vec<crate::persistence::InterruptedRunInfo>, String> {
    let s = state.lock().await;
    Ok(crate::persistence::get_interrupted_runs(&s))
}

#[tauri::command]
pub async fn resume_pipeline(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    run_id: String,
) -> Result<(), String> {
    let (repo, issue_number, issue_title, resume_stage, stored_labels) = {
        let mut s = state.lock().await;
        let run = s
            .runs
            .get(&run_id)
            .ok_or_else(|| format!("Run {} not found", run_id))?;

        if run.status != AgentStatus::Interrupted {
            return Err(format!(
                "Run {} is not interrupted (status: {:?})",
                run_id, run.status
            ));
        }

        let info = (
            run.repo.clone(),
            run.issue_number,
            run.issue_title.clone(),
            run.stage.clone(),
            run.issue_labels.clone(),
        );

        // Mark the old interrupted run as stopped so it doesn't block dispatch
        if let Some(run) = s.runs.get_mut(&run_id) {
            run.status = AgentStatus::Stopped;
            run.error = Some("Superseded by resumed pipeline".to_string());
        }
        s.persist();
        info
    };

    // Fetch the issue body and labels from GitHub so the prompt has full context
    let (issue_body, issue_labels) =
        match crate::github::get_issue_detail(repo.clone(), issue_number).await {
            Ok(issue) => (issue.body.unwrap_or_default(), issue.labels),
            Err(_) => (String::new(), stored_labels),
        };

    crate::agent::launch_agent(
        app,
        state.inner().clone(),
        repo,
        issue_number,
        issue_title,
        issue_body,
        resume_stage,
        issue_labels,
    )
    .await
    .map(|_| ())
    .map_err(|e| format!("Failed to resume pipeline: {}", e))
}

async fn poll_loop(app: AppHandle, state: SharedState, repos: Vec<String>) {
    let mut all_processed_notified = false;

    loop {
        let (
            should_stop,
            poll_interval,
            issue_label,
            notifications_enabled,
            notification_sound,
            scheduler_config,
        ) = {
            let s = state.lock().await;
            (
                s.stop_flag,
                s.config.poll_interval_secs,
                s.config.issue_label.clone(),
                s.config.notifications_enabled,
                s.config.notification_sound,
                scheduler::SchedulerConfig::from(&s.config),
            )
        };

        if should_stop {
            break;
        }

        reconciliation::reconcile_active_runs(&app, &state).await;

        let _ = app.emit(
            "orchestrator-poll",
            serde_json::json!({
                "timestamp": Utc::now().to_rfc3339(),
            }),
        );

        let snapshot = scan::collect_repository_snapshot(&repos, issue_label).await;
        emit_scan_errors(&app, &snapshot.fetch_errors);

        if snapshot.issues.is_empty() && snapshot.has_fetch_errors() {
            tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval)).await;
            continue;
        }

        let runtime = {
            let s = state.lock().await;
            scheduler::RuntimeSnapshot::from_state(&s)
        };
        let schedule = scheduler::plan_dispatch(&snapshot, &runtime, &scheduler_config);

        let no_active = runtime.active_count == 0;
        if schedule.all_issues_accounted_for && no_active && !all_processed_notified {
            if notifications_enabled {
                crate::notification::notify_all_processed(&app, notification_sound);
            }
            all_processed_notified = true;
        } else if !schedule.all_issues_accounted_for || !no_active {
            all_processed_notified = false;
        }

        emit_blocked_issues(&app, &schedule.blocked);
        launch_scheduled_runs(&app, &state, &schedule.launches).await;

        tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval)).await;
    }

    let mut s = state.lock().await;
    s.is_running = false;
    s.persist();
}

fn emit_scan_errors(app: &AppHandle, errors: &[String]) {
    for error in errors {
        let _ = app.emit(
            "orchestrator-error",
            serde_json::json!({
                "error": error,
            }),
        );
    }
}

fn emit_blocked_issues(app: &AppHandle, blocked: &[scheduler::BlockedIssue]) {
    for blocked_issue in blocked {
        let _ = app.emit(
            "orchestrator-blocked",
            serde_json::json!({
                "issue_number": blocked_issue.issue_number,
                "blocked_by": &blocked_issue.blocked_by,
            }),
        );
    }

    let _ = app.emit(
        "orchestrator-blocked-list",
        BlockedIssueListEvent {
            blocked: blocked
                .iter()
                .map(|b| BlockedIssue {
                    repo: b.repo.clone(),
                    issue_number: b.issue_number,
                    blocked_by: b.blocked_by.clone(),
                })
                .collect(),
        },
    );
}

async fn launch_scheduled_runs(
    app: &AppHandle,
    state: &SharedState,
    launches: &[scheduler::LaunchDecision],
) {
    for launch in launches {
        if let Err(error) = crate::agent::launch_agent(
            app.clone(),
            state.clone(),
            launch.repo.clone(),
            launch.issue_number,
            launch.issue_title.clone(),
            launch.issue_body.clone(),
            launch.stage.clone(),
            launch.issue_labels.clone(),
        )
        .await
        {
            let _ = app.emit(
                "orchestrator-error",
                serde_json::json!({
                    "error": format!(
                        "Failed to launch {} agent for {}#{}: {}",
                        launch.stage, launch.repo, launch.issue_number, error
                    ),
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::PersistedState;

    fn make_run(id: &str, status: AgentStatus, stage: PipelineStage) -> AgentRun {
        AgentRun {
            id: id.to_string(),
            repo: "pedrocid/SymphonyMac".to_string(),
            issue_number: 55,
            issue_title: "Refactor status API".to_string(),
            status,
            stage,
            started_at: "2026-03-08T09:00:00Z".to_string(),
            finished_at: Some("2026-03-08T09:10:00Z".to_string()),
            logs: Vec::new(),
            workspace_path: "/tmp/symphony-issue-55".to_string(),
            error: None,
            attempt: 2,
            max_retries: 3,
            lines_added: 42,
            lines_removed: 7,
            files_modified_list: vec!["src-tauri/src/orchestrator.rs".to_string()],
            report: None,
            command_display: Some("codex exec".to_string()),
            agent_type: "codex".to_string(),
            last_log_line: Some("last summary line".to_string()),
            log_count: 2048,
            activity: Some("Editing files".to_string()),
            last_log_timestamp: Some("2026-03-08T09:09:59Z".to_string()),
            input_tokens: 1200,
            output_tokens: 800,
            cost_usd: 0.34,
            issue_labels: vec!["enhancement".to_string()],
            skipped_stages: vec!["testing".to_string()],
            stage_context: None,
            pending_next_stage: Some("merge".to_string()),
        }
    }

    fn make_state(run: AgentRun) -> OrchestratorState {
        let mut runs = HashMap::new();
        runs.insert(run.id.clone(), run);

        OrchestratorState {
            is_running: true,
            repos: vec!["pedrocid/SymphonyMac".to_string()],
            runs,
            config: RunConfig::default(),
            agent_pids: HashMap::new(),
            stop_flag: false,
            total_input_tokens: 1200,
            total_output_tokens: 800,
            total_cost_usd: 0.34,
            total_runtime_secs: 600.0,
        }
    }

    #[test]
    fn test_overview_omits_large_run_buffers() {
        let summary_only_run = make_run("run-55", AgentStatus::Running, PipelineStage::Implement);

        let mut heavy_run = summary_only_run.clone();
        heavy_run.logs = (0..2048).map(|i| format!("log line {}", i)).collect();
        heavy_run.report = Some(crate::report::PipelineReport {
            issue_number: 55,
            issue_title: "Refactor status API".to_string(),
            repo: "pedrocid/SymphonyMac".to_string(),
            total_duration_secs: 600,
            total_duration_display: "10m".to_string(),
            stages: vec![],
            pr_number: Some(123),
            pr_url: Some("https://github.com/pedrocid/SymphonyMac/pull/123".to_string()),
            issue_url: "https://github.com/pedrocid/SymphonyMac/issues/55".to_string(),
            code_review_summary: "large review summary".repeat(20),
            testing_summary: "large testing summary".repeat(20),
            total_input_tokens: 1200,
            total_output_tokens: 800,
            total_cost_usd: 0.34,
        });
        heavy_run.stage_context = Some(StageContext {
            from_stage: "implement".to_string(),
            files_changed: vec!["src-tauri/src/orchestrator.rs".to_string()],
            lines_added: 42,
            lines_removed: 7,
            pr_number: Some(123),
            branch_name: Some("symphony/issue-55".to_string()),
            summary: "context summary".repeat(20),
        });

        let summary_value =
            serde_json::to_value(build_overview(&make_state(summary_only_run))).unwrap();
        let heavy_value = serde_json::to_value(build_overview(&make_state(heavy_run))).unwrap();
        let run_json = &heavy_value["runs"][0];

        assert_eq!(summary_value, heavy_value);
        assert!(run_json.get("logs").is_none());
        assert!(run_json.get("report").is_none());
        assert!(run_json.get("stage_context").is_none());
    }

    fn make_timed_run(id: &str, started_at: &str) -> AgentRun {
        AgentRun {
            id: id.to_string(),
            repo: "pedrocid/SymphonyMac".to_string(),
            issue_number: 62,
            issue_title: "Add automated coverage".to_string(),
            status: AgentStatus::AwaitingApproval,
            stage: PipelineStage::CodeReview,
            started_at: started_at.to_string(),
            finished_at: None,
            logs: vec![],
            workspace_path: "/tmp/symphony".to_string(),
            error: None,
            attempt: 2,
            max_retries: 3,
            lines_added: 10,
            lines_removed: 4,
            files_modified_list: vec!["src/App.tsx".to_string()],
            report: None,
            command_display: Some("claude --print".to_string()),
            agent_type: "claude".to_string(),
            last_log_line: Some("Awaiting approval".to_string()),
            log_count: 12,
            activity: Some("Analyzing code".to_string()),
            last_log_timestamp: Some(started_at.to_string()),
            input_tokens: 50,
            output_tokens: 75,
            cost_usd: 0.0123,
            issue_labels: vec!["feature".to_string()],
            skipped_stages: vec!["testing".to_string()],
            stage_context: None,
            pending_next_stage: Some("testing".to_string()),
        }
    }

    #[test]
    fn test_from_persisted_restores_config_and_migrates_legacy_repo() {
        let persisted = PersistedState {
            repo: Some("test/repo".to_string()),
            config: RunConfig {
                agent_type: "codex".to_string(),
                auto_approve: false,
                max_concurrent: 7,
                ..RunConfig::default()
            },
            ..PersistedState::default()
        };

        let state = OrchestratorState::from_persisted(persisted);

        assert_eq!(state.repos, vec!["test/repo".to_string()]);
        assert_eq!(state.config.agent_type, "codex");
        assert!(!state.config.auto_approve);
        assert_eq!(state.config.max_concurrent, 7);
    }

    #[test]
    fn test_stage_context_prompt_section_includes_metadata_and_truncates_file_list() {
        let context = StageContext {
            from_stage: "implement".to_string(),
            files_changed: (1..=22)
                .map(|index| format!("src/file-{}.ts", index))
                .collect(),
            lines_added: 120,
            lines_removed: 18,
            pr_number: Some(91),
            branch_name: Some("symphony/issue-62".to_string()),
            summary: "Implemented coverage and CI".to_string(),
        };

        let prompt_section = context.to_prompt_section();

        assert!(prompt_section.contains("## Context from implement stage"));
        assert!(prompt_section.contains("Files changed (120 added, 18 removed)"));
        assert!(prompt_section.contains("PR number: #91"));
        assert!(prompt_section.contains("Branch: symphony/issue-62"));
        assert!(prompt_section.contains("... and 2 more files"));
        assert!(prompt_section.contains("Summary: Implemented coverage and CI"));
    }

    #[test]
    fn test_latest_run_for_issue_uses_most_recent_started_at() {
        let mut state = OrchestratorState {
            is_running: false,
            repos: vec!["pedrocid/SymphonyMac".to_string()],
            runs: HashMap::new(),
            config: RunConfig::default(),
            agent_pids: HashMap::new(),
            stop_flag: false,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cost_usd: 0.0,
            total_runtime_secs: 0.0,
        };
        state.runs.insert(
            "run-early".to_string(),
            make_timed_run("run-early", "2026-03-08T10:00:00Z"),
        );
        state.runs.insert(
            "run-late".to_string(),
            make_timed_run("run-late", "2026-03-08T11:00:00Z"),
        );

        let latest = state
            .latest_run_for_issue("pedrocid/SymphonyMac", 62)
            .expect("latest run");

        assert_eq!(latest.id, "run-late");
    }

    #[test]
    fn test_agent_run_serialization_matches_frontend_contract() {
        let json = serde_json::to_value(make_timed_run("run-contract", "2026-03-08T11:00:00Z"))
            .expect("serialize agent run");

        assert_eq!(json["status"], "awaiting_approval");
        assert_eq!(json["stage"], "code_review");
        assert_eq!(json["pending_next_stage"], "testing");
        assert_eq!(json["skipped_stages"], serde_json::json!(["testing"]));
    }

    #[test]
    fn test_agent_status_deserializes_legacy_awaitingapproval_value() {
        let status: AgentStatus =
            serde_json::from_str("\"awaitingapproval\"").expect("deserialize legacy status");

        assert_eq!(status, AgentStatus::AwaitingApproval);
    }

    #[test]
    fn test_agent_run_serializes_null_optional_contract_fields() {
        let run = make_run("run-null-check", AgentStatus::Completed, PipelineStage::Done);
        let value = serde_json::to_value(&run).unwrap();

        assert!(value.get("report").is_some_and(serde_json::Value::is_null));
        assert!(value
            .get("stage_context")
            .is_some_and(serde_json::Value::is_null));
    }
}
