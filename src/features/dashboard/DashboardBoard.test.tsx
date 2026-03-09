import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DashboardBoard } from "./DashboardBoard";
import type { DashboardColumn } from "./types";

const columns: DashboardColumn[] = [
  {
    id: "review",
    title: "Code Review",
    color: "#bc8cff",
    items: [
      {
        id: "run-1",
        issueKey: "pedrocid/SymphonyMac:74",
        repo: "pedrocid/SymphonyMac",
        number: 74,
        title: "Manual stage advancement",
        labels: [],
        assignee: null,
        updated: "Just now",
        runId: "run-1",
        runStatus: "waiting",
        runStage: "implement",
      },
    ],
  },
];

function renderBoard(manualAdvanceEnabled: boolean) {
  const onAdvanceToStage = vi.fn();

  render(
    <DashboardBoard
      columns={columns}
      manualAdvanceEnabled={manualAdvanceEnabled}
      showRepoName={false}
      onViewLogs={vi.fn()}
      onViewReport={vi.fn()}
      onLaunchIssueByKey={vi.fn()}
      onStopAgent={vi.fn()}
      onRetryAgent={vi.fn()}
      onRetryAgentFromStage={vi.fn()}
      onApproveStage={vi.fn()}
      onRejectStage={vi.fn()}
      onAdvanceToStage={onAdvanceToStage}
    />,
  );

  return { onAdvanceToStage };
}

describe("DashboardBoard", () => {
  it("shows manual advance actions when auto-pilot is off", () => {
    const { onAdvanceToStage } = renderBoard(true);

    expect(screen.getByText("Ready for manual advance")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Testing" }));

    expect(onAdvanceToStage).toHaveBeenCalledWith("run-1", "testing");
  });

  it("hides manual advance actions while auto-pilot is running", () => {
    renderBoard(false);

    expect(screen.getByText("Starting next stage...")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Review" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Testing" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Merge" })).not.toBeInTheDocument();
  });
});
