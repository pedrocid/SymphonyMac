use std::path::PathBuf;
use std::process::Command;

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
    let output = Command::new("gh")
        .args(["repo", "clone", &clone_url, workspace_path.to_str().unwrap(), "--", "--depth=1"])
        .output()
        .map_err(|e| format!("Failed to clone repo: {}", e))?;

    if !output.status.success() {
        // Retry with gh repo clone format
        let output2 = Command::new("gh")
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
    let _ = Command::new("git")
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
