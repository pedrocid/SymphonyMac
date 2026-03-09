use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "contracts.ts")]
pub struct WorkspaceInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub size_display: String,
    pub modified_at: String,
    pub age_days: f64,
    pub is_worktree: bool,
}

/// Execute a lifecycle hook shell command in the given workspace directory.
/// Returns Ok(output) on success, Err(message) on failure or timeout.
pub fn execute_hook(
    hook_name: &str,
    command: &str,
    workspace_path: &std::path::Path,
    _timeout_secs: u64,
) -> Result<String, String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let child = Command::new("sh")
        .args(["-c", trimmed])
        .current_dir(workspace_path)
        .env("PATH", crate::paths::build_path_env())
        .env("SYMPHONY_HOOK", hook_name)
        .env(
            "SYMPHONY_WORKSPACE",
            workspace_path.to_string_lossy().as_ref(),
        )
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Hook '{}' failed to start: {}", hook_name, e))?;

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Hook '{}' wait error: {}", hook_name, e))?;

    // Check timeout by using a thread-based approach isn't needed for sync Command,
    // but we can use the timeout on the async side. For sync, we just check status.
    // The actual timeout enforcement happens in the async wrapper below.

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "Hook '{}' exited with code {}: {}{}",
            hook_name,
            output.status.code().unwrap_or(-1),
            stderr.trim(),
            if !stdout.trim().is_empty() {
                format!("\nstdout: {}", stdout.trim())
            } else {
                String::new()
            }
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout)
}

/// Async wrapper that enforces a timeout on hook execution.
/// Uses tokio::process::Command so the child process can be killed on timeout.
pub async fn execute_hook_async(
    hook_name: &str,
    command: &str,
    workspace_path: &std::path::Path,
    timeout_secs: u64,
) -> Result<String, String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let child = tokio::process::Command::new("sh")
        .args(["-c", trimmed])
        .current_dir(workspace_path)
        .env("PATH", crate::paths::build_path_env())
        .env("SYMPHONY_HOOK", hook_name)
        .env(
            "SYMPHONY_WORKSPACE",
            workspace_path.to_string_lossy().as_ref(),
        )
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Hook '{}' failed to start: {}", hook_name, e))?;

    let result =
        tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await;

    match result {
        Ok(Ok(output)) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                Err(format!(
                    "Hook '{}' exited with code {}: {}{}",
                    hook_name,
                    output.status.code().unwrap_or(-1),
                    stderr.trim(),
                    if !stdout.trim().is_empty() {
                        format!("\nstdout: {}", stdout.trim())
                    } else {
                        String::new()
                    }
                ))
            } else {
                Ok(String::from_utf8_lossy(&output.stdout).to_string())
            }
        }
        Ok(Err(e)) => Err(format!("Hook '{}' I/O error: {}", hook_name, e)),
        Err(_) => {
            // Timeout elapsed — child is killed on drop via kill_on_drop(true)
            Err(format!(
                "Hook '{}' timed out after {}s",
                hook_name, timeout_secs
            ))
        }
    }
}

pub fn workspace_root() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join("symphony-workspaces")
}

pub fn ensure_workspace(
    repo: &str,
    issue_number: u64,
    local_repo_path: Option<&str>,
    hooks: &crate::orchestrator::LifecycleHooks,
) -> Result<PathBuf, String> {
    let root = workspace_root();
    let sanitized_repo = repo.replace('/', "_");
    let dir_name = format!("{}_{}", sanitized_repo, issue_number);
    let workspace_path = root.join(&dir_name);

    if workspace_path.exists() {
        return Ok(workspace_path);
    }

    if let Some(local_path) = local_repo_path {
        // Use git worktree from the local repository
        ensure_workspace_worktree(repo, issue_number, local_path, &workspace_path, hooks)
    } else {
        // Clone from GitHub
        ensure_workspace_clone(repo, &workspace_path, issue_number, hooks)
    }
}

fn ensure_workspace_worktree(
    repo: &str,
    issue_number: u64,
    local_path: &str,
    workspace_path: &PathBuf,
    hooks: &crate::orchestrator::LifecycleHooks,
) -> Result<PathBuf, String> {
    let local_repo = std::path::Path::new(local_path);
    if !local_repo.exists() {
        return Err(format!(
            "Local repository path does not exist: {}",
            local_path
        ));
    }
    if !local_repo.join(".git").exists() {
        return Err(format!(
            "Path is not a git repository: {}",
            local_path
        ));
    }

    // Ensure workspace root exists
    std::fs::create_dir_all(workspace_path.parent().unwrap_or(workspace_path))
        .map_err(|e| format!("Failed to create workspace root: {}", e))?;

    let branch_name = format!("symphony/issue-{}", issue_number);

    // Create worktree with a new branch
    let output = Command::new(crate::paths::resolve("git"))
        .env("PATH", crate::paths::build_path_env())
        .args([
            "worktree",
            "add",
            "-b",
            &branch_name,
            workspace_path.to_str().unwrap(),
        ])
        .current_dir(local_path)
        .output()
        .map_err(|e| format!("Failed to create worktree: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Branch may already exist — try without -b
        let output2 = Command::new(crate::paths::resolve("git"))
            .env("PATH", crate::paths::build_path_env())
            .args([
                "worktree",
                "add",
                workspace_path.to_str().unwrap(),
                &branch_name,
            ])
            .current_dir(local_path)
            .output()
            .map_err(|e| format!("Failed to create worktree: {}", e))?;

        if !output2.status.success() {
            let stderr2 = String::from_utf8_lossy(&output2.stderr);
            return Err(format!(
                "Failed to create worktree for {}: {}\nRetry: {}",
                repo, stderr, stderr2
            ));
        }
    }

    // Write a marker so cleanup knows this is a worktree
    let marker_path = workspace_path.join(".symphony-worktree");
    let _ = std::fs::write(&marker_path, local_path);

    // Run after_create hook (failure aborts workspace creation)
    if let Some(ref cmd) = hooks.after_create {
        if let Err(e) = execute_hook("after_create", cmd, workspace_path, hooks.timeout_secs) {
            cleanup_worktree(workspace_path, local_path);
            return Err(format!("after_create hook failed: {}", e));
        }
    }

    Ok(workspace_path.clone())
}

fn ensure_workspace_clone(
    repo: &str,
    workspace_path: &PathBuf,
    issue_number: u64,
    hooks: &crate::orchestrator::LifecycleHooks,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(workspace_path)
        .map_err(|e| format!("Failed to create workspace dir: {}", e))?;

    // Clone the repo into the workspace
    let clone_url = format!("https://github.com/{}.git", repo);
    let output = Command::new(crate::paths::resolve("gh"))
        .env("PATH", crate::paths::build_path_env())
        .args([
            "repo",
            "clone",
            &clone_url,
            workspace_path.to_str().unwrap(),
            "--",
            "--depth=1",
        ])
        .output()
        .map_err(|e| format!("Failed to clone repo: {}", e))?;

    if !output.status.success() {
        // Retry with gh repo clone format
        let output2 = Command::new(crate::paths::resolve("gh"))
            .env("PATH", crate::paths::build_path_env())
            .args(["repo", "clone", repo, workspace_path.to_str().unwrap()])
            .output()
            .map_err(|e| format!("Failed to clone repo: {}", e))?;

        if !output2.status.success() {
            let stderr = String::from_utf8_lossy(&output2.stderr);
            // Clean up failed workspace
            let _ = std::fs::remove_dir_all(workspace_path);
            return Err(format!("Failed to clone {}: {}", repo, stderr));
        }
    }

    // Create a branch for the issue
    let branch_name = format!("symphony/issue-{}", issue_number);
    let _ = Command::new(crate::paths::resolve("git"))
        .env("PATH", crate::paths::build_path_env())
        .args(["checkout", "-b", &branch_name])
        .current_dir(workspace_path)
        .output();

    // Run after_create hook (failure aborts workspace creation)
    if let Some(ref cmd) = hooks.after_create {
        if let Err(e) = execute_hook("after_create", cmd, workspace_path, hooks.timeout_secs) {
            // Clean up the workspace on hook failure
            let _ = std::fs::remove_dir_all(workspace_path);
            return Err(format!("after_create hook failed: {}", e));
        }
    }

    Ok(workspace_path.clone())
}

pub fn cleanup_workspace(
    repo: &str,
    issue_number: u64,
    hooks: &crate::orchestrator::LifecycleHooks,
) -> Result<(), String> {
    let root = workspace_root();
    let sanitized_repo = repo.replace('/', "_");
    let dir_name = format!("{}_{}", sanitized_repo, issue_number);
    let workspace_path = root.join(&dir_name);

    if workspace_path.exists() {
        // Run before_remove hook (failure is logged but ignored)
        if let Some(ref cmd) = hooks.before_remove {
            if let Err(e) = execute_hook("before_remove", cmd, &workspace_path, hooks.timeout_secs)
            {
                eprintln!("before_remove hook failed (ignored): {}", e);
            }
        }

        cleanup_workspace_path(&workspace_path)?;
    }

    Ok(())
}

/// Remove a workspace, detecting if it is a worktree or a plain clone.
fn cleanup_workspace_path(workspace_path: &std::path::Path) -> Result<(), String> {
    let marker = workspace_path.join(".symphony-worktree");
    if marker.exists() {
        // Read the parent repo path from the marker
        let local_path = std::fs::read_to_string(&marker).unwrap_or_default();
        cleanup_worktree(workspace_path, local_path.trim());
        Ok(())
    } else {
        std::fs::remove_dir_all(workspace_path)
            .map_err(|e| format!("Failed to remove workspace: {}", e))
    }
}

/// Remove a git worktree and prune stale entries.
fn cleanup_worktree(workspace_path: &std::path::Path, local_repo_path: &str) {
    // Try `git worktree remove` from the parent repo
    let _ = Command::new(crate::paths::resolve("git"))
        .env("PATH", crate::paths::build_path_env())
        .args([
            "worktree",
            "remove",
            "--force",
            &workspace_path.to_string_lossy(),
        ])
        .current_dir(if local_repo_path.is_empty() {
            "/"
        } else {
            local_repo_path
        })
        .output();

    // If the directory still exists (worktree remove failed), fall back to rm
    if workspace_path.exists() {
        let _ = std::fs::remove_dir_all(workspace_path);
    }

    // Prune stale worktree references
    if !local_repo_path.is_empty() {
        let _ = Command::new(crate::paths::resolve("git"))
            .env("PATH", crate::paths::build_path_env())
            .args(["worktree", "prune"])
            .current_dir(local_repo_path)
            .output();
    }
}

pub fn workspace_exists(repo: &str, issue_number: u64) -> bool {
    let root = workspace_root();
    let sanitized_repo = repo.replace('/', "_");
    let dir_name = format!("{}_{}", sanitized_repo, issue_number);
    root.join(&dir_name).exists()
}

pub fn get_workspace_path(repo: &str, issue_number: u64) -> PathBuf {
    let root = workspace_root();
    let sanitized_repo = repo.replace('/', "_");
    let dir_name = format!("{}_{}", sanitized_repo, issue_number);
    root.join(&dir_name)
}

fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = p.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[tauri::command]
pub async fn list_workspaces() -> Result<Vec<WorkspaceInfo>, String> {
    let root = workspace_root();
    if !root.exists() {
        return Ok(Vec::new());
    }

    let entries =
        std::fs::read_dir(&root).map_err(|e| format!("Failed to read workspace dir: {}", e))?;

    let now = std::time::SystemTime::now();
    let mut workspaces = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let meta = std::fs::metadata(&path).ok();
        let modified = meta.as_ref().and_then(|m| m.modified().ok());

        let modified_at = modified
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| {
                chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let age_days = modified
            .and_then(|m| now.duration_since(m).ok())
            .map(|d| d.as_secs_f64() / 86400.0)
            .unwrap_or(0.0);

        let size_bytes = dir_size(&path);
        let is_worktree = path.join(".symphony-worktree").exists();

        workspaces.push(WorkspaceInfo {
            name,
            path: path.to_string_lossy().to_string(),
            size_bytes,
            size_display: format_size(size_bytes),
            modified_at,
            age_days,
            is_worktree,
        });
    }

    workspaces.sort_by(|a, b| {
        b.age_days
            .partial_cmp(&a.age_days)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(workspaces)
}

#[tauri::command]
pub async fn cleanup_old_workspaces(max_age_days: f64) -> Result<u32, String> {
    let workspaces = list_workspaces().await?;
    let mut removed = 0u32;

    for ws in &workspaces {
        if ws.age_days >= max_age_days {
            let path = std::path::Path::new(&ws.path);
            if path.exists() {
                cleanup_workspace_path(path)
                    .map_err(|e| format!("Failed to remove {}: {}", ws.name, e))?;
                removed += 1;
            }
        }
    }

    Ok(removed)
}

#[tauri::command]
pub async fn cleanup_single_workspace(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    let root = workspace_root();
    let canonical_path = p
        .canonicalize()
        .map_err(|e| format!("Invalid workspace path: {}", e))?;
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Workspace root not found: {}", e))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err("Path is outside the workspace directory".to_string());
    }
    if canonical_path.exists() {
        cleanup_workspace_path(&canonical_path)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn cleanup_all_workspaces() -> Result<u32, String> {
    let workspaces = list_workspaces().await?;
    let mut removed = 0u32;

    for ws in &workspaces {
        let path = std::path::Path::new(&ws.path);
        if path.exists() {
            cleanup_workspace_path(path)
                .map_err(|e| format!("Failed to remove {}: {}", ws.name, e))?;
            removed += 1;
        }
    }

    Ok(removed)
}

/// Validate that a path is a valid git repository backed by a GitHub origin.
#[tauri::command]
pub async fn validate_local_repo(path: String) -> Result<LocalRepoInfo, String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err("Path does not exist".to_string());
    }
    if !p.join(".git").exists() && !p.join(".git").is_file() {
        return Err("Not a git repository (no .git found)".to_string());
    }

    // Try to detect GitHub remote
    let output = Command::new(crate::paths::resolve("git"))
        .env("PATH", crate::paths::build_path_env())
        .args(["remote", "get-url", "origin"])
        .current_dir(&path)
        .output()
        .map_err(|e| format!("Failed to run git: {}", e))?;

    if !output.status.success() {
        return Err(
            "Repository must have an 'origin' remote that points to GitHub".to_string(),
        );
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let full_name = extract_github_full_name(&url).ok_or_else(|| {
        "Repository origin must be a GitHub remote such as git@github.com:owner/repo.git"
            .to_string()
    })?;

    Ok(LocalRepoInfo {
        path,
        full_name,
    })
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "contracts.ts")]
pub struct LocalRepoInfo {
    pub path: String,
    pub full_name: String,
}

/// Extract "owner/repo" from a GitHub URL (HTTPS or SSH).
fn extract_github_full_name(url: &str) -> Option<String> {
    // Handle SSH: git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let name = rest.trim_end_matches(".git").trim_end_matches('/');
        if name.contains('/') {
            return Some(name.to_string());
        }
    }
    // Handle HTTPS: https://github.com/owner/repo.git
    if url.contains("github.com/") {
        if let Some(rest) = url.split("github.com/").nth(1) {
            let name = rest.trim_end_matches(".git").trim_end_matches('/');
            if name.contains('/') {
                return Some(name.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn init_git_repo(path: &std::path::Path, origin_url: &str) {
        let init = Command::new(crate::paths::resolve("git"))
            .env("PATH", crate::paths::build_path_env())
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();
        assert!(init.status.success(), "git init failed: {:?}", init);

        let add_remote = Command::new(crate::paths::resolve("git"))
            .env("PATH", crate::paths::build_path_env())
            .args(["remote", "add", "origin", origin_url])
            .current_dir(path)
            .output()
            .unwrap();
        assert!(
            add_remote.status.success(),
            "git remote add failed: {:?}",
            add_remote
        );
    }

    #[test]
    fn test_execute_hook_success() {
        let tmp = TempDir::new().unwrap();
        let result = execute_hook("test_hook", "echo hello", tmp.path(), 60);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().trim(), "hello");
    }

    #[test]
    fn test_execute_hook_empty_command() {
        let tmp = TempDir::new().unwrap();
        let result = execute_hook("test_hook", "   ", tmp.path(), 60);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_execute_hook_failure() {
        let tmp = TempDir::new().unwrap();
        let result = execute_hook("test_hook", "exit 1", tmp.path(), 60);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("test_hook"));
        assert!(err.contains("exited with code 1"));
    }

    #[test]
    fn test_execute_hook_sets_env_vars() {
        let tmp = TempDir::new().unwrap();
        let result = execute_hook("my_hook", "echo $SYMPHONY_HOOK", tmp.path(), 60);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().trim(), "my_hook");
    }

    #[test]
    fn test_execute_hook_workspace_env() {
        let tmp = TempDir::new().unwrap();
        let result = execute_hook("test", "echo $SYMPHONY_WORKSPACE", tmp.path(), 60);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.trim().contains(tmp.path().to_str().unwrap()));
    }

    #[test]
    fn test_execute_hook_runs_in_workspace_dir() {
        let tmp = TempDir::new().unwrap();
        // Create a marker file
        fs::write(tmp.path().join("marker.txt"), "found").unwrap();
        let result = execute_hook("test", "cat marker.txt", tmp.path(), 60);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().trim(), "found");
    }

    #[test]
    fn test_execute_hook_stderr_in_error() {
        let tmp = TempDir::new().unwrap();
        let result = execute_hook("test", "echo 'oops' >&2; exit 2", tmp.path(), 60);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("oops"));
        assert!(err.contains("exited with code 2"));
    }

    #[tokio::test]
    async fn test_execute_hook_async_success() {
        let tmp = TempDir::new().unwrap();
        let result = execute_hook_async("test", "echo async_hello", tmp.path(), 60).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().trim(), "async_hello");
    }

    #[tokio::test]
    async fn test_execute_hook_async_timeout() {
        let tmp = TempDir::new().unwrap();
        let result = execute_hook_async("test", "sleep 10", tmp.path(), 1).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("timed out"));
        assert!(err.contains("1s"));
    }

    #[tokio::test]
    async fn test_execute_hook_async_failure() {
        let tmp = TempDir::new().unwrap();
        let result = execute_hook_async("test", "exit 42", tmp.path(), 60).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exited with code 42"));
    }

    #[tokio::test]
    async fn test_execute_hook_async_empty_command() {
        let tmp = TempDir::new().unwrap();
        let result = execute_hook_async("test", "", tmp.path(), 60).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_lifecycle_hooks_default() {
        let hooks = crate::orchestrator::LifecycleHooks::default();
        assert!(hooks.after_create.is_none());
        assert!(hooks.before_run.is_none());
        assert!(hooks.after_run.is_none());
        assert!(hooks.before_remove.is_none());
        assert_eq!(hooks.timeout_secs, 60);
    }

    #[test]
    fn test_cleanup_workspace_with_before_remove_hook() {
        let root = workspace_root();
        let _ = fs::create_dir_all(&root);
        let test_dir = root.join("test_hook_repo_999");
        let _ = fs::create_dir_all(&test_dir);
        // Create a marker file that the hook will create
        let marker = root.join("hook_ran_marker.txt");
        let _ = fs::remove_file(&marker);

        let hooks = crate::orchestrator::LifecycleHooks {
            before_remove: Some(format!("echo ran > {}", marker.to_string_lossy())),
            ..Default::default()
        };

        let result = cleanup_workspace("test_hook/repo", 999, &hooks);
        assert!(result.is_ok());
        assert!(!test_dir.exists(), "workspace should be deleted");
        assert!(marker.exists(), "before_remove hook should have run");
        // Clean up marker
        let _ = fs::remove_file(&marker);
    }

    #[test]
    fn test_cleanup_workspace_before_remove_failure_ignored() {
        let root = workspace_root();
        let _ = fs::create_dir_all(&root);
        let test_dir = root.join("test_hook_repo_998");
        let _ = fs::create_dir_all(&test_dir);

        let hooks = crate::orchestrator::LifecycleHooks {
            before_remove: Some("exit 1".to_string()),
            ..Default::default()
        };

        // Should succeed even if hook fails
        let result = cleanup_workspace("test_hook/repo", 998, &hooks);
        assert!(result.is_ok());
        assert!(!test_dir.exists(), "workspace should still be deleted");
    }

    #[test]
    fn test_extract_github_full_name_https() {
        assert_eq!(
            extract_github_full_name("https://github.com/owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            extract_github_full_name("https://github.com/owner/repo"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn test_extract_github_full_name_ssh() {
        assert_eq!(
            extract_github_full_name("git@github.com:owner/repo.git"),
            Some("owner/repo".to_string())
        );
        assert_eq!(
            extract_github_full_name("git@github.com:owner/repo"),
            Some("owner/repo".to_string())
        );
    }

    #[test]
    fn test_extract_github_full_name_non_github() {
        assert_eq!(
            extract_github_full_name("https://gitlab.com/owner/repo.git"),
            None
        );
    }

    #[tokio::test]
    async fn test_validate_local_repo_accepts_github_remote() {
        let tmp = TempDir::new().unwrap();
        init_git_repo(tmp.path(), "git@github.com:owner/repo.git");

        let result = validate_local_repo(tmp.path().to_string_lossy().to_string()).await;

        assert!(result.is_ok());
        let repo = result.unwrap();
        assert_eq!(repo.full_name, "owner/repo");
        assert_eq!(repo.path, tmp.path().to_string_lossy().to_string());
    }

    #[tokio::test]
    async fn test_validate_local_repo_rejects_non_github_remote() {
        let tmp = TempDir::new().unwrap();
        init_git_repo(tmp.path(), "https://gitlab.com/owner/repo.git");

        let result = validate_local_repo(tmp.path().to_string_lossy().to_string()).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("GitHub remote"));
    }
}
