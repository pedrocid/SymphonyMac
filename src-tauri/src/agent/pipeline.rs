use super::process::run_agent_process;
use super::prompt::{build_command_args, build_prompt, format_command_display};
use super::runtime;
use crate::orchestrator::{AgentRun, AgentStatus, PipelineStage, RunConfig, StageContext};
use crate::{report, SharedState};
use chrono::Utc;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub(crate) struct StageLaunchSpec {
    pub repo: String,
    pub issue_number: u64,
    pub issue_title: String,
    pub issue_body: String,
    pub stage: PipelineStage,
    pub issue_labels: Vec<String>,
    pub workspace_path: PathBuf,
    pub attempt: u32,
    pub max_retries: u32,
    pub previous_error: String,
    pub previous_context: Option<StageContext>,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentProcessRequest {
    pub run_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub spec: StageLaunchSpec,
}

struct PreparedStageRun {
    run: AgentRun,
    request: AgentProcessRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MergeVerification {
    NotRequired,
    Verified,
    NotMerged,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum SuccessfulStageAction {
    Advance {
        next_stage: PipelineStage,
        skipped_logs: Vec<String>,
    },
    AwaitingApproval {
        next_stage: PipelineStage,
        skipped_logs: Vec<String>,
    },
    FinishPipeline,
    MergeBlocked,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FailureAction {
    Retry {
        next_attempt: u32,
        backoff_secs: u64,
    },
    Exhausted,
}

#[derive(Debug, Clone)]
pub(crate) struct PipelineCompletionSpec {
    pub repo: String,
    pub issue_number: u64,
    pub issue_title: String,
    pub workspace_path: PathBuf,
    pub issue_labels: Vec<String>,
    pub skipped_stages: Vec<PipelineStage>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct UsageTotals {
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
}

pub(crate) fn compute_retry_backoff(config: &RunConfig, attempt: u32) -> u64 {
    let base_delay = if config.retry_base_delay_secs > 0 {
        config.retry_base_delay_secs
    } else {
        config.retry_backoff_secs
    };
    let exponent = attempt.saturating_sub(1);
    let exp_backoff = base_delay.saturating_mul(2u64.saturating_pow(exponent));
    exp_backoff.min(config.retry_max_backoff_secs)
}

pub(crate) fn decide_failure_action(
    current_attempt: u32,
    max_retries: u32,
    config: &RunConfig,
) -> FailureAction {
    if current_attempt <= max_retries {
        FailureAction::Retry {
            next_attempt: current_attempt + 1,
            backoff_secs: compute_retry_backoff(config, current_attempt),
        }
    } else {
        FailureAction::Exhausted
    }
}

pub(crate) fn decide_successful_stage_action(
    stage: &PipelineStage,
    skipped_stages: &[PipelineStage],
    gate_enabled: bool,
    merge_verification: MergeVerification,
) -> SuccessfulStageAction {
    match stage {
        PipelineStage::Merge => match merge_verification {
            MergeVerification::NotMerged => SuccessfulStageAction::MergeBlocked,
            MergeVerification::Verified
            | MergeVerification::Unknown
            | MergeVerification::NotRequired => SuccessfulStageAction::FinishPipeline,
        },
        PipelineStage::Done => SuccessfulStageAction::Noop,
        _ => match crate::orchestrator::next_pipeline_stage(stage, skipped_stages) {
            Some(next_stage) => {
                let skipped_logs = skipped_stage_logs(stage, &next_stage, skipped_stages);
                if gate_enabled {
                    SuccessfulStageAction::AwaitingApproval {
                        next_stage,
                        skipped_logs,
                    }
                } else {
                    SuccessfulStageAction::Advance {
                        next_stage,
                        skipped_logs,
                    }
                }
            }
            None => SuccessfulStageAction::Noop,
        },
    }
}

pub(crate) async fn prepare_and_register_stage_run(
    app: &AppHandle,
    state: &SharedState,
    config: &RunConfig,
    spec: StageLaunchSpec,
    emit_extra: Map<String, Value>,
) -> AgentProcessRequest {
    let PreparedStageRun { run, request } = prepare_stage_run(config, spec);
    runtime::register_preparing_run(app, state, run, emit_extra).await;
    request
}

pub(crate) fn spawn_next_stage(app: AppHandle, state: SharedState, spec: StageLaunchSpec) {
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        if !wait_for_stage_slot(&state, &spec.stage, spec.issue_number).await {
            return;
        }

        if should_skip_next_stage_launch(&spec).await {
            return;
        }

        let config = {
            let s = state.lock().await;
            s.config.clone()
        };

        let request = prepare_and_register_stage_run(&app, &state, &config, spec, Map::new()).await;
        run_agent_process(app, state, request).await;
    });
}

pub(crate) fn spawn_retry(
    app: AppHandle,
    state: SharedState,
    spec: StageLaunchSpec,
    backoff_secs: u64,
) {
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;

        if !wait_for_stage_slot(&state, &spec.stage, spec.issue_number).await {
            return;
        }

        let config = {
            let s = state.lock().await;
            s.config.clone()
        };

        let mut emit_extra = Map::new();
        emit_extra.insert("attempt".to_string(), json!(spec.attempt));
        emit_extra.insert("max_retries".to_string(), json!(spec.max_retries + 1));

        let request = prepare_and_register_stage_run(&app, &state, &config, spec, emit_extra).await;
        run_agent_process(app, state, request).await;
    });
}

pub(crate) async fn finish_pipeline(
    app: &AppHandle,
    state: &SharedState,
    completion: PipelineCompletionSpec,
) {
    let stage_runs =
        collect_latest_stage_runs(state, &completion.repo, completion.issue_number).await;
    let usage_totals = collect_usage_totals(state, &completion.repo, completion.issue_number).await;
    let (done_run, pipeline_report) = build_done_run(&completion, stage_runs, usage_totals);
    let done_run_id = done_run.id.clone();

    {
        let mut s = state.lock().await;
        s.runs.insert(done_run_id.clone(), done_run);
        s.persist();
    }

    runtime::emit_status(app, &done_run_id, "completed", "done", Map::new());
    let _ = app.emit("pipeline-report", &pipeline_report);

    {
        let s = state.lock().await;
        if s.config.notifications_enabled {
            crate::notification::notify_pipeline_done(
                app,
                completion.issue_number,
                &completion.issue_title,
                s.config.notification_sound,
            );
        }
    }

    runtime::update_dock_badge(state).await;

    let cleanup_hooks = {
        let s = state.lock().await;
        s.config.hooks.clone()
    };
    let _ = crate::workspace::cleanup_workspace(
        &completion.repo,
        completion.issue_number,
        &cleanup_hooks,
    );
}

pub(crate) fn extract_stage_context(run: &AgentRun, repo: &str) -> StageContext {
    let from_stage = run.stage.to_string();
    let files_changed = run.files_modified_list.clone();
    let lines_added = run.lines_added;
    let lines_removed = run.lines_removed;
    let pr_number = detect_pr_number_from_logs(&run.logs, repo);
    let branch_name = detect_branch_from_logs(&run.logs);
    let summary = build_stage_summary(&run.stage, &run.logs);

    StageContext {
        from_stage,
        files_changed,
        lines_added,
        lines_removed,
        pr_number,
        branch_name,
        summary,
    }
}

fn prepare_stage_run(config: &RunConfig, spec: StageLaunchSpec) -> PreparedStageRun {
    let run_id = Uuid::new_v4().to_string();
    let prompt = build_prompt(
        &spec.stage,
        spec.issue_number,
        &spec.repo,
        &spec.issue_title,
        &spec.issue_body,
        &config.stage_prompts,
        spec.attempt,
        &spec.previous_error,
        spec.previous_context.as_ref(),
    );
    let (command, args) = build_command_args(config, &prompt);
    let command_display = format_command_display(&command, &args);
    let skipped =
        crate::orchestrator::compute_skipped_stages(&spec.issue_labels, &config.stage_skip_labels);
    let skipped_stage_names: Vec<String> = skipped.iter().map(ToString::to_string).collect();

    let mut logs = Vec::new();
    if spec.attempt > 1 {
        logs.push(format!(
            "Retry attempt {}/{} (previous error: {})",
            spec.attempt,
            spec.max_retries + 1,
            spec.previous_error
        ));
    }

    let run = AgentRun {
        id: run_id.clone(),
        repo: spec.repo.clone(),
        issue_number: spec.issue_number,
        issue_title: spec.issue_title.clone(),
        status: AgentStatus::Preparing,
        stage: spec.stage.clone(),
        started_at: Utc::now().to_rfc3339(),
        finished_at: None,
        logs,
        workspace_path: spec.workspace_path.to_string_lossy().to_string(),
        error: None,
        attempt: spec.attempt,
        max_retries: spec.max_retries,
        lines_added: 0,
        lines_removed: 0,
        files_modified_list: Vec::new(),
        report: None,
        command_display: Some(command_display),
        agent_type: config.agent_type.clone(),
        last_log_line: None,
        log_count: 0,
        activity: None,
        input_tokens: 0,
        output_tokens: 0,
        cost_usd: 0.0,
        last_log_timestamp: None,
        issue_labels: spec.issue_labels.clone(),
        skipped_stages: skipped_stage_names,
        stage_context: None,
        pending_next_stage: None,
    };

    let request = AgentProcessRequest {
        run_id,
        command,
        args,
        spec,
    };

    PreparedStageRun { run, request }
}

fn skipped_stage_logs(
    current_stage: &PipelineStage,
    next_stage: &PipelineStage,
    skipped_stages: &[PipelineStage],
) -> Vec<String> {
    let default_chain: &[PipelineStage] = match current_stage {
        PipelineStage::Implement => &[PipelineStage::CodeReview, PipelineStage::Testing],
        PipelineStage::CodeReview => &[PipelineStage::Testing],
        _ => &[],
    };

    default_chain
        .iter()
        .filter(|stage| skipped_stages.contains(stage) && *stage != next_stage)
        .map(|stage| format!("[pipeline] Skipping {} stage (label rule)", stage))
        .collect()
}

async fn wait_for_stage_slot(
    state: &SharedState,
    stage: &PipelineStage,
    issue_number: u64,
) -> bool {
    let stage_label = stage.to_string();

    loop {
        let (can_launch, stopped) = {
            let s = state.lock().await;
            (
                crate::orchestrator::can_launch_stage(&s, stage),
                s.stop_flag,
            )
        };

        if stopped {
            eprintln!(
                "Orchestrator stopped; aborting queued stage {stage_label} for issue #{issue_number}"
            );
            return false;
        }

        if can_launch {
            return true;
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

async fn should_skip_next_stage_launch(spec: &StageLaunchSpec) -> bool {
    let repo = spec.repo.clone();
    let issue_number = spec.issue_number;
    let stage = spec.stage.clone();
    let stage_label = stage.to_string();

    tokio::task::spawn_blocking(move || {
        match crate::github::get_issue_state(&repo, issue_number) {
            Ok(ref issue_state) if issue_state != "OPEN" => {
                eprintln!(
                    "Issue #{issue_number} is {issue_state}; skipping stage {stage_label}"
                );
                return true;
            }
            Err(ref error) => {
                eprintln!(
                    "Warning: could not re-check issue #{issue_number} state: {error}; proceeding anyway"
                );
            }
            _ => {}
        }

        if matches!(stage, PipelineStage::Merge) {
            match crate::github::is_pr_merged_for_issue(&repo, issue_number) {
                Ok(true) => {
                    eprintln!(
                        "PR for issue #{issue_number} is already merged; skipping Merge stage"
                    );
                    return true;
                }
                Err(ref error) => {
                    eprintln!(
                        "Warning: could not check PR merge status for #{issue_number}: {error}; proceeding anyway"
                    );
                }
                _ => {}
            }
        }

        false
    })
    .await
    .unwrap_or(false)
}

async fn collect_latest_stage_runs(
    state: &SharedState,
    repo: &str,
    issue_number: u64,
) -> Vec<AgentRun> {
    let s = state.lock().await;
    let stage_order = ["implement", "code_review", "testing", "merge"];

    stage_order
        .iter()
        .filter_map(|stage_name| {
            s.runs
                .values()
                .filter(|run| {
                    run.repo == repo
                        && run.issue_number == issue_number
                        && run.stage.to_string() == *stage_name
                })
                .max_by_key(|run| run.started_at.clone())
                .cloned()
        })
        .collect()
}

async fn collect_usage_totals(state: &SharedState, repo: &str, issue_number: u64) -> UsageTotals {
    let s = state.lock().await;

    aggregate_usage_totals(s.runs.values().filter(|run| {
        run.repo == repo && run.issue_number == issue_number && run.stage != PipelineStage::Done
    }))
}

fn aggregate_usage_totals<'a>(runs: impl IntoIterator<Item = &'a AgentRun>) -> UsageTotals {
    runs.into_iter().fold(
        UsageTotals {
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
        },
        |totals, run| UsageTotals {
            input_tokens: totals.input_tokens + run.input_tokens,
            output_tokens: totals.output_tokens + run.output_tokens,
            cost_usd: totals.cost_usd + run.cost_usd,
        },
    )
}

fn build_done_run(
    completion: &PipelineCompletionSpec,
    stage_runs: Vec<AgentRun>,
    usage_totals: UsageTotals,
) -> (AgentRun, crate::report::PipelineReport) {
    let done_id = Uuid::new_v4().to_string();
    let mut aggregated_logs = Vec::new();
    let stage_order = ["implement", "code_review", "testing", "merge"];

    for stage_name in &stage_order {
        if let Some(run) = stage_runs
            .iter()
            .find(|run| run.stage.to_string() == *stage_name)
        {
            aggregated_logs.push(format!(
                "═══ {} ═══",
                stage_name.to_uppercase().replace('_', " ")
            ));
            aggregated_logs.extend(run.logs.iter().cloned());
            aggregated_logs.push(String::new());
        }
    }
    aggregated_logs.push("═══ PIPELINE COMPLETED ═══".to_string());

    let stage_refs: Vec<&AgentRun> = stage_runs.iter().collect();
    let pipeline_report = report::generate_report(
        completion.issue_number,
        &completion.issue_title,
        &completion.repo,
        stage_refs,
    );

    let done_run = AgentRun {
        id: done_id,
        repo: completion.repo.clone(),
        issue_number: completion.issue_number,
        issue_title: completion.issue_title.clone(),
        status: AgentStatus::Completed,
        stage: PipelineStage::Done,
        started_at: Utc::now().to_rfc3339(),
        finished_at: Some(Utc::now().to_rfc3339()),
        logs: aggregated_logs,
        workspace_path: completion.workspace_path.to_string_lossy().to_string(),
        error: None,
        attempt: 1,
        max_retries: 0,
        lines_added: 0,
        lines_removed: 0,
        files_modified_list: Vec::new(),
        report: Some(pipeline_report.clone()),
        command_display: None,
        agent_type: String::new(),
        last_log_line: None,
        log_count: 0,
        activity: Some("Completed".to_string()),
        input_tokens: usage_totals.input_tokens,
        output_tokens: usage_totals.output_tokens,
        cost_usd: usage_totals.cost_usd,
        last_log_timestamp: None,
        issue_labels: completion.issue_labels.clone(),
        skipped_stages: completion
            .skipped_stages
            .iter()
            .map(ToString::to_string)
            .collect(),
        stage_context: None,
        pending_next_stage: None,
    };

    (done_run, pipeline_report)
}

fn detect_pr_number_from_logs(logs: &[String], repo: &str) -> Option<u64> {
    let pr_url_suffix = format!("{}/pull/", repo);
    for line in logs.iter().rev() {
        if let Some(position) = line.find(&pr_url_suffix) {
            let after = &line[position + pr_url_suffix.len()..];
            let number: String = after
                .chars()
                .take_while(|character| character.is_ascii_digit())
                .collect();
            if let Ok(pr_number) = number.parse::<u64>() {
                return Some(pr_number);
            }
        }

        let line_lower = line.to_lowercase();
        for prefix in ["pull request #", "pr #"] {
            if let Some(position) = line_lower.find(prefix) {
                let after = &line[position + prefix.len()..];
                let number: String = after
                    .chars()
                    .take_while(|character| character.is_ascii_digit())
                    .collect();
                if let Ok(pr_number) = number.parse::<u64>() {
                    return Some(pr_number);
                }
            }
        }
    }
    None
}

fn detect_branch_from_logs(logs: &[String]) -> Option<String> {
    for line in logs.iter().rev() {
        if line.contains("headRefName") {
            if let Some(position) = line.find("headRefName") {
                let after = &line[position..];
                if let Some(colon_position) = after.find(':') {
                    let value_part = after[colon_position + 1..]
                        .trim()
                        .trim_matches(|character| {
                            character == '"' || character == ',' || character == ' '
                        });
                    let branch: String = value_part
                        .chars()
                        .take_while(|character| {
                            !character.is_whitespace() && *character != '"' && *character != ','
                        })
                        .collect();
                    if !branch.is_empty() {
                        return Some(branch);
                    }
                }
            }
        }

        if let Some(position) = line.find("checkout -b ") {
            let after = &line[position + "checkout -b ".len()..];
            let branch: String = after
                .chars()
                .take_while(|character| !character.is_whitespace())
                .collect();
            if !branch.is_empty() {
                return Some(branch);
            }
        }
    }
    None
}

fn build_stage_summary(stage: &PipelineStage, logs: &[String]) -> String {
    match stage {
        PipelineStage::Implement => {
            let mut commits = Vec::new();
            for line in logs {
                if line.contains("git commit") || line.contains("Commit") {
                    commits.push(line.chars().take(100).collect::<String>());
                }
            }
            if commits.is_empty() {
                "Implementation completed.".to_string()
            } else {
                commits.into_iter().take(3).collect::<Vec<_>>().join("; ")
            }
        }
        PipelineStage::CodeReview => {
            let mut findings = Vec::new();
            for line in logs {
                let lower = line.to_lowercase();
                if lower.contains("issue")
                    || lower.contains("fix")
                    || lower.contains("bug")
                    || lower.contains("suggestion")
                    || lower.contains("approved")
                    || lower.contains("review completed")
                {
                    findings.push(line.chars().take(100).collect::<String>());
                }
            }
            if findings.is_empty() {
                "Code review completed.".to_string()
            } else {
                findings.into_iter().take(3).collect::<Vec<_>>().join("; ")
            }
        }
        PipelineStage::Testing => {
            let mut results = Vec::new();
            for line in logs {
                let lower = line.to_lowercase();
                if lower.contains("pass")
                    || lower.contains("fail")
                    || lower.contains("test")
                    || lower.contains("error")
                    || lower.contains("ok")
                {
                    results.push(line.chars().take(100).collect::<String>());
                }
            }
            if results.is_empty() {
                "Testing completed.".to_string()
            } else {
                results
                    .into_iter()
                    .rev()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join("; ")
            }
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        aggregate_usage_totals, build_done_run, decide_failure_action,
        decide_successful_stage_action, FailureAction, MergeVerification, PipelineCompletionSpec,
        SuccessfulStageAction, UsageTotals,
    };
    use crate::orchestrator::{AgentRun, AgentStatus, PipelineStage, RunConfig};
    use std::path::PathBuf;

    fn sample_run(
        id: &str,
        stage: PipelineStage,
        started_at: &str,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: f64,
    ) -> AgentRun {
        AgentRun {
            id: id.to_string(),
            repo: "pedrocid/SymphonyMac".to_string(),
            issue_number: 57,
            issue_title: "Split agent.rs".to_string(),
            status: AgentStatus::Completed,
            stage,
            started_at: started_at.to_string(),
            finished_at: Some(started_at.to_string()),
            logs: vec![format!("log {}", id)],
            workspace_path: "/tmp/workspace".to_string(),
            error: None,
            attempt: 1,
            max_retries: 1,
            lines_added: 0,
            lines_removed: 0,
            files_modified_list: Vec::new(),
            report: None,
            command_display: None,
            agent_type: "claude".to_string(),
            last_log_line: None,
            log_count: 0,
            activity: None,
            input_tokens,
            output_tokens,
            cost_usd,
            last_log_timestamp: None,
            issue_labels: Vec::new(),
            skipped_stages: Vec::new(),
            stage_context: None,
            pending_next_stage: None,
        }
    }

    #[test]
    fn implementation_can_skip_review_and_advance_to_testing() {
        let action = decide_successful_stage_action(
            &PipelineStage::Implement,
            &[PipelineStage::CodeReview],
            false,
            MergeVerification::NotRequired,
        );

        assert_eq!(
            action,
            SuccessfulStageAction::Advance {
                next_stage: PipelineStage::Testing,
                skipped_logs: vec!["[pipeline] Skipping code_review stage (label rule)".to_string()],
            }
        );
    }

    #[test]
    fn approval_gate_pauses_after_successful_stage() {
        let action = decide_successful_stage_action(
            &PipelineStage::CodeReview,
            &[],
            true,
            MergeVerification::NotRequired,
        );

        assert_eq!(
            action,
            SuccessfulStageAction::AwaitingApproval {
                next_stage: PipelineStage::Testing,
                skipped_logs: Vec::new(),
            }
        );
    }

    #[test]
    fn merge_requires_verified_pr() {
        let action = decide_successful_stage_action(
            &PipelineStage::Merge,
            &[],
            false,
            MergeVerification::NotMerged,
        );

        assert_eq!(action, SuccessfulStageAction::MergeBlocked);
    }

    #[test]
    fn verified_merge_finishes_pipeline() {
        let action = decide_successful_stage_action(
            &PipelineStage::Merge,
            &[],
            true,
            MergeVerification::Verified,
        );

        assert_eq!(action, SuccessfulStageAction::FinishPipeline);
    }

    #[test]
    fn done_stage_does_not_advance() {
        let action = decide_successful_stage_action(
            &PipelineStage::Done,
            &[],
            false,
            MergeVerification::NotRequired,
        );

        assert_eq!(action, SuccessfulStageAction::Noop);
    }

    #[test]
    fn retry_action_uses_exponential_backoff() {
        let config = RunConfig {
            retry_base_delay_secs: 5,
            retry_max_backoff_secs: 30,
            ..RunConfig::default()
        };

        assert_eq!(
            decide_failure_action(2, 3, &config),
            FailureAction::Retry {
                next_attempt: 3,
                backoff_secs: 10,
            }
        );
    }

    #[test]
    fn retry_action_falls_back_to_legacy_retry_backoff_setting() {
        let config = RunConfig {
            retry_base_delay_secs: 0,
            retry_backoff_secs: 7,
            retry_max_backoff_secs: 30,
            ..RunConfig::default()
        };

        assert_eq!(
            decide_failure_action(1, 3, &config),
            FailureAction::Retry {
                next_attempt: 2,
                backoff_secs: 7,
            }
        );
    }

    #[test]
    fn retry_action_stops_after_max_attempts() {
        let config = RunConfig::default();
        assert_eq!(
            decide_failure_action(3, 2, &config),
            FailureAction::Exhausted
        );
    }

    #[test]
    fn aggregate_usage_totals_counts_all_attempts_and_ignores_done_stage() {
        let runs = vec![
            sample_run(
                "attempt-1",
                PipelineStage::Implement,
                "2026-03-08T10:00:00Z",
                100,
                40,
                1.5,
            ),
            sample_run(
                "attempt-2",
                PipelineStage::Implement,
                "2026-03-08T10:05:00Z",
                250,
                90,
                2.25,
            ),
            sample_run(
                "testing",
                PipelineStage::Testing,
                "2026-03-08T10:10:00Z",
                30,
                15,
                0.5,
            ),
            sample_run(
                "done",
                PipelineStage::Done,
                "2026-03-08T10:15:00Z",
                999,
                999,
                9.9,
            ),
        ];

        let totals =
            aggregate_usage_totals(runs.iter().filter(|run| run.stage != PipelineStage::Done));

        assert_eq!(
            totals,
            UsageTotals {
                input_tokens: 380,
                output_tokens: 145,
                cost_usd: 4.25,
            }
        );
    }

    #[test]
    fn build_done_run_uses_aggregate_usage_totals() {
        let completion = PipelineCompletionSpec {
            repo: "pedrocid/SymphonyMac".to_string(),
            issue_number: 57,
            issue_title: "Split agent.rs".to_string(),
            workspace_path: PathBuf::from("/tmp/workspace"),
            issue_labels: vec!["refactor".to_string()],
            skipped_stages: vec![PipelineStage::CodeReview],
        };
        let latest_stage_runs = vec![
            sample_run(
                "implement-latest",
                PipelineStage::Implement,
                "2026-03-08T10:05:00Z",
                250,
                90,
                2.25,
            ),
            sample_run(
                "testing",
                PipelineStage::Testing,
                "2026-03-08T10:10:00Z",
                30,
                15,
                0.5,
            ),
        ];

        let (done_run, _) = build_done_run(
            &completion,
            latest_stage_runs,
            UsageTotals {
                input_tokens: 380,
                output_tokens: 145,
                cost_usd: 4.25,
            },
        );

        assert_eq!(done_run.input_tokens, 380);
        assert_eq!(done_run.output_tokens, 145);
        assert_eq!(done_run.cost_usd, 4.25);
        assert_eq!(done_run.skipped_stages, vec!["code_review".to_string()]);
    }
}
