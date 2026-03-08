import { startTransition, useEffect, useState } from "react";
import { useStatusPolling } from "../../hooks/useStatusPolling";
import { useTauriSubscription } from "../../hooks/useTauriSubscription";
import { getStatus, stopAgent as stopAgentCommand, subscribeToAgentLog } from "../../lib/api";
import { groupRunsByIssue } from "../../lib/selectors";
import type { OrchestratorStatus } from "../../lib/types";
import type { LiveLogEntry, LogFilter } from "./types";

export function useActiveAgentsController() {
  const [status, setStatus] = useState<OrchestratorStatus | null>(null);
  const [liveLogs, setLiveLogs] = useState<Record<string, LiveLogEntry[]>>({});
  const [now, setNow] = useState(() => Date.now());
  const [logFilters, setLogFilters] = useState<Record<string, LogFilter>>({});

  async function refreshStatus() {
    try {
      const nextStatus = await getStatus();
      startTransition(() => {
        setStatus(nextStatus);
      });
    } catch {
      // Keep the last rendered snapshot if the poll fails.
    }
  }

  useStatusPolling(refreshStatus);
  useTauriSubscription(subscribeToAgentLog, (payload) => {
    setLiveLogs((currentLogs) => {
      const nextEntries = [...(currentLogs[payload.run_id] ?? []), {
        line: payload.line,
        ts: payload.timestamp,
      }].slice(-8);

      return {
        ...currentLogs,
        [payload.run_id]: nextEntries,
      };
    });
  });

  useEffect(() => {
    const intervalId = window.setInterval(() => setNow(Date.now()), 1000);
    return () => {
      window.clearInterval(intervalId);
    };
  }, []);

  const runsByIssue = status ? groupRunsByIssue(status.runs) : new Map();
  const activeRuns = status?.runs.filter(
    (run) => run.status === "running" || run.status === "preparing",
  ) ?? [];
  const queuedIssues = status
    ? Math.max(0, status.runs.filter((run) => run.status === "preparing").length)
    : 0;
  const completedLastHour = status
    ? status.runs.filter(
        (run) =>
          run.stage === "done" &&
          run.status === "completed" &&
          run.finished_at &&
          new Date(run.finished_at).getTime() >= now - 3600_000,
      ).length
    : 0;

  async function stopAgent(runId: string) {
    try {
      await stopAgentCommand(runId);
      await refreshStatus();
    } catch {
      // Preserve the current view if stopping a run fails.
    }
  }

  return {
    status,
    activeRuns,
    queuedIssues,
    completedLastHour,
    liveLogs,
    logFilters,
    runsByIssue,
    setLogFilter: (runId: string, filter: LogFilter) => {
      setLogFilters((currentFilters) => ({
        ...currentFilters,
        [runId]: filter,
      }));
    },
    stopAgent,
  };
}
