use crate::logs;
use crate::orchestrator::{AgentRun, AgentStatus, PipelineStage, RunConfig, StageContext};
use crate::report;
use crate::workspace;
use crate::SharedState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Notify;
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
    previous_context: Option<&StageContext>,
) -> String {
    let stage_key = stage.to_string();
    let template = match stage_prompts.get(&stage_key) {
        Some(custom) if !custom.trim().is_empty() => custom.as_str(),
        _ => default_prompt(stage),
    };
    let mut rendered = render_template(template, issue_number, repo, issue_title, issue_body, attempt, previous_error);

    // Inject structured context from the previous stage
    if let Some(ctx) = previous_context {
        rendered = format!("{}\n\n{}", rendered, ctx.to_prompt_section());
    }

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

/// Extract a structured `StageContext` from a completed agent run's data and logs.
fn extract_stage_context(run: &AgentRun, repo: &str) -> StageContext {
    let from_stage = run.stage.to_string();
    let files_changed = run.files_modified_list.clone();
    let lines_added = run.lines_added;
    let lines_removed = run.lines_removed;

    // Try to detect PR number from logs (looks for "gh pr create" output or PR URLs)
    let pr_number = detect_pr_number_from_logs(&run.logs, repo);

    // Try to detect branch name from logs
    let branch_name = detect_branch_from_logs(&run.logs);

    // Build a concise summary based on the stage type
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

/// Scan agent logs for a PR number (e.g. from "gh pr create" output or PR URLs).
fn detect_pr_number_from_logs(logs: &[String], repo: &str) -> Option<u64> {
    // Pattern: github.com/owner/repo/pull/123 or #123 in PR creation context
    let pr_url_suffix = format!("{}/pull/", repo);
    for line in logs.iter().rev() {
        // Check for PR URL pattern
        if let Some(pos) = line.find(&pr_url_suffix) {
            let after = &line[pos + pr_url_suffix.len()..];
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.parse::<u64>() {
                return Some(n);
            }
        }
        // Check for "pull request #N" or "PR #N" patterns
        for prefix in &["pull request #", "PR #", "pr #"] {
            if let Some(pos) = line.to_lowercase().find(prefix) {
                let after = &line[pos + prefix.len()..];
                let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = num_str.parse::<u64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Scan agent logs for a branch name (e.g. from git checkout or PR creation).
fn detect_branch_from_logs(logs: &[String]) -> Option<String> {
    for line in logs.iter().rev() {
        // Look for "headRefName" in JSON output from gh pr list
        if line.contains("headRefName") {
            if let Some(pos) = line.find("headRefName") {
                let after = &line[pos..];
                // Try to extract the value after the key
                if let Some(colon_pos) = after.find(':') {
                    let value_part = after[colon_pos + 1..].trim().trim_matches(|c| c == '"' || c == ',' || c == ' ');
                    let branch: String = value_part.chars().take_while(|c| !c.is_whitespace() && *c != '"' && *c != ',').collect();
                    if !branch.is_empty() {
                        return Some(branch);
                    }
                }
            }
        }
        // Look for "git checkout -b <branch>" or "Switched to branch"
        if line.contains("checkout -b ") {
            if let Some(pos) = line.find("checkout -b ") {
                let after = &line[pos + "checkout -b ".len()..];
                let branch: String = after.chars().take_while(|c| !c.is_whitespace()).collect();
                if !branch.is_empty() {
                    return Some(branch);
                }
            }
        }
    }
    None
}

/// Build a concise summary string from agent logs based on stage type.
/// Keeps output under ~200 chars to stay within the ~500 token budget.
fn build_stage_summary(stage: &PipelineStage, logs: &[String]) -> String {
    match stage {
        PipelineStage::Implement => {
            // Look for commit messages or key decisions
            let mut commits = Vec::new();
            for line in logs {
                if line.contains("git commit") || line.contains("Commit") {
                    let summary: String = line.chars().take(100).collect();
                    commits.push(summary);
                }
            }
            if commits.is_empty() {
                "Implementation completed.".to_string()
            } else {
                commits.into_iter().take(3).collect::<Vec<_>>().join("; ")
            }
        }
        PipelineStage::CodeReview => {
            // Look for review findings
            let mut findings = Vec::new();
            for line in logs {
                let lower = line.to_lowercase();
                if lower.contains("issue") || lower.contains("fix") || lower.contains("bug")
                    || lower.contains("suggestion") || lower.contains("approved")
                    || lower.contains("review completed")
                {
                    let summary: String = line.chars().take(100).collect();
                    findings.push(summary);
                }
            }
            if findings.is_empty() {
                "Code review completed.".to_string()
            } else {
                findings.into_iter().take(3).collect::<Vec<_>>().join("; ")
            }
        }
        PipelineStage::Testing => {
            // Look for test results
            let mut results = Vec::new();
            for line in logs {
                let lower = line.to_lowercase();
                if lower.contains("pass") || lower.contains("fail") || lower.contains("test")
                    || lower.contains("error") || lower.contains("ok")
                {
                    let summary: String = line.chars().take(100).collect();
                    results.push(summary);
                }
            }
            if results.is_empty() {
                "Testing completed.".to_string()
            } else {
                // Take the last few results (most relevant)
                results.into_iter().rev().take(3).collect::<Vec<_>>().join("; ")
            }
        }
        _ => String::new(),
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
    issue_labels: Vec<String>,
) -> Result<String, String> {
    let run_id = Uuid::new_v4().to_string();
    let config = {
        let s = state.lock().await;
        s.config.clone()
    };

    let workspace_path = workspace::ensure_workspace(&repo, issue_number, &config.hooks)?;

    let stage_label = stage.to_string();

    let max_retries = config.max_retries;

    let skipped = crate::orchestrator::compute_skipped_stages(&issue_labels, &config.stage_skip_labels);
    let skipped_stage_names: Vec<String> = skipped.iter().map(|s| s.to_string()).collect();

    let prompt = build_prompt(&stage, issue_number, &repo, &issue_title, &issue_body, &config.stage_prompts, 1, "", None);
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
        last_log_timestamp: None,
        issue_labels: issue_labels.clone(),
        skipped_stages: skipped_stage_names,
        stage_context: None,
        pending_next_stage: None,
    };

    {
        let mut s = state.lock().await;
        s.runs.insert(run_id.clone(), run);
        s.persist();
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
    let labels_clone = issue_labels.clone();

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
            labels_clone,
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
    issue_labels: Option<Vec<String>>,
) -> Result<String, String> {
    launch_agent(
        app,
        state.inner().clone(),
        repo,
        issue_number,
        issue_title,
        issue_body.unwrap_or_default(),
        PipelineStage::Implement,
        issue_labels.unwrap_or_default(),
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
    issue_labels: Vec<String>,
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
            s.persist();
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
        s.persist();
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
            s.persist();
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

    // Stall detection: shared notify so stdout/stderr can signal activity
    let stall_notify = Arc::new(Notify::new());

    let state_out = state.clone();
    let run_id_out = run_id.clone();
    let app_out = app.clone();
    let stall_notify_out = stall_notify.clone();

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
                        run.last_log_timestamp = Some(ts);
                        if let Some(ref act) = activity {
                            run.activity = Some(act.clone());
                        }
                    }
                }
                // Signal stall watcher that output was received
                stall_notify_out.notify_one();

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
    let stall_notify_err = stall_notify.clone();

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
                    run.last_log_timestamp = Some(Utc::now().to_rfc3339());
                }
                drop(s);
                stall_notify_err.notify_one();
            }
        }
    });

    // Stall detection task: periodically checks if agent has been idle too long
    let stall_state = state.clone();
    let stall_run_id = run_id.clone();
    let stall_app = app.clone();
    let stall_stage_label = stage_label.clone();
    let stall_notify_watch = stall_notify.clone();

    let stall_handle = tokio::spawn(async move {
        let stall_timeout = {
            let s = stall_state.lock().await;
            s.config.stall_timeout_secs
        };
        // 0 means disabled
        if stall_timeout == 0 {
            return false;
        }
        let timeout_duration = tokio::time::Duration::from_secs(stall_timeout);

        loop {
            // Wait for either a notification (output received) or the timeout
            let timed_out = tokio::time::timeout(timeout_duration, stall_notify_watch.notified())
                .await
                .is_err();

            if timed_out {
                // Check if the run is still active
                let is_running = {
                    let s = stall_state.lock().await;
                    s.runs
                        .get(&stall_run_id)
                        .map(|r| r.status == AgentStatus::Running)
                        .unwrap_or(false)
                };
                if !is_running {
                    return false;
                }

                // Kill the stalled agent
                {
                    let mut s = stall_state.lock().await;
                    if let Some(pid) = s.agent_pids.remove(&stall_run_id) {
                        unsafe {
                            libc::kill(pid as i32, libc::SIGTERM);
                        }
                    }
                    if let Some(run) = s.runs.get_mut(&stall_run_id) {
                        run.status = AgentStatus::Failed;
                        run.error = Some(format!(
                            "Agent stalled: no output for {}s",
                            stall_timeout
                        ));
                        run.finished_at = Some(Utc::now().to_rfc3339());
                        let stall_msg = format!(
                            "⚠ Agent killed: no output for {}s (stall timeout)",
                            stall_timeout
                        );
                        run.logs.push(stall_msg.clone());
                        logs::append_log_line(&stall_run_id, &stall_msg);
                    }
                    s.persist();
                }
                let _ = stall_app.emit(
                    "agent-status-changed",
                    serde_json::json!({
                        "run_id": &stall_run_id,
                        "status": "failed",
                        "stage": &stall_stage_label,
                        "error": format!("Agent stalled: no output for {}s", stall_timeout),
                    }),
                );
                return true;
            }

            // Not timed out - output was received, check if run is still active
            let is_running = {
                let s = stall_state.lock().await;
                s.runs
                    .get(&stall_run_id)
                    .map(|r| r.status == AgentStatus::Running)
                    .unwrap_or(false)
            };
            if !is_running {
                return false;
            }
        }
    });

    let exit_status = child.wait().await;

    // Cancel stall watcher since the process has exited
    stall_handle.abort();
    let stalled = stall_handle.await.unwrap_or(false);

    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    // If the agent was killed by stall detection, skip normal status handling
    if stalled {
        // Persist metadata to disk so stall-killed runs show correct final status
        if let Some(mut meta) = logs::load_meta(&run_id) {
            meta.finished_at = Some(Utc::now().to_rfc3339());
            meta.status = "failed".to_string();
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
                status: "failed".to_string(),
            });
        }

        // Run after_run hook (same as normal exit path)
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

        update_dock_badge(&state).await;

        // Trigger retry logic for stalled agents
        let (current_attempt, max_retries, retry_backoff, error_log) = {
            let s = state.lock().await;
            let run = s.runs.get(&run_id);
            let attempt = run.map(|r| r.attempt).unwrap_or(1);
            let max_r = run.map(|r| r.max_retries).unwrap_or(0);
            let base_delay = if s.config.retry_base_delay_secs > 0 {
                s.config.retry_base_delay_secs
            } else {
                s.config.retry_backoff_secs
            };
            let max_backoff = s.config.retry_max_backoff_secs;
            let exp_backoff = base_delay.saturating_mul(2u64.saturating_pow(attempt.saturating_sub(1)));
            let backoff = exp_backoff.min(max_backoff);
            let err = run.and_then(|r| r.error.clone()).unwrap_or_default();
            (attempt, max_r, backoff, err)
        };

        if current_attempt <= max_retries {
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
                issue_labels.clone(),
            );
        } else {
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
            if should_cleanup {
                let _ = workspace::cleanup_workspace(&repo, issue_number, &cleanup_hooks);
            }
        }
        return;
    }

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
        s.persist();
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
                issue_labels.clone(),
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
        // Extract structured context from the completed stage
        let stage_ctx = {
            let s = state.lock().await;
            s.runs.get(&run_id).map(|r| extract_stage_context(r, &repo))
        };

        // Store the context back into the run for persistence
        if let Some(ref ctx) = stage_ctx {
            let mut s = state.lock().await;
            if let Some(run) = s.runs.get_mut(&run_id) {
                run.stage_context = Some(ctx.clone());
            }
            s.persist();
        }

        // Compute which stages to skip based on issue labels
        let skipped_stages = {
            let s = state.lock().await;
            crate::orchestrator::compute_skipped_stages(&issue_labels, &s.config.stage_skip_labels)
        };

        let next_stage = match stage {
            PipelineStage::Implement | PipelineStage::CodeReview | PipelineStage::Testing => {
                let next = crate::orchestrator::next_pipeline_stage(&stage, &skipped_stages);
                if let Some(ref next_s) = next {
                    // Log skipped stages
                    let default_chain: &[PipelineStage] = match stage {
                        PipelineStage::Implement => &[PipelineStage::CodeReview, PipelineStage::Testing],
                        PipelineStage::CodeReview => &[PipelineStage::Testing],
                        _ => &[],
                    };
                    for s in default_chain {
                        if skipped_stages.contains(s) && s != next_s {
                            let msg = format!("[pipeline] Skipping {} stage (label rule)", s);
                            logs::append_log_line(&run_id, &msg);
                            let mut st = state.lock().await;
                            if let Some(run) = st.runs.get_mut(&run_id) {
                                run.logs.push(msg);
                            }
                        }
                    }
                }
                next
            }
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
                            s.persist();
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

                let done_skipped: Vec<String> = skipped_stages.iter().map(|s| s.to_string()).collect();
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
                    last_log_timestamp: None,
                    issue_labels: issue_labels.clone(),
                    skipped_stages: done_skipped,
                    stage_context: None,
                    pending_next_stage: None,
                };
                {
                    let mut s = state.lock().await;
                    s.runs.insert(done_id.clone(), done_run);
                    s.persist();
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
            // Check if the current stage has an approval gate before advancing
            let gate_enabled = {
                let s = state.lock().await;
                crate::orchestrator::is_gate_enabled(&s.config, &stage)
            };

            if gate_enabled {
                // Pause the pipeline: set status to AwaitingApproval
                let next_stage_name = next.to_string();
                {
                    let mut s = state.lock().await;
                    if let Some(run) = s.runs.get_mut(&run_id) {
                        run.status = AgentStatus::AwaitingApproval;
                        run.pending_next_stage = Some(next_stage_name.clone());
                        run.logs.push(format!(
                            "[pipeline] Approval gate: paused after {} stage. Awaiting user approval to proceed to {}.",
                            stage, next_stage_name
                        ));
                    }
                    s.persist();
                }
                let _ = app.emit(
                    "agent-status-changed",
                    serde_json::json!({
                        "run_id": &run_id,
                        "status": "awaiting_approval",
                        "stage": &stage_label,
                        "pending_next_stage": &next_stage_name,
                    }),
                );

                // Send macOS notification
                {
                    let s = state.lock().await;
                    if s.config.notifications_enabled {
                        crate::notification::notify_awaiting_approval(
                            &app,
                            issue_number,
                            &stage_label,
                            s.config.notification_sound,
                        );
                    }
                }

                update_dock_badge(&state).await;
            } else {
                spawn_next_stage(
                    app.clone(),
                    state.clone(),
                    repo,
                    issue_number,
                    issue_title,
                    issue_body,
                    next,
                    workspace_path,
                    issue_labels,
                    stage_ctx,
                );
            }
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
    issue_labels: Vec<String>,
    previous_context: Option<StageContext>,
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

        let skipped = crate::orchestrator::compute_skipped_stages(&issue_labels, &config.stage_skip_labels);
        let skipped_stage_names: Vec<String> = skipped.iter().map(|s| s.to_string()).collect();

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
            last_log_timestamp: None,
            issue_labels: issue_labels.clone(),
            skipped_stages: skipped_stage_names,
            stage_context: None,
            pending_next_stage: None,
        };

        {
            let mut s = state.lock().await;
            s.runs.insert(run_id.clone(), run);
            s.persist();
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

        let prompt = build_prompt(&stage, issue_number, &repo, &issue_title, &issue_body, &config.stage_prompts, 1, "", previous_context.as_ref());
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
            issue_labels,
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
    issue_labels: Vec<String>,
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

        let skipped = crate::orchestrator::compute_skipped_stages(&issue_labels, &config.stage_skip_labels);
        let skipped_stage_names: Vec<String> = skipped.iter().map(|s| s.to_string()).collect();

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
            last_log_timestamp: None,
            issue_labels: issue_labels.clone(),
            skipped_stages: skipped_stage_names,
            stage_context: None,
            pending_next_stage: None,
        };

        {
            let mut s = state.lock().await;
            s.runs.insert(run_id.clone(), run);
            s.persist();
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

        let prompt = build_prompt(&stage, issue_number, &repo, &issue_title, &issue_body, &config.stage_prompts, attempt, &previous_error, None);
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
            issue_labels,
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
    let (repo, issue_number, issue_title, issue_body, stage, labels) = {
        let s = state.lock().await;
        let run = s
            .runs
            .get(&run_id)
            .ok_or("Run not found")?;
        if run.status != AgentStatus::Failed && run.status != AgentStatus::Stopped && run.status != AgentStatus::Interrupted {
            return Err("Can only retry failed, stopped, or interrupted runs".to_string());
        }
        (
            run.repo.clone(),
            run.issue_number,
            run.issue_title.clone(),
            String::new(), // body is not stored in the run; will be fetched from issue
            run.stage.clone(),
            run.issue_labels.clone(),
        )
    };

    // Try to fetch the issue body and labels from GitHub
    let (body, issue_labels) = match crate::github::get_issue_detail(repo.clone(), issue_number).await {
        Ok(issue) => (issue.body.unwrap_or_default(), issue.labels),
        Err(_) => (issue_body, labels),
    };

    launch_agent(
        app,
        state.inner().clone(),
        repo,
        issue_number,
        issue_title,
        body,
        stage,
        issue_labels,
    )
    .await
}

#[tauri::command]
pub async fn retry_agent_from_stage(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    run_id: String,
    from_stage: String,
) -> Result<String, String> {
    let target_stage = match from_stage.as_str() {
        "implement" => PipelineStage::Implement,
        "code_review" => PipelineStage::CodeReview,
        "testing" => PipelineStage::Testing,
        "merge" => PipelineStage::Merge,
        _ => return Err(format!("Invalid stage: {}", from_stage)),
    };

    let (repo, issue_number, issue_title, stored_labels) = {
        let s = state.lock().await;
        let run = s.runs.get(&run_id).ok_or("Run not found")?;
        if run.status != AgentStatus::Failed
            && run.status != AgentStatus::Stopped
            && run.status != AgentStatus::Interrupted
        {
            return Err("Can only retry failed, stopped, or interrupted runs".to_string());
        }
        (
            run.repo.clone(),
            run.issue_number,
            run.issue_title.clone(),
            run.issue_labels.clone(),
        )
    };

    // Check if the workspace still exists; fall back to full restart if not
    let workspace_exists = workspace::workspace_exists(&repo, issue_number);
    let effective_stage = if workspace_exists {
        target_stage
    } else {
        // Workspace was cleaned up — fall back to full restart from Implement
        let mut s = state.lock().await;
        if let Some(run) = s.runs.get_mut(&run_id) {
            run.logs.push(format!(
                "[retry] Workspace missing, falling back to full restart from implement (requested: {})",
                from_stage
            ));
        }
        s.persist();
        drop(s);
        let _ = app.emit(
            "retry-fallback",
            serde_json::json!({
                "run_id": &run_id,
                "requested_stage": &from_stage,
                "actual_stage": "implement",
                "reason": "workspace_missing",
            }),
        );
        PipelineStage::Implement
    };

    // Fetch the issue body and labels from GitHub
    let (body, issue_labels) = match crate::github::get_issue_detail(repo.clone(), issue_number).await {
        Ok(issue) => (issue.body.unwrap_or_default(), issue.labels),
        Err(_) => (String::new(), stored_labels),
    };

    launch_agent(
        app,
        state.inner().clone(),
        repo,
        issue_number,
        issue_title,
        body,
        effective_stage,
        issue_labels,
    )
    .await
}

#[tauri::command]
pub async fn approve_stage(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    run_id: String,
) -> Result<(), String> {
    let (repo, issue_number, issue_title, issue_body, next_stage, workspace_path, issue_labels, previous_context) = {
        let mut s = state.lock().await;
        let run = s
            .runs
            .get(&run_id)
            .ok_or("Run not found")?;
        if run.status != AgentStatus::AwaitingApproval {
            return Err(format!("Run {} is not awaiting approval (status: {:?})", run_id, run.status));
        }
        let next_stage_str = run.pending_next_stage.clone()
            .ok_or("No pending next stage found")?;
        let next_stage = match next_stage_str.as_str() {
            "implement" => PipelineStage::Implement,
            "code_review" => PipelineStage::CodeReview,
            "testing" => PipelineStage::Testing,
            "merge" => PipelineStage::Merge,
            "done" => PipelineStage::Done,
            _ => return Err(format!("Invalid pending stage: {}", next_stage_str)),
        };
        let info = (
            run.repo.clone(),
            run.issue_number,
            run.issue_title.clone(),
            String::new(),
            next_stage,
            run.workspace_path.clone(),
            run.issue_labels.clone(),
            run.stage_context.clone(),
        );

        // Update the run: mark as completed (gate passed), clear pending
        if let Some(run) = s.runs.get_mut(&run_id) {
            run.status = AgentStatus::Completed;
            run.pending_next_stage = None;
            run.logs.push("[approval] Stage approved by user".to_string());
        }
        s.persist();
        info
    };

    let _ = app.emit(
        "agent-status-changed",
        serde_json::json!({
            "run_id": &run_id,
            "status": "completed",
            "stage": "approved",
        }),
    );

    // Fetch the issue body from GitHub
    let body = match crate::github::get_issue_detail(repo.clone(), issue_number).await {
        Ok(issue) => issue.body.unwrap_or_default(),
        Err(_) => issue_body,
    };

    // If the next stage is Done (approval after Merge), handle it directly
    if next_stage == PipelineStage::Done {
        // The Done stage is handled inline in run_agent_process for Merge.
        // For approval gates after Merge, we just need to re-trigger the Done creation.
        // We'll spawn the next stage which will handle it.
        return Ok(());
    }

    // Spawn the next stage
    let state_clone = state.inner().clone();
    spawn_next_stage(
        app,
        state_clone,
        repo,
        issue_number,
        issue_title,
        body,
        next_stage,
        std::path::PathBuf::from(workspace_path),
        issue_labels,
        previous_context,
    );

    Ok(())
}

#[tauri::command]
pub async fn reject_stage(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    run_id: String,
) -> Result<(), String> {
    let (issue_number, stage_label) = {
        let mut s = state.lock().await;
        let run = s
            .runs
            .get(&run_id)
            .ok_or("Run not found")?;
        if run.status != AgentStatus::AwaitingApproval {
            return Err(format!("Run {} is not awaiting approval (status: {:?})", run_id, run.status));
        }
        let info = (run.issue_number, run.stage.to_string());

        if let Some(run) = s.runs.get_mut(&run_id) {
            run.status = AgentStatus::Failed;
            run.error = Some("Rejected by user".to_string());
            run.finished_at = Some(chrono::Utc::now().to_rfc3339());
            run.pending_next_stage = None;
            run.logs.push("[approval] Stage rejected by user".to_string());
        }
        s.persist();
        info
    };

    let _ = app.emit(
        "agent-status-changed",
        serde_json::json!({
            "run_id": &run_id,
            "status": "failed",
            "stage": &stage_label,
            "error": "Rejected by user",
        }),
    );

    // Send notification about rejection
    {
        let s = state.lock().await;
        if s.config.notifications_enabled {
            crate::notification::notify_pipeline_failed(
                &app,
                issue_number,
                &stage_label,
                s.config.notification_sound,
            );
        }
    }

    update_dock_badge(&state.inner().clone()).await;

    Ok(())
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
        s.persist();
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
