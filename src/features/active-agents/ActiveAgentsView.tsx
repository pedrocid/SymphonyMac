import type { AgentRun, OrchestratorStatus } from "../../lib/types";
import { AgentRunCard, getRunIssueKey } from "./AgentRunCard";
import type { LiveLogEntry, LogFilter } from "./types";

interface ActiveAgentsViewProps {
  status: OrchestratorStatus;
  activeRuns: AgentRun[];
  queuedIssues: number;
  completedLastHour: number;
  liveLogs: Record<string, LiveLogEntry[]>;
  logFilters: Record<string, LogFilter>;
  runsByIssue: Map<string, AgentRun[]>;
  onViewLogs: (runId: string) => void;
  onStopAgent: (runId: string) => void;
  onSetLogFilter: (runId: string, filter: LogFilter) => void;
}

export function ActiveAgentsView({
  status,
  activeRuns,
  queuedIssues,
  completedLastHour,
  liveLogs,
  logFilters,
  runsByIssue,
  onViewLogs,
  onStopAgent,
  onSetLogFilter,
}: ActiveAgentsViewProps) {
  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="px-6 py-4 border-b border-[#30363d] shrink-0">
        <h2 className="text-lg font-semibold text-[#e6edf3]">Active Agents</h2>
        <p className="text-xs text-[#8b949e] mt-0.5">Real-time monitoring of running agents</p>
      </div>

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

      <div className="flex-1 overflow-y-auto p-6 space-y-3">
        {activeRuns.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-[#484f58]">
            <div className="text-4xl mb-3">&#9835;</div>
            <p className="text-sm">No agents running</p>
            <p className="text-xs mt-1">Launch issues from the Issues tab or enable Auto-pilot</p>
          </div>
        ) : (
          activeRuns.map((run) => (
            <AgentRunCard
              key={run.id}
              run={run}
              liveEntries={liveLogs[run.id] ?? []}
              logFilter={logFilters[run.id] ?? "all"}
              pipelineRuns={runsByIssue.get(getRunIssueKey(run)) ?? []}
              onViewLogs={onViewLogs}
              onStopAgent={onStopAgent}
              onSetLogFilter={onSetLogFilter}
            />
          ))
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
