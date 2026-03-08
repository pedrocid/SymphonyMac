use crate::github::{Issue, PullRequest};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use super::scan::{IssueSnapshot, RepositorySnapshot};
use super::{AgentStatus, OrchestratorState, PipelineStage, RunConfig};

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub max_concurrent: usize,
    pub stage_limits: HashMap<String, usize>,
    pub priority_labels: Vec<String>,
    pub skip_labels: HashMap<String, Vec<String>>,
}

impl From<&RunConfig> for SchedulerConfig {
    fn from(config: &RunConfig) -> Self {
        Self {
            max_concurrent: config.max_concurrent,
            stage_limits: config.max_concurrent_by_stage.clone(),
            priority_labels: config.priority_labels.clone(),
            skip_labels: config.stage_skip_labels.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeSnapshot {
    pub active_count: usize,
    pub active_by_stage: HashMap<String, usize>,
    pub already_working: HashSet<(String, u64)>,
    pub fully_done: HashSet<(String, u64)>,
    pub has_any_run: HashSet<(String, u64)>,
}

impl RuntimeSnapshot {
    pub fn from_state(state: &OrchestratorState) -> Self {
        let mut snapshot = Self::default();

        for run in state.runs.values() {
            let key = (run.repo.clone(), run.issue_number);
            snapshot.has_any_run.insert(key.clone());

            if run.stage == PipelineStage::Done && run.status == AgentStatus::Completed {
                snapshot.fully_done.insert(key.clone());
            }

            if run.status == AgentStatus::Running || run.status == AgentStatus::Preparing {
                snapshot.active_count += 1;
                snapshot.already_working.insert(key);
                *snapshot
                    .active_by_stage
                    .entry(run.stage.to_string())
                    .or_insert(0) += 1;
            }
        }

        snapshot
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockedIssue {
    pub repo: String,
    pub issue_number: u64,
    pub blocked_by: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchDecision {
    pub repo: String,
    pub issue_number: u64,
    pub issue_title: String,
    pub issue_body: String,
    pub issue_labels: Vec<String>,
    pub stage: PipelineStage,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScheduleOutcome {
    pub launches: Vec<LaunchDecision>,
    pub blocked: Vec<BlockedIssue>,
    pub all_issues_accounted_for: bool,
}

/// Given an issue's labels and the configured skip-label mappings, return the
/// list of pipeline stages that should be skipped for this issue.
/// Only CodeReview and Testing can be skipped.
pub fn compute_skipped_stages(
    issue_labels: &[String],
    skip_labels: &HashMap<String, Vec<String>>,
) -> Vec<PipelineStage> {
    let mut skipped = Vec::new();
    for label in issue_labels {
        let label_lower = label.to_lowercase();
        for (skip_label, stages) in skip_labels {
            if label_lower == skip_label.to_lowercase() {
                for stage_name in stages {
                    let stage = match stage_name.as_str() {
                        "code_review" => PipelineStage::CodeReview,
                        "testing" => PipelineStage::Testing,
                        _ => continue,
                    };
                    if !skipped.contains(&stage) {
                        skipped.push(stage);
                    }
                }
            }
        }
    }
    skipped
}

/// Return the next pipeline stage after `current`, skipping any stages in `skipped`.
/// Returns None if there is no next stage.
pub fn next_pipeline_stage(
    current: &PipelineStage,
    skipped: &[PipelineStage],
) -> Option<PipelineStage> {
    let chain = [
        PipelineStage::Implement,
        PipelineStage::CodeReview,
        PipelineStage::Testing,
        PipelineStage::Merge,
    ];
    let current_idx = chain.iter().position(|stage| stage == current)?;
    for next_stage in &chain[current_idx + 1..] {
        if !skipped.contains(next_stage) {
            return Some(next_stage.clone());
        }
    }
    None
}

/// Check whether an approval gate is enabled for the given stage.
pub fn is_gate_enabled(config: &RunConfig, stage: &PipelineStage) -> bool {
    let stage_name = stage.to_string();
    config
        .approval_gates
        .get(&stage_name)
        .copied()
        .unwrap_or(false)
}

/// Check whether an additional agent for the given stage can be launched
/// without exceeding the per-stage or global concurrency limit.
pub fn can_launch_stage(state: &OrchestratorState, stage: &PipelineStage) -> bool {
    let runtime = RuntimeSnapshot::from_state(state);
    let config = SchedulerConfig::from(&state.config);

    if runtime.active_count >= config.max_concurrent {
        return false;
    }

    stage_has_capacity(
        &runtime.active_by_stage,
        &HashMap::new(),
        stage,
        &config.stage_limits,
    )
}

pub fn plan_dispatch(
    snapshot: &RepositorySnapshot,
    runtime: &RuntimeSnapshot,
    config: &SchedulerConfig,
) -> ScheduleOutcome {
    let mut issues = snapshot.issues.clone();
    sort_issues_for_dispatch(&mut issues, &config.priority_labels);

    let available_slots = config.max_concurrent.saturating_sub(runtime.active_count);
    let mut launches = Vec::new();
    let mut blocked = Vec::new();
    let mut stage_slots_used: HashMap<String, usize> = HashMap::new();

    for issue_snapshot in issues {
        let key = (issue_snapshot.repo.clone(), issue_snapshot.issue.number);
        if is_accounted_for(runtime, &key) {
            continue;
        }

        if !issue_snapshot.open_blockers.is_empty() {
            blocked.push(BlockedIssue {
                repo: issue_snapshot.repo.clone(),
                issue_number: issue_snapshot.issue.number,
                blocked_by: issue_snapshot.open_blockers.clone(),
            });
            continue;
        }

        if launches.len() >= available_slots {
            continue;
        }

        let launch = build_launch_decision(&issue_snapshot, &config.skip_labels);
        if stage_has_capacity(
            &runtime.active_by_stage,
            &stage_slots_used,
            &launch.stage,
            &config.stage_limits,
        ) {
            *stage_slots_used
                .entry(launch.stage.to_string())
                .or_insert(0) += 1;
            launches.push(launch);
        }
    }

    let all_issues_accounted_for = !snapshot.issues.is_empty()
        && snapshot.issues.iter().all(|issue_snapshot| {
            let key = (issue_snapshot.repo.clone(), issue_snapshot.issue.number);
            is_accounted_for(runtime, &key)
        });

    ScheduleOutcome {
        launches,
        blocked,
        all_issues_accounted_for,
    }
}

fn build_launch_decision(
    issue_snapshot: &IssueSnapshot,
    skip_labels: &HashMap<String, Vec<String>>,
) -> LaunchDecision {
    let skipped = compute_skipped_stages(&issue_snapshot.issue.labels, skip_labels);
    let (stage, issue_title, issue_body) = match issue_snapshot.pull_request.as_ref() {
        Some(pr) => build_pr_launch_context(pr, &skipped),
        None => (
            PipelineStage::Implement,
            issue_snapshot.issue.title.clone(),
            issue_snapshot.issue.body.clone().unwrap_or_default(),
        ),
    };

    LaunchDecision {
        repo: issue_snapshot.repo.clone(),
        issue_number: issue_snapshot.issue.number,
        issue_title,
        issue_body,
        issue_labels: issue_snapshot.issue.labels.clone(),
        stage,
    }
}

fn build_pr_launch_context(
    pull_request: &PullRequest,
    skipped: &[PipelineStage],
) -> (PipelineStage, String, String) {
    let mut start_stage = PipelineStage::CodeReview;
    if skipped.contains(&start_stage) {
        start_stage =
            next_pipeline_stage(&PipelineStage::Implement, skipped).unwrap_or(PipelineStage::Merge);
    }

    (
        start_stage,
        pull_request.title.clone(),
        pull_request.body.clone().unwrap_or_default(),
    )
}

fn is_accounted_for(runtime: &RuntimeSnapshot, key: &(String, u64)) -> bool {
    runtime.already_working.contains(key)
        || runtime.fully_done.contains(key)
        || runtime.has_any_run.contains(key)
}

fn stage_has_capacity(
    active_by_stage: &HashMap<String, usize>,
    stage_slots_used: &HashMap<String, usize>,
    stage: &PipelineStage,
    stage_limits: &HashMap<String, usize>,
) -> bool {
    let stage_name = stage.to_string();
    if let Some(&limit) = stage_limits.get(&stage_name) {
        if limit > 0 {
            let current = active_by_stage.get(&stage_name).copied().unwrap_or(0)
                + stage_slots_used.get(&stage_name).copied().unwrap_or(0);
            if current >= limit {
                return false;
            }
        }
    }

    true
}

/// Returns the priority rank of an issue based on its labels and the configured ordering.
/// Lower rank = higher priority. Issues without any priority label get `usize::MAX`.
fn issue_priority_rank(issue: &Issue, priority_labels: &[String]) -> usize {
    let mut best = usize::MAX;
    for label in &issue.labels {
        let label_lower = label.to_lowercase();
        for (rank, priority) in priority_labels.iter().enumerate() {
            if rank >= best {
                break;
            }
            if label_lower == priority.to_lowercase() {
                best = rank;
                break;
            }
        }
    }
    best
}

fn sort_issues_for_dispatch(issues: &mut [IssueSnapshot], priority_labels: &[String]) {
    issues.sort_by(|a, b| {
        let rank_a = issue_priority_rank(&a.issue, priority_labels);
        let rank_b = issue_priority_rank(&b.issue, priority_labels);
        rank_a
            .cmp(&rank_b)
            .then_with(|| a.issue.created_at.cmp(&b.issue.created_at))
            .then_with(|| a.issue.number.cmp(&b.issue.number))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::Issue;

    fn make_issue(number: u64, labels: Vec<&str>, created_at: &str) -> Issue {
        Issue {
            number,
            title: format!("Issue #{}", number),
            body: None,
            state: "OPEN".to_string(),
            labels: labels.into_iter().map(|label| label.to_string()).collect(),
            assignee: None,
            url: String::new(),
            created_at: created_at.to_string(),
            updated_at: String::new(),
        }
    }

    fn make_pr(issue_number: u64, title: &str) -> PullRequest {
        PullRequest {
            number: issue_number + 100,
            title: title.to_string(),
            body: Some(format!("Closes #{}", issue_number)),
            state: "OPEN".to_string(),
            head_branch: format!("issue-{}", issue_number),
            url: String::new(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            author: None,
            closes_issue: Some(issue_number),
        }
    }

    fn make_snapshot_issue(number: u64, labels: Vec<&str>, created_at: &str) -> IssueSnapshot {
        IssueSnapshot {
            repo: "pedrocid/SymphonyMac".to_string(),
            issue: make_issue(number, labels, created_at),
            pull_request: None,
            open_blockers: Vec::new(),
        }
    }

    fn base_config() -> SchedulerConfig {
        SchedulerConfig {
            max_concurrent: 3,
            stage_limits: HashMap::new(),
            priority_labels: vec![
                "priority:critical".to_string(),
                "priority:high".to_string(),
                "priority:medium".to_string(),
                "priority:low".to_string(),
            ],
            skip_labels: {
                let mut labels = HashMap::new();
                labels.insert(
                    "skip:code-review".to_string(),
                    vec!["code_review".to_string()],
                );
                labels.insert("skip:testing".to_string(), vec!["testing".to_string()]);
                labels.insert(
                    "docs-only".to_string(),
                    vec!["code_review".to_string(), "testing".to_string()],
                );
                labels
            },
        }
    }

    #[test]
    fn test_priority_rank_multiple_labels_uses_highest() {
        let labels = base_config().priority_labels;
        let issue = make_issue(
            1,
            vec!["priority:low", "priority:critical"],
            "2024-01-01T00:00:00Z",
        );
        assert_eq!(issue_priority_rank(&issue, &labels), 0);
    }

    #[test]
    fn test_sort_by_priority_then_date_then_number() {
        let labels = base_config().priority_labels;
        let mut issues = vec![
            make_snapshot_issue(3, vec!["priority:low"], "2024-01-01T00:00:00Z"),
            make_snapshot_issue(1, vec!["priority:critical"], "2024-01-02T00:00:00Z"),
            make_snapshot_issue(2, vec!["priority:high"], "2024-01-01T00:00:00Z"),
            make_snapshot_issue(4, vec![], "2024-01-01T00:00:00Z"),
            make_snapshot_issue(5, vec!["priority:critical"], "2024-01-01T00:00:00Z"),
        ];
        sort_issues_for_dispatch(&mut issues, &labels);

        let numbers: Vec<u64> = issues.iter().map(|issue| issue.issue.number).collect();
        assert_eq!(numbers, vec![5, 1, 2, 3, 4]);
    }

    #[test]
    fn test_compute_skipped_stages_cannot_skip_implement_or_merge() {
        let mut skip_labels = HashMap::new();
        skip_labels.insert(
            "skip-all".to_string(),
            vec![
                "implement".to_string(),
                "code_review".to_string(),
                "testing".to_string(),
                "merge".to_string(),
            ],
        );

        let skipped = compute_skipped_stages(&["skip-all".to_string()], &skip_labels);
        assert_eq!(
            skipped,
            vec![PipelineStage::CodeReview, PipelineStage::Testing]
        );
    }

    #[test]
    fn test_next_pipeline_stage_skips_review_and_testing() {
        let skipped = vec![PipelineStage::CodeReview, PipelineStage::Testing];
        assert_eq!(
            next_pipeline_stage(&PipelineStage::Implement, &skipped),
            Some(PipelineStage::Merge)
        );
    }

    #[test]
    fn test_plan_dispatch_orders_launches_by_priority_and_age() {
        let snapshot = RepositorySnapshot {
            issues: vec![
                make_snapshot_issue(3, vec!["priority:low"], "2024-01-03T00:00:00Z"),
                make_snapshot_issue(2, vec!["priority:critical"], "2024-01-02T00:00:00Z"),
                make_snapshot_issue(1, vec!["priority:critical"], "2024-01-01T00:00:00Z"),
            ],
            fetch_errors: Vec::new(),
        };

        let outcome = plan_dispatch(&snapshot, &RuntimeSnapshot::default(), &base_config());
        let issue_numbers: Vec<u64> = outcome
            .launches
            .iter()
            .map(|launch| launch.issue_number)
            .collect();

        assert_eq!(issue_numbers, vec![1, 2, 3]);
    }

    #[test]
    fn test_plan_dispatch_reports_blocked_issues_without_launching_them() {
        let mut blocked_issue = make_snapshot_issue(10, vec![], "2024-01-01T00:00:00Z");
        blocked_issue.open_blockers = vec![5, 7];

        let ready_issue = make_snapshot_issue(11, vec![], "2024-01-02T00:00:00Z");
        let snapshot = RepositorySnapshot {
            issues: vec![blocked_issue, ready_issue],
            fetch_errors: Vec::new(),
        };

        let outcome = plan_dispatch(&snapshot, &RuntimeSnapshot::default(), &base_config());

        assert_eq!(
            outcome.blocked,
            vec![BlockedIssue {
                repo: "pedrocid/SymphonyMac".to_string(),
                issue_number: 10,
                blocked_by: vec![5, 7],
            }]
        );
        assert_eq!(outcome.launches.len(), 1);
        assert_eq!(outcome.launches[0].issue_number, 11);
    }

    #[test]
    fn test_plan_dispatch_uses_skip_labels_when_pr_already_exists() {
        let mut issue = make_snapshot_issue(20, vec!["skip:code-review"], "2024-01-01T00:00:00Z");
        issue.pull_request = Some(make_pr(20, "Fix #20: Existing PR"));

        let snapshot = RepositorySnapshot {
            issues: vec![issue],
            fetch_errors: Vec::new(),
        };

        let outcome = plan_dispatch(&snapshot, &RuntimeSnapshot::default(), &base_config());

        assert_eq!(outcome.launches.len(), 1);
        assert_eq!(outcome.launches[0].stage, PipelineStage::Testing);
        assert_eq!(outcome.launches[0].issue_title, "Fix #20: Existing PR");
    }

    #[test]
    fn test_plan_dispatch_respects_per_stage_limits_and_considers_later_issues() {
        let mut review_issue = make_snapshot_issue(30, vec![], "2024-01-01T00:00:00Z");
        review_issue.pull_request = Some(make_pr(30, "Fix #30: Needs review"));
        let implement_issue = make_snapshot_issue(31, vec![], "2024-01-02T00:00:00Z");

        let snapshot = RepositorySnapshot {
            issues: vec![review_issue, implement_issue],
            fetch_errors: Vec::new(),
        };

        let mut runtime = RuntimeSnapshot::default();
        runtime.active_count = 1;
        runtime
            .active_by_stage
            .insert(PipelineStage::CodeReview.to_string(), 1);

        let mut config = base_config();
        config.max_concurrent = 3;
        config
            .stage_limits
            .insert(PipelineStage::CodeReview.to_string(), 1);

        let outcome = plan_dispatch(&snapshot, &runtime, &config);

        assert_eq!(outcome.launches.len(), 1);
        assert_eq!(outcome.launches[0].issue_number, 31);
        assert_eq!(outcome.launches[0].stage, PipelineStage::Implement);
    }
}
