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

/// Render a custom template by replacing `{{variable}}` placeholders.
fn render_template(
    template: &str,
    issue_number: u64,
    repo: &str,
    issue_title: &str,
    issue_body: &str,
    attempt: u32,
    previous_error: &str,
) -> String {
    template
        .replace("{{issue_number}}", &issue_number.to_string())
        .replace("{{repo}}", repo)
        .replace("{{issue_title}}", issue_title)
        .replace("{{issue_body}}", &issue_body.chars().take(4000).collect::<String>())
        .replace("{{attempt}}", &attempt.to_string())
        .replace("{{previous_error}}", previous_error)
}

fn default_prompt(stage: &PipelineStage) -> &'static str {
    match stage {
        PipelineStage::Implement => "\
You are working on GitHub issue #{{issue_number}} in repository {{repo}}.

Title: {{issue_title}}

Description:
{{issue_body}}

Instructions:
1. Analyze the issue carefully
2. Implement the fix or feature with clean, well-structured code
3. Commit your changes with a descriptive message
4. Create a Pull Request:
   gh pr create --title \"Fix #{{issue_number}}: {{issue_title}}\" --body \"Closes #{{issue_number}}\"

Do NOT run tests - that will be handled in a later stage.",

        PipelineStage::CodeReview => "\
You are a code reviewer for repository {{repo}}.

A Pull Request has been created for issue #{{issue_number}}: {{issue_title}}

Instructions:
1. Run: gh pr list --state open --json number,title,headRefName | to find the PR for issue #{{issue_number}}
2. Check out the PR branch
3. Review ALL changed files carefully. Look for:
   - Bugs, logic errors, edge cases
   - Security issues
   - Code style and readability
   - Missing error handling
   - Performance issues
4. If you find issues, FIX them directly in the code, commit, and push
5. If the code looks good or after fixing issues, leave a summary comment on the PR:
   gh pr comment <PR_NUMBER> --body \"Code review completed. <summary of findings and fixes>\"

Be thorough but practical. Fix real problems, don't nitpick style.",

        PipelineStage::Testing => "\
You are a test engineer for repository {{repo}}.

A Pull Request for issue #{{issue_number}}: {{issue_title}} has been reviewed and is ready for testing.

Issue description:
{{issue_body}}

Instructions:
1. Run: gh pr list --state open --json number,title,headRefName | to find the PR for issue #{{issue_number}}
2. Check out the PR branch
3. Identify the project type and run the appropriate test commands:
   - Node.js: npm test or npm run test
   - Python: pytest or python -m pytest
   - Rust: cargo test
   - Go: go test ./...
   - Swift: swift test
   - Or check package.json / Makefile / README for test instructions
4. If tests fail:
   - Analyze the failures
   - Fix the issues in the code
   - Commit and push the fixes
   - Re-run tests to confirm they pass
5. END-TO-END TESTING (CRITICAL):
   After existing tests pass, you MUST perform end-to-end validation:
   a) Read the issue title and description above carefully to understand what was fixed or added.
   b) If the issue describes a specific bug or feature:
      - Reproduce the original scenario described in the issue to verify the fix works end-to-end.
      - For bugs: try to trigger the original bug and confirm it no longer occurs.
      - For features: exercise the new feature through its intended usage path.
      - Use the project's actual entry points (CLI commands, API endpoints, scripts, UI) to test, not just unit tests.
   c) If the issue is too abstract or there is no specific scenario to reproduce:
      - Perform a quick smoke test: build the project and run its main entry point to verify nothing is broken.
      - For web apps: start the dev server and verify it loads without errors.
      - For CLI tools: run the main command with --help or a basic invocation.
      - For libraries: run a quick import/usage check.
   d) If E2E testing reveals issues, fix them, commit, push, and re-test.
6. Comment on the PR with your findings:
   gh pr comment <PR_NUMBER> --body \"Testing completed. Unit tests: PASS. E2E validation: <describe what you tested and results>. Ready to merge.\"

Make sure ALL tests pass and E2E validation succeeds before finishing.",

        PipelineStage::Merge => "\
You are a release engineer for repository {{repo}}.

A Pull Request for issue #{{issue_number}}: {{issue_title}} has passed code review and all tests.

Instructions:
1. Run: gh pr list -R {{repo}} --state open --json number,title,headRefName | to find the PR for issue #{{issue_number}}
2. Check out the PR branch and update it against the base branch to detect conflicts BEFORE merging:
   gh pr checkout <PR_NUMBER> -R {{repo}}
   git fetch origin main && git rebase origin/main
3. If there are merge conflicts:
   - Resolve the conflicts in the affected files
   - Run: git add <resolved_files> && git rebase --continue
   - Push the updated branch: git push --force-with-lease
4. Merge the PR into the default branch:
   gh pr merge <PR_NUMBER> -R {{repo}} --merge --delete-branch
5. Confirm the merge was successful by checking:
   gh pr view <PR_NUMBER> -R {{repo}} --json state
   The state MUST be \"MERGED\". If it is not, the merge failed.
6. Close the issue if it wasn't auto-closed:
   gh issue close {{issue_number}} -R {{repo}}

IMPORTANT: If the merge fails due to conflicts that you cannot resolve, \
exit with a non-zero exit code so the pipeline knows the merge did not succeed.",

        PipelineStage::Done => "",
    }
}

fn build_prompt(
    stage: &PipelineStage,
    issue_number: u64,
    repo: &str,
    issue_title: &str,
    issue_body: &str,
    stage_prompts: &std::collections::HashMap<String, String>,
    attempt: u32,
    previous_error: &str,
) -> String {
    let stage_key = stage.to_string();
    let template = match stage_prompts.get(&stage_key) {
        Some(custom) if !custom.trim().is_empty() => custom.as_str(),
        _ => default_prompt(stage),
    };
    let rendered = render_template(template, issue_number, repo, issue_title, issue_body, attempt, previous_error);
    // If the template doesn't use {{previous_error}} and we have one, append it so retry context isn't lost
    if !previous_error.is_empty() && !template.contains("{{previous_error}}") {
        format!(
            "{}\n\nIMPORTANT: Previous attempt ({}) failed with: {}\nFix the issues and try again.",
            rendered, attempt, previous_error
        )
    } else {
        rendered
    }
}

/// Returns the default prompt templates for all stages.
#[tauri::command]
pub fn get_default_prompts() -> std::collections::HashMap<String, String> {
    let stages = [
        PipelineStage::Implement,
        PipelineStage::CodeReview,
        PipelineStage::Testing,
        PipelineStage::Merge,
    ];
    stages
        .into_iter()
        .map(|s| (s.to_string(), default_prompt(&s).to_string()))
        .collect()
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
    } else if lower.contains("editing file")
        || lower.contains("edit(")
        || lower.contains("write(")
        || lower.contains("sed ")
    {
        Some("Editing files".to_string())
    } else if lower.contains("bash(") || lower.contains("running command") || lower.contains("$ ") {
        Some("Running command".to_string())
    } else if lower.contains("grep(") || lower.contains("glob(") || lower.contains("searching") {
        Some("Searching code".to_string())
    } else if lower.contains("git commit") || lower.contains("git push") || lower.contains("gh pr")
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
/// Returns one or more display strings. Falls back to the raw line if not valid JSON.
fn parse_stream_json_event(raw: &str) -> Vec<String> {
    let parsed: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return vec![raw.to_string()],
    };

    let event_type = parsed["type"].as_str().unwrap_or("");

    match event_type {
        "system" => {
            let model = parsed["model"].as_str().unwrap_or("unknown");
            vec![format!("🔧 Session initialized (model: {})", model)]
        }
        "assistant" => {
            let content = match parsed["message"]["content"].as_array() {
                Some(arr) => arr,
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
                                let cmd = input["command"]
                                    .as_str()
                                    .unwrap_or("")
                                    .chars()
                                    .take(120)
                                    .collect::<String>();
                                format!("$ {}", cmd)
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
                            // Only show the first ~200 chars of assistant text
                            let preview: String = text.chars().take(200).collect();
                            lines.push(format!("💬 {}", preview));
                        }
                    }
                    _ => {}
                }
            }
            lines
        }
        "user" => {
            // Tool results - show a condensed version
            let content = match parsed["message"]["content"].as_array() {
                Some(arr) => arr,
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
        "rate_limit_event" => vec![], // silently skip
        _ => vec![],
    }
}

/// Token usage extracted from a Claude CLI result event.
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub duration_secs: f64,
}

/// Try to extract token usage from a stream-json result event line.
fn parse_token_usage(raw: &str) -> Option<TokenUsage> {
    let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
    if parsed["type"].as_str() != Some("result") {
        return None;
    }
    let usage = &parsed["usage"];
    let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
    let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0);
    let cost_usd = parsed["total_cost_usd"].as_f64().unwrap_or(0.0);
    let duration_ms = parsed["duration_ms"].as_u64().unwrap_or(0);
    Some(TokenUsage {
        input_tokens,
        output_tokens,
        cost_usd,
        duration_secs: duration_ms as f64 / 1000.0,
    })
}

/// Truncate a JSON value to a max character length for display.
fn truncate_json(value: &serde_json::Value, max_len: usize) -> String {
    let s = value.to_string();
    if s.len() > max_len {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{}...", truncated)
    } else {
        s
    }
}

fn build_command_args(config: &RunConfig, prompt: &str) -> (String, Vec<String>) {
    match config.agent_type.as_str() {
        "codex" => {
            let mut args = vec!["exec".to_string()];
            if config.auto_approve {
                args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
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
            let mut args = vec![
                "--print".to_string(),
                "--output-format".to_string(),
                "stream-json".to_string(),
                "--verbose".to_string(),
            ];
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

    let workspace_path = workspace::ensure_workspace(&repo, issue_number, &config.hooks)?;

    let stage_label = stage.to_string();

    let max_retries = config.max_retries;

    let prompt = build_prompt(&stage, issue_number, &repo, &issue_title, &issue_body, &config.stage_prompts, 1, "");
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
        input_tokens: 0,
        output_tokens: 0,
        cost_usd: 0.0,
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

    // Run before_run hook (failure aborts the agent run)
    let hooks = {
        let s = state.lock().await;
        s.config.hooks.clone()
    };
    if let Some(ref cmd) = hooks.before_run {
        let hook_result = workspace::execute_hook_async(
            "before_run",
            cmd,
            &workspace_path,
            hooks.timeout_secs,
        )
        .await;
        if let Err(e) = hook_result {
            let error_msg = format!("before_run hook failed: {}", e);
            let mut s = state.lock().await;
            if let Some(run) = s.runs.get_mut(&run_id) {
                run.status = AgentStatus::Failed;
                run.error = Some(error_msg.clone());
                run.finished_at = Some(Utc::now().to_rfc3339());
                run.logs.push(error_msg);
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
    }

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
                // Try to parse as stream-json event from Claude CLI
                let display_lines = parse_stream_json_event(&line);

                // Extract token usage from result events
                let token_usage = parse_token_usage(&line);

                for display_line in &display_lines {
                    let ts = Utc::now().to_rfc3339();
                    let log_line = AgentLogLine {
                        run_id: run_id_out.clone(),
                        timestamp: ts.clone(),
                        line: display_line.clone(),
                    };
                    let _ = app_out.emit("agent-log", &log_line);
                    logs::append_log_line(&run_id_out, display_line);
                    let activity = detect_activity(display_line);
                    let mut s = state_out.lock().await;
                    if let Some(run) = s.runs.get_mut(&run_id_out) {
                        run.logs.push(display_line.clone());
                        run.last_log_line = Some(display_line.clone());
                        run.log_count += 1;
                        if let Some(ref act) = activity {
                            run.activity = Some(act.clone());
                        }
                    }
                }

                // Accumulate token usage into run and global totals
                if let Some(ref usage) = token_usage {
                    let mut s = state_out.lock().await;
                    if let Some(run) = s.runs.get_mut(&run_id_out) {
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
        let (diff_added, diff_removed, diff_files) = capture_diff_stats(&workspace_path).await;
        let mut s = state.lock().await;
        if let Some(run) = s.runs.get_mut(&run_id) {
            run.lines_added = diff_added;
            run.lines_removed = diff_removed;
            run.files_modified_list = diff_files;
        }
    }

    // Run after_run hook (failure is logged but ignored)
    {
        let hooks = {
            let s = state.lock().await;
            s.config.hooks.clone()
        };
        if let Some(ref cmd) = hooks.after_run {
            if let Err(e) = workspace::execute_hook_async(
                "after_run",
                cmd,
                &workspace_path,
                hooks.timeout_secs,
            )
            .await
            {
                let warning = format!("[hook] after_run failed (ignored): {}", e);
                logs::append_log_line(&run_id, &warning);
                let mut s = state.lock().await;
                if let Some(run) = s.runs.get_mut(&run_id) {
                    run.logs.push(warning);
                }
            }
        }
    }

    // RETRY LOGIC: If the agent failed, check if we can retry
    if !succeeded {
        let (current_attempt, max_retries, retry_backoff, error_log) = {
            let s = state.lock().await;
            let run = s.runs.get(&run_id);
            let attempt = run.map(|r| r.attempt).unwrap_or(1);
            let max_r = run.map(|r| r.max_retries).unwrap_or(0);
            // Exponential backoff: min(base_delay * 2^(attempt-1), max_backoff)
            let base_delay = if s.config.retry_base_delay_secs > 0 {
                s.config.retry_base_delay_secs
            } else {
                // Backward compatibility: use retry_backoff_secs as base delay
                s.config.retry_backoff_secs
            };
            let max_backoff = s.config.retry_max_backoff_secs;
            let exp_backoff = base_delay.saturating_mul(2u64.saturating_pow(attempt.saturating_sub(1)));
            let backoff = exp_backoff.min(max_backoff);
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
        let cleanup_hooks = s.config.hooks.clone();
        drop(s);
        update_dock_badge(&state).await;
        if should_cleanup {
            let _ = workspace::cleanup_workspace(&repo, issue_number, &cleanup_hooks);
        }
    }

    // AUTO-CHAIN: If the stage completed successfully, advance to the next stage
    if succeeded {
        let next_stage = match stage {
            PipelineStage::Implement => Some(PipelineStage::CodeReview),
            PipelineStage::CodeReview => Some(PipelineStage::Testing),
            PipelineStage::Testing => Some(PipelineStage::Merge),
            PipelineStage::Merge => {
                // Verify the PR was actually merged before advancing to Done
                let merge_verified =
                    match crate::github::is_pr_merged_for_issue(&repo, issue_number) {
                        Ok(true) => true,
                        Ok(false) => {
                            // PR exists but is NOT merged (likely conflicts)
                            let mut s = state.lock().await;
                            if let Some(run) = s.runs.get_mut(&run_id) {
                                run.status = AgentStatus::Failed;
                                run.error = Some(format!(
                                    "PR for issue #{} was not merged (possible merge conflicts). \
                                     The PR needs manual conflict resolution.",
                                    issue_number
                                ));
                            }
                            if s.config.notifications_enabled {
                                crate::notification::notify_pipeline_failed(
                                    &app,
                                    issue_number,
                                    "merge",
                                    s.config.notification_sound,
                                );
                            }
                            drop(s);
                            let _ = app.emit(
                                "agent-status-changed",
                                serde_json::json!({
                                    "run_id": &run_id,
                                    "status": "failed",
                                    "stage": "merge",
                                    "error": "PR not actually merged - possible conflicts",
                                }),
                            );
                            update_dock_badge(&state).await;
                            false
                        }
                        Err(e) => {
                            // Could not verify - log warning but proceed
                            // (e.g. PR was already closed and issue auto-closed)
                            let log_msg = format!(
                                "[warning] Could not verify merge status: {}. Proceeding to Done.",
                                e
                            );
                            logs::append_log_line(&run_id, &log_msg);
                            let mut s = state.lock().await;
                            if let Some(run) = s.runs.get_mut(&run_id) {
                                run.logs.push(log_msg);
                            }
                            drop(s);
                            true
                        }
                    };

                if !merge_verified {
                    // Do NOT advance to Done - merge failed
                    None
                } else {
                // Merge verified → mark as Done with aggregated logs and enriched report
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

                // Aggregate token usage across all stage runs for this issue
                let (total_input, total_output, total_cost) = {
                    let s = state.lock().await;
                    let mut inp: u64 = 0;
                    let mut out: u64 = 0;
                    let mut cost: f64 = 0.0;
                    for r in s.runs.values() {
                        if r.issue_number == issue_number && r.stage != PipelineStage::Done {
                            inp += r.input_tokens;
                            out += r.output_tokens;
                            cost += r.cost_usd;
                        }
                    }
                    (inp, out, cost)
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
                    input_tokens: total_input,
                    output_tokens: total_output,
                    cost_usd: total_cost,
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
                {
                    let cleanup_hooks = {
                        let s = state.lock().await;
                        s.config.hooks.clone()
                    };
                    let _ = workspace::cleanup_workspace(&repo, issue_number, &cleanup_hooks);
                }

                None
            } // end else (merge_verified)
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

        // Wait until the per-stage (and global) concurrency limit allows launching
        loop {
            let (can_launch, stopped) = {
                let s = state.lock().await;
                (crate::orchestrator::can_launch_stage(&s, &stage), s.stop_flag)
            };
            if stopped {
                eprintln!("Orchestrator stopped; aborting queued stage {stage_label} for issue #{issue_number}");
                return;
            }
            if can_launch {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }

        // Re-check issue/PR state before proceeding to the next stage
        {
            let repo_clone = repo.clone();
            let issue_num = issue_number;
            let stage_clone = stage.clone();
            let stage_label_clone = stage_label.clone();

            let skip = tokio::task::spawn_blocking(move || {
                // Check if the issue has been closed externally
                match crate::github::get_issue_state(&repo_clone, issue_num) {
                    Ok(ref issue_state) if issue_state != "OPEN" => {
                        eprintln!(
                            "Issue #{issue_num} is {issue_state}; skipping stage {stage_label_clone}"
                        );
                        return true;
                    }
                    Err(ref e) => {
                        eprintln!(
                            "Warning: could not re-check issue #{issue_num} state: {e}; proceeding anyway"
                        );
                    }
                    _ => {}
                }

                // If the next stage is Merge, check if the PR is already merged
                if matches!(stage_clone, PipelineStage::Merge) {
                    match crate::github::is_pr_merged_for_issue(&repo_clone, issue_num) {
                        Ok(true) => {
                            eprintln!(
                                "PR for issue #{issue_num} is already merged; skipping Merge stage"
                            );
                            return true;
                        }
                        Err(ref e) => {
                            eprintln!(
                                "Warning: could not check PR merge status for #{issue_num}: {e}; proceeding anyway"
                            );
                        }
                        _ => {}
                    }
                }

                false
            })
            .await
            .unwrap_or(false);

            if skip {
                return;
            }
        }

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
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
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

        let prompt = build_prompt(&stage, issue_number, &repo, &issue_title, &issue_body, &config.stage_prompts, 1, "");
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

        // Wait until the per-stage (and global) concurrency limit allows launching
        loop {
            let (can_launch, stopped) = {
                let s = state.lock().await;
                (crate::orchestrator::can_launch_stage(&s, &stage), s.stop_flag)
            };
            if stopped {
                eprintln!("Orchestrator stopped; aborting retry of {stage_label} for issue #{issue_number}");
                return;
            }
            if can_launch {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }

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
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
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

        let prompt = build_prompt(&stage, issue_number, &repo, &issue_title, &issue_body, &config.stage_prompts, attempt, &previous_error);
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
        .filter(|r| r.status == AgentStatus::Running || r.status == AgentStatus::Preparing)
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
pub async fn retry_agent(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    run_id: String,
) -> Result<String, String> {
    let (repo, issue_number, issue_title, issue_body, stage) = {
        let s = state.lock().await;
        let run = s
            .runs
            .get(&run_id)
            .ok_or("Run not found")?;
        if run.status != AgentStatus::Failed && run.status != AgentStatus::Stopped {
            return Err("Can only retry failed or stopped runs".to_string());
        }
        (
            run.repo.clone(),
            run.issue_number,
            run.issue_title.clone(),
            String::new(), // body is not stored in the run; will be fetched from issue
            run.stage.clone(),
        )
    };

    // Try to fetch the issue body from GitHub
    let body = match crate::github::get_issue_detail(repo.clone(), issue_number).await {
        Ok(issue) => issue.body.unwrap_or_default(),
        Err(_) => issue_body,
    };

    launch_agent(
        app,
        state.inner().clone(),
        repo,
        issue_number,
        issue_title,
        body,
        stage,
    )
    .await
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
        let stop_hooks = s.config.hooks.clone();
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
                let _ = workspace::cleanup_workspace(&repo, issue_number, &stop_hooks);
            }
        }
        Ok(())
    } else {
        Err("Agent not found or already finished".to_string())
    }
}
