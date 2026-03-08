use super::pipeline::{
    self, AgentProcessRequest, FailureAction, MergeVerification, PipelineCompletionSpec,
    StageLaunchSpec, SuccessfulStageAction,
};
use super::runtime::{self, PendingNextStageUpdate, StatusTransition};
use crate::orchestrator::{AgentStatus, PipelineStage};
use crate::workspace;
use crate::SharedState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Notify;

pub(crate) async fn run_agent_process(
    app: AppHandle,
    state: SharedState,
    request: AgentProcessRequest,
) {
    let stage_label = request.spec.stage.to_string();
    let path_env = crate::paths::build_path_env();
    let hooks = {
        let s = state.lock().await;
        s.config.hooks.clone()
    };

    if let Some(command) = hooks.before_run.as_ref() {
        if let Err(error) = workspace::execute_hook_async(
            "before_run",
            command,
            &request.spec.workspace_path,
            hooks.timeout_secs,
        )
        .await
        {
            let error_message = format!("before_run hook failed: {}", error);
            let mut emit_extra = Map::new();
            emit_extra.insert("error".to_string(), json!(error_message.clone()));
            let _ = runtime::transition_run(
                &app,
                &state,
                &request.run_id,
                StatusTransition {
                    status: AgentStatus::Failed,
                    stage_label: stage_label.clone(),
                    error: Some(error_message.clone()),
                    finished: true,
                    log_message: Some(error_message),
                    pending_next_stage: PendingNextStageUpdate::Keep,
                    emit_extra,
                    persist_meta: true,
                },
            )
            .await;
            runtime::update_dock_badge(&state).await;
            return;
        }
    }

    let _ = runtime::transition_run(
        &app,
        &state,
        &request.run_id,
        StatusTransition {
            status: AgentStatus::Running,
            stage_label: stage_label.clone(),
            error: None,
            finished: false,
            log_message: None,
            pending_next_stage: PendingNextStageUpdate::Keep,
            emit_extra: Map::new(),
            persist_meta: false,
        },
    )
    .await;

    let gh_token = std::process::Command::new(crate::paths::resolve("gh"))
        .args(["auth", "token"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|token| !token.is_empty());

    let mut command = Command::new(&request.command);
    command
        .args(&request.args)
        .current_dir(&request.spec.workspace_path)
        .env("PATH", &path_env)
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_SESSION")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    if let Some(token) = gh_token.as_ref() {
        command.env("GH_TOKEN", token);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let error_message = format!("Failed to spawn {}: {}", request.command, error);
            let mut emit_extra = Map::new();
            emit_extra.insert("error".to_string(), json!(error_message.clone()));
            let _ = runtime::transition_run(
                &app,
                &state,
                &request.run_id,
                StatusTransition {
                    status: AgentStatus::Failed,
                    stage_label: stage_label.clone(),
                    error: Some(error_message.clone()),
                    finished: true,
                    log_message: Some(error_message),
                    pending_next_stage: PendingNextStageUpdate::Keep,
                    emit_extra,
                    persist_meta: true,
                },
            )
            .await;

            let config = {
                let s = state.lock().await;
                s.config.clone()
            };
            if config.notifications_enabled {
                crate::notification::notify_pipeline_failed(
                    &app,
                    request.spec.issue_number,
                    &stage_label,
                    config.notification_sound,
                );
            }
            runtime::update_dock_badge(&state).await;
            return;
        }
    };

    if let Some(pid) = child.id() {
        let mut s = state.lock().await;
        s.agent_pids.insert(request.run_id.clone(), pid);
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stall_notify = Arc::new(Notify::new());

    let stdout_state = state.clone();
    let stdout_app = app.clone();
    let stdout_run_id = request.run_id.clone();
    let stdout_notify = stall_notify.clone();
    let stdout_handle = tokio::spawn(async move {
        if let Some(stdout) = stdout {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let display_lines = parse_stream_json_event(&line);
                let token_usage = parse_token_usage(&line);

                for display_line in display_lines {
                    runtime::record_output_line(
                        &stdout_app,
                        &stdout_state,
                        &stdout_run_id,
                        display_line.clone(),
                        detect_activity(&display_line),
                    )
                    .await;
                }

                stdout_notify.notify_one();

                if let Some(usage) = token_usage.as_ref() {
                    let mut s = stdout_state.lock().await;
                    if let Some(run) = s.runs.get_mut(&stdout_run_id) {
                        run.input_tokens += usage.input_tokens;
                        run.output_tokens += usage.output_tokens;
                        run.cost_usd += usage.cost_usd;
                    }
                    s.total_input_tokens += usage.input_tokens;
                    s.total_output_tokens += usage.output_tokens;
                    s.total_cost_usd += usage.cost_usd;
                    s.total_runtime_secs += usage.duration_secs;
                }
            }
        }
    });

    let stderr_state = state.clone();
    let stderr_app = app.clone();
    let stderr_run_id = request.run_id.clone();
    let stderr_notify = stall_notify.clone();
    let stderr_handle = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                runtime::record_output_line(
                    &stderr_app,
                    &stderr_state,
                    &stderr_run_id,
                    format!("[stderr] {}", line),
                    None,
                )
                .await;
                stderr_notify.notify_one();
            }
        }
    });

    let stall_state = state.clone();
    let stall_app = app.clone();
    let stall_run_id = request.run_id.clone();
    let stall_stage_label = stage_label.clone();
    let stall_notify_watch = stall_notify.clone();
    let stall_handle = tokio::spawn(async move {
        let stall_timeout = {
            let s = stall_state.lock().await;
            s.config.stall_timeout_secs
        };
        if stall_timeout == 0 {
            return false;
        }

        let timeout_duration = tokio::time::Duration::from_secs(stall_timeout);

        loop {
            let timed_out = tokio::time::timeout(timeout_duration, stall_notify_watch.notified())
                .await
                .is_err();

            if timed_out {
                let is_running = {
                    let s = stall_state.lock().await;
                    s.runs
                        .get(&stall_run_id)
                        .map(|run| run.status == AgentStatus::Running)
                        .unwrap_or(false)
                };

                if !is_running {
                    return false;
                }

                {
                    let mut s = stall_state.lock().await;
                    if let Some(pid) = s.agent_pids.remove(&stall_run_id) {
                        unsafe {
                            libc::kill(pid as i32, libc::SIGTERM);
                        }
                    }
                }

                let error_message = format!("Agent stalled: no output for {}s", stall_timeout);
                let stall_message = format!(
                    "⚠ Agent killed: no output for {}s (stall timeout)",
                    stall_timeout
                );
                runtime::append_run_log(
                    &stall_state,
                    &stall_run_id,
                    stall_message,
                    false,
                    true,
                )
                .await;

                let mut emit_extra = Map::new();
                emit_extra.insert("error".to_string(), json!(error_message.clone()));
                let _ = runtime::transition_run(
                    &stall_app,
                    &stall_state,
                    &stall_run_id,
                    StatusTransition {
                        status: AgentStatus::Failed,
                        stage_label: stall_stage_label.clone(),
                        error: Some(error_message),
                        finished: true,
                        log_message: None,
                        pending_next_stage: PendingNextStageUpdate::Keep,
                        emit_extra,
                        persist_meta: true,
                    },
                )
                .await;

                return true;
            }

            let is_running = {
                let s = stall_state.lock().await;
                s.runs
                    .get(&stall_run_id)
                    .map(|run| run.status == AgentStatus::Running)
                    .unwrap_or(false)
            };
            if !is_running {
                return false;
            }
        }
    });

    let exit_status = child.wait().await;

    stall_handle.abort();
    let stalled = stall_handle.await.unwrap_or(false);

    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    if stalled {
        run_after_run_hook(&state, &request.run_id, &request.spec.workspace_path).await;
        handle_failed_attempt(&app, &state, &request, &stage_label).await;
        return;
    }

    let succeeded = exit_status.as_ref().map(|status| status.success()).unwrap_or(false);

    {
        let mut s = state.lock().await;
        s.agent_pids.remove(&request.run_id);
    }

    let mut emit_extra = Map::new();
    if !succeeded {
        let error_message = match &exit_status {
            Ok(status) => format!("Agent exited with code: {}", status.code().unwrap_or(-1)),
            Err(error) => format!("Agent process error: {}", error),
        };
        emit_extra.insert("error".to_string(), json!(error_message.clone()));
        let _ = runtime::transition_run(
            &app,
            &state,
            &request.run_id,
            StatusTransition {
                status: AgentStatus::Failed,
                stage_label: stage_label.clone(),
                error: Some(error_message),
                finished: true,
                log_message: None,
                pending_next_stage: PendingNextStageUpdate::Keep,
                emit_extra,
                persist_meta: true,
            },
        )
        .await;
    } else {
        let _ = runtime::transition_run(
            &app,
            &state,
            &request.run_id,
            StatusTransition {
                status: AgentStatus::Completed,
                stage_label: stage_label.clone(),
                error: None,
                finished: true,
                log_message: None,
                pending_next_stage: PendingNextStageUpdate::Keep,
                emit_extra,
                persist_meta: true,
            },
        )
        .await;
    }

    if succeeded {
        let (lines_added, lines_removed, files_modified) =
            capture_diff_stats(&request.spec.workspace_path).await;
        let _ = runtime::mutate_run(&state, &request.run_id, true, move |run| {
            run.lines_added = lines_added;
            run.lines_removed = lines_removed;
            run.files_modified_list = files_modified.clone();
        })
        .await;
    }

    run_after_run_hook(&state, &request.run_id, &request.spec.workspace_path).await;

    if !succeeded {
        handle_failed_attempt(&app, &state, &request, &stage_label).await;
        return;
    }

    let stage_context = {
        let s = state.lock().await;
        s.runs
            .get(&request.run_id)
            .map(|run| pipeline::extract_stage_context(run, &request.spec.repo))
    };

    if let Some(ref context) = stage_context {
        let stored_context = context.clone();
        let _ = runtime::mutate_run(&state, &request.run_id, true, move |run| {
            run.stage_context = Some(stored_context);
        })
        .await;
    }

    let skipped_stages = {
        let s = state.lock().await;
        crate::orchestrator::compute_skipped_stages(
            &request.spec.issue_labels,
            &s.config.stage_skip_labels,
        )
    };

    let merge_verification = match request.spec.stage {
        PipelineStage::Merge => match crate::github::is_pr_merged_for_issue(
            &request.spec.repo,
            request.spec.issue_number,
        ) {
            Ok(true) => MergeVerification::Verified,
            Ok(false) => MergeVerification::NotMerged,
            Err(error) => {
                let warning = format!(
                    "[warning] Could not verify merge status: {}. Proceeding to Done.",
                    error
                );
                runtime::append_run_log(&state, &request.run_id, warning, false, true).await;
                MergeVerification::Unknown
            }
        },
        _ => MergeVerification::NotRequired,
    };

    let gate_enabled = {
        let s = state.lock().await;
        crate::orchestrator::is_gate_enabled(&s.config, &request.spec.stage)
    };

    match pipeline::decide_successful_stage_action(
        &request.spec.stage,
        &skipped_stages,
        gate_enabled,
        merge_verification,
    ) {
        SuccessfulStageAction::Advance {
            next_stage,
            skipped_logs,
        } => {
            for message in skipped_logs {
                runtime::append_run_log(&state, &request.run_id, message, false, true).await;
            }

            let next_spec = next_stage_spec(&request.spec, next_stage, String::new(), stage_context);
            pipeline::spawn_next_stage(app.clone(), state.clone(), next_spec);
        }
        SuccessfulStageAction::AwaitingApproval {
            next_stage,
            skipped_logs,
        } => {
            for message in skipped_logs {
                runtime::append_run_log(&state, &request.run_id, message, false, true).await;
            }

            let next_stage_name = next_stage.to_string();
            let pause_message = format!(
                "[pipeline] Approval gate: paused after {} stage. Awaiting user approval to proceed to {}.",
                request.spec.stage, next_stage_name
            );
            let mut approval_extra = Map::new();
            approval_extra.insert(
                "pending_next_stage".to_string(),
                json!(next_stage_name.clone()),
            );
            let _ = runtime::transition_run(
                &app,
                &state,
                &request.run_id,
                StatusTransition {
                    status: AgentStatus::AwaitingApproval,
                    stage_label: stage_label.clone(),
                    error: None,
                    finished: false,
                    log_message: Some(pause_message),
                    pending_next_stage: PendingNextStageUpdate::Set(next_stage_name.clone()),
                    emit_extra: approval_extra,
                    persist_meta: true,
                },
            )
            .await;

            let config = {
                let s = state.lock().await;
                s.config.clone()
            };
            if config.notifications_enabled {
                crate::notification::notify_awaiting_approval(
                    &app,
                    request.spec.issue_number,
                    &stage_label,
                    config.notification_sound,
                );
            }
            runtime::update_dock_badge(&state).await;
        }
        SuccessfulStageAction::FinishPipeline => {
            pipeline::finish_pipeline(
                &app,
                &state,
                PipelineCompletionSpec {
                    repo: request.spec.repo.clone(),
                    issue_number: request.spec.issue_number,
                    issue_title: request.spec.issue_title.clone(),
                    workspace_path: request.spec.workspace_path.clone(),
                    issue_labels: request.spec.issue_labels.clone(),
                    skipped_stages,
                },
            )
            .await;
        }
        SuccessfulStageAction::MergeBlocked => {
            let error_message = format!(
                "PR for issue #{} was not merged (possible merge conflicts). The PR needs manual conflict resolution.",
                request.spec.issue_number
            );
            let mut merge_extra = Map::new();
            merge_extra.insert("error".to_string(), json!(error_message.clone()));
            let _ = runtime::transition_run(
                &app,
                &state,
                &request.run_id,
                StatusTransition {
                    status: AgentStatus::Failed,
                    stage_label: "merge".to_string(),
                    error: Some(error_message),
                    finished: false,
                    log_message: None,
                    pending_next_stage: PendingNextStageUpdate::Keep,
                    emit_extra: merge_extra,
                    persist_meta: true,
                },
            )
            .await;

            let config = {
                let s = state.lock().await;
                s.config.clone()
            };
            if config.notifications_enabled {
                crate::notification::notify_pipeline_failed(
                    &app,
                    request.spec.issue_number,
                    "merge",
                    config.notification_sound,
                );
            }
            runtime::update_dock_badge(&state).await;
        }
        SuccessfulStageAction::Noop => {}
    }
}

/// Detect the current agent activity from a log line.
fn detect_activity(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if lower.contains("reading file") || lower.contains("read(") || lower.contains("cat ") {
        Some("Reading files".to_string())
    } else if lower.contains("editing file")
        || lower.contains("edit(")
        || lower.contains("write(")
        || lower.contains("sed ")
    {
        Some("Editing files".to_string())
    } else if lower.contains("bash(") || lower.contains("running command") || lower.contains("$ ")
    {
        Some("Running command".to_string())
    } else if lower.contains("grep(") || lower.contains("glob(") || lower.contains("searching") {
        Some("Searching code".to_string())
    } else if lower.contains("git commit")
        || lower.contains("git push")
        || lower.contains("gh pr")
    {
        Some("Git operations".to_string())
    } else if lower.contains("npm test")
        || lower.contains("cargo test")
        || lower.contains("pytest")
        || lower.contains("go test")
    {
        Some("Running tests".to_string())
    } else if lower.contains("compiling")
        || lower.contains("building")
        || lower.contains("npm run build")
    {
        Some("Building".to_string())
    } else if lower.contains("analyzing") || lower.contains("reviewing") {
        Some("Analyzing code".to_string())
    } else {
        None
    }
}

/// Parse a stream-json event line from Claude CLI into human-readable log lines.
fn parse_stream_json_event(raw: &str) -> Vec<String> {
    let parsed: serde_json::Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(_) => return vec![raw.to_string()],
    };

    match parsed["type"].as_str().unwrap_or("") {
        "system" => {
            let model = parsed["model"].as_str().unwrap_or("unknown");
            vec![format!("🔧 Session initialized (model: {})", model)]
        }
        "assistant" => {
            let content = match parsed["message"]["content"].as_array() {
                Some(content) => content,
                None => return vec![],
            };
            let mut lines = Vec::new();
            for item in content {
                match item["type"].as_str().unwrap_or("") {
                    "tool_use" => {
                        let tool = item["name"].as_str().unwrap_or("unknown");
                        let input = &item["input"];
                        let detail = match tool {
                            "Read" => {
                                let path = input["file_path"].as_str().unwrap_or("");
                                let short = path.rsplit('/').next().unwrap_or(path);
                                format!("Reading file: {}", short)
                            }
                            "Edit" => {
                                let path = input["file_path"].as_str().unwrap_or("");
                                let short = path.rsplit('/').next().unwrap_or(path);
                                format!("Editing file: {}", short)
                            }
                            "Write" => {
                                let path = input["file_path"].as_str().unwrap_or("");
                                let short = path.rsplit('/').next().unwrap_or(path);
                                format!("Writing file: {}", short)
                            }
                            "Bash" => {
                                let command = input["command"]
                                    .as_str()
                                    .unwrap_or("")
                                    .chars()
                                    .take(120)
                                    .collect::<String>();
                                format!("$ {}", command)
                            }
                            "Grep" => {
                                let pattern = input["pattern"].as_str().unwrap_or("");
                                format!("Searching for: {}", pattern)
                            }
                            "Glob" => {
                                let pattern = input["pattern"].as_str().unwrap_or("");
                                format!("Finding files: {}", pattern)
                            }
                            _ => format!("{}: {}", tool, truncate_json(input, 100)),
                        };
                        lines.push(format!("→ {}", detail));
                    }
                    "text" => {
                        let text = item["text"].as_str().unwrap_or("");
                        if !text.trim().is_empty() {
                            lines.push(format!("💬 {}", text.chars().take(200).collect::<String>()));
                        }
                    }
                    _ => {}
                }
            }
            lines
        }
        "user" => {
            let content = match parsed["message"]["content"].as_array() {
                Some(content) => content,
                None => return vec![],
            };
            let mut lines = Vec::new();
            for item in content {
                if item["type"].as_str() == Some("tool_result") {
                    let result_text = item["content"].as_str().unwrap_or("");
                    let line_count = result_text.lines().count();
                    if line_count > 0 {
                        lines.push(format!("  ✓ Result ({} lines)", line_count));
                    }
                }
            }
            lines
        }
        "result" => {
            let duration_ms = parsed["duration_ms"].as_u64().unwrap_or(0);
            let turns = parsed["num_turns"].as_u64().unwrap_or(0);
            let cost = parsed["total_cost_usd"].as_f64().unwrap_or(0.0);
            let success = parsed["subtype"].as_str() == Some("success");
            let status = if success { "completed" } else { "failed" };
            vec![format!(
                "✅ Agent {} in {:.1}s ({} turns, ${:.4})",
                status,
                duration_ms as f64 / 1000.0,
                turns,
                cost
            )]
        }
        "rate_limit_event" => vec![],
        _ => vec![],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenUsage {
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: f64,
    duration_secs: f64,
}

fn parse_token_usage(raw: &str) -> Option<TokenUsage> {
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    if parsed["type"].as_str() != Some("result") {
        return None;
    }
    let usage = &parsed["usage"];
    Some(TokenUsage {
        input_tokens: usage["input_tokens"].as_u64().unwrap_or(0),
        output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
        cost_usd: parsed["total_cost_usd"].as_f64().unwrap_or(0.0),
        duration_secs: parsed["duration_ms"].as_u64().unwrap_or(0) as f64 / 1000.0,
    })
}

fn truncate_json(value: &serde_json::Value, max_len: usize) -> String {
    let rendered = value.to_string();
    if rendered.len() > max_len {
        format!("{}...", rendered.chars().take(max_len).collect::<String>())
    } else {
        rendered
    }
}

/// Run `git diff --stat origin/main...HEAD` in the workspace and parse the output.
async fn capture_diff_stats(workspace_path: &Path) -> (u32, u32, Vec<String>) {
    let output = Command::new("git")
        .args(["diff", "--stat", "origin/main...HEAD"])
        .current_dir(workspace_path)
        .env("PATH", crate::paths::build_path_env())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .output()
        .await;

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            parse_git_diff_stat(&stdout)
        }
        _ => (0, 0, Vec::new()),
    }
}

fn parse_git_diff_stat(output: &str) -> (u32, u32, Vec<String>) {
    let mut added = 0;
    let mut removed = 0;
    let mut files = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains("changed")
            && (trimmed.contains("insertion") || trimmed.contains("deletion"))
        {
            for part in trimmed.split(',') {
                let part = part.trim();
                if part.contains("insertion") {
                    if let Some(number) = part.split_whitespace().next() {
                        added = number.parse().unwrap_or(0);
                    }
                }
                if part.contains("deletion") {
                    if let Some(number) = part.split_whitespace().next() {
                        removed = number.parse().unwrap_or(0);
                    }
                }
            }
        } else if let Some(position) = trimmed.find(" | ") {
            let file = trimmed[..position].trim().to_string();
            if !file.is_empty() {
                files.push(file);
            }
        }
    }

    (added, removed, files)
}

async fn run_after_run_hook(state: &SharedState, run_id: &str, workspace_path: &Path) {
    let hooks = {
        let s = state.lock().await;
        s.config.hooks.clone()
    };

    if let Some(command) = hooks.after_run.as_ref() {
        if let Err(error) =
            workspace::execute_hook_async("after_run", command, workspace_path, hooks.timeout_secs)
                .await
        {
            runtime::append_run_log(
                state,
                run_id,
                format!("[hook] after_run failed (ignored): {}", error),
                false,
                true,
            )
            .await;
        }
    }
}

async fn handle_failed_attempt(
    app: &AppHandle,
    state: &SharedState,
    request: &AgentProcessRequest,
    stage_label: &str,
) {
    let (config, error_log) = {
        let s = state.lock().await;
        (
            s.config.clone(),
            s.runs
                .get(&request.run_id)
                .and_then(|run| run.error.clone())
                .unwrap_or_default(),
        )
    };

    runtime::update_dock_badge(state).await;

    match pipeline::decide_failure_action(request.spec.attempt, request.spec.max_retries, &config) {
        FailureAction::Retry {
            next_attempt,
            backoff_secs,
        } => {
            let retry_spec =
                next_stage_spec(&request.spec, request.spec.stage.clone(), error_log, None)
                    .with_attempt(next_attempt);
            pipeline::spawn_retry(app.clone(), state.clone(), retry_spec, backoff_secs);
        }
        FailureAction::Exhausted => {
            if config.notifications_enabled {
                crate::notification::notify_pipeline_failed(
                    app,
                    request.spec.issue_number,
                    stage_label,
                    config.notification_sound,
                );
            }
            if config.cleanup_on_failure {
                let _ = workspace::cleanup_workspace(
                    &request.spec.repo,
                    request.spec.issue_number,
                    &config.hooks,
                );
            }
        }
    }
}

fn next_stage_spec(
    current: &StageLaunchSpec,
    stage: PipelineStage,
    previous_error: String,
    previous_context: Option<crate::orchestrator::StageContext>,
) -> StageLaunchSpec {
    StageLaunchSpec {
        repo: current.repo.clone(),
        issue_number: current.issue_number,
        issue_title: current.issue_title.clone(),
        issue_body: current.issue_body.clone(),
        stage,
        issue_labels: current.issue_labels.clone(),
        workspace_path: current.workspace_path.clone(),
        attempt: 1,
        max_retries: current.max_retries,
        previous_error,
        previous_context,
    }
}

trait AttemptSpec {
    fn with_attempt(self, attempt: u32) -> Self;
}

impl AttemptSpec for StageLaunchSpec {
    fn with_attempt(mut self, attempt: u32) -> Self {
        self.attempt = attempt;
        self.previous_context = None;
        self
    }
}
