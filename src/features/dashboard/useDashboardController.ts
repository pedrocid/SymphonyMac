import { startTransition, useState } from "react";
import { useStatusPolling } from "../../hooks/useStatusPolling";
import { useTauriSubscription } from "../../hooks/useTauriSubscription";
import {
  advanceToStage,
  approveStage,
  getStatus,
  listIssuesForRepos,
  rejectStage,
  retryAgent,
  retryAgentFromStage,
  startIssue,
  stopAgent as stopAgentCommand,
  stopOrchestrator as stopOrchestratorCommand,
  subscribeToBlockedIssues,
} from "../../lib/api";
import { getIssueKeyFromIssue } from "../../lib/selectors";
import type { OrchestratorStatus, RepoIssue } from "../../lib/types";
import { buildDashboardColumns } from "./selectors";

export function useDashboardController() {
  const [status, setStatus] = useState<OrchestratorStatus | null>(null);
  const [issues, setIssues] = useState<RepoIssue[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [blockedMap, setBlockedMap] = useState<Map<string, number[]>>(new Map());

  async function refreshDashboard() {
    try {
      const nextStatus = await getStatus();
      let nextIssues: RepoIssue[] | null = null;
      let nextError: string | null = null;

      if (nextStatus.repos.length > 0) {
        try {
          nextIssues = await listIssuesForRepos(nextStatus.repos, { state: "all" });
        } catch (issueError) {
          nextError = String(issueError);
        }
      } else {
        nextIssues = [];
      }

      startTransition(() => {
        setStatus(nextStatus);
        if (nextIssues !== null) {
          setIssues(nextIssues);
        }
      });
      setError(nextError);
    } catch (statusError) {
      setError(String(statusError));
    }
  }

  useStatusPolling(refreshDashboard);
  useTauriSubscription(subscribeToBlockedIssues, (payload) => {
    const nextBlockedMap = new Map<string, number[]>();
    for (const entry of payload.blocked) {
      nextBlockedMap.set(`${entry.repo}:${entry.issue_number}`, entry.blocked_by);
    }
    setBlockedMap(nextBlockedMap);
  });

  async function runAndRefresh(action: () => Promise<void>) {
    try {
      setError(null);
      await action();
      await refreshDashboard();
    } catch (actionError) {
      setError(String(actionError));
    }
  }

  const issuesByKey = new Map(issues.map((issue) => [getIssueKeyFromIssue(issue), issue]));
  const columns = status ? buildDashboardColumns(status, issues, blockedMap) : [];

  return {
    status,
    error,
    columns,
    stopOrchestrator: () => runAndRefresh(() => stopOrchestratorCommand()),
    stopAgent: (runId: string) => runAndRefresh(() => stopAgentCommand(runId)),
    retryAgent: (runId: string) => runAndRefresh(() => retryAgent(runId)),
    retryAgentFromStage: (runId: string, fromStage: string) =>
      runAndRefresh(() => retryAgentFromStage(runId, fromStage)),
    approveStage: (runId: string) => runAndRefresh(() => approveStage(runId)),
    rejectStage: (runId: string) => runAndRefresh(() => rejectStage(runId)),
    advanceToStage: (runId: string, targetStage: string) =>
      runAndRefresh(() => advanceToStage(runId, targetStage)),
    launchIssueByKey: (issueKey: string) => {
      const issue = issuesByKey.get(issueKey);
      if (!issue) {
        return Promise.resolve();
      }
      return runAndRefresh(() => startIssue(issue));
    },
  };
}
