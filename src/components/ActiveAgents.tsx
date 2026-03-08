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
  logs: string[];
  command_display: string | null;
  agent_type: string;
  last_log_line: string | null;
  log_count: number;
  activity: string | null;
}

interface OrchestratorStatus {
  is_running: boolean;
  repos: string[];
  runs: AgentRun[];
  config: { max_concurrent: number };
  total_completed: number;
  total_failed: number;
  active_count: number;
}

interface AgentLogLine {
  run_id: string;
  timestamp: string;
  line: string;
}

const STAGE_LABELS: Record<string, string> = {
  implement: "Implement",
  code_review: "Code Review",
  testing: "Testing",
  merge: "Merge",
  done: "Done",
};

const STAGE_COLORS: Record<string, string> = {
  implement: "#d29922",
  code_review: "#bc8cff",
  testing: "#58a6ff",
  merge: "#d2a8ff",
  done: "#3fb950",
};

const ACTIVITY_ICONS: Record<string, string> = {
  "Reading files": "\u{1F4D6}",
  "Editing files": "\u{270F}\u{FE0F}",
  "Running command": "\u{1F6E0}\u{FE0F}",
  "Searching code": "\u{1F50D}",
  "Git operations": "\u{1F500}",
  "Running tests": "\u{1F9EA}",
  "Building": "\u{1F3D7}\u{FE0F}",
  "Analyzing code": "\u{1F4CA}",
  "Completed": "\u{2705}",
};

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

function getPipelineTotal(runs: AgentRun[], repo: string, issueNumber: number): string {
  const issueRuns = runs.filter((r) => r.repo === repo && r.issue_number === issueNumber);
  if (issueRuns.length === 0) return "0s";
  const earliest = issueRuns.reduce((min, r) =>
    new Date(r.started_at) < new Date(min.started_at) ? r : min
  );
  const latest = issueRuns.reduce((max, r) => {
    const endA = max.finished_at || max.started_at;
    const endB = r.finished_at || r.started_at;
    return new Date(endB) > new Date(endA) ? r : max;
  });
  return formatElapsed(earliest.started_at, latest.finished_at);
}

function formatTimestamp(ts: string): string {
  try {
    const d = new Date(ts);
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  } catch {
    return "";
  }
}

type LogFilter = "all" | "stdout" | "stderr";

export function ActiveAgents({ onViewLogs }: { onViewLogs: (runId: string) => void }) {
  const [status, setStatus] = useState<OrchestratorStatus | null>(null);
  const [liveLogs, setLiveLogs] = useState<Record<string, { line: string; ts: string }[]>>({});
  const [now, setNow] = useState(Date.now());
  const [completedTimestamps, setCompletedTimestamps] = useState<number[]>([]);
  const [logFilters, setLogFilters] = useState<Record<string, LogFilter>>({});

  useEffect(() => {
    loadStatus();
    const interval = setInterval(loadStatus, 3000);
    const tickInterval = setInterval(() => setNow(Date.now()), 1000);
    const unlistenStatus = listen("agent-status-changed", () => loadStatus());
    const unlistenOrch = listen("orchestrator-status", () => loadStatus());
    const unlistenLog = listen<AgentLogLine>("agent-log", (event) => {
      setLiveLogs((prev) => {
        const entries = prev[event.payload.run_id] || [];
        const updated = [...entries, { line: event.payload.line, ts: event.payload.timestamp }].slice(-8);
        return { ...prev, [event.payload.run_id]: updated };
      });
    });

    return () => {
      clearInterval(interval);
      clearInterval(tickInterval);
      unlistenStatus.then((f) => f());
      unlistenOrch.then((f) => f());
      unlistenLog.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (!status) return;
    const newCompleted: number[] = [];
    for (const run of status.runs) {
      if (run.stage === "done" && run.status === "completed" && run.finished_at) {
        newCompleted.push(new Date(run.finished_at).getTime());
      }
    }
    setCompletedTimestamps(newCompleted);
  }, [status]);

  async function loadStatus() {
    try {
      const result = await invoke<OrchestratorStatus>("get_status");
      setStatus(result);
    } catch (_) {}
  }

  async function stopAgent(runId: string) {
    try {
      await invoke("stop_agent", { runId });
      loadStatus();
    } catch (_) {}
  }

  if (!status) {
    return <div className="flex-1 flex items-center justify-center text-[#8b949e]">Loading...</div>;
  }

  const activeRuns = status.runs.filter(
    (r) => r.status === "running" || r.status === "preparing"
  );
  const queuedIssues = Math.max(0, status.runs.filter(
    (r) => r.status === "preparing"
  ).length);

  // Throughput: completed in last hour
  const oneHourAgo = now - 3600_000;
  const completedLastHour = completedTimestamps.filter((t) => t >= oneHourAgo).length;

  // Pipeline stages for progress bar
  const STAGES = ["implement", "code_review", "testing", "merge", "done"];

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {/* Header */}
      <div className="px-6 py-4 border-b border-[#30363d] shrink-0">
        <h2 className="text-lg font-semibold text-[#e6edf3]">Active Agents</h2>
        <p className="text-xs text-[#8b949e] mt-0.5">Real-time monitoring of running agents</p>
      </div>

      {/* Global metrics */}
      <div className="px-6 py-4 border-b border-[#30363d] shrink-0">
        <div className="flex gap-6">
          <MetricCard
            label="Agents Running"
            value={`${status.active_count} / ${status.config.max_concurrent}`}
            color={status.active_count > 0 ? "#3fb950" : "#484f58"}
            subtext={status.active_count > 0 ? "slots occupied" : "all slots free"}
          />
          <MetricCard
            label="Queued"
            value={String(queuedIssues)}
            color="#d29922"
            subtext="issues waiting"
          />
          <MetricCard
            label="Throughput"
            value={`${completedLastHour}/hr`}
            color="#58a6ff"
            subtext="issues completed"
          />
          <MetricCard
            label="Completed"
            value={String(status.total_completed)}
            color="#3fb950"
            subtext="total"
          />
          <MetricCard
            label="Failed"
            value={String(status.total_failed)}
            color={status.total_failed > 0 ? "#f85149" : "#484f58"}
            subtext="total"
          />
        </div>
      </div>

      {/* Agent list */}
      <div className="flex-1 overflow-y-auto p-6 space-y-3">
        {activeRuns.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-[#484f58]">
            <div className="text-4xl mb-3">&#9835;</div>
            <p className="text-sm">No agents running</p>
            <p className="text-xs mt-1">Launch issues from the Issues tab or enable Auto-pilot</p>
          </div>
        ) : (
          activeRuns.map((run) => {
            const stageColor = STAGE_COLORS[run.stage] || "#8b949e";
            const stageIdx = STAGES.indexOf(run.stage);
            const liveEntries = liveLogs[run.id] || [];
            const logFilter = logFilters[run.id] || "all";
            const filteredEntries = liveEntries.filter((e) => {
              if (logFilter === "stderr") return e.line.startsWith("[stderr]");
              if (logFilter === "stdout") return !e.line.startsWith("[stderr]");
              return true;
            });
            const hasOutput = run.log_count > 0 || liveEntries.length > 0;
            const activityIcon = run.activity ? ACTIVITY_ICONS[run.activity] || "" : "";
            const agentLabel = (run.agent_type || "claude").charAt(0).toUpperCase() + (run.agent_type || "claude").slice(1);

            return (
              <div
                key={run.id}
                className="bg-[#161b22] border border-[#30363d] rounded-lg p-4 hover:border-[#484f58] transition-colors"
              >
                {/* Top row: issue info + actions */}
                <div className="flex items-start justify-between mb-3">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="text-xs text-[#484f58] font-mono">#{run.issue_number}</span>
                      <span
                        className="text-[10px] px-1.5 py-0.5 rounded-full font-medium"
                        style={{ backgroundColor: stageColor + "26", color: stageColor }}
                      >
                        {STAGE_LABELS[run.stage] || run.stage}
                      </span>
                      {/* Agent type badge */}
                      <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-[#388bfd26] text-[#58a6ff] font-medium">
                        {agentLabel}
                      </span>
                      {run.status === "preparing" ? (
                        <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-[#d2992226] text-[#d29922] flex items-center gap-1">
                          <span className="inline-block w-1.5 h-1.5 rounded-full bg-[#d29922] animate-pulse" />
                          Preparing
                        </span>
                      ) : run.activity ? (
                        <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-[#3fb95026] text-[#3fb950] flex items-center gap-1">
                          <span>{activityIcon}</span>
                          {run.activity}
                        </span>
                      ) : run.status === "running" && !hasOutput ? (
                        <span className="text-[10px] px-1.5 py-0.5 rounded-full bg-[#d2992226] text-[#d29922] flex items-center gap-1">
                          <span className="inline-block w-1.5 h-1.5 rounded-full bg-[#d29922] animate-pulse" />
                          Starting...
                        </span>
                      ) : null}
                    </div>
                    <h3 className="text-sm text-[#e6edf3] font-medium truncate">
                      {run.issue_title}
                    </h3>
                  </div>
                  <div className="flex items-center gap-2 ml-3 shrink-0">
                    <button
                      onClick={() => onViewLogs(run.id)}
                      className="text-xs px-2.5 py-1 rounded bg-[#21262d] text-[#58a6ff] border border-[#30363d] hover:bg-[#30363d] transition-colors"
                    >
                      Logs
                    </button>
                    <button
                      onClick={() => stopAgent(run.id)}
                      className="text-xs px-2.5 py-1 rounded bg-[#21262d] text-[#f85149] border border-[#30363d] hover:bg-[#30363d] transition-colors"
                    >
                      Stop
                    </button>
                  </div>
                </div>

                {/* Command display */}
                {run.command_display && (
                  <div className="mb-3 bg-[#0d1117] rounded px-2.5 py-1.5 flex items-center gap-2">
                    <span className="text-[10px] text-[#484f58] shrink-0">$</span>
                    <span className="text-[11px] font-mono text-[#8b949e] truncate">{run.command_display}</span>
                  </div>
                )}

                {/* Timers + log count */}
                <div className="flex items-center gap-4 mb-3 text-xs text-[#8b949e]">
                  <div className="flex items-center gap-1.5">
                    <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ backgroundColor: stageColor }} />
                    <span>Stage: {formatElapsed(run.started_at, run.finished_at)}</span>
                  </div>
                  <div>
                    <span>Pipeline: {getPipelineTotal(status.runs, run.repo, run.issue_number)}</span>
                  </div>
                  <div className="flex items-center gap-1">
                    <span className="text-[#484f58]">{run.log_count || liveEntries.length} lines</span>
                  </div>
                </div>

                {/* Stage progress bar */}
                <div className="flex gap-1 mb-3">
                  {STAGES.slice(0, 4).map((stage, idx) => (
                    <div
                      key={stage}
                      className="h-1.5 flex-1 rounded-full"
                      style={{
                        backgroundColor:
                          idx < stageIdx
                            ? "#3fb950"
                            : idx === stageIdx
                              ? stageColor
                              : "#21262d",
                      }}
                    />
                  ))}
                </div>

                {/* Live log preview */}
                <div className="bg-[#0d1117] rounded overflow-hidden">
                  {/* Log filter tabs */}
                  <div className="flex items-center gap-1 px-2 pt-1.5 pb-1 border-b border-[#21262d]">
                    {(["all", "stdout", "stderr"] as LogFilter[]).map((f) => (
                      <button
                        key={f}
                        onClick={() => setLogFilters((prev) => ({ ...prev, [run.id]: f }))}
                        className={`text-[10px] px-1.5 py-0.5 rounded transition-colors ${
                          logFilter === f
                            ? "bg-[#30363d] text-[#e6edf3]"
                            : "text-[#484f58] hover:text-[#8b949e]"
                        }`}
                      >
                        {f === "all" ? "All" : f === "stdout" ? "Output" : "Errors"}
                      </button>
                    ))}
                  </div>
                  <div className="p-2 font-mono text-[11px] leading-relaxed max-h-28 overflow-hidden">
                    {filteredEntries.length > 0 ? (
                      filteredEntries.map((entry, i) => (
                        <div key={i} className="flex gap-2 truncate">
                          <span className="text-[#30363d] shrink-0 text-[10px]">
                            {formatTimestamp(entry.ts)}
                          </span>
                          <span
                            className={`truncate ${
                              entry.line.startsWith("[stderr]") ? "text-[#f85149]" : "text-[#8b949e]"
                            }`}
                          >
                            {entry.line}
                          </span>
                        </div>
                      ))
                    ) : hasOutput ? (
                      <div className="text-[#484f58] truncate">
                        {run.last_log_line || "Processing..."}
                      </div>
                    ) : (
                      <div className="flex items-center gap-2 text-[#484f58]">
                        <span className="inline-block w-3 h-3 border-2 border-[#30363d] border-t-[#58a6ff] rounded-full animate-spin" />
                        <span>Running...</span>
                      </div>
                    )}
                  </div>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

function MetricCard({
  label,
  value,
  color,
  subtext,
}: {
  label: string;
  value: string;
  color: string;
  subtext: string;
}) {
  return (
    <div className="bg-[#161b22] border border-[#30363d] rounded-lg px-4 py-3 min-w-[120px]">
      <p className="text-xs text-[#8b949e] mb-1">{label}</p>
      <p className="text-xl font-bold" style={{ color }}>
        {value}
      </p>
      <p className="text-[10px] text-[#484f58] mt-0.5">{subtext}</p>
    </div>
  );
}
