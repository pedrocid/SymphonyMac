mod pipeline;
mod process;
mod prompt;
mod runtime;

use self::pipeline::{
    PipelineCompletionSpec, StageLaunchSpec, prepare_and_register_stage_run, spawn_next_stage,
};
use self::process::run_agent_process;
use self::runtime::{PendingNextStageUpdate, StatusTransition};
use crate::orchestrator::{AgentStatus, PipelineStage};
use crate::workspace;
use crate::SharedState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map};
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "contracts.ts")]
pub struct AgentLogLine {
    pub run_id: String,
    pub timestamp: String,
    pub line: String,
}

#[tauri::command]
pub fn get_default_prompts() -> std::collections::HashMap<String, String> {
    prompt::get_default_prompts()
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
    let config = {
        let s = state.lock().await;
        s.config.clone()
    };
    let local_repo_path = config.local_repos.get(&repo).map(|s| s.as_str());
    let workspace_path =
        workspace::ensure_workspace(&repo, issue_number, local_repo_path, &config.hooks)?;

    let request = prepare_and_register_stage_run(
        &app,
        &state,
        &config,
        StageLaunchSpec {
            repo: repo.clone(),
            issue_number,
            issue_title: issue_title.clone(),
            issue_body: issue_body.clone(),
            stage,
            issue_labels,
            workspace_path,
            attempt: 1,
            max_retries: config.max_retries,
            previous_error: String::new(),
            previous_context: None,
        },
        Map::new(),
    )
    .await;

    let run_id = request.run_id.clone();
    let app_clone = app.clone();
    let state_clone = state.clone();
    tokio::spawn(async move {
        run_agent_process(app_clone, state_clone, request).await;
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

#[tauri::command]
pub async fn retry_agent(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    run_id: String,
) -> Result<String, String> {
    let (repo, issue_number, issue_title, issue_body, stage, labels) = {
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
            String::new(),
            run.stage.clone(),
            run.issue_labels.clone(),
        )
    };

    // Try to fetch the issue body and labels from GitHub
    let (body, issue_labels) =
        match crate::github::get_issue_detail(repo.clone(), issue_number).await {
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
    let target_stage = parse_stage(&from_stage, false)?;

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

    let workspace_exists = workspace::workspace_exists(&repo, issue_number);
    let effective_stage = if workspace_exists {
        target_stage
    } else {
        runtime::append_run_log(
            state.inner(),
            &run_id,
            format!(
                "[retry] Workspace missing, falling back to full restart from implement (requested: {})",
                from_stage
            ),
            true,
            false,
        )
        .await;

        let _ = app.emit(
            "retry-fallback",
            json!({
                "run_id": &run_id,
                "requested_stage": &from_stage,
                "actual_stage": "implement",
                "reason": "workspace_missing",
            }),
        );
        PipelineStage::Implement
    };

    // Fetch the issue body and labels from GitHub
    let (body, issue_labels) =
        match crate::github::get_issue_detail(repo.clone(), issue_number).await {
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
    let (
        repo,
        issue_number,
        issue_title,
        next_stage,
        workspace_path,
        issue_labels,
        previous_context,
    ) = {
        let s = state.lock().await;
        let run = s.runs.get(&run_id).ok_or("Run not found")?;
        if run.status != AgentStatus::AwaitingApproval {
            return Err(format!(
                "Run {} is not awaiting approval (status: {:?})",
                run_id, run.status
            ));
        }
        let next_stage_name = run
            .pending_next_stage
            .as_deref()
            .ok_or("No pending next stage found")?;
        (
            run.repo.clone(),
            run.issue_number,
            run.issue_title.clone(),
            parse_stage(next_stage_name, true)?,
            run.workspace_path.clone(),
            run.issue_labels.clone(),
            run.stage_context.clone(),
        )
    };

    let _ = runtime::transition_run(
        &app,
        state.inner(),
        &run_id,
        StatusTransition {
            status: AgentStatus::Completed,
            stage_label: "approved".to_string(),
            error: None,
            finished: false,
            log_message: Some("[approval] Stage approved by user".to_string()),
            pending_next_stage: PendingNextStageUpdate::Clear,
            emit_extra: Map::new(),
            persist_meta: true,
        },
    )
    .await;

    let body = match crate::github::get_issue_detail(repo.clone(), issue_number).await {
        Ok(issue) => issue.body.unwrap_or_default(),
        Err(_) => String::new(),
    };

    if next_stage == PipelineStage::Done {
        let skipped_stages = {
            let s = state.lock().await;
            crate::orchestrator::compute_skipped_stages(&issue_labels, &s.config.stage_skip_labels)
        };
        pipeline::finish_pipeline(
            &app,
            state.inner(),
            PipelineCompletionSpec {
                repo,
                issue_number,
                issue_title,
                workspace_path: std::path::PathBuf::from(workspace_path),
                issue_labels,
                skipped_stages,
            },
        )
        .await;
        return Ok(());
    }

    spawn_next_stage(
        app,
        state.inner().clone(),
        StageLaunchSpec {
            repo,
            issue_number,
            issue_title,
            issue_body: body,
            stage: next_stage,
            issue_labels,
            workspace_path: std::path::PathBuf::from(workspace_path),
            attempt: 1,
            max_retries: {
                let s = state.lock().await;
                s.config.max_retries
            },
            previous_error: String::new(),
            previous_context,
        },
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
        let s = state.lock().await;
        let run = s.runs.get(&run_id).ok_or("Run not found")?;
        if run.status != AgentStatus::AwaitingApproval {
            return Err(format!(
                "Run {} is not awaiting approval (status: {:?})",
                run_id, run.status
            ));
        }
        (run.issue_number, run.stage.to_string())
    };

    let mut emit_extra = Map::new();
    emit_extra.insert("error".to_string(), json!("Rejected by user"));
    let _ = runtime::transition_run(
        &app,
        state.inner(),
        &run_id,
        StatusTransition {
            status: AgentStatus::Failed,
            stage_label: stage_label.clone(),
            error: Some("Rejected by user".to_string()),
            finished: true,
            log_message: Some("[approval] Stage rejected by user".to_string()),
            pending_next_stage: PendingNextStageUpdate::Clear,
            emit_extra,
            persist_meta: true,
        },
    )
    .await;

    let config = {
        let s = state.lock().await;
        s.config.clone()
    };
    if config.notifications_enabled {
        crate::notification::notify_pipeline_failed(
            &app,
            issue_number,
            &stage_label,
            config.notification_sound,
        );
    }

    runtime::update_dock_badge(state.inner()).await;
    Ok(())
}

#[tauri::command]
pub async fn stop_agent(
    app: AppHandle,
    state: tauri::State<'_, SharedState>,
    run_id: String,
) -> Result<(), String> {
    let (stage_label, repo_issue, should_cleanup, stop_hooks) = {
        let mut s = state.lock().await;
        let pid = s
            .agent_pids
            .remove(&run_id)
            .ok_or("Agent not found or already finished")?;
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        let run = s
            .runs
            .get(&run_id)
            .ok_or("Agent not found or already finished")?;
        (
            run.stage.to_string(),
            Some((run.repo.clone(), run.issue_number)),
            s.config.cleanup_on_stop,
            s.config.hooks.clone(),
        )
    };

    let _ = runtime::transition_run(
        &app,
        state.inner(),
        &run_id,
        StatusTransition {
            status: AgentStatus::Stopped,
            stage_label,
            error: None,
            finished: true,
            log_message: None,
            pending_next_stage: PendingNextStageUpdate::Keep,
            emit_extra: Map::new(),
            persist_meta: true,
        },
    )
    .await;

    runtime::update_dock_badge(state.inner()).await;

    if should_cleanup {
        if let Some((repo, issue_number)) = repo_issue {
            let _ = workspace::cleanup_workspace(&repo, issue_number, &stop_hooks);
        }
    }

    Ok(())
}

fn parse_stage(stage_name: &str, allow_done: bool) -> Result<PipelineStage, String> {
    match stage_name {
        "implement" => Ok(PipelineStage::Implement),
        "code_review" => Ok(PipelineStage::CodeReview),
        "testing" => Ok(PipelineStage::Testing),
        "merge" => Ok(PipelineStage::Merge),
        "done" if allow_done => Ok(PipelineStage::Done),
        _ => Err(format!("Invalid stage: {}", stage_name)),
    }
}
