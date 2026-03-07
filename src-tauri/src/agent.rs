use crate::orchestrator::{AgentRun, AgentStatus, PipelineStage, RunConfig};
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
             5. If all tests pass, comment on the PR:\n\
                gh pr comment <PR_NUMBER> --body \"All tests passing. Ready to merge.\"\n\n\
             Make sure ALL tests pass before finishing.",
            repo = repo,
            num = issue_number,
            title = issue_title,
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

fn build_command_args(config: &RunConfig, prompt: &str) -> (String, Vec<String>) {
    match config.agent_type.as_str() {
        "codex" => {
            let mut args = vec!["exec".to_string()];
            if config.auto_approve {
                args.push("--full-auto".to_string());
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
        }),
    );

    let prompt = build_prompt(&stage, issue_number, &repo, &issue_title, &issue_body);
    let (cmd, args) = build_command_args(&config, &prompt);

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

    let result = Command::new(&cmd)
        .args(&args)
        .current_dir(&workspace_path)
        .env("PATH", &path_env)
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_SESSION")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn();

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
                let log_line = AgentLogLine {
                    run_id: run_id_out.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    line: line.clone(),
                };
                let _ = app_out.emit("agent-log", &log_line);
                let mut s = state_out.lock().await;
                if let Some(run) = s.runs.get_mut(&run_id_out) {
                    run.logs.push(line);
                    if run.logs.len() > 1000 {
                        run.logs.drain(0..run.logs.len() - 1000);
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
                let log_line = AgentLogLine {
                    run_id: run_id_err.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    line: format!("[stderr] {}", line),
                };
                let _ = app_err.emit("agent-log", &log_line);
                let mut s = state_err.lock().await;
                if let Some(run) = s.runs.get_mut(&run_id_err) {
                    run.logs.push(format!("[stderr] {}", line));
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
    }

    let _ = app.emit(
        "agent-status-changed",
        serde_json::json!({
            "run_id": &run_id,
            "status": if succeeded { "completed" } else { "failed" },
            "stage": &stage_label,
        }),
    );

    // Send notification on failure
    if !succeeded {
        let s = state.lock().await;
        if s.config.notifications_enabled {
            crate::notification::notify_pipeline_failed(
                &app,
                issue_number,
                &stage_label,
                s.config.notification_sound,
            );
        }
        drop(s);
        update_dock_badge(&state).await;
    }

    // AUTO-CHAIN: If the stage completed successfully, advance to the next stage
    if succeeded {
        let next_stage = match stage {
            PipelineStage::Implement => Some(PipelineStage::CodeReview),
            PipelineStage::CodeReview => Some(PipelineStage::Testing),
            PipelineStage::Testing => Some(PipelineStage::Merge),
            PipelineStage::Merge => {
                // Merge passed → mark as Done with aggregated logs from all stages
                let done_id = Uuid::new_v4().to_string();
                let aggregated_logs = {
                    let s = state.lock().await;
                    let stage_order = ["implement", "code_review", "testing", "merge"];
                    let mut all_logs: Vec<String> = Vec::new();
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
                        }
                    }
                    all_logs.push("═══ PIPELINE COMPLETED ═══".to_string());
                    all_logs
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
            }),
        );

        let prompt = build_prompt(&stage, issue_number, &repo, &issue_title, &issue_body);
        let (cmd, args) = build_command_args(&config, &prompt);

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
        if let Some(run) = s.runs.get_mut(&run_id) {
            run.status = AgentStatus::Stopped;
            run.finished_at = Some(Utc::now().to_rfc3339());
        }
        let _ = app.emit(
            "agent-status-changed",
            serde_json::json!({
                "run_id": &run_id,
                "status": "stopped",
            }),
        );
        Ok(())
    } else {
        Err("Agent not found or already finished".to_string())
    }
}
