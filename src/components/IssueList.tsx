import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

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
  config: {
    agent_type: string;
    auto_approve: boolean;
    max_concurrent: number;
    poll_interval_secs: number;
    issue_label: string | null;
    max_turns: number;
  };
  runs: any[];
  total_completed: number;
  total_failed: number;
  active_count: number;
}

export function IssueList({
  repo,
  onRunStarted,
}: {
  repo: string;
  onRunStarted: () => void;
}) {
  const [issues, setIssues] = useState<Issue[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedIssues, setSelectedIssues] = useState<Set<number>>(new Set());
  const [launching, setLaunching] = useState(false);
  const [orchestratorRunning, setOrchestratorRunning] = useState(false);

  useEffect(() => {
    loadIssues();
    checkOrchestratorStatus();
  }, [repo]);

  async function loadIssues() {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<Issue[]>("list_issues", {
        repo,
        state: "open",
        label: null,
      });
      setIssues(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function checkOrchestratorStatus() {
    try {
      const status = await invoke<OrchestratorStatus>("get_status");
      setOrchestratorRunning(status.is_running);
    } catch (_) {}
  }

  function toggleIssue(number: number) {
    setSelectedIssues((prev) => {
      const next = new Set(prev);
      if (next.has(number)) {
        next.delete(number);
      } else {
        next.add(number);
      }
      return next;
    });
  }

  function selectAll() {
    if (selectedIssues.size === issues.length) {
      setSelectedIssues(new Set());
    } else {
      setSelectedIssues(new Set(issues.map((i) => i.number)));
    }
  }

  async function launchSelected() {
    setLaunching(true);
    try {
      for (const issue of issues.filter((i) => selectedIssues.has(i.number))) {
        await invoke("start_single_issue", {
          repo,
          issueNumber: issue.number,
          issueTitle: issue.title,
          issueBody: issue.body,
          issueLabels: issue.labels,
        });
      }
      onRunStarted();
    } catch (e) {
      setError(String(e));
    } finally {
      setLaunching(false);
    }
  }

  async function startOrchestrator() {
    try {
      await invoke("start_orchestrator", { repo });
      setOrchestratorRunning(true);
      onRunStarted();
    } catch (e) {
      setError(String(e));
    }
  }

  async function stopOrchestrator() {
    try {
      await invoke("stop_orchestrator");
      setOrchestratorRunning(false);
    } catch (e) {
      setError(String(e));
    }
  }

  const labelColors: Record<string, string> = {
    bug: "bg-[#f8514926] text-[#f85149] border-[#f85149]",
    enhancement: "bg-[#3fb95026] text-[#3fb950] border-[#3fb950]",
    feature: "bg-[#58a6ff26] text-[#58a6ff] border-[#58a6ff]",
    documentation: "bg-[#d2992226] text-[#d29922] border-[#d29922]",
  };

  return (
    <div className="flex-1 overflow-auto flex flex-col">
      {/* Header */}
      <div className="p-6 border-b border-[#30363d]">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-2xl font-bold text-[#e6edf3]">Issues</h2>
            <p className="text-[#8b949e] text-sm">{repo}</p>
          </div>
          <div className="flex gap-2">
            {orchestratorRunning ? (
              <button
                onClick={stopOrchestrator}
                className="px-4 py-2 bg-[#f8514926] text-[#f85149] border border-[#f85149] rounded-lg text-sm font-medium hover:bg-[#f8514940] transition-colors"
              >
                Stop Auto-Pilot
              </button>
            ) : (
              <button
                onClick={startOrchestrator}
                className="px-4 py-2 bg-[#3fb95026] text-[#3fb950] border border-[#3fb950] rounded-lg text-sm font-medium hover:bg-[#3fb95040] transition-colors"
              >
                Start Auto-Pilot
              </button>
            )}
          </div>
        </div>

        {/* Bulk actions */}
        <div className="flex items-center gap-3">
          <button
            onClick={selectAll}
            className="text-sm text-[#58a6ff] hover:underline"
          >
            {selectedIssues.size === issues.length ? "Deselect all" : "Select all"}
          </button>

          {selectedIssues.size > 0 && (
            <button
              onClick={launchSelected}
              disabled={launching}
              className="px-3 py-1.5 bg-[#58a6ff] text-white rounded-md text-sm font-medium hover:bg-[#79b8ff] disabled:opacity-50 transition-colors"
            >
              {launching
                ? "Launching..."
                : `Launch agents for ${selectedIssues.size} issue${selectedIssues.size > 1 ? "s" : ""}`}
            </button>
          )}

          <button
            onClick={loadIssues}
            className="text-sm text-[#8b949e] hover:text-[#e6edf3] transition-colors ml-auto"
          >
            Refresh
          </button>
        </div>
      </div>

      {/* Error */}
      {error && (
        <div className="mx-6 mt-4 bg-[#f8514926] border border-[#f85149] rounded-lg p-3">
          <p className="text-[#f85149] text-sm">{error}</p>
        </div>
      )}

      {/* Issue list */}
      <div className="flex-1 overflow-auto p-6">
        {loading ? (
          <div className="text-center py-12 text-[#8b949e]">Loading issues...</div>
        ) : issues.length === 0 ? (
          <div className="text-center py-12 text-[#8b949e]">No open issues found.</div>
        ) : (
          <div className="space-y-2">
            {issues.map((issue) => (
              <div
                key={issue.number}
                onClick={() => toggleIssue(issue.number)}
                className={`p-4 rounded-lg border cursor-pointer transition-colors ${
                  selectedIssues.has(issue.number)
                    ? "bg-[#58a6ff15] border-[#58a6ff]"
                    : "bg-[#161b22] border-[#30363d] hover:border-[#484f58]"
                }`}
              >
                <div className="flex items-start gap-3">
                  <input
                    type="checkbox"
                    checked={selectedIssues.has(issue.number)}
                    onChange={() => {}}
                    className="mt-1 accent-[#58a6ff]"
                  />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-[#8b949e] text-sm">#{issue.number}</span>
                      <span className="text-[#e6edf3] font-medium">{issue.title}</span>
                    </div>
                    {issue.body && (
                      <p className="text-sm text-[#8b949e] mt-1 line-clamp-2">
                        {issue.body.slice(0, 200)}
                      </p>
                    )}
                    <div className="flex items-center gap-2 mt-2">
                      {issue.labels.map((label) => (
                        <span
                          key={label}
                          className={`text-xs px-2 py-0.5 rounded-full border ${
                            labelColors[label.toLowerCase()] ||
                            "bg-[#21262d] text-[#8b949e] border-[#30363d]"
                          }`}
                        >
                          {label}
                        </span>
                      ))}
                      {issue.assignee && (
                        <span className="text-xs text-[#8b949e]">
                          assigned to {issue.assignee}
                        </span>
                      )}
                    </div>
                  </div>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      invoke("start_single_issue", {
                        repo,
                        issueNumber: issue.number,
                        issueTitle: issue.title,
                        issueBody: issue.body,
                        issueLabels: issue.labels,
                      }).then(() => onRunStarted());
                    }}
                    className="px-3 py-1 bg-[#21262d] border border-[#30363d] rounded-md text-sm text-[#8b949e] hover:text-[#e6edf3] hover:border-[#484f58] transition-colors shrink-0"
                  >
                    Run
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
