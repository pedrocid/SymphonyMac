# Symphony Mac

A macOS desktop application that orchestrates AI coding agents to automatically implement, review, test, and merge GitHub issues. Built with Tauri v2 (Rust backend + React frontend).

## How It Works

Symphony connects to your GitHub repositories, picks up open issues, and launches AI agents (Claude Code or Codex CLI) to work through a fully automated pipeline. Each issue progresses through discrete stages, with a separate agent subprocess handling each one.

```
  GitHub Issues          Symphony Orchestrator              AI Agents
 ┌─────────────┐       ┌──────────────────────┐       ┌──────────────┐
 │  Open Issue  │──────>│  Poll Loop           │──────>│  Claude CLI  │
 │  Open Issue  │       │  - Fetch issues      │       │     or       │
 │  Open Issue  │       │  - Check PR status   │       │  Codex CLI   │
 └─────────────┘       │  - Dispatch agents   │       └──────┬───────┘
                       │  - Auto-chain stages │              │
                       └──────────────────────┘              │
                                  ^                          │
                                  │    stdout/stderr logs    │
                                  └──────────────────────────┘
```

## Pipeline Stages

Each issue flows through four stages. On success, the next stage launches automatically (auto-chaining). On failure, the stage retries up to a configurable number of attempts.

```
┌────────────┐     ┌─────────────┐     ┌──────────┐     ┌─────────┐     ┌──────┐
│ Implement  │────>│ Code Review │────>│ Testing  │────>│  Merge  │────>│ Done │
│            │     │             │     │          │     │         │     │      │
│ Write code │     │ Review diff │     │ Run tests│     │ Merge PR│     │Report│
│ Create PR  │     │ Fix issues  │     │ Fix fails│     │ Close # │     │Clean │
└────────────┘     └─────────────┘     └──────────┘     └─────────┘     └──────┘
       │                  │                  │                │
       │           On failure: retry with backoff            │
       └─────────────────────────────────────────────────────┘
```

| Stage | Agent Role | What It Does |
|-------|-----------|--------------|
| **Implement** | Developer | Reads the issue, writes code, commits, and creates a Pull Request |
| **Code Review** | Reviewer | Checks out the PR branch, reviews for bugs/security/style, fixes issues directly |
| **Testing** | Test Engineer | Runs the project's test suite, fixes failing tests, pushes fixes |
| **Merge** | Release Engineer | Merges the PR and closes the issue |
| **Done** | - | Aggregates logs from all stages, generates a pipeline report, cleans up the workspace |

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Tauri v2 App                          │
│                                                         │
│  ┌──────────────────────┐  ┌─────────────────────────┐  │
│  │   React Frontend     │  │     Rust Backend        │  │
│  │                      │  │                         │  │
│  │  App.tsx             │  │  lib.rs (entry point)   │  │
│  │  ├─ RepoSelector     │  │  ├─ orchestrator.rs     │  │
│  │  ├─ IssueList        │◄─┼─►│  ├─ poll loop       │  │
│  │  ├─ Dashboard        │  │  │  ├─ state mgmt      │  │
│  │  │  (Kanban board)   │  │  │  └─ config           │  │
│  │  ├─ ActiveAgents     │  │  ├─ agent.rs            │  │
│  │  ├─ LogViewer        │  │  │  ├─ prompt builder   │  │
│  │  ├─ PipelineReport   │  │  │  ├─ subprocess exec  │  │
│  │  └─ Settings         │  │  │  └─ auto-chaining    │  │
│  │                      │  │  ├─ github.rs           │  │
│  │  Tailwind CSS        │  │  │  └─ gh CLI wrapper   │  │
│  │                      │  │  └─ workspace.rs        │  │
│  └──────────────────────┘  │     └─ clone/cleanup    │  │
│                            └─────────────────────────┘  │
│         Tauri IPC (invoke / emit)                       │
└─────────────────────────────────────────────────────────┘
         │                              │
         ▼                              ▼
   Browser Webview               Agent Subprocesses
                                 (claude / codex CLI)
                                       │
                                       ▼
                                 ~/symphony-workspaces/
                                 └─ owner_repo_42/
```

### Backend Modules (Rust)

| File | Purpose |
|------|---------|
| `src-tauri/src/lib.rs` | App entry point. Registers all Tauri commands, initializes shared state, runs workspace cleanup on startup |
| `src-tauri/src/orchestrator.rs` | Core poll loop that fetches open issues, checks for existing PRs, determines available slots, and dispatches agents. Manages `OrchestratorState` with all active runs |
| `src-tauri/src/agent.rs` | Builds stage-specific prompts, spawns agent subprocesses (Claude CLI or Codex CLI), streams stdout/stderr as log events, handles auto-chaining to next stages and retry logic |
| `src-tauri/src/github.rs` | Wraps the `gh` CLI for listing repos, issues, PRs, and parsing "Closes #N" references |
| `src-tauri/src/workspace.rs` | Manages isolated workspaces under `~/symphony-workspaces/`. Clones repos, creates issue branches (`symphony/issue-N`), handles cleanup and TTL-based expiration |

### Frontend Components (React + TypeScript)

| Component | Purpose |
|-----------|---------|
| `Dashboard.tsx` | Kanban board with columns: Open, In Progress, Code Review, Testing, Merging, Done, Failed |
| `App.tsx` | Main layout with sidebar navigation between Repositories, Issues, Dashboard, Active Agents, and Settings |
| `RepoSelector` | Browse and select GitHub repositories |
| `IssueList` | View issues for the selected repo, launch agents on individual issues |
| `ActiveAgents` | Monitor running agent subprocesses |
| `LogViewer` | Real-time streaming logs from agent stdout/stderr |
| `Settings` | Configure agent type, concurrency, polling interval, retries, notifications, and workspace TTL |

## Workspace Isolation

Each issue gets its own workspace directory:

```
~/symphony-workspaces/
├── owner_repo_1/    # Shallow clone for issue #1
├── owner_repo_2/    # Shallow clone for issue #2
└── owner_repo_17/   # Shallow clone for issue #17
```

- Repos are shallow-cloned via `gh repo clone`
- A dedicated branch `symphony/issue-N` is created per issue
- Workspaces are cleaned up after pipeline completion or configurable TTL expiration

## Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `agent_type` | `claude` | Agent CLI to use (`claude` or `codex`) |
| `auto_approve` | `true` | Skip permission prompts in agent CLI |
| `max_concurrent` | `3` | Maximum parallel agent subprocesses |
| `poll_interval_secs` | `60` | Seconds between issue polling cycles |
| `issue_label` | `null` | Only process issues with this label |
| `max_retries` | `1` | Retry attempts per failed stage |
| `retry_backoff_secs` | `10` | Delay before retrying a failed stage |
| `workspace_ttl_days` | `7` | Auto-delete workspaces older than this |

## Prerequisites

- **macOS** (Tauri v2 desktop app)
- **GitHub CLI** (`gh`) installed and authenticated
- **Claude Code CLI** (`claude`) or **Codex CLI** (`codex`) installed
- **Node.js** and **Rust** toolchain for building from source

## Build

```bash
npm install
npx tauri build
```

The built app is located at `src-tauri/target/release/bundle/macos/Symphony Mac.app`.
