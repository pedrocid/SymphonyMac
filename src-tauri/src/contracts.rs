use std::path::PathBuf;

use ts_rs::{Config, ExportError, TS};

fn contracts_output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("src")
        .join("contracts")
        .join("generated")
}

pub fn export_typescript_contracts() -> Result<(), ExportError> {
    let cfg = Config::new()
        .with_large_int("number")
        .with_out_dir(contracts_output_dir());

    crate::orchestrator::OrchestratorOverview::export_all(&cfg)?;
    crate::orchestrator::BlockedIssueListEvent::export_all(&cfg)?;
    crate::github::Issue::export_all(&cfg)?;
    crate::github::Repo::export_all(&cfg)?;
    crate::agent::AgentLogLine::export_all(&cfg)?;
    crate::workspace::WorkspaceInfo::export_all(&cfg)?;

    Ok(())
}
