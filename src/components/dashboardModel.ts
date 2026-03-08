export interface DashboardAgentRun {
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
  max_retries: number;
  logs: string[];
  issue_labels: string[];
  skipped_stages: string[];
  pending_next_stage: string | null;
}

export interface DashboardIssue {
  number: number;
  title: string;
  body: string | null;
  state: string;
  labels: string[];
  assignee: string | null;
  url: string;
  created_at: string;
  updated_at: string;
  _repo?: string;
}

export interface KanbanCard {
  id: string;
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

const COLUMN_META = [
  { id: "open", title: "Open", color: "#8b949e" },
  { id: "blocked", title: "Blocked", color: "#da3633" },
  { id: "implement", title: "In Progress", color: "#d29922" },
  { id: "review", title: "Code Review", color: "#bc8cff" },
  { id: "testing", title: "Testing", color: "#58a6ff" },
  { id: "merge", title: "Merging", color: "#d2a8ff" },
  { id: "approval", title: "Awaiting Approval", color: "#d29922" },
  { id: "done", title: "Done", color: "#3fb950" },
  { id: "failed", title: "Failed", color: "#f85149" },
] as const;

function issueKey(repo: string, issueNumber: number): string {
  return `${repo}:${issueNumber}`;
}

export function formatElapsed(
  startedAt: string,
  finishedAt: string | null,
  totalSecs?: number,
  now = Date.now(),
): string {
  const secs =
    totalSecs !== undefined
      ? Math.floor(totalSecs)
      : Math.floor(
          ((finishedAt ? new Date(finishedAt).getTime() : now) -
            new Date(startedAt).getTime()) /
            1000,
        );

  if (secs < 60) {
    return `${secs}s`;
  }

  const mins = Math.floor(secs / 60);
  if (mins < 60) {
    return `${mins}m ${secs % 60}s`;
  }

  const hours = Math.floor(mins / 60);
  return `${hours}h ${mins % 60}m`;
}

export function formatRelativeDate(dateStr: string, now = Date.now()): string {
  const date = new Date(dateStr);
  const timestamp = date.getTime();

  if (Number.isNaN(timestamp)) {
    return "";
  }

  const diffMs = now - timestamp;
  const diffMins = Math.floor(diffMs / 60000);

  if (diffMins < 1) {
    return "Just now";
  }
  if (diffMins < 60) {
    return `${diffMins}m ago`;
  }

  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) {
    return `${diffHours}h ago`;
  }

  return `${Math.floor(diffHours / 24)}d ago`;
}

function buildIssueRunMaps(runs: DashboardAgentRun[]) {
  const latestRunByIssue = new Map<string, DashboardAgentRun>();
  const allRunsByIssue = new Map<string, DashboardAgentRun[]>();

  for (const run of runs) {
    const key = issueKey(run.repo, run.issue_number);
    const currentRuns = allRunsByIssue.get(key) ?? [];
    currentRuns.push(run);
    allRunsByIssue.set(key, currentRuns);

    const latestRun = latestRunByIssue.get(key);
    if (
      !latestRun ||
      new Date(run.started_at).getTime() > new Date(latestRun.started_at).getTime()
    ) {
      latestRunByIssue.set(key, run);
    }
  }

  return { latestRunByIssue, allRunsByIssue };
}

function makeCard(
  issue: DashboardIssue | null,
  run: DashboardAgentRun | null,
  allRunsByIssue: Map<string, DashboardAgentRun[]>,
  now: number,
): KanbanCard {
  const repo = run?.repo ?? issue?._repo ?? "";
  const issueNumber = run?.issue_number ?? issue?.number ?? 0;
  const key = issueKey(repo, issueNumber);
  const allIssueRuns = allRunsByIssue.get(key) ?? [];
  const skippedStages =
    run?.skipped_stages?.length
      ? run.skipped_stages
      : allIssueRuns.find((candidate) => candidate.skipped_stages?.length)?.skipped_stages ??
        [];

  return {
    id: run?.id ?? `issue-${key}`,
    repo,
    number: issueNumber,
    title: issue?.title ?? run?.issue_title ?? "",
    labels: issue?.labels ?? [],
    assignee: issue?.assignee ?? null,
    updated: formatRelativeDate(run?.started_at ?? issue?.updated_at ?? "", now),
    runId: run?.id,
    runStatus: run?.status,
    runStage: run?.stage,
    error: run?.error,
    elapsed: run ? formatElapsed(run.started_at, run.finished_at, undefined, now) : undefined,
    attempt: run?.attempt,
    maxRetries: run?.max_retries,
    skippedStages,
    pendingNextStage: run?.pending_next_stage,
  };
}

export function buildDashboardColumns({
  issues,
  runs,
  blockedMap,
  now = Date.now(),
}: {
  issues: DashboardIssue[];
  runs: DashboardAgentRun[];
  blockedMap: Map<string, number[]>;
  now?: number;
}): DashboardColumn[] {
  const { latestRunByIssue, allRunsByIssue } = buildIssueRunMaps(runs);
  const buckets = {
    open: [] as KanbanCard[],
    blocked: [] as KanbanCard[],
    implement: [] as KanbanCard[],
    review: [] as KanbanCard[],
    testing: [] as KanbanCard[],
    merge: [] as KanbanCard[],
    approval: [] as KanbanCard[],
    done: [] as KanbanCard[],
    failed: [] as KanbanCard[],
  };
  const processedIssues = new Set<string>();

  for (const [key, issueRuns] of allRunsByIssue.entries()) {
    processedIssues.add(key);

    const latestRun = latestRunByIssue.get(key);
    if (!latestRun) {
      continue;
    }

    const issue =
      issues.find(
        (candidate) =>
          candidate.number === latestRun.issue_number && candidate._repo === latestRun.repo,
      ) ?? null;
    const card = makeCard(issue, latestRun, allRunsByIssue, now);

    if (issueRuns.some((run) => run.stage === "done" && run.status === "completed")) {
      buckets.done.push(card);
      continue;
    }

    if (latestRun.status === "awaiting_approval") {
      buckets.approval.push(card);
      continue;
    }

    if (
      latestRun.status === "failed" ||
      latestRun.status === "stopped" ||
      latestRun.status === "interrupted"
    ) {
      buckets.failed.push(card);
      continue;
    }

    if (latestRun.status === "running" || latestRun.status === "preparing") {
      switch (latestRun.stage) {
        case "implement":
          buckets.implement.push(card);
          break;
        case "code_review":
          buckets.review.push(card);
          break;
        case "testing":
          buckets.testing.push(card);
          break;
        case "merge":
          buckets.merge.push(card);
          break;
        default:
          buckets.implement.push(card);
          break;
      }
      continue;
    }

    if (latestRun.status === "completed") {
      switch (latestRun.stage) {
        case "implement":
          buckets.review.push({ ...card, runStatus: "waiting" });
          break;
        case "code_review":
          buckets.testing.push({ ...card, runStatus: "waiting" });
          break;
        case "testing":
          buckets.merge.push({ ...card, runStatus: "waiting" });
          break;
        case "merge":
          buckets.done.push(card);
          break;
        default:
          buckets.done.push(card);
          break;
      }
    }
  }

  for (const issue of issues) {
    const key = issueKey(issue._repo ?? "", issue.number);
    if (processedIssues.has(key)) {
      continue;
    }

    const card = makeCard(issue, null, allRunsByIssue, now);
    if (issue.state === "OPEN") {
      const blockers = blockedMap.get(key);
      if (blockers && blockers.length > 0) {
        buckets.blocked.push({ ...card, blockedBy: blockers });
      } else {
        buckets.open.push(card);
      }
      continue;
    }

    buckets.done.push(card);
  }

  return COLUMN_META.map((column) => ({
    ...column,
    items: buckets[column.id],
  }));
}
