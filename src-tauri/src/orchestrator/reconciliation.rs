use crate::github;
use crate::SharedState;
use chrono::Utc;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter};

use super::AgentStatus;

#[derive(Debug, Clone)]
struct ActiveRunSnapshot {
    run_id: String,
    issue_number: u64,
    repo: String,
}

#[derive(Debug, Clone)]
struct ClosedIssueAction {
    run_id: String,
    issue_number: u64,
    repo: String,
}

pub async fn reconcile_active_runs(app: &AppHandle, state: &SharedState) {
    let active_runs = collect_active_runs(state).await;
    if active_runs.is_empty() {
        return;
    }

    let closed_issues = resolve_closed_issues(&active_runs).await;
    for action in closed_issues {
        let _ = app.emit(
            "orchestrator-reconcile",
            serde_json::json!({
                "run_id": action.run_id,
                "issue_number": action.issue_number,
                "reason": "issue_closed",
            }),
        );

        apply_closed_issue_action(app, state, action).await;
    }
}

async fn collect_active_runs(state: &SharedState) -> Vec<ActiveRunSnapshot> {
    let s = state.lock().await;
    s.runs
        .values()
        .filter(|run| run.status == AgentStatus::Running || run.status == AgentStatus::Preparing)
        .map(|run| ActiveRunSnapshot {
            run_id: run.id.clone(),
            issue_number: run.issue_number,
            repo: run.repo.clone(),
        })
        .collect()
}

async fn resolve_closed_issues(active_runs: &[ActiveRunSnapshot]) -> Vec<ClosedIssueAction> {
    let mut checked_issues: HashMap<(String, u64), bool> = HashMap::new();
    let mut actions = Vec::new();

    for active_run in active_runs {
        let cache_key = (active_run.repo.clone(), active_run.issue_number);
        let is_open = if let Some(&cached) = checked_issues.get(&cache_key) {
            cached
        } else {
            let is_open = github::get_issue_state(&active_run.repo, active_run.issue_number)
                .await
                .map(|state| state == "OPEN")
                .unwrap_or(true);
            checked_issues.insert(cache_key, is_open);
            is_open
        };

        if !is_open {
            actions.push(ClosedIssueAction {
                run_id: active_run.run_id.clone(),
                issue_number: active_run.issue_number,
                repo: active_run.repo.clone(),
            });
        }
    }

    actions
}

async fn apply_closed_issue_action(
    app: &AppHandle,
    state: &SharedState,
    action: ClosedIssueAction,
) {
    let mut s = state.lock().await;
    if let Some(pid) = s.agent_pids.remove(&action.run_id) {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    if let Some(run) = s.runs.get_mut(&action.run_id) {
        run.status = AgentStatus::Stopped;
        run.finished_at = Some(Utc::now().to_rfc3339());
        run.logs.push(format!(
            "[reconciler] Stopped: issue #{} was closed externally",
            action.issue_number
        ));
        run.error = Some(format!("Issue #{} closed externally", action.issue_number));
    }
    s.persist();
    let should_cleanup = s.config.cleanup_on_stop;
    let hooks = s.config.hooks.clone();
    drop(s);

    let _ = app.emit(
        "agent-status-changed",
        serde_json::json!({
            "run_id": action.run_id,
            "status": "stopped",
            "reason": "issue_closed",
        }),
    );

    if should_cleanup {
        let _ = crate::workspace::cleanup_workspace(&action.repo, action.issue_number, &hooks);
    }
}
