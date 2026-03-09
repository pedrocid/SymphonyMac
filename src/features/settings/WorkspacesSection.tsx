import { formatWorkspaceAge } from "../../lib/formatters";
import type { WorkspaceInfo } from "../../lib/types";

interface WorkspacesSectionProps {
  workspaces: WorkspaceInfo[];
  wsLoading: boolean;
  wsMessage: string | null;
  totalSizeDisplay: string;
  workspaceTtlDays: number;
  onRefresh: () => void;
  onCleanupOld: () => void;
  onCleanupAll: () => void;
  onCleanupSingle: (path: string) => void;
}

export function WorkspacesSection({
  workspaces,
  wsLoading,
  wsMessage,
  totalSizeDisplay,
  workspaceTtlDays,
  onRefresh,
  onCleanupOld,
  onCleanupAll,
  onCleanupSingle,
}: WorkspacesSectionProps) {
  return (
    <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-5">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h3 className="text-sm font-medium text-[#e6edf3]">Workspaces</h3>
          <p className="text-xs text-[#8b949e] mt-0.5">
            {workspaces.length} workspace{workspaces.length !== 1 ? "s" : ""} &middot;{" "}
            {totalSizeDisplay} total
          </p>
        </div>
        <div className="flex gap-2">
          <button
            onClick={onCleanupOld}
            disabled={workspaces.length === 0}
            className="px-3 py-1.5 bg-[#21262d] text-[#d29922] border border-[#30363d] rounded-md text-xs hover:bg-[#30363d] transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Clean old ({workspaceTtlDays}d+)
          </button>
          <button
            onClick={onCleanupAll}
            disabled={workspaces.length === 0}
            className="px-3 py-1.5 bg-[#21262d] text-[#f85149] border border-[#30363d] rounded-md text-xs hover:bg-[#30363d] transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Clean all
          </button>
          <button
            onClick={onRefresh}
            className="px-3 py-1.5 bg-[#21262d] text-[#8b949e] border border-[#30363d] rounded-md text-xs hover:bg-[#30363d] transition-colors"
          >
            Refresh
          </button>
        </div>
      </div>

      {wsMessage && <div className="mb-3 text-sm text-[#3fb950]">{wsMessage}</div>}

      {wsLoading ? (
        <div className="text-sm text-[#8b949e] py-4 text-center">Loading...</div>
      ) : workspaces.length === 0 ? (
        <div className="text-sm text-[#484f58] py-4 text-center">No workspaces found</div>
      ) : (
        <div className="space-y-2 max-h-80 overflow-y-auto">
          {workspaces.map((workspace) => (
            <div
              key={workspace.path}
              className="flex items-center justify-between bg-[#0d1117] border border-[#30363d] rounded-lg px-3 py-2"
            >
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <p className="text-sm text-[#e6edf3] truncate">{workspace.name}</p>
                  {workspace.is_worktree && (
                    <span className="px-1.5 py-0.5 text-[10px] bg-[#3fb95015] border border-[#3fb950] text-[#3fb950] rounded shrink-0">
                      worktree
                    </span>
                  )}
                </div>
                <p className="text-xs text-[#8b949e]">
                  {workspace.size_display} &middot; {formatWorkspaceAge(workspace.age_days)} old
                </p>
              </div>
              <button
                onClick={() => onCleanupSingle(workspace.path)}
                className="ml-3 px-2 py-1 text-xs text-[#f85149] hover:bg-[#f8514915] rounded transition-colors shrink-0"
              >
                Remove
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
