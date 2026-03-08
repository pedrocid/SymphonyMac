import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { IssueList } from "./IssueList";
import { invokeMock } from "../test/tauri";

const REPO = "pedrocid/SymphonyMac";

const ISSUES = [
  {
    number: 41,
    title: "Add retry guards",
    body: "Protect the retry path.",
    state: "OPEN",
    labels: ["bug"],
    assignee: null,
    url: `https://github.com/${REPO}/issues/41`,
    created_at: "2026-03-08T09:00:00Z",
    updated_at: "2026-03-08T10:00:00Z",
  },
  {
    number: 42,
    title: "Add smoke coverage",
    body: "Exercise the main flow.",
    state: "OPEN",
    labels: ["feature"],
    assignee: "pedrocid",
    url: `https://github.com/${REPO}/issues/42`,
    created_at: "2026-03-08T09:30:00Z",
    updated_at: "2026-03-08T10:30:00Z",
  },
];

function mockIssueListInvokes() {
  invokeMock.mockImplementation(async (command, args) => {
    if (command === "list_issues") {
      expect(args).toMatchObject({ repo: REPO, state: "open", label: null });
      return ISSUES;
    }

    if (command === "get_status") {
      return {
        is_running: false,
        repos: [REPO],
        config: {
          agent_type: "claude",
          auto_approve: true,
          max_concurrent: 3,
          poll_interval_secs: 60,
          issue_label: null,
          max_turns: 1,
        },
        runs: [],
        total_completed: 0,
        total_failed: 0,
        active_count: 0,
      };
    }

    if (command === "start_single_issue" || command === "start_orchestrator") {
      return "ok";
    }

    throw new Error(`Unexpected invoke: ${command}`);
  });
}

describe("IssueList", () => {
  it("launches all selected issues through the Tauri command bridge", async () => {
    mockIssueListInvokes();
    const onRunStarted = vi.fn();

    render(<IssueList repos={[REPO]} onRunStarted={onRunStarted} />);

    expect(await screen.findByText("Add retry guards")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Select all" }));
    fireEvent.click(
      screen.getByRole("button", { name: /Launch agents for 2 issues/i }),
    );

    await waitFor(() => expect(onRunStarted).toHaveBeenCalledTimes(1));

    const startCalls = invokeMock.mock.calls.filter(
      ([command]) => command === "start_single_issue",
    );
    expect(startCalls).toHaveLength(2);
    expect(startCalls[0][1]).toMatchObject({
      repo: REPO,
      issueNumber: 41,
      issueTitle: "Add retry guards",
    });
    expect(startCalls[1][1]).toMatchObject({
      repo: REPO,
      issueNumber: 42,
      issueTitle: "Add smoke coverage",
    });
  });

  it("starts auto-pilot for the selected repositories", async () => {
    mockIssueListInvokes();
    const onRunStarted = vi.fn();

    render(<IssueList repos={[REPO]} onRunStarted={onRunStarted} />);

    expect(await screen.findByText("Add retry guards")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Start Auto-Pilot" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("start_orchestrator", { repos: [REPO] }),
    );
    expect(onRunStarted).toHaveBeenCalledTimes(1);
  });
});
