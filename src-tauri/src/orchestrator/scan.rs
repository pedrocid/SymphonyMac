use crate::github;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct IssueSnapshot {
    pub repo: String,
    pub issue: github::Issue,
    pub pull_request: Option<github::PullRequest>,
    pub open_blockers: Vec<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct RepositorySnapshot {
    pub issues: Vec<IssueSnapshot>,
    pub fetch_errors: Vec<String>,
}

impl RepositorySnapshot {
    pub fn has_fetch_errors(&self) -> bool {
        !self.fetch_errors.is_empty()
    }
}

pub async fn collect_repository_snapshot(
    repos: &[String],
    label: Option<String>,
) -> RepositorySnapshot {
    let mut snapshot = RepositorySnapshot::default();

    for repo in repos {
        let issues = match github::list_issues(
            repo.clone(),
            Some("open".to_string()),
            label.clone(),
        )
        .await
        {
            Ok(issues) => issues,
            Err(error) => {
                snapshot
                    .fetch_errors
                    .push(format!("Failed to fetch issues from {}: {}", repo, error));
                continue;
            }
        };

        let pull_requests = match github::list_open_prs(repo.clone()).await {
            Ok(pull_requests) => pull_requests,
            Err(error) => {
                snapshot
                    .fetch_errors
                    .push(format!("Failed to fetch open PRs from {}: {}", repo, error));
                continue;
            }
        };
        let mut prs_by_issue: HashMap<u64, github::PullRequest> = HashMap::new();
        for pr in pull_requests {
            let issue_number = pr
                .closes_issue
                .or_else(|| github::parse_issue_from_title(&pr.title));
            if let Some(issue_number) = issue_number {
                prs_by_issue.insert(issue_number, pr);
            }
        }

        // Collect all blocker numbers across issues for batch resolution
        let mut all_blocker_numbers: Vec<u64> = Vec::new();
        let mut issue_blockers: Vec<(github::Issue, Vec<u64>)> = Vec::new();
        for issue in issues {
            let blocker_numbers = github::parse_blockers(issue.body.as_deref().unwrap_or(""));
            for &num in &blocker_numbers {
                if !all_blocker_numbers.contains(&num) {
                    all_blocker_numbers.push(num);
                }
            }
            issue_blockers.push((issue, blocker_numbers));
        }

        // Batch-resolve blocker states with a single GraphQL call
        let blocker_states = if all_blocker_numbers.is_empty() {
            HashMap::new()
        } else {
            github::get_issue_states(repo, &all_blocker_numbers)
                .await
                .unwrap_or_default()
        };

        for (issue, blocker_numbers) in issue_blockers {
            let open_blockers: Vec<u64> = blocker_numbers
                .iter()
                .filter(|num| {
                    blocker_states
                        .get(num)
                        .map(|state| state == "OPEN")
                        .unwrap_or(true) // assume blocking on unknown state
                })
                .copied()
                .collect();

            snapshot.issues.push(IssueSnapshot {
                repo: repo.clone(),
                pull_request: prs_by_issue.get(&issue.number).cloned(),
                issue,
                open_blockers,
            });
        }
    }

    snapshot
}
