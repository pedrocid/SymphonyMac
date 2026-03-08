import { getIssueKeyFromIssue } from "../../lib/selectors";
import type { RepoIssue } from "../../lib/types";
import { IssueRow } from "./IssueRow";

interface IssueListViewProps {
  repos: string[];
  issues: RepoIssue[];
  loading: boolean;
  error: string | null;
  launching: boolean;
  orchestratorRunning: boolean;
  selectedIssueKeys: Set<string>;
  onSelectAll: () => void;
  onToggleIssue: (issue: RepoIssue) => void;
  onLaunchSelected: () => void;
  onRefreshIssues: () => void;
  onRunIssue: (issue: RepoIssue) => void;
  onStartOrchestrator: () => void;
  onStopOrchestrator: () => void;
}

export function IssueListView({
  repos,
  issues,
  loading,
  error,
  launching,
  orchestratorRunning,
  selectedIssueKeys,
  onSelectAll,
  onToggleIssue,
  onLaunchSelected,
  onRefreshIssues,
  onRunIssue,
  onStartOrchestrator,
  onStopOrchestrator,
}: IssueListViewProps) {
  const selectedCount = selectedIssueKeys.size;
  const showRepoName = repos.length > 1;

  return (
    <div className="flex-1 overflow-auto flex flex-col">
      <div className="p-6 border-b border-[#30363d]">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-2xl font-bold text-[#e6edf3]">Issues</h2>
            <p className="text-[#8b949e] text-sm">{repos.join(", ")}</p>
          </div>
          <div className="flex gap-2">
            {orchestratorRunning ? (
              <button
                onClick={onStopOrchestrator}
                className="px-4 py-2 bg-[#f8514926] text-[#f85149] border border-[#f85149] rounded-lg text-sm font-medium hover:bg-[#f8514940] transition-colors"
              >
                Stop Auto-Pilot
              </button>
            ) : (
              <button
                onClick={onStartOrchestrator}
                className="px-4 py-2 bg-[#3fb95026] text-[#3fb950] border border-[#3fb950] rounded-lg text-sm font-medium hover:bg-[#3fb95040] transition-colors"
              >
                Start Auto-Pilot
              </button>
            )}
          </div>
        </div>

        <div className="flex items-center gap-3">
          <button onClick={onSelectAll} className="text-sm text-[#58a6ff] hover:underline">
            {selectedCount === issues.length ? "Deselect all" : "Select all"}
          </button>

          {selectedCount > 0 && (
            <button
              onClick={onLaunchSelected}
              disabled={launching}
              className="px-3 py-1.5 bg-[#58a6ff] text-white rounded-md text-sm font-medium hover:bg-[#79b8ff] disabled:opacity-50 transition-colors"
            >
              {launching
                ? "Launching..."
                : `Launch agents for ${selectedCount} issue${selectedCount > 1 ? "s" : ""}`}
            </button>
          )}

          <button
            onClick={onRefreshIssues}
            className="text-sm text-[#8b949e] hover:text-[#e6edf3] transition-colors ml-auto"
          >
            Refresh
          </button>
        </div>
      </div>

      {error && (
        <div className="mx-6 mt-4 bg-[#f8514926] border border-[#f85149] rounded-lg p-3">
          <p className="text-[#f85149] text-sm">{error}</p>
        </div>
      )}

      <div className="flex-1 overflow-auto p-6">
        {loading ? (
          <div className="text-center py-12 text-[#8b949e]">Loading issues...</div>
        ) : issues.length === 0 ? (
          <div className="text-center py-12 text-[#8b949e]">No open issues found.</div>
        ) : (
          <div className="space-y-2">
            {issues.map((issue) => (
              <IssueRow
                key={getIssueKeyFromIssue(issue)}
                issue={issue}
                isSelected={selectedIssueKeys.has(getIssueKeyFromIssue(issue))}
                showRepoName={showRepoName}
                onToggle={onToggleIssue}
                onRun={onRunIssue}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
