use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repo {
    pub full_name: String,
    pub name: String,
    pub owner: String,
    pub description: Option<String>,
    pub url: String,
    pub default_branch: String,
    pub is_private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub labels: Vec<String>,
    pub assignee: Option<String>,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
}

fn run_gh(args: &[&str]) -> Result<String, String> {
    let output = Command::new(crate::paths::resolve("gh"))
        .env("PATH", crate::paths::build_path_env())
        .args(args)
        .output()
        .map_err(|e| {
            format!(
                "Failed to run gh CLI: {}. Make sure gh is installed and authenticated.",
                e
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh command failed: {}", stderr));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8 output: {}", e))
}

#[tauri::command]
pub async fn list_repos(filter: Option<String>) -> Result<Vec<Repo>, String> {
    let limit = "100";
    let json_fields = "nameWithOwner,name,owner,description,url,defaultBranchRef,isPrivate";

    let output = run_gh(&["repo", "list", "--limit", limit, "--json", json_fields])?;

    let raw: Vec<serde_json::Value> =
        serde_json::from_str(&output).map_err(|e| format!("Failed to parse repos JSON: {}", e))?;

    let mut repos: Vec<Repo> = raw
        .into_iter()
        .map(|v| {
            let owner_obj = &v["owner"];
            let owner_login = owner_obj["login"].as_str().unwrap_or("").to_string();
            Repo {
                full_name: v["nameWithOwner"].as_str().unwrap_or("").to_string(),
                name: v["name"].as_str().unwrap_or("").to_string(),
                owner: owner_login,
                description: v["description"].as_str().map(|s| s.to_string()),
                url: v["url"].as_str().unwrap_or("").to_string(),
                default_branch: v["defaultBranchRef"]
                    .as_object()
                    .and_then(|o| o["name"].as_str())
                    .unwrap_or("main")
                    .to_string(),
                is_private: v["isPrivate"].as_bool().unwrap_or(false),
            }
        })
        .collect();

    if let Some(f) = filter {
        let f_lower = f.to_lowercase();
        repos.retain(|r| r.full_name.to_lowercase().contains(&f_lower));
    }

    Ok(repos)
}

#[tauri::command]
pub async fn list_issues(
    repo: String,
    state: Option<String>,
    label: Option<String>,
) -> Result<Vec<Issue>, String> {
    let json_fields = "number,title,body,state,labels,assignees,url,createdAt,updatedAt";
    let state_filter = state.unwrap_or_else(|| "open".to_string());

    let mut args = vec![
        "issue",
        "list",
        "-R",
        &repo,
        "--state",
        &state_filter,
        "--limit",
        "100",
        "--json",
        json_fields,
    ];

    let label_owned;
    if let Some(ref l) = label {
        label_owned = l.clone();
        args.push("--label");
        args.push(&label_owned);
    }

    let output = run_gh(&args)?;

    let raw: Vec<serde_json::Value> =
        serde_json::from_str(&output).map_err(|e| format!("Failed to parse issues JSON: {}", e))?;

    let issues: Vec<Issue> = raw
        .into_iter()
        .map(|v| {
            let labels = v["labels"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|l| l["name"].as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let assignee = v["assignees"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|a| a["login"].as_str())
                .map(|s| s.to_string());

            Issue {
                number: v["number"].as_u64().unwrap_or(0),
                title: v["title"].as_str().unwrap_or("").to_string(),
                body: v["body"].as_str().map(|s| s.to_string()),
                state: v["state"].as_str().unwrap_or("OPEN").to_string(),
                labels,
                assignee,
                url: v["url"].as_str().unwrap_or("").to_string(),
                created_at: v["createdAt"].as_str().unwrap_or("").to_string(),
                updated_at: v["updatedAt"].as_str().unwrap_or("").to_string(),
            }
        })
        .collect();

    Ok(issues)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub head_branch: String,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
    pub author: Option<String>,
    /// The issue number this PR closes, extracted from body "Closes #N"
    pub closes_issue: Option<u64>,
}

pub async fn list_open_prs(repo: String) -> Result<Vec<PullRequest>, String> {
    let json_fields = "number,title,body,state,headRefName,url,createdAt,updatedAt,author";

    let output = run_gh(&[
        "pr",
        "list",
        "-R",
        &repo,
        "--state",
        "open",
        "--limit",
        "100",
        "--json",
        json_fields,
    ])?;

    let raw: Vec<serde_json::Value> =
        serde_json::from_str(&output).map_err(|e| format!("Failed to parse PRs JSON: {}", e))?;

    let prs: Vec<PullRequest> = raw
        .into_iter()
        .map(|v| {
            let body = v["body"].as_str().map(|s| s.to_string());
            let closes_issue = body.as_ref().and_then(|b| parse_closes_issue(b));

            PullRequest {
                number: v["number"].as_u64().unwrap_or(0),
                title: v["title"].as_str().unwrap_or("").to_string(),
                body,
                state: v["state"].as_str().unwrap_or("OPEN").to_string(),
                head_branch: v["headRefName"].as_str().unwrap_or("").to_string(),
                url: v["url"].as_str().unwrap_or("").to_string(),
                created_at: v["createdAt"].as_str().unwrap_or("").to_string(),
                updated_at: v["updatedAt"].as_str().unwrap_or("").to_string(),
                author: v["author"]
                    .as_object()
                    .and_then(|o| o["login"].as_str())
                    .map(|s| s.to_string()),
                closes_issue,
            }
        })
        .collect();

    Ok(prs)
}

/// Parse blocker references from issue body text.
/// Looks for patterns like "blocked by #X", "depends on #X", "requires #X".
pub fn parse_blockers(text: &str) -> Vec<u64> {
    let text_lower = text.to_lowercase();
    let mut blockers = Vec::new();
    let patterns = [
        "blocked by #",
        "depends on #",
        "requires #",
        "waiting on #",
        "waiting for #",
        "after #",
    ];

    for pattern in &patterns {
        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(pattern) {
            let abs_pos = search_from + pos + pattern.len();
            let after = &text[abs_pos..];
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.parse::<u64>() {
                if n > 0 && !blockers.contains(&n) {
                    blockers.push(n);
                }
            }
            search_from = abs_pos;
        }
    }

    blockers
}

/// Check which of the given blocker issue numbers are still open in the repo.
/// Returns the list of issue numbers that are still open.
pub fn check_blockers_open(repo: &str, blocker_numbers: &[u64]) -> Vec<u64> {
    let mut open_blockers = Vec::new();
    for &num in blocker_numbers {
        let num_str = num.to_string();
        match run_gh(&[
            "issue",
            "view",
            &num_str,
            "-R",
            repo,
            "--json",
            "state",
        ]) {
            Ok(output) => {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&output) {
                    if v["state"].as_str() == Some("OPEN") {
                        open_blockers.push(num);
                    }
                }
            }
            Err(_) => {
                // If we can't check, assume it's still blocking to be safe
                open_blockers.push(num);
            }
        }
    }
    open_blockers
}

/// Parse "Closes #123" or "Fixes #123" from PR body
fn parse_closes_issue(body: &str) -> Option<u64> {
    let body_lower = body.to_lowercase();
    for keyword in &[
        "closes #",
        "fixes #",
        "resolves #",
        "close #",
        "fix #",
        "resolve #",
    ] {
        if let Some(pos) = body_lower.find(keyword) {
            let after = &body[pos + keyword.len()..];
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.parse::<u64>() {
                return Some(n);
            }
        }
    }
    // Also try to extract from title pattern "Fix #123"
    None
}

/// Parse issue number from PR title like "Fix #14: ..."
pub fn parse_issue_from_title(title: &str) -> Option<u64> {
    let title_lower = title.to_lowercase();
    for keyword in &[
        "fix #",
        "fixes #",
        "closes #",
        "resolve #",
        "resolves #",
        "close #",
        "feat #",
        "issue #",
    ] {
        if let Some(pos) = title_lower.find(keyword) {
            let after = &title[pos + keyword.len()..];
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.parse::<u64>() {
                return Some(n);
            }
        }
    }
    // Try pattern "#123" anywhere
    for (i, c) in title.chars().enumerate() {
        if c == '#' {
            let after = &title[i + 1..];
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.parse::<u64>() {
                if n > 0 {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Check if a PR associated with a given issue number is actually merged.
/// Returns Ok(true) if merged, Ok(false) if still open/closed-not-merged, Err on failure.
pub fn is_pr_merged_for_issue(repo: &str, issue_number: u64) -> Result<bool, String> {
    // List all PRs (open and merged) to find one that references this issue
    let output = run_gh(&[
        "pr",
        "list",
        "-R",
        repo,
        "--state",
        "all",
        "--limit",
        "50",
        "--json",
        "number,title,body,state",
    ])?;

    let prs: Vec<serde_json::Value> =
        serde_json::from_str(&output).map_err(|e| format!("Failed to parse PRs: {}", e))?;

    for pr in &prs {
        let body = pr["body"].as_str().unwrap_or("");
        let title = pr["title"].as_str().unwrap_or("");
        let state = pr["state"].as_str().unwrap_or("");

        let references_issue = parse_closes_issue(body) == Some(issue_number)
            || parse_issue_from_title(title) == Some(issue_number);

        if references_issue {
            return Ok(state == "MERGED");
        }
    }

    // No PR found for this issue
    Err(format!("No PR found referencing issue #{}", issue_number))
}

#[tauri::command]
pub async fn get_issue_detail(repo: String, number: u64) -> Result<Issue, String> {
    let num_str = number.to_string();
    let json_fields = "number,title,body,state,labels,assignees,url,createdAt,updatedAt";

    let output = run_gh(&[
        "issue",
        "view",
        &num_str,
        "-R",
        &repo,
        "--json",
        json_fields,
    ])?;

    let v: serde_json::Value =
        serde_json::from_str(&output).map_err(|e| format!("Failed to parse issue JSON: {}", e))?;

    let labels = v["labels"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|l| l["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let assignee = v["assignees"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|a| a["login"].as_str())
        .map(|s| s.to_string());

    Ok(Issue {
        number: v["number"].as_u64().unwrap_or(0),
        title: v["title"].as_str().unwrap_or("").to_string(),
        body: v["body"].as_str().map(|s| s.to_string()),
        state: v["state"].as_str().unwrap_or("OPEN").to_string(),
        labels,
        assignee,
        url: v["url"].as_str().unwrap_or("").to_string(),
        created_at: v["createdAt"].as_str().unwrap_or("").to_string(),
        updated_at: v["updatedAt"].as_str().unwrap_or("").to_string(),
    })
}
