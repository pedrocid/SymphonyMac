import { startTransition, useEffect, useState } from "react";
import { useStatusPolling } from "../../hooks/useStatusPolling";
import {
  getStatus,
  listIssuesForRepos,
  startIssue,
  startIssues,
  startOrchestrator as startOrchestratorCommand,
  stopOrchestrator as stopOrchestratorCommand,
} from "../../lib/api";
import { getIssueKeyFromIssue } from "../../lib/selectors";
import type { RepoIssue } from "../../lib/types";

interface UseIssueListControllerOptions {
  repos: string[];
  onRunStarted: () => void;
}

export function useIssueListController({
  repos,
  onRunStarted,
}: UseIssueListControllerOptions) {
  const [issues, setIssues] = useState<RepoIssue[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedIssues, setSelectedIssues] = useState<Set<string>>(new Set());
  const [launching, setLaunching] = useState(false);
  const [orchestratorRunning, setOrchestratorRunning] = useState(false);

  const reposKey = repos.join("|");

  async function refreshIssues() {
    if (repos.length === 0) {
      setIssues([]);
      setLoading(false);
      return;
    }

    setLoading(true);
    try {
      const nextIssues = await listIssuesForRepos(repos, { state: "open" });
      startTransition(() => {
        setIssues(nextIssues);
      });
      setError(null);
    } catch (refreshError) {
      setError(String(refreshError));
    } finally {
      setLoading(false);
    }
  }

  async function refreshOrchestratorStatus() {
    try {
      const status = await getStatus();
      setOrchestratorRunning(status.is_running);
    } catch {
      // The issue list can still function without a fresh orchestrator snapshot.
    }
  }

  useEffect(() => {
    setSelectedIssues(new Set());
    void refreshIssues();
  }, [reposKey]);

  useStatusPolling(refreshOrchestratorStatus, { enabled: repos.length > 0 });

  function toggleIssue(issue: RepoIssue) {
    const issueKey = getIssueKeyFromIssue(issue);
    setSelectedIssues((currentSelection) => {
      const nextSelection = new Set(currentSelection);
      if (nextSelection.has(issueKey)) {
        nextSelection.delete(issueKey);
      } else {
        nextSelection.add(issueKey);
      }
      return nextSelection;
    });
  }

  function selectAll() {
    if (selectedIssues.size === issues.length) {
      setSelectedIssues(new Set());
      return;
    }

    setSelectedIssues(new Set(issues.map((issue) => getIssueKeyFromIssue(issue))));
  }

  async function launchSelected() {
    setLaunching(true);
    try {
      const issuesToLaunch = issues.filter((issue) => selectedIssues.has(getIssueKeyFromIssue(issue)));
      await startIssues(issuesToLaunch);
      setError(null);
      onRunStarted();
    } catch (launchError) {
      setError(String(launchError));
    } finally {
      setLaunching(false);
    }
  }

  async function launchIssue(issue: RepoIssue) {
    setLaunching(true);
    try {
      await startIssue(issue);
      setError(null);
      onRunStarted();
    } catch (launchError) {
      setError(String(launchError));
    } finally {
      setLaunching(false);
    }
  }

  async function startOrchestrator() {
    try {
      await startOrchestratorCommand(repos);
      setOrchestratorRunning(true);
      setError(null);
      onRunStarted();
    } catch (startError) {
      setError(String(startError));
    }
  }

  async function stopOrchestrator() {
    try {
      await stopOrchestratorCommand();
      setOrchestratorRunning(false);
      setError(null);
    } catch (stopError) {
      setError(String(stopError));
    }
  }

  return {
    issues,
    loading,
    error,
    selectedIssues,
    launching,
    orchestratorRunning,
    toggleIssue,
    selectAll,
    launchSelected,
    launchIssue,
    refreshIssues,
    startOrchestrator,
    stopOrchestrator,
  };
}
