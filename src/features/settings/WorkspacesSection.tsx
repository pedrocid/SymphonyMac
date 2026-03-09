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
    <div className="bg-[#161b22] border border-[#30363d] rounded-xl overflow-hidden">
      <div className="px-5 pt-5 pb-0">
        <div className="flex items-start gap-3 mb-1">
          <div className="w-8 h-8 rounded-lg bg-[#21262d] flex items-center justify-center shrink-0 mt-0.5">
            <svg className="w-4 h-4 text-[#8b949e]" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round">
              <path d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
            </svg>
          </div>
          <div className="flex-1">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-semibold text-[#e6edf3]">Active Workspaces</h3>
              <div className="flex gap-2">
                <button
                  onClick={onCleanupOld}
                  disabled={workspaces.length === 0}
                  className="px-2.5 py-1 bg-[#21262d] text-[#d29922] border border-[#30363d] rounded-lg text-xs hover:bg-[#30363d] transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                >
                  Clean old ({workspaceTtlDays}d+)
                </button>
                <button
                  onClick={onCleanupAll}
                  disabled={workspaces.length === 0}
                  className="px-2.5 py-1 bg-[#21262d] text-[#f85149] border border-[#30363d] rounded-lg text-xs hover:bg-[#30363d] transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                >
                  Clean all
                </button>
                <button
                  onClick={onRefresh}
                  className="px-2.5 py-1 bg-[#21262d] text-[#8b949e] border border-[#30363d] rounded-lg text-xs hover:bg-[#30363d] hover:text-[#e6edf3] transition-colors"
                >
                  Refresh
                </button>
              </div>
            </div>
            <p className="text-xs text-[#8b949e] mt-1">
              {workspaces.length} workspace{workspaces.length !== 1 ? "s" : ""} &middot;{" "}
              {totalSizeDisplay} total
            </p>
          </div>
        </div>
      </div>

      <div className="px-5 pb-5 pt-4">
        {wsMessage && (
          <div className="mb-3 text-sm text-[#3fb950] flex items-center gap-1.5">
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
              <path d="M5 13l4 4L19 7" />
            </svg>
            {wsMessage}
          </div>
        )}

        {wsLoading ? (
          <div className="text-sm text-[#8b949e] py-6 text-center">Loading...</div>
        ) : workspaces.length === 0 ? (
          <div className="text-sm text-[#484f58] py-6 text-center rounded-lg bg-[#0d1117] border border-dashed border-[#21262d]">
            No workspaces found
          </div>
        ) : (
          <div className="space-y-2 max-h-80 overflow-y-auto">
            {workspaces.map((workspace) => (
              <div
                key={workspace.path}
                className="flex items-center justify-between bg-[#0d1117] border border-[#21262d] rounded-lg px-3 py-2.5 group hover:border-[#30363d] transition-colors"
              >
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <p className="text-sm text-[#e6edf3] truncate">{workspace.name}</p>
                    {workspace.is_worktree && (
                      <span className="px-1.5 py-0.5 text-[10px] bg-[#3fb95015] border border-[#3fb950] text-[#3fb950] rounded-full shrink-0">
                        worktree
                      </span>
                    )}
                  </div>
                  <p className="text-xs text-[#484f58]">
                    {workspace.size_display} &middot; {formatWorkspaceAge(workspace.age_days)} old
                  </p>
                </div>
                <button
                  onClick={() => onCleanupSingle(workspace.path)}
                  className="ml-3 px-2 py-1 text-xs text-[#f85149] opacity-0 group-hover:opacity-100 hover:bg-[#f8514910] rounded-md transition-all shrink-0"
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
