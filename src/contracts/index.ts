export type {
  AgentLogLine,
  AgentRun,
  AgentStatus,
  BlockedIssue,
  BlockedIssueListEvent,
  Issue,
  LifecycleHooks,
  OrchestratorOverview,
  PipelineReport,
  PipelineStage,
  Repo,
  RunConfig,
  RunSummary,
  StageContext,
  StageReport,
  WorkspaceInfo,
} from "./generated/contracts";

import type { Issue as ContractIssue } from "./generated/contracts";

export type IssueWithRepo = ContractIssue & {
  _repo: string;
};
