use crate::orchestrator::AgentRun;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageReport {
    pub name: String,
    pub status: String,
    pub duration_secs: Option<i64>,
    pub duration_display: String,
    pub files_modified: Vec<String>,
    pub lines_added: u32,
    pub lines_removed: u32,
    pub commands_executed: Vec<String>,
    pub summary: String,
    pub attempt: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineReport {
    pub issue_number: u64,
    pub issue_title: String,
    pub repo: String,
    pub total_duration_secs: i64,
    pub total_duration_display: String,
    pub stages: Vec<StageReport>,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
    pub issue_url: String,
    pub code_review_summary: String,
    pub testing_summary: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
}

pub fn generate_report(
    issue_number: u64,
    issue_title: &str,
    repo: &str,
    stage_runs: Vec<&AgentRun>,
) -> PipelineReport {
    let mut stages: Vec<StageReport> = Vec::new();
    let mut total_duration: i64 = 0;
    let mut pr_number: Option<u64> = None;
    let mut pr_url: Option<String> = None;
    let mut code_review_summary = String::new();
    let mut testing_summary = String::new();
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut total_cost_usd: f64 = 0.0;

    let stage_order = ["implement", "code_review", "testing", "merge"];

    for stage_name in &stage_order {
        let matching: Vec<&&AgentRun> = stage_runs
            .iter()
            .filter(|r| r.stage.to_string() == *stage_name)
            .collect();

        if let Some(run) = matching.iter().max_by_key(|r| r.started_at.clone()) {
            let duration_secs = calculate_duration(&run.started_at, &run.finished_at);
            total_duration += duration_secs.unwrap_or(0);

            // Use stored diff stats from git (captured after agent success)
            // Fall back to log parsing only if stored stats are empty
            let files = if !run.files_modified_list.is_empty() {
                run.files_modified_list.clone()
            } else {
                extract_files_modified(&run.logs)
            };
            let (added, removed) = if run.lines_added > 0 || run.lines_removed > 0 {
                (run.lines_added, run.lines_removed)
            } else {
                extract_diff_stats(&run.logs)
            };
            let commands = extract_commands(&run.logs);
            let summary = extract_stage_summary(stage_name, &run.logs);

            if *stage_name == "implement" || *stage_name == "merge" {
                let (num, url) = extract_pr_info(&run.logs);
                if pr_number.is_none() {
                    pr_number = num;
                }
                if pr_url.is_none() {
                    pr_url = url;
                }
            }

            if *stage_name == "code_review" {
                code_review_summary = summary.clone();
            }
            if *stage_name == "testing" {
                testing_summary = summary.clone();
            }

            total_input_tokens += run.input_tokens;
            total_output_tokens += run.output_tokens;
            total_cost_usd += run.cost_usd;

            stages.push(StageReport {
                name: format_stage_name(stage_name),
                status: run.status.clone().into(),
                duration_secs,
                duration_display: format_duration(duration_secs.unwrap_or(0)),
                files_modified: files,
                lines_added: added,
                lines_removed: removed,
                commands_executed: commands,
                summary,
                attempt: run.attempt,
            });
        }
    }

    let issue_url = format!("https://github.com/{}/issues/{}", repo, issue_number);

    PipelineReport {
        issue_number,
        issue_title: issue_title.to_string(),
        repo: repo.to_string(),
        total_duration_secs: total_duration,
        total_duration_display: format_duration(total_duration),
        stages,
        pr_number,
        pr_url,
        issue_url,
        code_review_summary,
        testing_summary,
        total_input_tokens,
        total_output_tokens,
        total_cost_usd,
    }
}

impl From<crate::orchestrator::AgentStatus> for String {
    fn from(s: crate::orchestrator::AgentStatus) -> Self {
        match s {
            crate::orchestrator::AgentStatus::Preparing => "preparing".to_string(),
            crate::orchestrator::AgentStatus::Running => "running".to_string(),
            crate::orchestrator::AgentStatus::Completed => "completed".to_string(),
            crate::orchestrator::AgentStatus::Failed => "failed".to_string(),
            crate::orchestrator::AgentStatus::Stopped => "stopped".to_string(),
            crate::orchestrator::AgentStatus::Interrupted => "interrupted".to_string(),
            crate::orchestrator::AgentStatus::AwaitingApproval => "awaiting_approval".to_string(),
        }
    }
}

fn calculate_duration(started_at: &str, finished_at: &Option<String>) -> Option<i64> {
    let start = chrono::DateTime::parse_from_rfc3339(started_at).ok()?;
    let end = finished_at
        .as_ref()
        .and_then(|f| chrono::DateTime::parse_from_rfc3339(f).ok())
        .unwrap_or_else(|| chrono::Utc::now().into());
    Some((end - start).num_seconds())
}

fn format_duration(secs: i64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn format_stage_name(name: &str) -> String {
    match name {
        "implement" => "Implement".to_string(),
        "code_review" => "Code Review".to_string(),
        "testing" => "Testing".to_string(),
        "merge" => "Merge".to_string(),
        _ => name.to_string(),
    }
}

fn extract_files_modified(logs: &[String]) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for line in logs {
        // Match patterns like "modified: src/foo.rs" or "create src/foo.rs"
        // Also match git diff output like " src/foo.rs | 10 +++---"
        if let Some(rest) = line.strip_prefix("modified:") {
            let f = rest.trim().to_string();
            if !files.contains(&f) {
                files.push(f);
            }
        } else if line.contains("| ") && !line.starts_with("[stderr]") {
            // git diff --stat format: " src/file.rs | 5 ++-"
            let trimmed = line.trim();
            if let Some(pos) = trimmed.find(" | ") {
                let file = trimmed[..pos].trim().to_string();
                if file.contains('.') && !file.contains(' ') && !files.contains(&file) {
                    files.push(file);
                }
            }
        }
        // Match "Create file.rs", "Edit file.rs", "Write file.rs"
        for prefix in &["Create ", "Edit ", "Write "] {
            if let Some(rest) = line.strip_prefix(prefix) {
                let f = rest.trim().to_string();
                if f.contains('.') && !f.contains(' ') && !files.contains(&f) {
                    files.push(f);
                }
            }
        }
    }
    files
}

fn extract_diff_stats(logs: &[String]) -> (u32, u32) {
    let mut added: u32 = 0;
    let mut removed: u32 = 0;
    for line in logs {
        // Match "X insertions(+), Y deletions(-)" from git diff --stat
        let trimmed = line.trim();
        if trimmed.contains("insertion") && trimmed.contains("deletion") {
            for part in trimmed.split(',') {
                let part = part.trim();
                if part.contains("insertion") {
                    if let Some(num) = part.split_whitespace().next() {
                        added += num.parse::<u32>().unwrap_or(0);
                    }
                }
                if part.contains("deletion") {
                    if let Some(num) = part.split_whitespace().next() {
                        removed += num.parse::<u32>().unwrap_or(0);
                    }
                }
            }
        } else if trimmed.contains("insertion") {
            for part in trimmed.split(',') {
                let part = part.trim();
                if part.contains("insertion") {
                    if let Some(num) = part.split_whitespace().next() {
                        added += num.parse::<u32>().unwrap_or(0);
                    }
                }
            }
        } else if trimmed.contains("deletion") {
            for part in trimmed.split(',') {
                let part = part.trim();
                if part.contains("deletion") {
                    if let Some(num) = part.split_whitespace().next() {
                        removed += num.parse::<u32>().unwrap_or(0);
                    }
                }
            }
        }
    }
    (added, removed)
}

fn extract_commands(logs: &[String]) -> Vec<String> {
    let mut commands: Vec<String> = Vec::new();
    let command_prefixes = ["$ ", "> ", "Running: ", "Executing: "];
    for line in logs {
        let trimmed = line.trim();
        for prefix in &command_prefixes {
            if let Some(cmd) = trimmed.strip_prefix(prefix) {
                let cmd = cmd.trim().to_string();
                if !cmd.is_empty() && !commands.contains(&cmd) {
                    commands.push(cmd);
                }
            }
        }
        // Also match gh commands mentioned in logs
        if trimmed.starts_with("gh ") && !commands.contains(&trimmed.to_string()) {
            commands.push(trimmed.to_string());
        }
    }
    commands
}

fn extract_stage_summary(stage_name: &str, logs: &[String]) -> String {
    let log_text = logs.join("\n");

    match stage_name {
        "code_review" => {
            // Look for review comments or summary
            for line in logs.iter().rev() {
                if line.contains("Code review completed")
                    || line.contains("review completed")
                    || line.contains("LGTM")
                    || line.contains("looks good")
                {
                    return line.trim().to_string();
                }
            }
            if log_text.contains("fix") || log_text.contains("Fix") {
                return "Issues found and fixed during review".to_string();
            }
            "Code review completed".to_string()
        }
        "testing" => {
            // Look for test results
            for line in logs.iter().rev() {
                if line.contains("tests pass")
                    || line.contains("All tests")
                    || line.contains("test result")
                    || line.contains("Tests passed")
                {
                    return line.trim().to_string();
                }
            }
            "Testing completed".to_string()
        }
        "implement" => {
            for line in logs.iter().rev() {
                if line.contains("PR created")
                    || line.contains("pull request")
                    || line.contains("Pull Request")
                {
                    return line.trim().to_string();
                }
            }
            "Implementation completed".to_string()
        }
        "merge" => {
            for line in logs.iter().rev() {
                if line.contains("merged") || line.contains("Merged") {
                    return line.trim().to_string();
                }
            }
            "PR merged successfully".to_string()
        }
        _ => "Completed".to_string(),
    }
}

fn extract_pr_info(logs: &[String]) -> (Option<u64>, Option<String>) {
    let mut pr_number: Option<u64> = None;
    let mut pr_url: Option<String> = None;

    for line in logs {
        // Match GitHub PR URLs like https://github.com/owner/repo/pull/123
        if line.contains("github.com") && line.contains("/pull/") {
            if let Some(pos) = line.find("/pull/") {
                let after = &line[pos + 6..];
                let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = num_str.parse::<u64>() {
                    pr_number = Some(n);
                }
                // Extract full URL
                if let Some(start) = line.find("https://github.com") {
                    let url: String = line[start..]
                        .chars()
                        .take_while(|c| !c.is_whitespace())
                        .collect();
                    pr_url = Some(url);
                }
            }
        }
    }
    (pr_number, pr_url)
}
