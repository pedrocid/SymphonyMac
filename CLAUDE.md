# Symphony Mac - Project Instructions

## Build & Deploy

After any code change, always rebuild and copy to /Applications:

```bash
export PATH="$HOME/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:$PATH"
cd /Users/pedrocid/Programming/utilities/SymphonyMac
npx tauri build
rm -rf "/Applications/Symphony Mac.app"
cp -R "src-tauri/target/release/bundle/macos/Symphony Mac.app" "/Applications/Symphony Mac.app"
```

## Tech Stack

- **Frontend**: React + TypeScript + Tailwind CSS
- **Backend**: Rust (Tauri v2)
- **GitHub integration**: `gh` CLI (not API tokens)
- **Agents**: Claude Code CLI (`claude --print --dangerously-skip-permissions`) or Codex CLI

## Architecture

- `src-tauri/src/lib.rs` - Entry point, registers Tauri commands
- `src-tauri/src/orchestrator.rs` - Poll loop, state management, PipelineStage enum
- `src-tauri/src/agent.rs` - Tauri-facing agent commands and launch/approval entrypoints
- `src-tauri/src/agent/prompt.rs` - Prompt templates and CLI command construction
- `src-tauri/src/agent/process.rs` - Agent subprocess execution, stream parsing, stall handling
- `src-tauri/src/agent/pipeline.rs` - Stage preparation, retries, transitions, finalization
- `src-tauri/src/agent/runtime.rs` - Shared state mutation, persist, emit helpers
- `src-tauri/src/github.rs` - GitHub operations via `gh` CLI
- `src-tauri/src/workspace.rs` - Workspace cloning/cleanup in ~/symphony-workspaces/
- `src/components/Dashboard.tsx` - Kanban board UI
- `src/App.tsx` - Main app with sidebar navigation

## Pipeline Stages

Implement -> Code Review -> Testing -> Merge -> Done

Each stage launches a separate agent subprocess. Auto-chains on success.
When Done: aggregates logs from all stages and cleans up workspace.
