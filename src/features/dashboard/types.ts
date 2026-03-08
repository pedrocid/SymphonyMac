export interface KanbanCard {
  id: string;
  issueKey: string;
  repo?: string;
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
  blockedBy?: number[];
  skippedStages?: string[];
  pendingNextStage?: string | null;
}

export interface DashboardColumn {
  id: string;
  title: string;
  color: string;
  items: KanbanCard[];
}
