use std::collections::HashSet;
use std::path::PathBuf;

const SEARCH_PATHS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
];

const HOME_RELATIVE_PATHS: &[&str] = &[".local/bin", ".cargo/bin"];

fn preferred_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = SEARCH_PATHS.iter().map(PathBuf::from).collect();

    if let Some(home) = dirs::home_dir() {
        for rel in HOME_RELATIVE_PATHS {
            dirs.push(home.join(rel));
        }
    }

    dirs
}

/// Build a predictable PATH for subprocesses launched from the app bundle.
///
/// macOS GUI apps started from Finder often miss Homebrew and user-local paths,
/// which breaks scripts like `codex` that use `#!/usr/bin/env node`.
pub fn build_path_env() -> String {
    let mut ordered_paths: Vec<PathBuf> = Vec::new();
    let mut seen = HashSet::new();

    if let Some(current_path) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&current_path) {
            if seen.insert(path.clone()) {
                ordered_paths.push(path);
            }
        }
    }

    for path in preferred_dirs() {
        if seen.insert(path.clone()) {
            ordered_paths.push(path);
        }
    }

    std::env::join_paths(ordered_paths)
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

/// Resolve a binary name to its full path, searching common macOS locations.
/// Falls back to the bare name if not found (letting the OS try).
pub fn resolve(name: &str) -> String {
    for dir in preferred_dirs() {
        let candidate = dir.join(name);
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
    }

    name.to_string()
}
