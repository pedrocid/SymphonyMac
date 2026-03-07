use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub size_display: String,
    pub modified_at: String,
    pub age_days: f64,
}

pub fn workspace_root() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join("symphony-workspaces")
}

pub fn ensure_workspace(repo: &str, issue_number: u64) -> Result<PathBuf, String> {
    let root = workspace_root();
    let sanitized_repo = repo.replace('/', "_");
    let dir_name = format!("{}_{}", sanitized_repo, issue_number);
    let workspace_path = root.join(&dir_name);

    if workspace_path.exists() {
        return Ok(workspace_path);
    }

    std::fs::create_dir_all(&workspace_path)
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
            let _ = std::fs::remove_dir_all(&workspace_path);
            return Err(format!("Failed to clone {}: {}", repo, stderr));
        }
    }

    // Create a branch for the issue
    let branch_name = format!("symphony/issue-{}", issue_number);
    let _ = Command::new(crate::paths::resolve("git"))
        .env("PATH", crate::paths::build_path_env())
        .args(["checkout", "-b", &branch_name])
        .current_dir(&workspace_path)
        .output();

    Ok(workspace_path)
}

pub fn cleanup_workspace(repo: &str, issue_number: u64) -> Result<(), String> {
    let root = workspace_root();
    let sanitized_repo = repo.replace('/', "_");
    let dir_name = format!("{}_{}", sanitized_repo, issue_number);
    let workspace_path = root.join(&dir_name);

    if workspace_path.exists() {
        std::fs::remove_dir_all(&workspace_path)
            .map_err(|e| format!("Failed to remove workspace: {}", e))?;
    }

    Ok(())
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

    let entries = std::fs::read_dir(&root)
        .map_err(|e| format!("Failed to read workspace dir: {}", e))?;

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

        workspaces.push(WorkspaceInfo {
            name,
            path: path.to_string_lossy().to_string(),
            size_bytes,
            size_display: format_size(size_bytes),
            modified_at,
            age_days,
        });
    }

    workspaces.sort_by(|a, b| b.age_days.partial_cmp(&a.age_days).unwrap_or(std::cmp::Ordering::Equal));

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
                std::fs::remove_dir_all(path)
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
    if p.exists() {
        std::fs::remove_dir_all(p)
            .map_err(|e| format!("Failed to remove workspace: {}", e))?;
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
            std::fs::remove_dir_all(path)
                .map_err(|e| format!("Failed to remove {}: {}", ws.name, e))?;
            removed += 1;
        }
    }

    Ok(removed)
}
