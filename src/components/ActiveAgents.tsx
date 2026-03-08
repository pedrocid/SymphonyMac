import { ActiveAgentsView } from "../features/active-agents/ActiveAgentsView";
import { useActiveAgentsController } from "../features/active-agents/useActiveAgentsController";

export function ActiveAgents({ onViewLogs }: { onViewLogs: (runId: string) => void }) {
  const controller = useActiveAgentsController();

  if (!controller.status) {
    return <div className="flex-1 flex items-center justify-center text-[#8b949e]">Loading...</div>;
  }

  return (
    <ActiveAgentsView
      status={controller.status}
      activeRuns={controller.activeRuns}
      queuedIssues={controller.queuedIssues}
      completedLastHour={controller.completedLastHour}
      liveLogs={controller.liveLogs}
      logFilters={controller.logFilters}
      runsByIssue={controller.runsByIssue}
      onViewLogs={onViewLogs}
      onStopAgent={controller.stopAgent}
      onSetLogFilter={controller.setLogFilter}
    />
  );
}
