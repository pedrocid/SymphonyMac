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
    Interrupted,
    AwaitingApproval,
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

/// Structured context generated at the end of each pipeline stage,
/// injected into the next stage's prompt to provide continuity.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    pub lines_added: u32,
    pub lines_removed: u32,
    pub files_modified_list: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
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

/// Given an issue's labels and the configured skip-label mappings, return the
/// list of pipeline stages that should be skipped for this issue.
/// Only CodeReview and Testing can be skipped.
pub fn compute_skipped_stages(
    issue_labels: &[String],
    skip_labels: &HashMap<String, Vec<String>>,
) -> Vec<PipelineStage> {
    let mut skipped = Vec::new();
    for label in issue_labels {
        let label_lower = label.to_lowercase();
        for (skip_label, stages) in skip_labels {
            if label_lower == skip_label.to_lowercase() {
                for stage_name in stages {
                    let stage = match stage_name.as_str() {
                        "code_review" => PipelineStage::CodeReview,
                        "testing" => PipelineStage::Testing,
                        _ => continue, // Implement and Merge cannot be skipped
                    };
                    if !skipped.contains(&stage) {
                        skipped.push(stage);
                    }
                }
            }
        }
    }
    skipped
}

/// Return the next pipeline stage after `current`, skipping any stages in `skipped`.
/// Returns None if there is no next stage (i.e. current is Merge or Done).
pub fn next_pipeline_stage(
    current: &PipelineStage,
    skipped: &[PipelineStage],
) -> Option<PipelineStage> {
    let chain = [
        PipelineStage::Implement,
        PipelineStage::CodeReview,
        PipelineStage::Testing,
        PipelineStage::Merge,
    ];
    let current_idx = chain.iter().position(|s| s == current)?;
    for next in &chain[current_idx + 1..] {
        if !skipped.contains(next) {
            return Some(next.clone());
        }
    }
    None
}

/// Returns the priority rank of an issue based on its labels and the configured priority ordering.
/// Lower rank = higher priority. Issues without any priority label get `usize::MAX`.
fn issue_priority_rank(issue: &crate::github::Issue, priority_labels: &[String]) -> usize {
    let mut best = usize::MAX;
    for label in &issue.labels {
        let label_lower = label.to_lowercase();
        for (rank, priority) in priority_labels.iter().enumerate() {
            if rank >= best {
                break;
            }
            if label_lower == priority.to_lowercase() {
                best = rank;
                break;
            }
        }
    }
    best
}

/// Sort issues for dispatch: priority labels ascending, created_at oldest first, issue number as tiebreaker.
fn sort_issues_for_dispatch(issues: &mut [crate::github::Issue], priority_labels: &[String]) {
    issues.sort_by(|a, b| {
        let rank_a = issue_priority_rank(a, priority_labels);
        let rank_b = issue_priority_rank(b, priority_labels);
        rank_a
            .cmp(&rank_b)
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.number.cmp(&b.number))
    });
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    let previous_config = s.config.clone();
    s.config = config;
    if let Err(err) = s.try_persist() {
        s.config = previous_config;
        return Err(format!("Failed to save settings: {}", err));
    }
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

/// Reconcile active runs against current GitHub issue state.
/// Stops agents for issues that have been closed externally.
async fn reconcile_active_runs(app: &AppHandle, state: &SharedState) {
    // Collect active runs' info while holding the lock briefly
    let active_runs: Vec<(String, u64, String)> = {
        let s = state.lock().await;
        s.runs
            .values()
            .filter(|r| r.status == AgentStatus::Running || r.status == AgentStatus::Preparing)
            .map(|r| (r.id.clone(), r.issue_number, r.repo.clone()))
            .collect()
    };

    if active_runs.is_empty() {
        return;
    }

    // Deduplicate (repo, issue_number) pairs to avoid redundant API calls
    let mut checked_issues: std::collections::HashMap<(String, u64), bool> =
        std::collections::HashMap::new();

    for (run_id, issue_number, repo) in &active_runs {
        let cache_key = (repo.clone(), *issue_number);
        let is_open = if let Some(&cached) = checked_issues.get(&cache_key) {
            cached
        } else {
            let open = github::is_issue_open(repo, *issue_number).unwrap_or(true);
            checked_issues.insert(cache_key, open);
            open
        };

        if !is_open {
            let _ = app.emit(
                "orchestrator-reconcile",
                serde_json::json!({
                    "run_id": run_id,
                    "issue_number": issue_number,
                    "reason": "issue_closed",
                }),
            );

            // Stop the agent: kill process, update state, optionally clean up
            let mut s = state.lock().await;
            if let Some(pid) = s.agent_pids.remove(run_id) {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
            }
            if let Some(run) = s.runs.get_mut(run_id) {
                run.status = AgentStatus::Stopped;
                run.finished_at = Some(Utc::now().to_rfc3339());
                run.logs.push(format!(
                    "[reconciler] Stopped: issue #{} was closed externally",
                    issue_number
                ));
                run.error = Some(format!("Issue #{} closed externally", issue_number));
            }
            s.persist();
            let should_cleanup = s.config.cleanup_on_stop;
            let hooks = s.config.hooks.clone();
            drop(s);

            let _ = app.emit(
                "agent-status-changed",
                serde_json::json!({
                    "run_id": run_id,
                    "status": "stopped",
                    "reason": "issue_closed",
                }),
            );

            if should_cleanup {
                let _ = crate::workspace::cleanup_workspace(repo, *issue_number, &hooks);
            }
        }
    }
}

async fn poll_loop(app: AppHandle, state: SharedState, repos: Vec<String>) {
    let mut all_processed_notified = false;

    loop {
        let (
            should_stop,
            poll_interval,
            max_concurrent,
            label,
            stage_limits,
            priority_labels,
            skip_labels,
        ) = {
            let s = state.lock().await;
            (
                s.stop_flag,
                s.config.poll_interval_secs,
                s.config.max_concurrent,
                s.config.issue_label.clone(),
                s.config.max_concurrent_by_stage.clone(),
                s.config.priority_labels.clone(),
                s.config.stage_skip_labels.clone(),
            )
        };

        if should_stop {
            break;
        }

        // ---- RECONCILIATION: Stop agents for externally-closed issues ----
        reconcile_active_runs(&app, &state).await;

        let _ = app.emit(
            "orchestrator-poll",
            serde_json::json!({
                "timestamp": Utc::now().to_rfc3339(),
            }),
        );

        // ---- STEP 1: Fetch issues and PRs from ALL repos ----
        // Each issue is tagged with its repo for dispatch
        let mut all_issues: Vec<(String, github::Issue)> = Vec::new();
        let mut all_issues_with_pr: std::collections::HashMap<(String, u64), github::PullRequest> =
            std::collections::HashMap::new();
        let mut had_fetch_error = false;

        for repo in &repos {
            let issues =
                match github::list_issues(repo.clone(), Some("open".to_string()), label.clone())
                    .await
                {
                    Ok(issues) => issues,
                    Err(e) => {
                        let _ = app.emit(
                            "orchestrator-error",
                            serde_json::json!({
                                "error": format!("Failed to fetch issues from {}: {}", repo, e),
                            }),
                        );
                        had_fetch_error = true;
                        continue;
                    }
                };

            for issue in issues {
                all_issues.push((repo.clone(), issue));
            }

            let open_prs = github::list_open_prs(repo.clone())
                .await
                .unwrap_or_default();
            for pr in open_prs {
                let issue_num = pr
                    .closes_issue
                    .or_else(|| github::parse_issue_from_title(&pr.title));
                if let Some(n) = issue_num {
                    all_issues_with_pr.insert((repo.clone(), n), pr);
                }
            }
        }

        if all_issues.is_empty() && had_fetch_error {
            tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval)).await;
            continue;
        }

        // Sort all issues across repos by priority labels, then created_at (oldest first), then issue number
        {
            let pl = &priority_labels;
            all_issues.sort_by(|(_, a), (_, b)| {
                let rank_a = issue_priority_rank(a, pl);
                let rank_b = issue_priority_rank(b, pl);
                rank_a
                    .cmp(&rank_b)
                    .then_with(|| a.created_at.cmp(&b.created_at))
                    .then_with(|| a.number.cmp(&b.number))
            });
        }

        // ---- STEP 2: Determine available slots ----
        let (active_count, active_by_stage) = {
            let s = state.lock().await;
            let active: Vec<&AgentRun> = s
                .runs
                .values()
                .filter(|r| r.status == AgentStatus::Running || r.status == AgentStatus::Preparing)
                .collect();
            let count = active.len();
            let mut by_stage: HashMap<String, usize> = HashMap::new();
            for r in &active {
                *by_stage.entry(r.stage.to_string()).or_insert(0) += 1;
            }
            (count, by_stage)
        };
        let available_slots = max_concurrent.saturating_sub(active_count);
        if available_slots == 0 {
            tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval)).await;
            continue;
        }

        // ---- STEP 3: Filter issues that need work ----
        // Use (repo, issue_number) tuples to avoid cross-repo collisions
        let (already_working, fully_done, has_any_run) = {
            let s = state.lock().await;
            let working: Vec<(String, u64)> = s
                .runs
                .values()
                .filter(|r| r.status == AgentStatus::Running || r.status == AgentStatus::Preparing)
                .map(|r| (r.repo.clone(), r.issue_number))
                .collect();
            let done: Vec<(String, u64)> = s
                .runs
                .values()
                .filter(|r| r.stage == PipelineStage::Done && r.status == AgentStatus::Completed)
                .map(|r| (r.repo.clone(), r.issue_number))
                .collect();
            let any: Vec<(String, u64)> = s
                .runs
                .values()
                .map(|r| (r.repo.clone(), r.issue_number))
                .collect();
            (working, done, any)
        };

        // ---- Check if all issues are processed ----
        {
            let s = state.lock().await;
            let all_done = !all_issues.is_empty()
                && all_issues.iter().all(|(repo, issue)| {
                    let key = (repo.clone(), issue.number);
                    already_working.contains(&key)
                        || fully_done.contains(&key)
                        || has_any_run.contains(&key)
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
        let mut blocked_issues: Vec<(String, u64, Vec<u64>)> = Vec::new();
        let mut stage_slots_used: HashMap<String, usize> = HashMap::new();

        for (repo, issue) in &all_issues {
            if used_slots >= available_slots {
                break;
            }

            let key = (repo.clone(), issue.number);

            // Skip already active, fully done, or already has a run
            if already_working.contains(&key)
                || fully_done.contains(&key)
                || has_any_run.contains(&key)
            {
                continue;
            }

            // Check for blockers in issue body
            let body_text = issue.body.as_deref().unwrap_or("");
            let blocker_nums = github::parse_blockers(body_text);
            if !blocker_nums.is_empty() {
                let open_blockers = github::check_blockers_open(repo, &blocker_nums);
                if !open_blockers.is_empty() {
                    blocked_issues.push((repo.clone(), issue.number, open_blockers.clone()));
                    let _ = app.emit(
                        "orchestrator-blocked",
                        serde_json::json!({
                            "issue_number": issue.number,
                            "blocked_by": open_blockers,
                        }),
                    );
                    continue;
                }
            }

            // Compute which stages to skip based on issue labels
            let skipped = compute_skipped_stages(&issue.labels, &skip_labels);

            // Decide which stage to start at
            let pr_key = (repo.clone(), issue.number);
            let (stage, title, body) = if let Some(pr) = all_issues_with_pr.get(&pr_key) {
                // This issue already has a PR → start at Code Review (or next non-skipped)
                let mut start = PipelineStage::CodeReview;
                if skipped.contains(&start) {
                    start = next_pipeline_stage(&PipelineStage::Implement, &skipped)
                        .unwrap_or(PipelineStage::Merge);
                }
                (start, pr.title.clone(), pr.body.clone().unwrap_or_default())
            } else {
                // No PR → start at Implement (always required)
                (
                    PipelineStage::Implement,
                    issue.title.clone(),
                    issue.body.clone().unwrap_or_default(),
                )
            };

            // Check per-stage concurrency limit
            let stage_name = stage.to_string();
            if let Some(&limit) = stage_limits.get(&stage_name) {
                if limit > 0 {
                    let current = active_by_stage.get(&stage_name).copied().unwrap_or(0)
                        + stage_slots_used.get(&stage_name).copied().unwrap_or(0);
                    if current >= limit {
                        continue; // Skip: per-stage limit reached
                    }
                }
            }

            if let Err(e) = crate::agent::launch_agent(
                app.clone(),
                state.clone(),
                repo.clone(),
                issue.number,
                title,
                body,
                stage.clone(),
                issue.labels.clone(),
            )
            .await
            {
                let _ = app.emit("orchestrator-error", serde_json::json!({
                    "error": format!("Failed to launch {} agent for {}#{}: {}", stage, repo, issue.number, e),
                }));
            } else {
                used_slots += 1;
                *stage_slots_used.entry(stage_name).or_insert(0) += 1;
            }
        }

        // Emit the full list of currently blocked issues for the UI
        // Always emit so the UI clears stale blocked state when blockers resolve
        let _ = app.emit(
            "orchestrator-blocked-list",
            serde_json::json!({
                "blocked": blocked_issues.iter().map(|(repo, num, blockers)| {
                    serde_json::json!({
                        "repo": repo,
                        "issue_number": num,
                        "blocked_by": blockers,
                    })
                }).collect::<Vec<_>>(),
            }),
        );

        tokio::time::sleep(tokio::time::Duration::from_secs(poll_interval)).await;
    }

    let mut s = state.lock().await;
    s.is_running = false;
    s.persist();
}

/// Check whether an approval gate is enabled for the given stage.
pub fn is_gate_enabled(config: &RunConfig, stage: &PipelineStage) -> bool {
    let stage_name = stage.to_string();
    config
        .approval_gates
        .get(&stage_name)
        .copied()
        .unwrap_or(false)
}

/// Check whether an additional agent for the given stage can be launched
/// without exceeding the per-stage (or global) concurrency limit.
pub fn can_launch_stage(state: &OrchestratorState, stage: &PipelineStage) -> bool {
    let active_count = state
        .runs
        .values()
        .filter(|r| r.status == AgentStatus::Running || r.status == AgentStatus::Preparing)
        .count();

    // Global limit
    if active_count >= state.config.max_concurrent {
        return false;
    }

    // Per-stage limit (0 means use global only)
    let stage_name = stage.to_string();
    if let Some(&limit) = state.config.max_concurrent_by_stage.get(&stage_name) {
        if limit > 0 {
            let stage_count = state
                .runs
                .values()
                .filter(|r| {
                    (r.status == AgentStatus::Running || r.status == AgentStatus::Preparing)
                        && r.stage.to_string() == stage_name
                })
                .count();
            if stage_count >= limit {
                return false;
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::Issue;
    use std::collections::HashMap;
    use crate::persistence::PersistedState;

    fn make_issue(number: u64, labels: Vec<&str>, created_at: &str) -> Issue {
        Issue {
            number,
            title: format!("Issue #{}", number),
            body: None,
            state: "OPEN".to_string(),
            labels: labels.into_iter().map(|s| s.to_string()).collect(),
            assignee: None,
            url: String::new(),
            created_at: created_at.to_string(),
            updated_at: String::new(),
        }
    }

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

    #[test]
    fn test_priority_rank_critical_is_first() {
        let labels = default_priority_labels();
        let issue = make_issue(1, vec!["priority:critical"], "2024-01-01T00:00:00Z");
        assert_eq!(issue_priority_rank(&issue, &labels), 0);
    }

    #[test]
    fn test_priority_rank_low_is_last_defined() {
        let labels = default_priority_labels();
        let issue = make_issue(1, vec!["priority:low"], "2024-01-01T00:00:00Z");
        assert_eq!(issue_priority_rank(&issue, &labels), 3);
    }

    #[test]
    fn test_priority_rank_no_label_is_max() {
        let labels = default_priority_labels();
        let issue = make_issue(1, vec!["bug"], "2024-01-01T00:00:00Z");
        assert_eq!(issue_priority_rank(&issue, &labels), usize::MAX);
    }

    #[test]
    fn test_priority_rank_case_insensitive() {
        let labels = default_priority_labels();
        let issue = make_issue(1, vec!["Priority:Critical"], "2024-01-01T00:00:00Z");
        assert_eq!(issue_priority_rank(&issue, &labels), 0);
    }

    #[test]
    fn test_priority_rank_multiple_labels_uses_highest() {
        let labels = default_priority_labels();
        // Issue has both low and critical labels — should use critical (rank 0), not low (rank 3)
        let issue = make_issue(
            1,
            vec!["priority:low", "priority:critical"],
            "2024-01-01T00:00:00Z",
        );
        assert_eq!(issue_priority_rank(&issue, &labels), 0);
    }

    #[test]
    fn test_sort_by_priority_then_date_then_number() {
        let labels = default_priority_labels();
        let mut issues = vec![
            make_issue(3, vec!["priority:low"], "2024-01-01T00:00:00Z"),
            make_issue(1, vec!["priority:critical"], "2024-01-02T00:00:00Z"),
            make_issue(2, vec!["priority:high"], "2024-01-01T00:00:00Z"),
            make_issue(4, vec![], "2024-01-01T00:00:00Z"),
            make_issue(5, vec!["priority:critical"], "2024-01-01T00:00:00Z"),
        ];
        sort_issues_for_dispatch(&mut issues, &labels);

        let numbers: Vec<u64> = issues.iter().map(|i| i.number).collect();
        // critical oldest first (5 before 1), then high (2), then low (3), then unlabeled (4)
        assert_eq!(numbers, vec![5, 1, 2, 3, 4]);
    }

    #[test]
    fn test_sort_same_priority_same_date_uses_number() {
        let labels = default_priority_labels();
        let mut issues = vec![
            make_issue(10, vec!["priority:high"], "2024-01-01T00:00:00Z"),
            make_issue(5, vec!["priority:high"], "2024-01-01T00:00:00Z"),
        ];
        sort_issues_for_dispatch(&mut issues, &labels);

        let numbers: Vec<u64> = issues.iter().map(|i| i.number).collect();
        assert_eq!(numbers, vec![5, 10]);
    }

    #[test]
    fn test_sort_unlabeled_fifo() {
        let labels = default_priority_labels();
        let mut issues = vec![
            make_issue(3, vec![], "2024-01-03T00:00:00Z"),
            make_issue(1, vec![], "2024-01-01T00:00:00Z"),
            make_issue(2, vec![], "2024-01-02T00:00:00Z"),
        ];
        sort_issues_for_dispatch(&mut issues, &labels);

        let numbers: Vec<u64> = issues.iter().map(|i| i.number).collect();
        assert_eq!(numbers, vec![1, 2, 3]);
    }

    #[test]
    fn test_sort_custom_priority_labels() {
        let labels = vec!["urgent".to_string(), "normal".to_string()];
        let mut issues = vec![
            make_issue(1, vec!["normal"], "2024-01-01T00:00:00Z"),
            make_issue(2, vec!["urgent"], "2024-01-01T00:00:00Z"),
            make_issue(3, vec![], "2024-01-01T00:00:00Z"),
        ];
        sort_issues_for_dispatch(&mut issues, &labels);

        let numbers: Vec<u64> = issues.iter().map(|i| i.number).collect();
        assert_eq!(numbers, vec![2, 1, 3]);
    }

    #[test]
    fn test_compute_skipped_stages_skip_testing() {
        let skip_labels = default_stage_skip_labels();
        let issue_labels = vec!["skip:testing".to_string()];
        let skipped = compute_skipped_stages(&issue_labels, &skip_labels);
        assert_eq!(skipped, vec![PipelineStage::Testing]);
    }

    #[test]
    fn test_compute_skipped_stages_skip_code_review() {
        let skip_labels = default_stage_skip_labels();
        let issue_labels = vec!["skip:code-review".to_string()];
        let skipped = compute_skipped_stages(&issue_labels, &skip_labels);
        assert_eq!(skipped, vec![PipelineStage::CodeReview]);
    }

    #[test]
    fn test_compute_skipped_stages_docs_only() {
        let skip_labels = default_stage_skip_labels();
        let issue_labels = vec!["docs-only".to_string()];
        let skipped = compute_skipped_stages(&issue_labels, &skip_labels);
        assert!(skipped.contains(&PipelineStage::CodeReview));
        assert!(skipped.contains(&PipelineStage::Testing));
        assert_eq!(skipped.len(), 2);
    }

    #[test]
    fn test_compute_skipped_stages_case_insensitive() {
        let skip_labels = default_stage_skip_labels();
        let issue_labels = vec!["Skip:Testing".to_string()];
        let skipped = compute_skipped_stages(&issue_labels, &skip_labels);
        assert_eq!(skipped, vec![PipelineStage::Testing]);
    }

    #[test]
    fn test_compute_skipped_stages_no_matching_labels() {
        let skip_labels = default_stage_skip_labels();
        let issue_labels = vec!["bug".to_string(), "priority:high".to_string()];
        let skipped = compute_skipped_stages(&issue_labels, &skip_labels);
        assert!(skipped.is_empty());
    }

    #[test]
    fn test_compute_skipped_stages_cannot_skip_implement_or_merge() {
        let mut skip_labels = HashMap::new();
        skip_labels.insert(
            "skip-all".to_string(),
            vec![
                "implement".to_string(),
                "code_review".to_string(),
                "testing".to_string(),
                "merge".to_string(),
            ],
        );
        let issue_labels = vec!["skip-all".to_string()];
        let skipped = compute_skipped_stages(&issue_labels, &skip_labels);
        // Only CodeReview and Testing can be skipped
        assert!(skipped.contains(&PipelineStage::CodeReview));
        assert!(skipped.contains(&PipelineStage::Testing));
        assert_eq!(skipped.len(), 2);
    }

    #[test]
    fn test_next_pipeline_stage_no_skips() {
        let skipped = vec![];
        assert_eq!(
            next_pipeline_stage(&PipelineStage::Implement, &skipped),
            Some(PipelineStage::CodeReview)
        );
        assert_eq!(
            next_pipeline_stage(&PipelineStage::CodeReview, &skipped),
            Some(PipelineStage::Testing)
        );
        assert_eq!(
            next_pipeline_stage(&PipelineStage::Testing, &skipped),
            Some(PipelineStage::Merge)
        );
        assert_eq!(next_pipeline_stage(&PipelineStage::Merge, &skipped), None);
    }

    #[test]
    fn test_next_pipeline_stage_skip_code_review() {
        let skipped = vec![PipelineStage::CodeReview];
        assert_eq!(
            next_pipeline_stage(&PipelineStage::Implement, &skipped),
            Some(PipelineStage::Testing)
        );
    }

    #[test]
    fn test_next_pipeline_stage_skip_both() {
        let skipped = vec![PipelineStage::CodeReview, PipelineStage::Testing];
        assert_eq!(
            next_pipeline_stage(&PipelineStage::Implement, &skipped),
            Some(PipelineStage::Merge)
        );
    }

    #[test]
    fn test_next_pipeline_stage_skip_testing() {
        let skipped = vec![PipelineStage::Testing];
        assert_eq!(
            next_pipeline_stage(&PipelineStage::CodeReview, &skipped),
            Some(PipelineStage::Merge)
        );
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
}
