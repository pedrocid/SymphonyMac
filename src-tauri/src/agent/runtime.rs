use super::AgentLogLine;
use crate::logs;
use crate::orchestrator::{AgentRun, AgentStatus};
use crate::SharedState;
use chrono::Utc;
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PendingNextStageUpdate {
    Keep,
    Clear,
    Set(String),
}

pub(crate) struct StatusTransition {
    pub status: AgentStatus,
    pub stage_label: String,
    pub error: Option<String>,
    pub finished: bool,
    pub log_message: Option<String>,
    pub pending_next_stage: PendingNextStageUpdate,
    pub emit_extra: Map<String, Value>,
    pub persist_meta: bool,
}

pub(crate) async fn register_preparing_run(
    app: &AppHandle,
    state: &SharedState,
    run: AgentRun,
    emit_extra: Map<String, Value>,
) {
    let run_id = run.id.clone();
    let stage_label = run.stage.to_string();

    {
        let mut s = state.lock().await;
        s.runs.insert(run_id.clone(), run.clone());
        s.persist();
    }

    persist_run_meta(&run, status_label(&AgentStatus::Preparing));
    emit_status(
        app,
        &run_id,
        status_label(&AgentStatus::Preparing),
        &stage_label,
        emit_extra,
    );
    update_dock_badge(state).await;
}

pub(crate) async fn mutate_run<R, F>(
    state: &SharedState,
    run_id: &str,
    persist: bool,
    update: F,
) -> Option<R>
where
    F: FnOnce(&mut AgentRun) -> R,
{
    let mut s = state.lock().await;
    let result = s.runs.get_mut(run_id).map(update);
    if persist && result.is_some() {
        s.persist();
    }
    result
}

pub(crate) async fn transition_run(
    app: &AppHandle,
    state: &SharedState,
    run_id: &str,
    transition: StatusTransition,
) -> Option<AgentRun> {
    let mut snapshot = None;

    {
        let mut s = state.lock().await;
        if let Some(run) = s.runs.get_mut(run_id) {
            run.status = transition.status.clone();
            if let Some(error) = transition.error.clone() {
                run.error = Some(error);
            }
            if transition.finished {
                run.finished_at = Some(Utc::now().to_rfc3339());
            }
            match transition.pending_next_stage {
                PendingNextStageUpdate::Keep => {}
                PendingNextStageUpdate::Clear => run.pending_next_stage = None,
                PendingNextStageUpdate::Set(next_stage) => run.pending_next_stage = Some(next_stage),
            }
            if let Some(log_message) = transition.log_message.clone() {
                run.logs.push(log_message);
            }
            snapshot = Some(run.clone());
        }

        if snapshot.is_some() {
            s.persist();
        }
    }

    if let Some(run) = snapshot.clone() {
        let status = status_label(&transition.status);
        emit_status(app, &run.id, status, &transition.stage_label, transition.emit_extra);
        if transition.persist_meta {
            persist_run_meta(&run, status);
        }
    }

    snapshot
}

pub(crate) async fn append_run_log(
    state: &SharedState,
    run_id: &str,
    message: String,
    persist_state: bool,
    write_to_disk: bool,
) {
    if write_to_disk {
        logs::append_log_line(run_id, &message);
    }

    let log_message = message.clone();
    let _ = mutate_run(state, run_id, persist_state, move |run| {
        run.logs.push(log_message);
    })
    .await;
}

pub(crate) async fn record_output_line(
    app: &AppHandle,
    state: &SharedState,
    run_id: &str,
    line: String,
    activity: Option<String>,
) {
    let timestamp = Utc::now().to_rfc3339();
    let log_line = AgentLogLine {
        run_id: run_id.to_string(),
        timestamp: timestamp.clone(),
        line: line.clone(),
    };

    let _ = app.emit("agent-log", &log_line);
    logs::append_log_line(run_id, &line);

    let line_for_run = line.clone();
    let timestamp_for_run = timestamp.clone();
    let activity_for_run = activity.clone();
    let _ = mutate_run(state, run_id, false, move |run| {
        run.logs.push(line_for_run.clone());
        run.last_log_line = Some(line_for_run.clone());
        run.log_count += 1;
        run.last_log_timestamp = Some(timestamp_for_run.clone());
        if let Some(ref detected_activity) = activity_for_run {
            run.activity = Some(detected_activity.clone());
        }
    })
    .await;
}

pub(crate) fn persist_run_meta(run: &AgentRun, status: &str) {
    logs::save_meta(&logs::LogMeta {
        run_id: run.id.clone(),
        repo: run.repo.clone(),
        issue_number: run.issue_number,
        issue_title: run.issue_title.clone(),
        stage: run.stage.to_string(),
        started_at: run.started_at.clone(),
        finished_at: run.finished_at.clone(),
        status: status.to_string(),
    });
}

pub(crate) fn emit_status(
    app: &AppHandle,
    run_id: &str,
    status: &str,
    stage: &str,
    extra: Map<String, Value>,
) {
    let mut payload = Map::new();
    payload.insert("run_id".to_string(), Value::String(run_id.to_string()));
    payload.insert("status".to_string(), Value::String(status.to_string()));
    payload.insert("stage".to_string(), Value::String(stage.to_string()));
    for (key, value) in extra {
        payload.insert(key, value);
    }
    let _ = app.emit("agent-status-changed", Value::Object(payload));
}

pub(crate) fn status_label(status: &AgentStatus) -> &'static str {
    match status {
        AgentStatus::Preparing => "preparing",
        AgentStatus::Running => "running",
        AgentStatus::Completed => "completed",
        AgentStatus::Failed => "failed",
        AgentStatus::Stopped => "stopped",
        AgentStatus::Interrupted => "interrupted",
        AgentStatus::AwaitingApproval => "awaiting_approval",
    }
}

pub(crate) async fn update_dock_badge(state: &SharedState) {
    let s = state.lock().await;
    let active = s
        .runs
        .values()
        .filter(|run| run.status == AgentStatus::Running || run.status == AgentStatus::Preparing)
        .count();
    crate::dock::set_badge_count(active);
}
