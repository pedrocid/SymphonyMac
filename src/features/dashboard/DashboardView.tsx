import type { OrchestratorStatus } from "../../lib/types";
import { DashboardBoard } from "./DashboardBoard";
import { DashboardMetricsBar } from "./DashboardMetricsBar";
import type { DashboardColumn } from "./types";

interface DashboardViewProps {
  status: OrchestratorStatus;
  error: string | null;
  columns: DashboardColumn[];
  onViewLogs: (runId: string) => void;
  onViewReport?: (runId: string) => void;
  onStopOrchestrator: () => void;
  onStopAgent: (runId: string) => void;
  onRetryAgent: (runId: string) => void;
  onRetryAgentFromStage: (runId: string, fromStage: string) => void;
  onApproveStage: (runId: string) => void;
  onRejectStage: (runId: string) => void;
  onAdvanceToStage: (runId: string, targetStage: string) => void;
  onLaunchIssueByKey: (issueKey: string) => void;
}

export function DashboardView({
  status,
  error,
  columns,
  onViewLogs,
  onViewReport,
  onStopOrchestrator,
  onStopAgent,
  onRetryAgent,
  onRetryAgentFromStage,
  onApproveStage,
  onRejectStage,
  onAdvanceToStage,
  onLaunchIssueByKey,
}: DashboardViewProps) {
  const title = status.repos.length > 0 ? status.repos.join(", ") : "Symphony";

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="px-6 py-4 border-b border-[#30363d] flex items-center justify-between shrink-0">
        <div className="flex items-center gap-4">
          <h2 className="text-lg font-semibold text-[#e6edf3]">{title}</h2>
          <span className="text-xs text-[#8b949e] border border-[#30363d] rounded px-2 py-0.5">
            Board
          </span>
        </div>
        <div className="flex items-center gap-3">
          {status.is_running ? (
            <>
              <span className="flex items-center gap-2 text-sm text-[#3fb950]">
                <span className="w-2 h-2 bg-[#3fb950] rounded-full animate-pulse" />
                Auto-pilot
              </span>
              <button
                onClick={onStopOrchestrator}
                className="px-3 py-1.5 bg-[#21262d] text-[#f85149] border border-[#30363d] rounded-md text-sm hover:bg-[#30363d] transition-colors"
              >
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

      <DashboardMetricsBar status={status} />

      <DashboardBoard
        columns={columns}
        manualAdvanceEnabled={!status.is_running}
        showRepoName={status.repos.length > 1}
        onViewLogs={onViewLogs}
        onViewReport={onViewReport}
        onLaunchIssueByKey={onLaunchIssueByKey}
        onStopAgent={onStopAgent}
        onRetryAgent={onRetryAgent}
        onRetryAgentFromStage={onRetryAgentFromStage}
        onApproveStage={onApproveStage}
        onRejectStage={onRejectStage}
        onAdvanceToStage={onAdvanceToStage}
      />
    </div>
  );
}
