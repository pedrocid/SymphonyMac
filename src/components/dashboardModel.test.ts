import { describe, expect, it } from "vitest";
import {
  buildDashboardColumns,
  type DashboardAgentRun,
  type DashboardIssue,
} from "./dashboardModel";

const NOW = Date.parse("2026-03-08T12:00:00Z");
const REPO = "pedrocid/SymphonyMac";

function makeIssue(
  number: number,
  overrides: Partial<DashboardIssue> = {},
): DashboardIssue {
  return {
    number,
    title: `Issue ${number}`,
    body: "Body",
    state: "OPEN",
    labels: [],
    assignee: null,
    url: `https://github.com/${REPO}/issues/${number}`,
    created_at: "2026-03-08T08:00:00Z",
    updated_at: "2026-03-08T11:45:00Z",
    _repo: REPO,
    ...overrides,
  };
}

function makeRun(
  issueNumber: number,
  stage: string,
  status: string,
  overrides: Partial<DashboardAgentRun> = {},
): DashboardAgentRun {
  return {
    id: `run-${issueNumber}-${stage}`,
    repo: REPO,
    issue_number: issueNumber,
    issue_title: `Issue ${issueNumber}`,
    status,
    stage,
    started_at: "2026-03-08T11:00:00Z",
    finished_at: status === "running" || status === "preparing" ? null : "2026-03-08T11:30:00Z",
    workspace_path: `/tmp/${issueNumber}`,
    error: null,
    attempt: 1,
    max_retries: 2,
    logs: [],
    issue_labels: [],
    skipped_stages: [],
    pending_next_stage: null,
    ...overrides,
  };
}

function columnItems(columns: ReturnType<typeof buildDashboardColumns>, columnId: string) {
  const column = columns.find((entry) => entry.id === columnId);
  if (!column) {
    throw new Error(`Missing column ${columnId}`);
  }

  return column.items;
}

describe("buildDashboardColumns", () => {
  it("separates open, blocked, and closed issues without runs", () => {
    const columns = buildDashboardColumns({
      issues: [
        makeIssue(1),
        makeIssue(2),
        makeIssue(3, { state: "CLOSED" }),
      ],
      runs: [],
      blockedMap: new Map([[`${REPO}:2`, [99]]]),
      now: NOW,
    });

    expect(columnItems(columns, "open").map((card) => card.number)).toEqual([1]);
    expect(columnItems(columns, "blocked")[0]).toMatchObject({
      number: 2,
      blockedBy: [99],
    });
    expect(columnItems(columns, "done").map((card) => card.number)).toContain(3);
  });

  it("routes active, waiting, approval, failed, and completed runs to the expected columns", () => {
    const columns = buildDashboardColumns({
      issues: [
        makeIssue(4),
        makeIssue(5),
        makeIssue(6),
        makeIssue(7),
        makeIssue(8),
      ],
      runs: [
        makeRun(4, "implement", "completed", {
          skipped_stages: ["testing"],
        }),
        makeRun(5, "testing", "running"),
        makeRun(6, "code_review", "awaiting_approval", {
          pending_next_stage: "testing",
        }),
        makeRun(7, "merge", "failed", {
          error: "merge conflict",
        }),
        makeRun(8, "merge", "completed"),
      ],
      blockedMap: new Map(),
      now: NOW,
    });

    expect(columnItems(columns, "review")[0]).toMatchObject({
      number: 4,
      runStatus: "waiting",
      skippedStages: ["testing"],
    });
    expect(columnItems(columns, "testing")[0]).toMatchObject({
      number: 5,
      runStatus: "running",
    });
    expect(columnItems(columns, "approval")[0]).toMatchObject({
      number: 6,
      pendingNextStage: "testing",
    });
    expect(columnItems(columns, "failed")[0]).toMatchObject({
      number: 7,
      error: "merge conflict",
    });
    expect(columnItems(columns, "done").map((card) => card.number)).toContain(8);
  });
});
