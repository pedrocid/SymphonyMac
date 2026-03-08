import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import App from "./App";
import { invokeMock } from "./test/tauri";

const REPO = "pedrocid/SymphonyMac";
const ISSUE = {
  number: 62,
  title: "Add automated test and CI coverage",
  body: "Cover the Tauri, React, and pipeline flows.",
  state: "OPEN",
  labels: ["feature"],
  assignee: null,
  url: `https://github.com/${REPO}/issues/62`,
  created_at: "2026-03-08T09:00:00Z",
  updated_at: "2026-03-08T10:00:00Z",
};

describe("App smoke flow", () => {
  it("moves from repo selection to issues and dashboard with mocked Tauri commands", async () => {
    let autoPilotRunning = false;

    invokeMock.mockImplementation(async (command, args) => {
      if (command === "list_repos") {
        return [
          {
            full_name: REPO,
            name: "SymphonyMac",
            owner: "pedrocid",
            description: "Agent orchestrator",
            url: `https://github.com/${REPO}`,
            default_branch: "main",
            is_private: false,
          },
        ];
      }

      if (command === "list_issues") {
        expect(args).toMatchObject({ repo: REPO });
        return [ISSUE];
      }

      if (command === "get_status") {
        return {
          is_running: autoPilotRunning,
          repos: [REPO],
          runs: [],
          config: {
            max_concurrent: 3,
            agent_type: "claude",
            auto_approve: true,
            poll_interval_secs: 60,
            issue_label: null,
            max_turns: 1,
          },
          total_completed: 0,
          total_failed: 0,
          active_count: 0,
          total_input_tokens: 0,
          total_output_tokens: 0,
          total_cost_usd: 0,
          total_runtime_secs: 0,
        };
      }

      if (command === "start_orchestrator") {
        autoPilotRunning = true;
        return null;
      }

      throw new Error(`Unexpected invoke: ${command}`);
    });

    render(<App />);

    expect(await screen.findByText(REPO)).toBeInTheDocument();
    fireEvent.click(screen.getByText(REPO));
    fireEvent.click(screen.getByRole("button", { name: /Continue with 1 repo/i }));

    expect(await screen.findByRole("heading", { name: "Issues" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Start Auto-Pilot" }));

    await waitFor(() =>
      expect(screen.getByRole("heading", { name: REPO })).toBeInTheDocument(),
    );
    expect(screen.getByText("Board")).toBeInTheDocument();
  });
});
