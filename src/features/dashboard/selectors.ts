import { formatElapsed, formatRelativeDate } from "../../lib/formatters";
import {
  getIssueKey,
  getIssueKeyFromIssue,
  getLatestRunByIssue,
  groupRunsByIssue,
} from "../../lib/selectors";
import type { AgentRun, OrchestratorStatus, RepoIssue } from "../../lib/types";
import type { DashboardColumn, KanbanCard } from "./types";

const COLUMN_DEFINITIONS = [
  { id: "open", title: "Open", color: "#8b949e" },
  { id: "blocked", title: "Blocked", color: "#da3633" },
  { id: "implement", title: "In Progress", color: "#d29922" },
  { id: "review", title: "Code Review", color: "#bc8cff" },
  { id: "testing", title: "Testing", color: "#58a6ff" },
  { id: "merge", title: "Merging", color: "#d2a8ff" },
  { id: "approval", title: "Awaiting Approval", color: "#d29922" },
  { id: "done", title: "Done", color: "#3fb950" },
  { id: "failed", title: "Failed", color: "#f85149" },
] satisfies Omit<DashboardColumn, "items">[];

export function buildDashboardColumns(
  status: OrchestratorStatus,
  issues: RepoIssue[],
  blockedMap: Map<string, number[]>,
): DashboardColumn[] {
  const runsByIssue = groupRunsByIssue(status.runs);
  const latestRunByIssue = getLatestRunByIssue(status.runs);
  const issuesByKey = new Map(issues.map((issue) => [getIssueKeyFromIssue(issue), issue]));
  const columns = new Map(COLUMN_DEFINITIONS.map((column) => [column.id, [] as KanbanCard[]]));

  function pushCard(columnId: string, card: KanbanCard) {
    columns.get(columnId)?.push(card);
  }

  function makeCard(issue: RepoIssue | null, run: AgentRun | null, repo?: string): KanbanCard {
    const issueNumber = issue?.number ?? run?.issue_number ?? 0;
    const issueRepo = repo ?? run?.repo ?? issue?._repo ?? "";
    const issueKey = getIssueKey(issueRepo, issueNumber);
    const issueRuns = runsByIssue.get(issueKey) ?? [];
    const skippedStages = run?.skipped_stages?.length
      ? run.skipped_stages
      : issueRuns.find((candidate) => candidate.skipped_stages?.length)?.skipped_stages ?? [];

    return {
      id: run?.id ?? `issue-${issueKey}`,
      issueKey,
      repo: issueRepo || undefined,
      number: issueNumber,
      title: issue?.title ?? run?.issue_title ?? "",
      labels: issue?.labels ?? [],
      assignee: issue?.assignee ?? null,
      updated: formatRelativeDate(run?.started_at ?? issue?.updated_at ?? ""),
      runId: run?.id,
      runStatus: run?.status,
      runStage: run?.stage,
      error: run?.error,
      elapsed: run ? formatElapsed(run.started_at, run.finished_at) : undefined,
      attempt: run?.attempt,
      maxRetries: run?.max_retries,
      blockedBy: blockedMap.get(issueKey),
      skippedStages,
      pendingNextStage: run?.pending_next_stage ?? null,
    };
  }

  const processedIssues = new Set<string>();

  for (const [issueKey, issueRuns] of runsByIssue.entries()) {
    processedIssues.add(issueKey);
    const latestRun = latestRunByIssue.get(issueKey);
    if (!latestRun) {
      continue;
    }

    const issue = issuesByKey.get(issueKey) ?? null;
    const card = makeCard(issue, latestRun);
    const hasDoneRun = issueRuns.some((run) => run.stage === "done" && run.status === "completed");

    if (hasDoneRun) {
      pushCard("done", card);
      continue;
    }

    if (latestRun.status === "awaiting_approval") {
      pushCard("approval", card);
      continue;
    }

    if (
      latestRun.status === "failed" ||
      latestRun.status === "stopped" ||
      latestRun.status === "interrupted"
    ) {
      pushCard("failed", card);
      continue;
    }

    if (latestRun.status === "running" || latestRun.status === "preparing") {
      switch (latestRun.stage) {
        case "implement":
          pushCard("implement", card);
          break;
        case "code_review":
          pushCard("review", card);
          break;
        case "testing":
          pushCard("testing", card);
          break;
        case "merge":
          pushCard("merge", card);
          break;
        default:
          pushCard("implement", card);
          break;
      }
      continue;
    }

    if (latestRun.status === "completed") {
      switch (latestRun.stage) {
        case "implement":
          pushCard("review", { ...card, runStatus: "waiting" });
          break;
        case "code_review":
          pushCard("testing", { ...card, runStatus: "waiting" });
          break;
        case "testing":
          pushCard("merge", { ...card, runStatus: "waiting" });
          break;
        case "merge":
        default:
          pushCard("done", card);
          break;
      }
    }
  }

  for (const issue of issues) {
    const issueKey = getIssueKeyFromIssue(issue);
    if (processedIssues.has(issueKey)) {
      continue;
    }

    const card = makeCard(issue, null, issue._repo);
    if (issue.state.toLowerCase() === "open") {
      if ((card.blockedBy?.length ?? 0) > 0) {
        pushCard("blocked", card);
      } else {
        pushCard("open", card);
      }
      continue;
    }

    pushCard("done", card);
  }

  return COLUMN_DEFINITIONS.map((column) => ({
    ...column,
    items: columns.get(column.id) ?? [],
  }));
}
