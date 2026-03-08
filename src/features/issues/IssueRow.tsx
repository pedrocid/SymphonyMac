import type { RepoIssue } from "../../lib/types";

const LABEL_COLORS: Record<string, string> = {
  bug: "bg-[#f8514926] text-[#f85149] border-[#f85149]",
  enhancement: "bg-[#3fb95026] text-[#3fb950] border-[#3fb950]",
  feature: "bg-[#58a6ff26] text-[#58a6ff] border-[#58a6ff]",
  documentation: "bg-[#d2992226] text-[#d29922] border-[#d29922]",
};

interface IssueRowProps {
  issue: RepoIssue;
  isSelected: boolean;
  showRepoName: boolean;
  onToggle: (issue: RepoIssue) => void;
  onRun: (issue: RepoIssue) => void;
}

export function IssueRow({
  issue,
  isSelected,
  showRepoName,
  onToggle,
  onRun,
}: IssueRowProps) {
  return (
    <div
      onClick={() => onToggle(issue)}
      className={`p-4 rounded-lg border cursor-pointer transition-colors ${
        isSelected
          ? "bg-[#58a6ff15] border-[#58a6ff]"
          : "bg-[#161b22] border-[#30363d] hover:border-[#484f58]"
      }`}
    >
      <div className="flex items-start gap-3">
        <input
          type="checkbox"
          checked={isSelected}
          onChange={() => {
            // Selection is handled by the row click to keep a larger hit target.
          }}
          className="mt-1 accent-[#58a6ff]"
        />
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            {showRepoName && (
              <span className="text-[#8b949e] text-xs">{issue._repo.split("/").pop()}</span>
            )}
            <span className="text-[#8b949e] text-sm">#{issue.number}</span>
            <span className="text-[#e6edf3] font-medium">{issue.title}</span>
          </div>
          {issue.body && (
            <p className="text-sm text-[#8b949e] mt-1 line-clamp-2">{issue.body.slice(0, 200)}</p>
          )}
          <div className="flex items-center gap-2 mt-2">
            {issue.labels.map((label) => (
              <span
                key={label}
                className={`text-xs px-2 py-0.5 rounded-full border ${
                  LABEL_COLORS[label.toLowerCase()] ||
                  "bg-[#21262d] text-[#8b949e] border-[#30363d]"
                }`}
              >
                {label}
              </span>
            ))}
            {issue.assignee && (
              <span className="text-xs text-[#8b949e]">assigned to {issue.assignee}</span>
            )}
          </div>
        </div>
        <button
          onClick={(event) => {
            event.stopPropagation();
            onRun(issue);
          }}
          className="px-3 py-1 bg-[#21262d] border border-[#30363d] rounded-md text-sm text-[#8b949e] hover:text-[#e6edf3] hover:border-[#484f58] transition-colors shrink-0"
        >
          Run
        </button>
      </div>
    </div>
  );
}
