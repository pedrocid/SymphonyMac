import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { RepoSelector } from "./RepoSelector";
import { invokeMock } from "../test/tauri";

const REPOS = [
  {
    full_name: "pedrocid/SymphonyMac",
    name: "SymphonyMac",
    owner: "pedrocid",
    description: "Native orchestrator",
    url: "https://github.com/pedrocid/SymphonyMac",
    default_branch: "main",
    is_private: false,
  },
  {
    full_name: "pedrocid/world-situation-report",
    name: "world-situation-report",
    owner: "pedrocid",
    description: "Another repo",
    url: "https://github.com/pedrocid/world-situation-report",
    default_branch: "main",
    is_private: true,
  },
];

function Harness({ onConfirm }: { onConfirm: () => void }) {
  const [selectedRepos, setSelectedRepos] = useState<string[]>([]);

  return (
    <RepoSelector
      selectedRepos={selectedRepos}
      onToggleRepo={(repo) =>
        setSelectedRepos((current) =>
          current.includes(repo)
            ? current.filter((entry) => entry !== repo)
            : [...current, repo],
        )
      }
      onConfirm={onConfirm}
    />
  );
}

describe("RepoSelector", () => {
  it("filters repositories and confirms the current selection", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "list_repos") {
        return REPOS;
      }

      throw new Error(`Unexpected invoke: ${command}`);
    });

    const onConfirm = vi.fn();
    render(<Harness onConfirm={onConfirm} />);

    expect(await screen.findByText("pedrocid/SymphonyMac")).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Filter repositories..."), {
      target: { value: "world" },
    });

    expect(screen.queryByText("pedrocid/SymphonyMac")).not.toBeInTheDocument();
    expect(screen.getByText("pedrocid/world-situation-report")).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Filter repositories..."), {
      target: { value: "symphony" },
    });
    fireEvent.click(screen.getByText("pedrocid/SymphonyMac"));

    const continueButton = screen.getByRole("button", {
      name: /Continue with 1 repo/i,
    });
    fireEvent.click(continueButton);

    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("shows configured local repositories without blocking GitHub repo loading", async () => {
    invokeMock.mockImplementation(async (command) => {
      if (command === "list_repos") {
        return REPOS;
      }

      if (command === "get_status") {
        return {
          config: {
            local_repos: {
              "pedrocid/SymphonyMac": "/Users/pedrocid/Programming/utilities/SymphonyMac",
            },
          },
        };
      }

      throw new Error(`Unexpected invoke: ${command}`);
    });

    render(<Harness onConfirm={vi.fn()} />);

    expect(await screen.findByText("Local (worktree)")).toBeInTheDocument();
    expect(
      screen.getByText("/Users/pedrocid/Programming/utilities/SymphonyMac"),
    ).toBeInTheDocument();
  });
});
