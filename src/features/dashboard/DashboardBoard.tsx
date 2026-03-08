import type { DashboardColumn, KanbanCard } from "./types";

const STAGE_LABELS: Record<string, string> = {
  implement: "Implementing",
  code_review: "Reviewing",
  testing: "Testing",
  merge: "Merging",
};

const STAGE_ORDER = ["implement", "code_review", "testing", "merge"];

const STAGE_DISPLAY: Record<string, string> = {
  implement: "Implement",
  code_review: "Review",
  testing: "Testing",
  merge: "Merge",
};

function getAdvanceableStages(currentStage: string | undefined): string[] {
  if (!currentStage) return STAGE_ORDER;
  const idx = STAGE_ORDER.indexOf(currentStage);
  if (idx < 0) return [];
  return STAGE_ORDER.slice(idx + 1);
}

interface DashboardBoardProps {
  columns: DashboardColumn[];
  showRepoName: boolean;
  onViewLogs: (runId: string) => void;
  onViewReport?: (runId: string) => void;
  onLaunchIssueByKey: (issueKey: string) => void;
  onStopAgent: (runId: string) => void;
  onRetryAgent: (runId: string) => void;
  onRetryAgentFromStage: (runId: string, fromStage: string) => void;
  onApproveStage: (runId: string) => void;
  onRejectStage: (runId: string) => void;
  onAdvanceToStage: (runId: string, targetStage: string) => void;
}

export function DashboardBoard({
  columns,
  showRepoName,
  onViewLogs,
  onViewReport,
  onLaunchIssueByKey,
  onStopAgent,
  onRetryAgent,
  onRetryAgentFromStage,
  onApproveStage,
  onRejectStage,
  onAdvanceToStage,
}: DashboardBoardProps) {
  return (
    <div className="flex-1 overflow-x-auto overflow-y-hidden p-6">
      <div className="flex gap-4 h-full min-w-max">
        {columns.map((column) => (
          <div key={column.id} className="w-72 flex flex-col bg-[#0d1117] rounded-lg shrink-0">
            <div className="flex items-center justify-between px-3 py-3 shrink-0">
              <div className="flex items-center gap-2">
                <span className="w-3 h-3 rounded-full" style={{ backgroundColor: column.color }} />
                <span className="text-sm font-medium text-[#e6edf3]">{column.title}</span>
                <span className="text-xs text-[#484f58] ml-1">{column.items.length}</span>
              </div>
            </div>

            <div className="flex-1 overflow-y-auto px-2 pb-2 space-y-2">
              {column.items.map((card) => (
                <DashboardCard
                  key={card.id}
                  card={card}
                  columnId={column.id}
                  color={column.color}
                  showRepoName={showRepoName}
                  onViewLogs={onViewLogs}
                  onViewReport={onViewReport}
                  onLaunchIssueByKey={onLaunchIssueByKey}
                  onStopAgent={onStopAgent}
                  onRetryAgent={onRetryAgent}
                  onRetryAgentFromStage={onRetryAgentFromStage}
                  onApproveStage={onApproveStage}
                  onRejectStage={onRejectStage}
                  onAdvanceToStage={onAdvanceToStage}
                />
              ))}

              {column.items.length === 0 && (
                <div className="text-center py-8 text-xs text-[#30363d]">No issues</div>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

interface DashboardCardProps extends Omit<DashboardBoardProps, "columns" | "showRepoName"> {
  card: KanbanCard;
  color: string;
  columnId: string;
  showRepoName: boolean;
}

function DashboardCard({
  card,
  color,
  columnId,
  showRepoName,
  onViewLogs,
  onViewReport,
  onLaunchIssueByKey,
  onStopAgent,
  onRetryAgent,
  onRetryAgentFromStage,
  onApproveStage,
  onRejectStage,
  onAdvanceToStage,
}: DashboardCardProps) {
  return (
    <div
      className="bg-[#161b22] border border-[#30363d] rounded-lg p-3 hover:border-[#484f58] transition-colors cursor-pointer group"
      onClick={() => card.runId && onViewLogs(card.runId)}
    >
      <div className="text-xs text-[#484f58] mb-1">
        {card.repo && showRepoName && (
          <span className="text-[#8b949e] mr-1">{card.repo.split("/").pop()}</span>
        )}
        #{card.number}
      </div>
      <div className="text-sm text-[#e6edf3] font-medium mb-2 leading-snug">{card.title}</div>

      {card.runStatus === "running" && (
        <div className="flex items-center gap-1.5 mb-2">
          <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ backgroundColor: color }} />
          <span className="text-xs" style={{ color }}>
            {STAGE_LABELS[card.runStage || ""] || card.runStage}
            {card.attempt && card.attempt > 1 && ` (attempt ${card.attempt}/${(card.maxRetries || 0) + 1})`}
            {" - "}
            {card.elapsed}
          </span>
        </div>
      )}

      {card.runStatus === "preparing" && (
        <div className="flex items-center gap-1.5 mb-2">
          <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ backgroundColor: color }} />
          <span className="text-xs" style={{ color }}>
            Preparing...
          </span>
        </div>
      )}

      {card.runStatus === "waiting" && (
        <div className="flex items-center gap-1.5 mb-2">
          <span className="w-1.5 h-1.5 bg-[#484f58] rounded-full animate-pulse" />
          <span className="text-xs text-[#484f58]">Starting next stage...</span>
        </div>
      )}

      {card.runStatus === "awaiting_approval" && (
        <div className="mb-2">
          <div className="flex items-center gap-1.5 mb-2">
            <span className="w-1.5 h-1.5 bg-[#d29922] rounded-full animate-pulse" />
            <span className="text-xs text-[#d29922]">
              Awaiting approval{" "}
              {card.pendingNextStage &&
                `to proceed to ${card.pendingNextStage.replace("_", " ")}`}
            </span>
          </div>
          <div className="flex gap-2">
            <button
              onClick={(event) => {
                event.stopPropagation();
                if (card.runId) {
                  onApproveStage(card.runId);
                }
              }}
              className="px-2.5 py-1 text-xs font-medium bg-[#3fb95015] text-[#3fb950] border border-[#3fb950] rounded-md hover:bg-[#3fb95030] transition-colors"
            >
              Approve
            </button>
            <button
              onClick={(event) => {
                event.stopPropagation();
                if (card.runId) {
                  onRejectStage(card.runId);
                }
              }}
              className="px-2.5 py-1 text-xs font-medium bg-[#f8514915] text-[#f85149] border border-[#f85149] rounded-md hover:bg-[#f8514930] transition-colors"
            >
              Reject
            </button>
          </div>
        </div>
      )}

      {card.runStatus === "completed" && card.runId && card.runStage && card.runStage !== "done" && (() => {
        const stages = getAdvanceableStages(card.runStage);
        if (stages.length === 0) return null;
        return (
          <div className="mb-2">
            <div className="flex items-center gap-1.5 mb-2">
              <span className="w-1.5 h-1.5 bg-[#58a6ff] rounded-full" />
              <span className="text-xs text-[#58a6ff]">Advance to next stage</span>
            </div>
            <div className="flex flex-wrap gap-1.5">
              {stages.map((stage) => (
                <button
                  key={stage}
                  onClick={(event) => {
                    event.stopPropagation();
                    onAdvanceToStage(card.runId!, stage);
                  }}
                  className="px-2 py-0.5 text-xs font-medium bg-[#58a6ff15] text-[#58a6ff] border border-[#58a6ff] rounded-md hover:bg-[#58a6ff30] transition-colors"
                >
                  {STAGE_DISPLAY[stage] || stage}
                </button>
              ))}
            </div>
          </div>
        );
      })()}

      {card.blockedBy && card.blockedBy.length > 0 && (
        <div className="flex items-center gap-1.5 mb-2">
          <span className="text-xs text-[#da3633]">
            Blocked by {card.blockedBy.map((issueNumber) => `#${issueNumber}`).join(", ")}
          </span>
        </div>
      )}

      {card.skippedStages && card.skippedStages.length > 0 && (
        <div className="flex items-center gap-1.5 mb-2">
          <span className="text-xs text-[#d29922]">
            Skipped: {card.skippedStages.map((stage) => stage.replace("_", " ")).join(", ")}
          </span>
        </div>
      )}

      {card.error && (
        <div className="text-xs text-[#f85149] truncate mb-2">
          {card.attempt && card.attempt > 1 && `[Attempt ${card.attempt}/${(card.maxRetries || 0) + 1}] `}
          {card.error}
        </div>
      )}

      {card.labels.length > 0 && (
        <div className="flex flex-wrap gap-1 mb-2">
          {card.labels.map((label) => (
            <span
              key={label}
              className="text-[10px] px-1.5 py-0.5 rounded-full bg-[#21262d] text-[#8b949e] border border-[#30363d]"
            >
              {label}
            </span>
          ))}
        </div>
      )}

      <div className="flex items-center justify-between text-xs text-[#484f58]">
        <span>{card.updated}</span>
        <div className="flex items-center gap-2">
          {card.runId && card.runStage === "done" && onViewReport && (
            <button
              onClick={(event) => {
                event.stopPropagation();
                onViewReport(card.runId!);
              }}
              className="text-[#3fb950] hover:underline opacity-0 group-hover:opacity-100 transition-opacity"
            >
              Report
            </button>
          )}
          {card.runId && (
            <button
              onClick={(event) => {
                event.stopPropagation();
                onViewLogs(card.runId!);
              }}
              className="text-[#58a6ff] hover:underline opacity-0 group-hover:opacity-100 transition-opacity"
            >
              Logs
            </button>
          )}
          {(card.runStatus === "running" || card.runStatus === "preparing") && card.runId && (
            <button
              onClick={(event) => {
                event.stopPropagation();
                onStopAgent(card.runId!);
              }}
              className="text-[#f85149] hover:underline opacity-0 group-hover:opacity-100 transition-opacity"
            >
              Stop
            </button>
          )}
          {(card.runStatus === "failed" ||
            card.runStatus === "stopped" ||
            card.runStatus === "interrupted") &&
            card.runId &&
            card.runStage &&
            card.runStage !== "implement" && (
              <>
                <button
                  onClick={(event) => {
                    event.stopPropagation();
                    onRetryAgentFromStage(card.runId!, card.runStage!);
                  }}
                  className="text-[#d29922] hover:underline opacity-0 group-hover:opacity-100 transition-opacity"
                >
                  Retry {STAGE_LABELS[card.runStage] || card.runStage}
                </button>
                <button
                  onClick={(event) => {
                    event.stopPropagation();
                    onRetryAgentFromStage(card.runId!, "implement");
                  }}
                  className="text-[#8b949e] hover:underline opacity-0 group-hover:opacity-100 transition-opacity"
                >
                  Restart
                </button>
              </>
            )}
          {(card.runStatus === "failed" ||
            card.runStatus === "stopped" ||
            card.runStatus === "interrupted") &&
            card.runId &&
            (!card.runStage || card.runStage === "implement") && (
              <button
                onClick={(event) => {
                  event.stopPropagation();
                  onRetryAgent(card.runId!);
                }}
                className="text-[#d29922] hover:underline opacity-0 group-hover:opacity-100 transition-opacity"
              >
                Retry
              </button>
            )}
          {!card.runId && columnId === "open" && (
            <button
              onClick={(event) => {
                event.stopPropagation();
                onLaunchIssueByKey(card.issueKey);
              }}
              className="text-[#3fb950] hover:underline opacity-0 group-hover:opacity-100 transition-opacity"
            >
              Run
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
