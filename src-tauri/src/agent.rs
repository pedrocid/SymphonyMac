use crate::orchestrator::{AgentRun, AgentStatus, RunConfig};
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

fn build_agent_prompt(issue_number: u64, repo: &str, issue_title: &str, issue_body: &str) -> String {
    format!(
        "You are working on GitHub issue #{} in repository {}.\n\n\
         Title: {}\n\n\
         Description:\n{}\n\n\
         Please analyze the issue, implement the fix or feature, and commit your changes. \
         Create a PR when done using: gh pr create --title \"Fix #{}\" --body \"Closes #{}\"",
        issue_number, repo, issue_title,
        issue_body.chars().take(4000).collect::<String>(),
        issue_number, issue_number
    )
}

fn build_command_args(config: &RunConfig, prompt: &str) -> (String, Vec<String>) {
    match config.agent_type.as_str() {
        "codex" => {
            let mut args = vec![
                "--quiet".to_string(),
                "--approval-mode".to_string(),
                "full-auto".to_string(),
            ];
            args.push(prompt.to_string());
            ("codex".to_string(), args)
        }
        _ => {
            let mut args = vec!["--print".to_string()];
            if config.auto_approve {
                args.push("--dangerously-skip-permissions".to_string());
            }
            args.push(prompt.to_string());
            ("claude".to_string(), args)
        }
    }
}

/// Internal function to launch an agent for an issue. Used by both the Tauri command
/// and the orchestrator poll loop.
pub async fn launch_agent(
    app: AppHandle,
    state: SharedState,
    repo: String,
    issue_number: u64,
    issue_title: String,
    issue_body: String,
) -> Result<String, String> {
    let run_id = Uuid::new_v4().to_string();
    let config = {
        let s = state.lock().await;
        s.config.clone()
    };

    let workspace_path = workspace::ensure_workspace(&repo, issue_number)?;

    let run = AgentRun {
        id: run_id.clone(),
        repo: repo.clone(),
        issue_number,
        issue_title: issue_title.clone(),
        status: AgentStatus::Preparing,
        started_at: Utc::now().to_rfc3339(),
        finished_at: None,
        logs: Vec::new(),
        workspace_path: workspace_path.to_string_lossy().to_string(),
        error: None,
        attempt: 1,
    };

    {
        let mut s = state.lock().await;
        s.runs.insert(run_id.clone(), run);
    }

    let _ = app.emit("agent-status-changed", serde_json::json!({
        "run_id": &run_id,
        "status": "preparing",
    }));

    let prompt = build_agent_prompt(issue_number, &repo, &issue_title, &issue_body);
    let (cmd, args) = build_command_args(&config, &prompt);

    let run_id_clone = run_id.clone();
    let state_clone = state.clone();
    let app_clone = app.clone();

    tokio::spawn(async move {
        run_agent_process(app_clone, state_clone, run_id_clone, cmd, args, workspace_path).await;
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
) {
    // Update status to running
    {
        let mut s = state.lock().await;
        if let Some(run) = s.runs.get_mut(&run_id) {
            run.status = AgentStatus::Running;
        }
    }
    let _ = app.emit("agent-status-changed", serde_json::json!({
        "run_id": &run_id,
        "status": "running",
    }));

    let result = Command::new(&cmd)
        .args(&args)
        .current_dir(&workspace_path)
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_SESSION")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn();

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
            let _ = app.emit("agent-status-changed", serde_json::json!({
                "run_id": &run_id,
                "status": "failed",
            }));
            return;
        }
    };

    // Store PID for potential kill
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
                let log_line = AgentLogLine {
                    run_id: run_id_out.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    line: line.clone(),
                };
                let _ = app_out.emit("agent-log", &log_line);
                let mut s = state_out.lock().await;
                if let Some(run) = s.runs.get_mut(&run_id_out) {
                    run.logs.push(line);
                    if run.logs.len() > 1000 {
                        run.logs.drain(0..run.logs.len() - 1000);
                    }
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
                let log_line = AgentLogLine {
                    run_id: run_id_err.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    line: format!("[stderr] {}", line),
                };
                let _ = app_err.emit("agent-log", &log_line);
                let mut s = state_err.lock().await;
                if let Some(run) = s.runs.get_mut(&run_id_err) {
                    run.logs.push(format!("[stderr] {}", line));
                }
            }
        }
    });

    let exit_status = child.wait().await;

    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    let mut s = state.lock().await;
    s.agent_pids.remove(&run_id);
    if let Some(run) = s.runs.get_mut(&run_id) {
        run.finished_at = Some(Utc::now().to_rfc3339());
        match exit_status {
            Ok(status) if status.success() => {
                run.status = AgentStatus::Completed;
            }
            Ok(status) => {
                run.status = AgentStatus::Failed;
                run.error = Some(format!("Agent exited with code: {}", status.code().unwrap_or(-1)));
            }
            Err(e) => {
                run.status = AgentStatus::Failed;
                run.error = Some(format!("Agent process error: {}", e));
            }
        }
    }
    drop(s);

    let _ = app.emit("agent-status-changed", serde_json::json!({
        "run_id": &run_id,
        "status": "completed",
    }));
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
        if let Some(run) = s.runs.get_mut(&run_id) {
            run.status = AgentStatus::Stopped;
            run.finished_at = Some(Utc::now().to_rfc3339());
        }
        let _ = app.emit("agent-status-changed", serde_json::json!({
            "run_id": &run_id,
            "status": "stopped",
        }));
        Ok(())
    } else {
        Err("Agent not found or already finished".to_string())
    }
}
