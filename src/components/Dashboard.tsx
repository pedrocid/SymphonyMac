import { DashboardView } from "../features/dashboard/DashboardView";
import { useDashboardController } from "../features/dashboard/useDashboardController";

interface DashboardProps {
  onViewLogs: (runId: string) => void;
  onViewReport?: (runId: string) => void;
}

export function Dashboard({ onViewLogs, onViewReport }: DashboardProps) {
  const controller = useDashboardController();

  if (!controller.status) {
    return <div className="flex-1 flex items-center justify-center text-[#8b949e]">Loading...</div>;
  }

  return (
    <DashboardView
      status={controller.status}
      error={controller.error}
      columns={controller.columns}
      onViewLogs={onViewLogs}
      onViewReport={onViewReport}
      onStopOrchestrator={controller.stopOrchestrator}
      onStopAgent={controller.stopAgent}
      onRetryAgent={controller.retryAgent}
      onRetryAgentFromStage={controller.retryAgentFromStage}
      onApproveStage={controller.approveStage}
      onRejectStage={controller.rejectStage}
      onAdvanceToStage={controller.advanceToStage}
      onLaunchIssueByKey={controller.launchIssueByKey}
    />
  );
}
