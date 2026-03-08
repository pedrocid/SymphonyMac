use crate::logs;
use crate::orchestrator::{AgentRun, AgentStatus, PipelineStage, RunConfig};
use crate::report;
use crate::workspace;
use crate::SharedState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLogLine {
    pub run_id: String,
    pub timestamp: String,
    pub line: String,
}

fn build_prompt(
    stage: &PipelineStage,
    issue_number: u64,
    repo: &str,
    issue_title: &str,
    issue_body: &str,
) -> String {
    match stage {
        PipelineStage::Implement => format!(
            "You are working on GitHub issue #{num} in repository {repo}.\n\n\
             Title: {title}\n\n\
             Description:\n{body}\n\n\
             Instructions:\n\
             1. Analyze the issue carefully\n\
             2. Implement the fix or feature with clean, well-structured code\n\
             3. Commit your changes with a descriptive message\n\
             4. Create a Pull Request:\n\
                gh pr create --title \"Fix #{num}: {title}\" --body \"Closes #{num}\"\n\n\
             Do NOT run tests - that will be handled in a later stage.",
            num = issue_number,
            repo = repo,
            title = issue_title,
            body = issue_body.chars().take(4000).collect::<String>(),
        ),

        PipelineStage::CodeReview => format!(
            "You are a code reviewer for repository {repo}.\n\n\
             A Pull Request has been created for issue #{num}: {title}\n\n\
             Instructions:\n\
             1. Run: gh pr list --state open --json number,title,headRefName | to find the PR for issue #{num}\n\
             2. Check out the PR branch\n\
             3. Review ALL changed files carefully. Look for:\n\
                - Bugs, logic errors, edge cases\n\
                - Security issues\n\
                - Code style and readability\n\
                - Missing error handling\n\
                - Performance issues\n\
             4. If you find issues, FIX them directly in the code, commit, and push\n\
             5. If the code looks good or after fixing issues, leave a summary comment on the PR:\n\
                gh pr comment <PR_NUMBER> --body \"Code review completed. <summary of findings and fixes>\"\n\n\
             Be thorough but practical. Fix real problems, don't nitpick style.",
            repo = repo,
            num = issue_number,
            title = issue_title,
        ),

        PipelineStage::Testing => format!(
            "You are a test engineer for repository {repo}.\n\n\
             A Pull Request for issue #{num}: {title} has been reviewed and is ready for testing.\n\n\
             Issue description:\n{body}\n\n\
             Instructions:\n\
             1. Run: gh pr list --state open --json number,title,headRefName | to find the PR for issue #{num}\n\
             2. Check out the PR branch\n\
             3. Identify the project type and run the appropriate test commands:\n\
                - Node.js: npm test or npm run test\n\
                - Python: pytest or python -m pytest\n\
                - Rust: cargo test\n\
                - Go: go test ./...\n\
                - Swift: swift test\n\
                - Or check package.json / Makefile / README for test instructions\n\
             4. If tests fail:\n\
                - Analyze the failures\n\
                - Fix the issues in the code\n\
                - Commit and push the fixes\n\
                - Re-run tests to confirm they pass\n\
             5. END-TO-END TESTING (CRITICAL):\n\
                After existing tests pass, you MUST perform end-to-end validation:\n\
                a) Read the issue title and description above carefully to understand what was fixed or added.\n\
                b) If the issue describes a specific bug or feature:\n\
                   - Reproduce the original scenario described in the issue to verify the fix works end-to-end.\n\
                   - For bugs: try to trigger the original bug and confirm it no longer occurs.\n\
                   - For features: exercise the new feature through its intended usage path.\n\
                   - Use the project's actual entry points (CLI commands, API endpoints, scripts, UI) to test, not just unit tests.\n\
                c) If the issue is too abstract or there is no specific scenario to reproduce:\n\
                   - Perform a quick smoke test: build the project and run its main entry point to verify nothing is broken.\n\
                   - For web apps: start the dev server and verify it loads without errors.\n\
                   - For CLI tools: run the main command with --help or a basic invocation.\n\
                   - For libraries: run a quick import/usage check.\n\
                d) If E2E testing reveals issues, fix them, commit, push, and re-test.\n\
             6. Comment on the PR with your findings:\n\
                gh pr comment <PR_NUMBER> --body \"Testing completed. Unit tests: PASS. E2E validation: <describe what you tested and results>. Ready to merge.\"\n\n\
             Make sure ALL tests pass and E2E validation succeeds before finishing.",
            repo = repo,
            num = issue_number,
            title = issue_title,
            body = issue_body.chars().take(4000).collect::<String>(),
        ),

        PipelineStage::Merge => format!(
            "You are a release engineer for repository {repo}.\n\n\
             A Pull Request for issue #{num}: {title} has passed code review and all tests.\n\n\
             Instructions:\n\
             1. Run: gh pr list -R {repo} --state open --json number,title,headRefName | to find the PR for issue #{num}\n\
             2. Merge the PR into the default branch:\n\
                gh pr merge <PR_NUMBER> -R {repo} --merge --delete-branch\n\
             3. Close the issue if it wasn't auto-closed:\n\
                gh issue close {num} -R {repo}\n\
             4. Confirm the merge was successful by checking:\n\
                gh pr view <PR_NUMBER> -R {repo} --json state\n\n\
             Do NOT make any code changes. Only merge and close.",
            repo = repo,
            num = issue_number,
            title = issue_title,
        ),

        PipelineStage::Done => String::new(), // Should never be used
    }
}

/// Create a short display string for the command being run (truncates the prompt).
fn format_command_display(cmd: &str, args: &[String]) -> String {
    let binary = std::path::Path::new(cmd)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| cmd.to_string());
    let display_args: Vec<String> = args
        .iter()
        .map(|a| {
            if a.len() > 80 {
                let truncated: String = a.chars().take(77).collect();
                format!("\"{}...\"", truncated)
            } else if a.contains(' ') {
                format!("\"{}\"", a)
            } else {
                a.clone()
            }
        })
        .collect();
    format!("{} {}", binary, display_args.join(" "))
}

/// Detect the current agent activity from a log line.
fn detect_activity(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if lower.contains("reading file") || lower.contains("read(") || lower.contains("cat ") {
        Some("Reading files".to_string())
    } else if lower.contains("editing file") || lower.contains("edit(") || lower.contains("write(") || lower.contains("sed ") {
        Some("Editing files".to_string())
    } else if lower.contains("bash(") || lower.contains("running command") || lower.contains("$ ") {
        Some("Running command".to_string())
    } else if lower.contains("grep(") || lower.contains("glob(") || lower.contains("searching") {
        Some("Searching code".to_string())
    } else if lower.contains("git commit") || lower.contains("git push") || lower.contains("gh pr") {
        Some("Git operations".to_string())
    } else if lower.contains("npm test") || lower.contains("cargo test") || lower.contains("pytest") || lower.contains("go test") {
        Some("Running tests".to_string())
    } else if lower.contains("compiling") || lower.contains("building") || lower.contains("npm run build") {
        Some("Building".to_string())
    } else if lower.contains("analyzing") || lower.contains("reviewing") {
        Some("Analyzing code".to_string())
    } else {
        None
    }
}

fn build_command_args(config: &RunConfig, prompt: &str) -> (String, Vec<String>) {
    match config.agent_type.as_str() {
        "codex" => {
            let mut args = vec!["exec".to_string()];
            if config.auto_approve {
                args.push("--full-auto".to_string());
            }
            // Allow Codex sandbox to read gh auth config
            if let Some(home) = dirs::home_dir() {
                let gh_config = home.join(".config/gh");
                if gh_config.exists() {
                    args.push("--add-dir".to_string());
                    args.push(gh_config.to_string_lossy().to_string());
                }
            }
            args.push(prompt.to_string());
            (crate::paths::resolve("codex"), args)
        }
        _ => {
            let mut args = vec!["--print".to_string()];
            if config.auto_approve {
                args.push("--dangerously-skip-permissions".to_string());
            }
            args.push(prompt.to_string());
            (crate::paths::resolve("claude"), args)
        }
    }
}

/// Launch an agent for a specific pipeline stage.
pub async fn launch_agent(
    app: AppHandle,
    state: SharedState,
    repo: String,
    issue_number: u64,
    issue_title: String,
    issue_body: String,
    stage: PipelineStage,
) -> Result<String, String> {
    let run_id = Uuid::new_v4().to_string();
    let config = {
        let s = state.lock().await;
        s.config.clone()
    };

    let workspace_path = workspace::ensure_workspace(&repo, issue_number)?;

    let stage_label = stage.to_string();

    let max_retries = config.max_retries;

    let prompt = build_prompt(&stage, issue_number, &repo, &issue_title, &issue_body);
    let (cmd, args) = build_command_args(&config, &prompt);
    let command_display = format_command_display(&cmd, &args);

    let run = AgentRun {
        id: run_id.clone(),
        repo: repo.clone(),
        issue_number,
        issue_title: issue_title.clone(),
        status: AgentStatus::Preparing,
        stage: stage.clone(),
        started_at: Utc::now().to_rfc3339(),
        finished_at: None,
        logs: Vec::new(),
        workspace_path: workspace_path.to_string_lossy().to_string(),
        error: None,
        attempt: 1,
        max_retries,
        lines_added: 0,
        lines_removed: 0,
        files_modified_list: Vec::new(),
        report: None,
        command_display: Some(command_display.clone()),
        agent_type: config.agent_type.clone(),
        last_log_line: None,
        log_count: 0,
        activity: None,
    };

    {
        let mut s = state.lock().await;
        s.runs.insert(run_id.clone(), run);
    }

    // Persist initial metadata to disk
    logs::save_meta(&logs::LogMeta {
        run_id: run_id.clone(),
        repo: repo.clone(),
        issue_number,
        issue_title: issue_title.clone(),
        stage: stage_label.clone(),
        started_at: Utc::now().to_rfc3339(),
        finished_at: None,
        status: "preparing".to_string(),
    });

    let _ = app.emit(
        "agent-status-changed",
        serde_json::json!({
            "run_id": &run_id,
            "status": "preparing",
            "stage": &stage_label,
        }),
    );

    // Update dock badge when a new agent starts
    update_dock_badge(&state).await;

    let run_id_clone = run_id.clone();
    let state_clone = state.clone();
    let app_clone = app.clone();
    let repo_clone = repo.clone();
    let title_clone = issue_title.clone();
    let body_clone = issue_body.clone();

    tokio::spawn(async move {
        run_agent_process(
            app_clone,
            state_clone,
            run_id_clone,
            cmd,
            args,
            workspace_path,
            stage,
            repo_clone,
            issue_number,
            title_clone,
            body_clone,
        )
        .await;
    });

    Ok(run_id)
}

#[tauri::command]
pub async fn start_single_issue(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    repo: String,
    issue_number: u64,
    issue_title: String,
    issue_body: Option<String>,
) -> Result<String, String> {
    launch_agent(
        app,
        state.inner().clone(),
        repo,
        issue_number,
        issue_title,
        issue_body.unwrap_or_default(),
        PipelineStage::Implement,
    )
    .await
}

async fn run_agent_process(
    app: AppHandle,
    state: SharedState,
    run_id: String,
    cmd: String,
    args: Vec<String>,
    workspace_path: std::path::PathBuf,
    stage: PipelineStage,
    repo: String,
    issue_number: u64,
    issue_title: String,
    issue_body: String,
) {
    let stage_label = stage.to_string();
    let path_env = crate::paths::build_path_env();

    // Update status to running
    {
        let mut s = state.lock().await;
        if let Some(run) = s.runs.get_mut(&run_id) {
            run.status = AgentStatus::Running;
        }
    }
    let _ = app.emit(
        "agent-status-changed",
        serde_json::json!({
            "run_id": &run_id,
            "status": "running",
            "stage": &stage_label,
        }),
    );

    // Obtain GH_TOKEN so agents (especially Codex in sandbox) can access GitHub API
    let gh_token = std::process::Command::new(crate::paths::resolve("gh"))
        .args(["auth", "token"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|t| !t.is_empty());

    let mut command = Command::new(&cmd);
    command
        .args(&args)
        .current_dir(&workspace_path)
        .env("PATH", &path_env)
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_SESSION")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    if let Some(ref token) = gh_token {
        command.env("GH_TOKEN", token);
    }

    let result = command.spawn();

    let mut child = match result {
        Ok(c) => c,
        Err(e) => {
            let error_msg = format!("Failed to spawn {}: {}", cmd, e);
            let mut s = state.lock().await;
            if let Some(run) = s.runs.get_mut(&run_id) {
                run.status = AgentStatus::Failed;
                run.error = Some(error_msg.clone());
                run.finished_at = Some(Utc::now().to_rfc3339());
                run.logs.push(error_msg);
            }
            if s.config.notifications_enabled {
                crate::notification::notify_pipeline_failed(
                    &app,
                    issue_number,
                    &stage_label,
                    s.config.notification_sound,
                );
            }
            drop(s);
            let _ = app.emit(
                "agent-status-changed",
                serde_json::json!({
                    "run_id": &run_id,
                    "status": "failed",
                    "stage": &stage_label,
                }),
            );
            update_dock_badge(&state).await;
            return;
        }
    };

    if let Some(pid) = child.id() {
        let mut s = state.lock().await;
        s.agent_pids.insert(run_id.clone(), pid);
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let state_out = state.clone();
    let run_id_out = run_id.clone();
    let app_out = app.clone();

    let stdout_handle = tokio::spawn(async move {
        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let ts = Utc::now().to_rfc3339();
                let log_line = AgentLogLine {
                    run_id: run_id_out.clone(),
                    timestamp: ts.clone(),
                    line: line.clone(),
                };
                let _ = app_out.emit("agent-log", &log_line);
                // Persist to disk
                logs::append_log_line(&run_id_out, &line);
                let activity = detect_activity(&line);
                let mut s = state_out.lock().await;
                if let Some(run) = s.runs.get_mut(&run_id_out) {
                    run.logs.push(line.clone());
                    run.last_log_line = Some(line.clone());
                    run.log_count += 1;
                    if let Some(ref act) = activity {
                        run.activity = Some(act.clone());
                    }
                }
            }
        }
    });

    let state_err = state.clone();
    let run_id_err = run_id.clone();
    let app_err = app.clone();

    let stderr_handle = tokio::spawn(async move {
        if let Some(stderr) = stderr {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let stderr_line = format!("[stderr] {}", line);
                let log_line = AgentLogLine {
                    run_id: run_id_err.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    line: stderr_line.clone(),
                };
                let _ = app_err.emit("agent-log", &log_line);
                // Persist to disk
                logs::append_log_line(&run_id_err, &stderr_line);
                let mut s = state_err.lock().await;
                if let Some(run) = s.runs.get_mut(&run_id_err) {
                    run.logs.push(stderr_line.clone());
                    run.last_log_line = Some(stderr_line);
                    run.log_count += 1;
                }
            }
        }
    });

    let exit_status = child.wait().await;

    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    // Determine if agent succeeded
    let succeeded = match &exit_status {
        Ok(s) => s.success(),
        Err(_) => false,
    };

    let final_status;
    {
        let mut s = state.lock().await;
        s.agent_pids.remove(&run_id);
        if let Some(run) = s.runs.get_mut(&run_id) {
            run.finished_at = Some(Utc::now().to_rfc3339());
            if succeeded {
                run.status = AgentStatus::Completed;
            } else {
                run.status = AgentStatus::Failed;
                run.error = Some(match &exit_status {
                    Ok(status) => {
                        format!("Agent exited with code: {}", status.code().unwrap_or(-1))
                    }
                    Err(e) => format!("Agent process error: {}", e),
                });
            }
        }
        final_status = if succeeded { "completed" } else { "failed" };
    }

    // Update metadata on disk with final status, preserving the original started_at
    if let Some(mut meta) = logs::load_meta(&run_id) {
        meta.finished_at = Some(Utc::now().to_rfc3339());
        meta.status = final_status.to_string();
        logs::save_meta(&meta);
    } else {
        logs::save_meta(&logs::LogMeta {
            run_id: run_id.clone(),
            repo: repo.clone(),
            issue_number,
            issue_title: issue_title.clone(),
            stage: stage_label.clone(),
            started_at: Utc::now().to_rfc3339(),
            finished_at: Some(Utc::now().to_rfc3339()),
            status: final_status.to_string(),
        });
    }

    let _ = app.emit(
        "agent-status-changed",
        serde_json::json!({
            "run_id": &run_id,
            "status": if succeeded { "completed" } else { "failed" },
            "stage": &stage_label,
        }),
    );

    // After agent completes successfully, capture actual diff stats from git
    if succeeded {
        let (diff_added, diff_removed, diff_files) =
            capture_diff_stats(&workspace_path).await;
        let mut s = state.lock().await;
        if let Some(run) = s.runs.get_mut(&run_id) {
            run.lines_added = diff_added;
            run.lines_removed = diff_removed;
            run.files_modified_list = diff_files;
        }
    }

    // RETRY LOGIC: If the agent failed, check if we can retry
    if !succeeded {
        let (current_attempt, max_retries, retry_backoff, error_log) = {
            let s = state.lock().await;
            let run = s.runs.get(&run_id);
            let attempt = run.map(|r| r.attempt).unwrap_or(1);
            let max_r = run.map(|r| r.max_retries).unwrap_or(0);
            let backoff = s.config.retry_backoff_secs;
            let err = run.and_then(|r| r.error.clone()).unwrap_or_default();
            (attempt, max_r, backoff, err)
        };

        if current_attempt <= max_retries {
            // We can retry - spawn a retry attempt
            spawn_retry(
                app.clone(),
                state.clone(),
                repo,
                issue_number,
                issue_title,
                issue_body,
                stage,
                workspace_path,
                current_attempt + 1,
                max_retries,
                retry_backoff,
                error_log,
            );
            return;
        }

        // No more retries - notify failure
        let s = state.lock().await;
        if s.config.notifications_enabled {
            crate::notification::notify_pipeline_failed(
                &app,
                issue_number,
                &stage_label,
                s.config.notification_sound,
            );
        }
        let should_cleanup = s.config.cleanup_on_failure;
        drop(s);
        update_dock_badge(&state).await;
        if should_cleanup {
            let _ = workspace::cleanup_workspace(&repo, issue_number);
        }
    }

    // AUTO-CHAIN: If the stage completed successfully, advance to the next stage
    if succeeded {
        let next_stage = match stage {
            PipelineStage::Implement => Some(PipelineStage::CodeReview),
            PipelineStage::CodeReview => Some(PipelineStage::Testing),
            PipelineStage::Testing => Some(PipelineStage::Merge),
            PipelineStage::Merge => {
                // Merge passed → mark as Done with aggregated logs and enriched report
                let done_id = Uuid::new_v4().to_string();
                let (aggregated_logs, pipeline_report) = {
                    let s = state.lock().await;
                    let stage_order = ["implement", "code_review", "testing", "merge"];
                    let mut all_logs: Vec<String> = Vec::new();
                    let mut stage_runs_for_report: Vec<&AgentRun> = Vec::new();

                    for stage_name in &stage_order {
                        let stage_runs: Vec<&AgentRun> = s
                            .runs
                            .values()
                            .filter(|r| {
                                r.issue_number == issue_number && r.stage.to_string() == *stage_name
                            })
                            .collect();
                        if let Some(run) =
                            stage_runs.into_iter().max_by_key(|r| r.started_at.clone())
                        {
                            all_logs.push(format!(
                                "═══ {} ═══",
                                stage_name.to_uppercase().replace('_', " ")
                            ));
                            all_logs.extend(run.logs.iter().cloned());
                            all_logs.push(String::new());
                            stage_runs_for_report.push(run);
                        }
                    }
                    all_logs.push("═══ PIPELINE COMPLETED ═══".to_string());

                    let report = report::generate_report(
                        issue_number,
                        &issue_title,
                        &repo,
                        stage_runs_for_report,
                    );
                    (all_logs, report)
                };

                let done_run = AgentRun {
                    id: done_id.clone(),
                    repo: repo.clone(),
                    issue_number,
                    issue_title: issue_title.clone(),
                    status: AgentStatus::Completed,
                    stage: PipelineStage::Done,
                    started_at: Utc::now().to_rfc3339(),
                    finished_at: Some(Utc::now().to_rfc3339()),
                    logs: aggregated_logs,
                    workspace_path: workspace_path.to_string_lossy().to_string(),
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
                };
                {
                    let mut s = state.lock().await;
                    s.runs.insert(done_id.clone(), done_run);
                }
                let _ = app.emit(
                    "agent-status-changed",
                    serde_json::json!({
                        "run_id": &done_id,
                        "status": "completed",
                        "stage": "done",
                    }),
                );
                let _ = app.emit("pipeline-report", &pipeline_report);

                // Send notification for pipeline completion
                {
                    let s = state.lock().await;
                    if s.config.notifications_enabled {
                        crate::notification::notify_pipeline_done(
                            &app,
                            issue_number,
                            &issue_title,
                            s.config.notification_sound,
                        );
                    }
                }

                // Update dock badge
                update_dock_badge(&state).await;

                // Clean up the workspace clone
                let _ = workspace::cleanup_workspace(&repo, issue_number);

                None
            }
            PipelineStage::Done => None,
        };

        if let Some(next) = next_stage {
            spawn_next_stage(
                app.clone(),
                state.clone(),
                repo,
                issue_number,
                issue_title,
                issue_body,
                next,
                workspace_path,
            );
        }
    }
}

/// Spawn the next pipeline stage in a separate task, breaking the async recursion cycle.
fn spawn_next_stage(
    app: AppHandle,
    state: SharedState,
    repo: String,
    issue_number: u64,
    issue_title: String,
    issue_body: String,
    stage: PipelineStage,
    workspace_path: std::path::PathBuf,
) {
    let stage_label = stage.to_string();

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        let run_id = Uuid::new_v4().to_string();
        let config = {
            let s = state.lock().await;
            s.config.clone()
        };

        let run = AgentRun {
            id: run_id.clone(),
            repo: repo.clone(),
            issue_number,
            issue_title: issue_title.clone(),
            status: AgentStatus::Preparing,
            stage: stage.clone(),
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            logs: Vec::new(),
            workspace_path: workspace_path.to_string_lossy().to_string(),
            error: None,
            attempt: 1,
            max_retries: config.max_retries,
            lines_added: 0,
            lines_removed: 0,
            files_modified_list: Vec::new(),
            report: None,
            command_display: None,
            agent_type: config.agent_type.clone(),
            last_log_line: None,
            log_count: 0,
            activity: None,
        };

        {
            let mut s = state.lock().await;
            s.runs.insert(run_id.clone(), run);
        }

        // Persist metadata for chained stage
        logs::save_meta(&logs::LogMeta {
            run_id: run_id.clone(),
            repo: repo.clone(),
            issue_number,
            issue_title: issue_title.clone(),
            stage: stage_label.clone(),
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            status: "preparing".to_string(),
        });

        let _ = app.emit(
            "agent-status-changed",
            serde_json::json!({
                "run_id": &run_id,
                "status": "preparing",
                "stage": &stage_label,
            }),
        );

        let prompt = build_prompt(&stage, issue_number, &repo, &issue_title, &issue_body);
        let (cmd, args) = build_command_args(&config, &prompt);
        let command_display = format_command_display(&cmd, &args);

        // Update command_display in the run
        {
            let mut s = state.lock().await;
            if let Some(run) = s.runs.get_mut(&run_id) {
                run.command_display = Some(command_display);
                run.agent_type = config.agent_type.clone();
            }
        }

        // Update dock badge for the new chained stage
        update_dock_badge(&state).await;

        run_agent_process(
            app,
            state,
            run_id,
            cmd,
            args,
            workspace_path,
            stage,
            repo,
            issue_number,
            issue_title,
            issue_body,
        )
        .await;
    });
}

/// Spawn a retry of the same pipeline stage after a backoff delay.
fn spawn_retry(
    app: AppHandle,
    state: SharedState,
    repo: String,
    issue_number: u64,
    issue_title: String,
    issue_body: String,
    stage: PipelineStage,
    workspace_path: std::path::PathBuf,
    attempt: u32,
    max_retries: u32,
    backoff_secs: u64,
    previous_error: String,
) {
    let stage_label = stage.to_string();

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;

        let run_id = Uuid::new_v4().to_string();
        let config = {
            let s = state.lock().await;
            s.config.clone()
        };

        let run = AgentRun {
            id: run_id.clone(),
            repo: repo.clone(),
            issue_number,
            issue_title: issue_title.clone(),
            status: AgentStatus::Preparing,
            stage: stage.clone(),
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            logs: vec![format!(
                "Retry attempt {}/{} (previous error: {})",
                attempt,
                max_retries + 1,
                &previous_error
            )],
            workspace_path: workspace_path.to_string_lossy().to_string(),
            error: None,
            attempt,
            max_retries,
            lines_added: 0,
            lines_removed: 0,
            files_modified_list: Vec::new(),
            report: None,
            command_display: None,
            agent_type: config.agent_type.clone(),
            last_log_line: None,
            log_count: 0,
            activity: None,
        };

        {
            let mut s = state.lock().await;
            s.runs.insert(run_id.clone(), run);
        }

        let _ = app.emit(
            "agent-status-changed",
            serde_json::json!({
                "run_id": &run_id,
                "status": "preparing",
                "stage": &stage_label,
                "attempt": attempt,
                "max_retries": max_retries + 1,
            }),
        );

        let base_prompt = build_prompt(&stage, issue_number, &repo, &issue_title, &issue_body);
        let prompt = format!(
            "{}\n\nIMPORTANT: Previous attempt failed with: {}\nFix the issues and try again.",
            base_prompt, previous_error
        );
        let (cmd, args) = build_command_args(&config, &prompt);
        let command_display = format_command_display(&cmd, &args);
        {
            let mut s = state.lock().await;
            if let Some(run) = s.runs.get_mut(&run_id) {
                run.command_display = Some(command_display);
            }
        }

        update_dock_badge(&state).await;

        run_agent_process(
            app,
            state,
            run_id,
            cmd,
            args,
            workspace_path,
            stage,
            repo,
            issue_number,
            issue_title,
            issue_body,
        )
        .await;
    });
}

async fn update_dock_badge(state: &SharedState) {
    let s = state.lock().await;
    let active = s
        .runs
        .values()
        .filter(|r| {
            r.status == AgentStatus::Running || r.status == AgentStatus::Preparing
        })
        .count();
    crate::dock::set_badge_count(active);
}

/// Run `git diff --stat origin/main...HEAD` in the workspace and parse the output.
async fn capture_diff_stats(workspace_path: &std::path::Path) -> (u32, u32, Vec<String>) {
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
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            parse_git_diff_stat(&stdout)
        }
        _ => (0, 0, Vec::new()),
    }
}

fn parse_git_diff_stat(output: &str) -> (u32, u32, Vec<String>) {
    let mut added: u32 = 0;
    let mut removed: u32 = 0;
    let mut files: Vec<String> = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        // Summary line: "3 files changed, 45 insertions(+), 12 deletions(-)"
        if trimmed.contains("changed")
            && (trimmed.contains("insertion") || trimmed.contains("deletion"))
        {
            for part in trimmed.split(',') {
                let part = part.trim();
                if part.contains("insertion") {
                    if let Some(num) = part.split_whitespace().next() {
                        added = num.parse().unwrap_or(0);
                    }
                }
                if part.contains("deletion") {
                    if let Some(num) = part.split_whitespace().next() {
                        removed = num.parse().unwrap_or(0);
                    }
                }
            }
        }
        // File lines: " src/agent.rs | 15 +++---"
        else if trimmed.contains(" | ") {
            if let Some(pos) = trimmed.find(" | ") {
                let file = trimmed[..pos].trim().to_string();
                if !file.is_empty() {
                    files.push(file);
                }
            }
        }
    }

    (added, removed, files)
}

#[tauri::command]
pub async fn stop_agent(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    run_id: String,
) -> Result<(), String> {
    let mut s = state.lock().await;
    if let Some(pid) = s.agent_pids.remove(&run_id) {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        let mut repo_issue: Option<(String, u64)> = None;
        if let Some(run) = s.runs.get_mut(&run_id) {
            run.status = AgentStatus::Stopped;
            run.finished_at = Some(Utc::now().to_rfc3339());
            repo_issue = Some((run.repo.clone(), run.issue_number));
        }
        let should_cleanup = s.config.cleanup_on_stop;
        drop(s);
        let _ = app.emit(
            "agent-status-changed",
            serde_json::json!({
                "run_id": &run_id,
                "status": "stopped",
            }),
        );
        if should_cleanup {
            if let Some((repo, issue_number)) = repo_issue {
                let _ = workspace::cleanup_workspace(&repo, issue_number);
            }
        }
        Ok(())
    } else {
        Err("Agent not found or already finished".to_string())
    }
}
