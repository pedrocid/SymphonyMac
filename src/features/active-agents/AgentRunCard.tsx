import { formatElapsed, formatTimestamp } from "../../lib/formatters";
import { getPipelineElapsed, getIssueKey } from "../../lib/selectors";
import type { AgentRun } from "../../lib/types";
import type { LiveLogEntry, LogFilter } from "./types";

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
  Building: "\u{1F3D7}\u{FE0F}",
  "Analyzing code": "\u{1F4CA}",
  Completed: "\u{2705}",
};

const STAGES = ["implement", "code_review", "testing", "merge", "done"];

interface AgentRunCardProps {
  run: AgentRun;
  liveEntries: LiveLogEntry[];
  logFilter: LogFilter;
  pipelineRuns: AgentRun[];
  onViewLogs: (runId: string) => void;
  onStopAgent: (runId: string) => void;
  onSetLogFilter: (runId: string, filter: LogFilter) => void;
}

export function AgentRunCard({
  run,
  liveEntries,
  logFilter,
  pipelineRuns,
  onViewLogs,
  onStopAgent,
  onSetLogFilter,
}: AgentRunCardProps) {
  const stageColor = STAGE_COLORS[run.stage] || "#8b949e";
  const stageIndex = STAGES.indexOf(run.stage);
  const filteredEntries = liveEntries.filter((entry) => {
    if (logFilter === "stderr") return entry.line.startsWith("[stderr]");
    if (logFilter === "stdout") return !entry.line.startsWith("[stderr]");
    return true;
  });
  const hasOutput = (run.log_count ?? 0) > 0 || liveEntries.length > 0;
  const activityIcon = run.activity ? ACTIVITY_ICONS[run.activity] || "" : "";
  const agentType = run.agent_type || "claude";
  const agentLabel = agentType.charAt(0).toUpperCase() + agentType.slice(1);

  return (
    <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-4 hover:border-[#484f58] transition-colors">
      <div className="flex items-start justify-between mb-3">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-1">
            <span className="text-xs text-[#484f58] font-mono">#{run.issue_number}</span>
            <span
              className="text-[10px] px-1.5 py-0.5 rounded-full font-medium"
              style={{ backgroundColor: `${stageColor}26`, color: stageColor }}
            >
              {STAGE_LABELS[run.stage] || run.stage}
            </span>
            <span className="text-xs text-[#484f58] truncate">{run.repo.split("/").pop()}</span>
          </div>
          <div className="flex items-center gap-2 mb-1">
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
          <h3 className="text-sm text-[#e6edf3] font-medium truncate">{run.issue_title}</h3>
        </div>
        <div className="flex items-center gap-2 ml-3 shrink-0">
          <button
            onClick={() => onViewLogs(run.id)}
            className="text-xs px-2.5 py-1 rounded bg-[#21262d] text-[#58a6ff] border border-[#30363d] hover:bg-[#30363d] transition-colors"
          >
            Logs
          </button>
          <button
            onClick={() => onStopAgent(run.id)}
            className="text-xs px-2.5 py-1 rounded bg-[#21262d] text-[#f85149] border border-[#30363d] hover:bg-[#30363d] transition-colors"
          >
            Stop
          </button>
        </div>
      </div>

      {run.command_display && (
        <div className="mb-3 bg-[#0d1117] rounded px-2.5 py-1.5 flex items-center gap-2">
          <span className="text-[10px] text-[#484f58] shrink-0">$</span>
          <span className="text-[11px] font-mono text-[#8b949e] truncate">{run.command_display}</span>
        </div>
      )}

      <div className="flex items-center gap-4 mb-3 text-xs text-[#8b949e]">
        <div className="flex items-center gap-1.5">
          <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ backgroundColor: stageColor }} />
          <span>Stage: {formatElapsed(run.started_at, run.finished_at)}</span>
        </div>
        <div>
          <span>Pipeline: {getPipelineElapsed(pipelineRuns)}</span>
        </div>
        <div className="flex items-center gap-1">
          <span className="text-[#484f58]">{run.log_count || liveEntries.length} lines</span>
        </div>
      </div>

      <div className="flex gap-1 mb-3">
        {STAGES.slice(0, 4).map((stage, index) => (
          <div
            key={stage}
            className="h-1.5 flex-1 rounded-full"
            style={{
              backgroundColor:
                index < stageIndex ? "#3fb950" : index === stageIndex ? stageColor : "#21262d",
            }}
          />
        ))}
      </div>

      <div className="bg-[#0d1117] rounded overflow-hidden">
        <div className="flex items-center gap-1 px-2 pt-1.5 pb-1 border-b border-[#21262d]">
          {(["all", "stdout", "stderr"] as LogFilter[]).map((filter) => (
            <button
              key={filter}
              onClick={() => onSetLogFilter(run.id, filter)}
              className={`text-[10px] px-1.5 py-0.5 rounded transition-colors ${
                logFilter === filter
                  ? "bg-[#30363d] text-[#e6edf3]"
                  : "text-[#484f58] hover:text-[#8b949e]"
              }`}
            >
              {filter === "all" ? "All" : filter === "stdout" ? "Output" : "Errors"}
            </button>
          ))}
        </div>
        <div className="p-2 font-mono text-[11px] leading-relaxed max-h-28 overflow-hidden">
          {filteredEntries.length > 0 ? (
            filteredEntries.map((entry, index) => (
              <div key={`${run.id}-${index}-${entry.ts}`} className="flex gap-2 truncate">
                <span className="text-[#30363d] shrink-0 text-[10px]">{formatTimestamp(entry.ts)}</span>
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
            <div className="text-[#484f58] truncate">{run.last_log_line || "Processing..."}</div>
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
}

export function getRunIssueKey(run: AgentRun): string {
  return getIssueKey(run.repo, run.issue_number);
}
