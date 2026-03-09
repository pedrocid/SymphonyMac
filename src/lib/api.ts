import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AgentLogLine,
  LocalRepoInfo,
  OrchestratorBlockedPayload,
  OrchestratorStatus,
  RepoIssue,
  RunConfig,
  WorkspaceInfo,
} from "./types";

type IssueStateFilter = "open" | "all";
type UnlistenFn = () => void;

interface StartIssueInput {
  repo: string;
  issueNumber: number;
  issueTitle: string;
  issueBody: string | null;
  issueLabels: string[];
}

async function command<T>(name: string, args?: object): Promise<T> {
  return invoke<T>(name, args as Record<string, unknown> | undefined);
}

export function getStatus(): Promise<OrchestratorStatus> {
  return command<OrchestratorStatus>("get_status");
}

export async function getConfig(): Promise<RunConfig> {
  const status = await getStatus();
  return status.config;
}

export function listIssues(
  repo: string,
  state: IssueStateFilter,
  label: string | null,
): Promise<Omit<RepoIssue, "_repo">[]> {
  return command<Omit<RepoIssue, "_repo">[]>("list_issues", { repo, state, label });
}

export async function listIssuesForRepos(
  repos: string[],
  options: { state: IssueStateFilter; label?: string | null },
): Promise<RepoIssue[]> {
  const { state, label = null } = options;
  const results = await Promise.allSettled(
    repos.map(async (repo) => {
      const issues = await listIssues(repo, state, label);
      return issues.map((issue) => ({ ...issue, _repo: repo }));
    }),
  );

  return results
    .filter((r): r is PromiseFulfilledResult<RepoIssue[]> => r.status === "fulfilled")
    .flatMap((r) => r.value);
}

function toStartIssueInput(issue: RepoIssue): StartIssueInput {
  return {
    repo: issue._repo,
    issueNumber: issue.number,
    issueTitle: issue.title,
    issueBody: issue.body,
    issueLabels: issue.labels,
  };
}

export async function startIssue(issue: RepoIssue): Promise<void> {
  await command<void>("start_single_issue", toStartIssueInput(issue));
}

export async function startIssues(issues: RepoIssue[]): Promise<void> {
  await Promise.all(issues.map((issue) => startIssue(issue)));
}

export async function startOrchestrator(repos: string[]): Promise<void> {
  await command<void>("start_orchestrator", { repos });
}

export async function stopOrchestrator(): Promise<void> {
  await command<void>("stop_orchestrator");
}

export async function stopAgent(runId: string): Promise<void> {
  await command<void>("stop_agent", { runId });
}

export async function retryAgent(runId: string): Promise<void> {
  await command<void>("retry_agent", { runId });
}

export async function retryAgentFromStage(runId: string, fromStage: string): Promise<void> {
  await command<void>("retry_agent_from_stage", { runId, fromStage });
}

export async function approveStage(runId: string): Promise<void> {
  await command<void>("approve_stage", { runId });
}

export async function rejectStage(runId: string): Promise<void> {
  await command<void>("reject_stage", { runId });
}

export async function advanceToStage(runId: string, targetStage: string): Promise<void> {
  await command<void>("advance_to_stage", { runId, targetStage });
}

export async function updateConfig(config: RunConfig): Promise<void> {
  await command<void>("update_config", { config });
}

export function getDefaultPrompts(): Promise<Record<string, string>> {
  return command<Record<string, string>>("get_default_prompts");
}

export function listWorkspaces(): Promise<WorkspaceInfo[]> {
  return command<WorkspaceInfo[]>("list_workspaces");
}

export async function cleanupSingleWorkspace(path: string): Promise<void> {
  await command<void>("cleanup_single_workspace", { path });
}

export function cleanupAllWorkspaces(): Promise<number> {
  return command<number>("cleanup_all_workspaces");
}

export function cleanupOldWorkspaces(maxAgeDays: number): Promise<number> {
  return command<number>("cleanup_old_workspaces", { maxAgeDays });
}

export function subscribeToAgentStatusChanged(callback: () => void): Promise<UnlistenFn> {
  return listen("agent-status-changed", () => callback());
}

export function subscribeToOrchestratorStatus(callback: () => void): Promise<UnlistenFn> {
  return listen("orchestrator-status", () => callback());
}

export function subscribeToBlockedIssues(
  callback: (payload: OrchestratorBlockedPayload) => void,
): Promise<UnlistenFn> {
  return listen<OrchestratorBlockedPayload>("orchestrator-blocked-list", (event) => {
    callback(event.payload);
  });
}

export function subscribeToAgentLog(
  callback: (payload: AgentLogLine) => void,
): Promise<UnlistenFn> {
  return listen<AgentLogLine>("agent-log", (event) => {
    callback(event.payload);
  });
}

export function validateLocalRepo(path: string): Promise<LocalRepoInfo> {
  return command<LocalRepoInfo>("validate_local_repo", { path });
}
