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

        let pull_requests = github::list_open_prs(repo.clone())
            .await
            .unwrap_or_default();
        let mut prs_by_issue: HashMap<u64, github::PullRequest> = HashMap::new();
        for pr in pull_requests {
            let issue_number = pr
                .closes_issue
                .or_else(|| github::parse_issue_from_title(&pr.title));
            if let Some(issue_number) = issue_number {
                prs_by_issue.insert(issue_number, pr);
            }
        }

        for issue in issues {
            let blocker_numbers = github::parse_blockers(issue.body.as_deref().unwrap_or(""));
            let open_blockers = if blocker_numbers.is_empty() {
                Vec::new()
            } else {
                github::check_blockers_open(repo, &blocker_numbers)
            };

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
