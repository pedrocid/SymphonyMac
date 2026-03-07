import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface AgentRun {
  id: string;
  repo: string;
  issue_number: number;
  issue_title: string;
  status: string;
  stage: string;
  started_at: string;
  finished_at: string | null;
  workspace_path: string;
  error: string | null;
  attempt: number;
  max_retries: number;
  logs: string[];
}

interface Issue {
  number: number;
  title: string;
  body: string | null;
  state: string;
  labels: string[];
  assignee: string | null;
  url: string;
  created_at: string;
  updated_at: string;
}

interface OrchestratorStatus {
  is_running: boolean;
  repo: string | null;
  runs: AgentRun[];
  config: any;
  total_completed: number;
  total_failed: number;
  active_count: number;
}

type KanbanCard = {
  id: string;
  number: number;
  title: string;
  labels: string[];
  assignee: string | null;
  updated: string;
  runId?: string;
  runStatus?: string;
  runStage?: string;
  error?: string | null;
  elapsed?: string;
  attempt?: number;
  maxRetries?: number;
};

const STAGE_LABELS: Record<string, string> = {
  implement: "Implementing",
  code_review: "Reviewing",
  testing: "Testing",
  merge: "Merging",
};

export function Dashboard({ onViewLogs, onViewReport }: { onViewLogs: (runId: string) => void; onViewReport?: (runId: string) => void }) {
  const [status, setStatus] = useState<OrchestratorStatus | null>(null);
  const [issues, setIssues] = useState<Issue[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadStatus();
    const interval = setInterval(loadStatus, 3000);
    const unlistenStatus = listen("agent-status-changed", () => loadStatus());
    const unlistenOrch = listen("orchestrator-status", () => loadStatus());
    return () => {
      clearInterval(interval);
      unlistenStatus.then((f) => f());
      unlistenOrch.then((f) => f());
    };
  }, []);

  async function loadStatus() {
    try {
      const result = await invoke<OrchestratorStatus>("get_status");
      setStatus(result);
      if (result.repo) {
        try {
          const allIssues = await invoke<Issue[]>("list_issues", {
            repo: result.repo, state: "all", label: null,
          });
          setIssues(allIssues);
        } catch (_) {}
      }
    } catch (e) {
      setError(String(e));
    }
  }

  async function stopOrchestrator() {
    try { await invoke("stop_orchestrator"); loadStatus(); } catch (_) {}
  }

  async function stopAgent(runId: string) {
    try { await invoke("stop_agent", { runId }); loadStatus(); } catch (_) {}
  }

  async function launchIssue(issue: Issue) {
    if (!status?.repo) return;
    try {
      await invoke("start_single_issue", {
        repo: status.repo, issueNumber: issue.number,
        issueTitle: issue.title, issueBody: issue.body,
      });
      loadStatus();
    } catch (e) { setError(String(e)); }
  }

  function formatElapsed(startedAt: string, finishedAt: string | null): string {
    const start = new Date(startedAt).getTime();
    const end = finishedAt ? new Date(finishedAt).getTime() : Date.now();
    const secs = Math.floor((end - start) / 1000);
    if (secs < 60) return `${secs}s`;
    const mins = Math.floor(secs / 60);
    if (mins < 60) return `${mins}m ${secs % 60}s`;
    const hours = Math.floor(mins / 60);
    return `${hours}h ${mins % 60}m`;
  }

  function formatDate(dateStr: string): string {
    const d = new Date(dateStr);
    const diffMs = Date.now() - d.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    if (diffMins < 1) return "Just now";
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    return `${Math.floor(diffHours / 24)}d ago`;
  }

  if (!status) {
    return <div className="flex-1 flex items-center justify-center text-[#8b949e]">Loading...</div>;
  }

  // For each issue, find the latest run (by started_at)
  const latestRunByIssue = new Map<number, AgentRun>();
  // Also collect all runs per issue to find the "best" stage
  const allRunsByIssue = new Map<number, AgentRun[]>();
  for (const run of status.runs) {
    const existing = allRunsByIssue.get(run.issue_number) || [];
    existing.push(run);
    allRunsByIssue.set(run.issue_number, existing);

    const current = latestRunByIssue.get(run.issue_number);
    if (!current || new Date(run.started_at) > new Date(current.started_at)) {
      latestRunByIssue.set(run.issue_number, run);
    }
  }

  // Determine which column each issue belongs to based on its pipeline state
  const openCards: KanbanCard[] = [];
  const implementCards: KanbanCard[] = [];
  const reviewCards: KanbanCard[] = [];
  const testingCards: KanbanCard[] = [];
  const mergeCards: KanbanCard[] = [];
  const doneCards: KanbanCard[] = [];
  const failedCards: KanbanCard[] = [];

  function makeCard(issue: Issue | null, run: AgentRun | null): KanbanCard {
    return {
      id: run?.id || `issue-${issue?.number}`,
      number: issue?.number || run?.issue_number || 0,
      title: issue?.title || run?.issue_title || "",
      labels: issue?.labels || [],
      assignee: issue?.assignee || null,
      updated: formatDate(run?.started_at || issue?.updated_at || ""),
      runId: run?.id,
      runStatus: run?.status,
      runStage: run?.stage,
      error: run?.error,
      elapsed: run ? formatElapsed(run.started_at, run.finished_at) : undefined,
      attempt: run?.attempt,
      maxRetries: run?.max_retries,
    };
  }

  // Track which issues we've processed
  const processedIssues = new Set<number>();

  // First: process all issues that have runs
  for (const [issueNum, runs] of allRunsByIssue.entries()) {
    processedIssues.add(issueNum);
    const issue = issues.find((i) => i.number === issueNum) || null;
    const latestRun = latestRunByIssue.get(issueNum)!;
    const card = makeCard(issue, latestRun);

    // Check if there's a "done" stage
    const hasDone = runs.some((r) => r.stage === "done" && r.status === "completed");
    if (hasDone) {
      doneCards.push(card);
      continue;
    }

    // Check if latest run failed
    if (latestRun.status === "failed" || latestRun.status === "stopped") {
      failedCards.push(card);
      continue;
    }

    // Active or completed stages
    if (latestRun.status === "running" || latestRun.status === "preparing") {
      // Currently active - put in the stage column
      switch (latestRun.stage) {
        case "implement": implementCards.push(card); break;
        case "code_review": reviewCards.push(card); break;
        case "testing": testingCards.push(card); break;
        case "merge": mergeCards.push(card); break;
        default: implementCards.push(card); break;
      }
    } else if (latestRun.status === "completed") {
      // Completed but next stage hasn't started yet (brief transition)
      switch (latestRun.stage) {
        case "implement": reviewCards.push({ ...card, runStatus: "waiting" }); break;
        case "code_review": testingCards.push({ ...card, runStatus: "waiting" }); break;
        case "testing": mergeCards.push({ ...card, runStatus: "waiting" }); break;
        case "merge": doneCards.push(card); break;
        default: doneCards.push(card); break;
      }
    }
  }

  // Then: issues without any runs
  for (const issue of issues) {
    if (processedIssues.has(issue.number)) continue;
    if (issue.state === "OPEN") {
      openCards.push(makeCard(issue, null));
    } else {
      doneCards.push(makeCard(issue, null));
    }
  }

  const columns = [
    { id: "open", title: "Open", color: "#8b949e", items: openCards },
    { id: "implement", title: "In Progress", color: "#d29922", items: implementCards },
    { id: "review", title: "Code Review", color: "#bc8cff", items: reviewCards },
    { id: "testing", title: "Testing", color: "#58a6ff", items: testingCards },
    { id: "merge", title: "Merging", color: "#d2a8ff", items: mergeCards },
    { id: "done", title: "Done", color: "#3fb950", items: doneCards },
    { id: "failed", title: "Failed", color: "#f85149", items: failedCards },
  ];

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {/* Top bar */}
      <div className="px-6 py-4 border-b border-[#30363d] flex items-center justify-between shrink-0">
        <div className="flex items-center gap-4">
          <h2 className="text-lg font-semibold text-[#e6edf3]">{status.repo || "Symphony"}</h2>
          <span className="text-xs text-[#8b949e] border border-[#30363d] rounded px-2 py-0.5">Board</span>
        </div>
        <div className="flex items-center gap-3">
          {status.is_running ? (
            <>
              <span className="flex items-center gap-2 text-sm text-[#3fb950]">
                <span className="w-2 h-2 bg-[#3fb950] rounded-full animate-pulse" />
                Auto-pilot
              </span>
              <button onClick={stopOrchestrator}
                className="px-3 py-1.5 bg-[#21262d] text-[#f85149] border border-[#30363d] rounded-md text-sm hover:bg-[#30363d] transition-colors">
                Stop
              </button>
            </>
          ) : (
            <span className="text-sm text-[#484f58]">Auto-pilot off</span>
          )}
        </div>
      </div>

      {error && (
        <div className="mx-6 mt-3 bg-[#f8514926] border border-[#f85149] rounded-lg p-3">
          <p className="text-[#f85149] text-sm">{error}</p>
        </div>
      )}

      {/* Kanban Board */}
      <div className="flex-1 overflow-x-auto overflow-y-hidden p-6">
        <div className="flex gap-4 h-full min-w-max">
          {columns.map((col) => (
            <div key={col.id} className="w-72 flex flex-col bg-[#0d1117] rounded-lg shrink-0">
              {/* Column header */}
              <div className="flex items-center justify-between px-3 py-3 shrink-0">
                <div className="flex items-center gap-2">
                  <span className="w-3 h-3 rounded-full" style={{ backgroundColor: col.color }} />
                  <span className="text-sm font-medium text-[#e6edf3]">{col.title}</span>
                  <span className="text-xs text-[#484f58] ml-1">{col.items.length}</span>
                </div>
              </div>

              {/* Cards */}
              <div className="flex-1 overflow-y-auto px-2 pb-2 space-y-2">
                {col.items.map((card) => (
                  <div
                    key={card.id}
                    className="bg-[#161b22] border border-[#30363d] rounded-lg p-3 hover:border-[#484f58] transition-colors cursor-pointer group"
                    onClick={() => card.runId && onViewLogs(card.runId)}
                  >
                    <div className="text-xs text-[#484f58] mb-1">#{card.number}</div>
                    <div className="text-sm text-[#e6edf3] font-medium mb-2 leading-snug">{card.title}</div>

                    {/* Running indicator with stage */}
                    {card.runStatus === "running" && (
                      <div className="flex items-center gap-1.5 mb-2">
                        <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ backgroundColor: col.color }} />
                        <span className="text-xs" style={{ color: col.color }}>
                          {STAGE_LABELS[card.runStage || ""] || card.runStage}
                          {card.attempt && card.attempt > 1 && ` (attempt ${card.attempt}/${(card.maxRetries || 0) + 1})`}
                          {" "}- {card.elapsed}
                        </span>
                      </div>
                    )}
                    {card.runStatus === "preparing" && (
                      <div className="flex items-center gap-1.5 mb-2">
                        <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ backgroundColor: col.color }} />
                        <span className="text-xs" style={{ color: col.color }}>Preparing...</span>
                      </div>
                    )}
                    {card.runStatus === "waiting" && (
                      <div className="flex items-center gap-1.5 mb-2">
                        <span className="w-1.5 h-1.5 bg-[#484f58] rounded-full animate-pulse" />
                        <span className="text-xs text-[#484f58]">Starting next stage...</span>
                      </div>
                    )}

                    {/* Error */}
                    {card.error && (
                      <div className="text-xs text-[#f85149] truncate mb-2">
                        {card.attempt && card.attempt > 1 && `[Attempt ${card.attempt}/${(card.maxRetries || 0) + 1}] `}
                        {card.error}
                      </div>
                    )}

                    {/* Labels */}
                    {card.labels.length > 0 && (
                      <div className="flex flex-wrap gap-1 mb-2">
                        {card.labels.map((label) => (
                          <span key={label}
                            className="text-[10px] px-1.5 py-0.5 rounded-full bg-[#21262d] text-[#8b949e] border border-[#30363d]">
                            {label}
                          </span>
                        ))}
                      </div>
                    )}

                    {/* Footer */}
                    <div className="flex items-center justify-between text-xs text-[#484f58]">
                      <span>{card.updated}</span>
                      <div className="flex items-center gap-2">
                        {card.runId && card.runStage === "done" && onViewReport && (
                          <button onClick={(e) => { e.stopPropagation(); onViewReport(card.runId!); }}
                            className="text-[#3fb950] hover:underline opacity-0 group-hover:opacity-100 transition-opacity">
                            Report
                          </button>
                        )}
                        {card.runId && (
                          <button onClick={(e) => { e.stopPropagation(); onViewLogs(card.runId!); }}
                            className="text-[#58a6ff] hover:underline opacity-0 group-hover:opacity-100 transition-opacity">
                            Logs
                          </button>
                        )}
                        {(card.runStatus === "running" || card.runStatus === "preparing") && card.runId && (
                          <button onClick={(e) => { e.stopPropagation(); stopAgent(card.runId!); }}
                            className="text-[#f85149] hover:underline opacity-0 group-hover:opacity-100 transition-opacity">
                            Stop
                          </button>
                        )}
                        {!card.runId && col.id === "open" && (
                          <button onClick={(e) => {
                              e.stopPropagation();
                              const issue = issues.find((i) => i.number === card.number);
                              if (issue) launchIssue(issue);
                            }}
                            className="text-[#3fb950] hover:underline opacity-0 group-hover:opacity-100 transition-opacity">
                            Run
                          </button>
                        )}
                      </div>
                    </div>
                  </div>
                ))}

                {col.items.length === 0 && (
                  <div className="text-center py-8 text-xs text-[#30363d]">No issues</div>
                )}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
