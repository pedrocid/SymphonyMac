use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Returns the logs directory: ~/Library/Application Support/SymphonyMac/logs/
pub fn logs_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join("Library")
        .join("Application Support")
        .join("SymphonyMac")
        .join("logs")
}

/// Ensures the logs directory exists and returns the log file path for a given run.
pub fn log_file_path(run_id: &str) -> PathBuf {
    let dir = logs_dir();
    let _ = fs::create_dir_all(&dir);
    dir.join(format!("{}.log", run_id))
}

/// Metadata stored alongside each log file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMeta {
    pub run_id: String,
    pub repo: String,
    pub issue_number: u64,
    pub issue_title: String,
    pub stage: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
}

fn meta_file_path(run_id: &str) -> PathBuf {
    let dir = logs_dir();
    dir.join(format!("{}.meta.json", run_id))
}

/// Write a single log line to the file on disk (append).
pub fn append_log_line(run_id: &str, line: &str) {
    let path = log_file_path(run_id);
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let timestamp = Utc::now().to_rfc3339();
        let _ = writeln!(file, "[{}] {}", timestamp, line);
    }
}

/// Save metadata for a run.
pub fn save_meta(meta: &LogMeta) {
    let path = meta_file_path(&meta.run_id);
    if let Ok(json) = serde_json::to_string_pretty(meta) {
        let _ = fs::write(path, json);
    }
}

/// Load metadata for a run.
pub fn load_meta(run_id: &str) -> Option<LogMeta> {
    let path = meta_file_path(run_id);
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// Read all log lines from disk for a given run.
pub fn read_log_lines(run_id: &str) -> Vec<String> {
    let path = log_file_path(run_id);
    match fs::read_to_string(&path) {
        Ok(content) => content.lines().map(|l| l.to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

/// List all run metadata files from the logs directory (sorted by started_at descending).
pub fn list_all_runs() -> Vec<LogMeta> {
    let dir = logs_dir();
    let mut metas: Vec<LogMeta> = Vec::new();

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(".meta.json"))
                .unwrap_or(false)
            {
                if let Ok(data) = fs::read_to_string(&path) {
                    if let Ok(meta) = serde_json::from_str::<LogMeta>(&data) {
                        metas.push(meta);
                    }
                }
            }
        }
    }

    metas.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    metas
}

/// Search log lines on disk for a given run, returning matching lines.
pub fn search_logs(run_id: &str, query: &str) -> Vec<String> {
    let lines = read_log_lines(run_id);
    let query_lower = query.to_lowercase();
    lines
        .into_iter()
        .filter(|line| line.to_lowercase().contains(&query_lower))
        .collect()
}

/// Export logs as structured JSON.
#[derive(Serialize)]
pub struct LogExportJson {
    pub run_id: String,
    pub meta: Option<LogMeta>,
    pub lines: Vec<LogExportLine>,
}

#[derive(Serialize)]
pub struct LogExportLine {
    pub timestamp: String,
    pub content: String,
}

pub fn export_as_json(run_id: &str) -> String {
    let meta = load_meta(run_id);
    let raw_lines = read_log_lines(run_id);

    let lines: Vec<LogExportLine> = raw_lines
        .into_iter()
        .map(|line| {
            // Lines are stored as "[timestamp] content"
            if line.starts_with('[') {
                if let Some(end) = line.find("] ") {
                    return LogExportLine {
                        timestamp: line[1..end].to_string(),
                        content: line[end + 2..].to_string(),
                    };
                }
            }
            LogExportLine {
                timestamp: String::new(),
                content: line,
            }
        })
        .collect();

    let export = LogExportJson {
        run_id: run_id.to_string(),
        meta,
        lines,
    };

    serde_json::to_string_pretty(&export).unwrap_or_default()
}

/// Export logs as plain text.
pub fn export_as_text(run_id: &str) -> String {
    let meta = load_meta(run_id);
    let lines = read_log_lines(run_id);

    let mut output = String::new();
    if let Some(m) = meta {
        output.push_str(&format!("Run: {}\n", m.run_id));
        output.push_str(&format!("Repo: {}\n", m.repo));
        output.push_str(&format!("Issue: #{} - {}\n", m.issue_number, m.issue_title));
        output.push_str(&format!("Stage: {}\n", m.stage));
        output.push_str(&format!("Started: {}\n", m.started_at));
        if let Some(finished) = &m.finished_at {
            output.push_str(&format!("Finished: {}\n", finished));
        }
        output.push_str(&format!("Status: {}\n", m.status));
        output.push_str("---\n");
    }
    for line in lines {
        output.push_str(&line);
        output.push('\n');
    }
    output
}
