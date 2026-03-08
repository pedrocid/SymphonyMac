import { formatElapsed } from "./formatters";
import type { AgentRun, RepoIssue } from "./types";

export function getIssueKey(repo: string, issueNumber: number): string {
  return `${repo}:${issueNumber}`;
}

export function getIssueKeyFromIssue(issue: Pick<RepoIssue, "_repo" | "number">): string {
  return getIssueKey(issue._repo, issue.number);
}

export function groupRunsByIssue(runs: AgentRun[]): Map<string, AgentRun[]> {
  const groupedRuns = new Map<string, AgentRun[]>();

  for (const run of runs) {
    const issueKey = getIssueKey(run.repo, run.issue_number);
    const issueRuns = groupedRuns.get(issueKey) ?? [];
    issueRuns.push(run);
    groupedRuns.set(issueKey, issueRuns);
  }

  return groupedRuns;
}

export function getLatestRunByIssue(runs: AgentRun[]): Map<string, AgentRun> {
  const latestRuns = new Map<string, AgentRun>();

  for (const run of runs) {
    const issueKey = getIssueKey(run.repo, run.issue_number);
    const currentRun = latestRuns.get(issueKey);

    if (!currentRun || new Date(run.started_at) > new Date(currentRun.started_at)) {
      latestRuns.set(issueKey, run);
    }
  }

  return latestRuns;
}

export function getPipelineElapsed(issueRuns: AgentRun[]): string {
  if (issueRuns.length === 0) return "0s";

  const earliestRun = issueRuns.reduce((earliest, run) =>
    new Date(run.started_at) < new Date(earliest.started_at) ? run : earliest,
  );

  const latestRun = issueRuns.reduce((latest, run) => {
    const latestEnd = latest.finished_at || latest.started_at;
    const runEnd = run.finished_at || run.started_at;
    return new Date(runEnd) > new Date(latestEnd) ? run : latest;
  });

  return formatElapsed(earliestRun.started_at, latestRun.finished_at);
}
