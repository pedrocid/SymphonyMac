import { IssueListView } from "../features/issues/IssueListView";
import { useIssueListController } from "../features/issues/useIssueListController";

interface IssueListProps {
  repos: string[];
  onRunStarted: () => void;
}

export function IssueList({ repos, onRunStarted }: IssueListProps) {
  const controller = useIssueListController({ repos, onRunStarted });

  return (
    <IssueListView
      repos={repos}
      issues={controller.issues}
      loading={controller.loading}
      error={controller.error}
      launching={controller.launching}
      orchestratorRunning={controller.orchestratorRunning}
      selectedIssueKeys={controller.selectedIssues}
      onSelectAll={controller.selectAll}
      onToggleIssue={controller.toggleIssue}
      onLaunchSelected={controller.launchSelected}
      onRefreshIssues={controller.refreshIssues}
      onRunIssue={controller.launchIssue}
      onStartOrchestrator={controller.startOrchestrator}
      onStopOrchestrator={controller.stopOrchestrator}
    />
  );
}
