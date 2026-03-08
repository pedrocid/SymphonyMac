# Agent Pipeline Module Layout

Issue #57 split the former `src-tauri/src/agent.rs` monolith into smaller modules so stage transitions can be reasoned about and tested without spawning a real agent process.

## Module boundaries

- `src-tauri/src/agent.rs`
  - Thin Tauri command layer and public entrypoints such as launch, retry, approval, rejection, and stop.
  - Delegates stage preparation, subprocess execution, and state transitions to focused modules.
- `src-tauri/src/agent/prompt.rs`
  - Prompt templating, default stage instructions, and CLI command construction for Claude/Codex.
- `src-tauri/src/agent/process.rs`
  - Subprocess lifecycle, stdout/stderr stream parsing, stall detection, hook execution, diff-stat capture, and post-exit orchestration.
- `src-tauri/src/agent/pipeline.rs`
  - Stage launch preparation, retry scheduling, next-stage scheduling, pipeline completion/report aggregation, and pure transition decisions.
- `src-tauri/src/agent/runtime.rs`
  - Shared `status -> persist -> emit` helpers for run registration, log recording, dock badge updates, and status transitions.

## Data flow

1. `agent.rs` receives a Tauri command and builds a `StageLaunchSpec`.
2. `pipeline.rs` prepares and registers an `AgentRun`, including prompt + command generation.
3. `process.rs` executes the agent CLI, parses output, and reports state mutations through `runtime.rs`.
4. `pipeline.rs` decides the next action after success or failure:
   - retry the same stage
   - pause for approval
   - launch the next pipeline stage
   - finish the pipeline and aggregate the report

## Testing surface

The stage transition rules now live behind pure functions in `pipeline.rs`, with unit tests covering retry decisions, skipped-stage advancement, approval pauses, merge verification, and terminal pipeline states. This keeps the highest-coupling control flow testable without standing up a subprocess.
