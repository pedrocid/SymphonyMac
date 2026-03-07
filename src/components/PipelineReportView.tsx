import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface StageReport {
  name: string;
  status: string;
  duration_secs: number | null;
  duration_display: string;
  files_modified: string[];
  lines_added: number;
  lines_removed: number;
  commands_executed: string[];
  summary: string;
  attempt: number;
}

export interface PipelineReport {
  issue_number: number;
  issue_title: string;
  repo: string;
  total_duration_secs: number;
  total_duration_display: string;
  stages: StageReport[];
  pr_number: number | null;
  pr_url: string | null;
  issue_url: string;
  code_review_summary: string;
  testing_summary: string;
}

const STAGE_COLORS: Record<string, string> = {
  Implement: "#d29922",
  "Code Review": "#bc8cff",
  Testing: "#58a6ff",
  Merge: "#d2a8ff",
};

export function PipelineReportView({
  runId,
  onClose,
  onViewLogs,
}: {
  runId: string;
  onClose: () => void;
  onViewLogs: (runId: string) => void;
}) {
  const [report, setReport] = useState<PipelineReport | null>(null);
  const [expandedStages, setExpandedStages] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let issueNumber: number | null = null;
    loadReport().then((r) => { if (r) issueNumber = r.issue_number; });
    const unlisten = listen<PipelineReport>("pipeline-report", (event) => {
      // Only update if this event matches the pipeline we're viewing
      if (issueNumber !== null && event.payload.issue_number !== issueNumber) return;
      issueNumber = event.payload.issue_number;
      setReport(event.payload);
      setLoading(false);
    });
    return () => { unlisten.then((f) => f()); };
  }, [runId]);

  async function loadReport(): Promise<PipelineReport | null> {
    try {
      const result = await invoke<PipelineReport | null>("get_pipeline_report", { runId });
      if (result) setReport(result);
      setLoading(false);
      return result;
    } catch (_) {
      setLoading(false);
      return null;
    }
  }

  function toggleStage(name: string) {
    setExpandedStages((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }

  if (loading) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between p-3 border-b border-[#30363d]">
          <h3 className="text-sm font-medium text-[#e6edf3]">Pipeline Report</h3>
          <button onClick={onClose} className="text-[#8b949e] hover:text-[#e6edf3] text-lg">&times;</button>
        </div>
        <div className="flex-1 flex items-center justify-center text-[#8b949e]">Loading...</div>
      </div>
    );
  }

  if (!report) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between p-3 border-b border-[#30363d]">
          <h3 className="text-sm font-medium text-[#e6edf3]">Pipeline Report</h3>
          <button onClick={onClose} className="text-[#8b949e] hover:text-[#e6edf3] text-lg">&times;</button>
        </div>
        <div className="flex-1 flex flex-col items-center justify-center gap-3 text-[#8b949e]">
          <p>No report available for this run.</p>
          <button
            onClick={() => onViewLogs(runId)}
            className="text-[#58a6ff] hover:underline text-sm"
          >
            View raw logs instead
          </button>
        </div>
      </div>
    );
  }

  const totalFiles = report.stages.reduce((acc, s) => acc + s.files_modified.length, 0);
  const totalAdded = report.stages.reduce((acc, s) => acc + s.lines_added, 0);
  const totalRemoved = report.stages.reduce((acc, s) => acc + s.lines_removed, 0);

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between p-3 border-b border-[#30363d]">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-medium text-[#e6edf3]">Pipeline Report</h3>
          <span className="text-xs text-[#3fb950] bg-[#3fb95026] px-1.5 py-0.5 rounded">Completed</span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => onViewLogs(runId)}
            className="text-xs text-[#58a6ff] hover:underline"
          >
            Raw logs
          </button>
          <button onClick={onClose} className="text-[#8b949e] hover:text-[#e6edf3] text-lg leading-none">&times;</button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4 space-y-4">
        {/* Executive Summary */}
        <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-4">
          <h4 className="text-xs font-semibold text-[#8b949e] uppercase tracking-wider mb-3">Executive Summary</h4>
          <div className="text-sm text-[#e6edf3] mb-2">
            <span className="text-[#8b949e]">Issue:</span>{" "}
            <a href={report.issue_url} target="_blank" rel="noopener noreferrer" className="text-[#58a6ff] hover:underline">
              #{report.issue_number}
            </a>{" "}
            {report.issue_title}
          </div>
          {report.pr_url && (
            <div className="text-sm text-[#e6edf3] mb-2">
              <span className="text-[#8b949e]">PR:</span>{" "}
              <a href={report.pr_url} target="_blank" rel="noopener noreferrer" className="text-[#58a6ff] hover:underline">
                #{report.pr_number}
              </a>{" "}
              <span className="text-[#3fb950]">merged</span>
            </div>
          )}
          <div className="text-sm text-[#e6edf3]">
            <span className="text-[#8b949e]">Total time:</span> {report.total_duration_display}
          </div>
        </div>

        {/* Metrics */}
        <div className="grid grid-cols-3 gap-3">
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-3 text-center">
            <div className="text-lg font-bold text-[#e6edf3]">{report.total_duration_display}</div>
            <div className="text-xs text-[#8b949e]">Total Duration</div>
          </div>
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-3 text-center">
            <div className="text-lg font-bold text-[#3fb950]">+{totalAdded}</div>
            <div className="text-xs text-[#8b949e]">Lines Added</div>
          </div>
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-3 text-center">
            <div className="text-lg font-bold text-[#f85149]">-{totalRemoved}</div>
            <div className="text-xs text-[#8b949e]">Lines Removed</div>
          </div>
        </div>

        {/* Time Breakdown */}
        <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-4">
          <h4 className="text-xs font-semibold text-[#8b949e] uppercase tracking-wider mb-3">Time Breakdown</h4>
          <div className="space-y-2">
            {report.stages.map((stage) => {
              const pct = report.total_duration_secs > 0
                ? ((stage.duration_secs || 0) / report.total_duration_secs) * 100
                : 0;
              return (
                <div key={stage.name} className="flex items-center gap-2">
                  <span className="text-xs text-[#e6edf3] w-24 shrink-0">{stage.name}</span>
                  <div className="flex-1 h-2 bg-[#21262d] rounded-full overflow-hidden">
                    <div
                      className="h-full rounded-full"
                      style={{
                        width: `${Math.max(pct, 2)}%`,
                        backgroundColor: STAGE_COLORS[stage.name] || "#8b949e",
                      }}
                    />
                  </div>
                  <span className="text-xs text-[#8b949e] w-16 text-right shrink-0">{stage.duration_display}</span>
                </div>
              );
            })}
          </div>
        </div>

        {/* Code Review Summary */}
        {report.code_review_summary && (
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-4">
            <h4 className="text-xs font-semibold text-[#8b949e] uppercase tracking-wider mb-2">Code Review</h4>
            <p className="text-sm text-[#e6edf3]">{report.code_review_summary}</p>
          </div>
        )}

        {/* Testing Summary */}
        {report.testing_summary && (
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-4">
            <h4 className="text-xs font-semibold text-[#8b949e] uppercase tracking-wider mb-2">Testing</h4>
            <p className="text-sm text-[#e6edf3]">{report.testing_summary}</p>
          </div>
        )}

        {/* Stage Details */}
        <div className="space-y-2">
          <h4 className="text-xs font-semibold text-[#8b949e] uppercase tracking-wider">Stage Details</h4>
          {report.stages.map((stage) => (
            <div key={stage.name} className="bg-[#161b22] border border-[#30363d] rounded-lg overflow-hidden">
              <button
                onClick={() => toggleStage(stage.name)}
                className="w-full flex items-center justify-between p-3 hover:bg-[#21262d] transition-colors"
              >
                <div className="flex items-center gap-2">
                  <span
                    className="w-2 h-2 rounded-full"
                    style={{ backgroundColor: STAGE_COLORS[stage.name] || "#8b949e" }}
                  />
                  <span className="text-sm font-medium text-[#e6edf3]">{stage.name}</span>
                  <span className="text-xs text-[#3fb950]">{stage.status}</span>
                  {stage.attempt > 1 && (
                    <span className="text-xs text-[#d29922]">attempt #{stage.attempt}</span>
                  )}
                </div>
                <div className="flex items-center gap-3">
                  <span className="text-xs text-[#8b949e]">{stage.duration_display}</span>
                  <span className="text-xs text-[#484f58]">{expandedStages.has(stage.name) ? "^" : "v"}</span>
                </div>
              </button>

              {expandedStages.has(stage.name) && (
                <div className="border-t border-[#30363d] p-3 space-y-3">
                  {/* Summary */}
                  <div>
                    <div className="text-xs text-[#8b949e] mb-1">Summary</div>
                    <div className="text-sm text-[#e6edf3]">{stage.summary}</div>
                  </div>

                  {/* Diff stats */}
                  {(stage.lines_added > 0 || stage.lines_removed > 0) && (
                    <div>
                      <div className="text-xs text-[#8b949e] mb-1">Changes</div>
                      <div className="text-sm">
                        <span className="text-[#3fb950]">+{stage.lines_added}</span>
                        {" / "}
                        <span className="text-[#f85149]">-{stage.lines_removed}</span>
                      </div>
                    </div>
                  )}

                  {/* Files modified */}
                  {stage.files_modified.length > 0 && (
                    <div>
                      <div className="text-xs text-[#8b949e] mb-1">Files Modified ({stage.files_modified.length})</div>
                      <div className="space-y-0.5">
                        {stage.files_modified.map((file) => (
                          <div key={file} className="text-xs text-[#e6edf3] font-mono bg-[#0d1117] rounded px-2 py-1">
                            {file}
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Commands executed */}
                  {stage.commands_executed.length > 0 && (
                    <div>
                      <div className="text-xs text-[#8b949e] mb-1">Commands ({stage.commands_executed.length})</div>
                      <div className="space-y-0.5">
                        {stage.commands_executed.slice(0, 10).map((cmd, i) => (
                          <div key={i} className="text-xs text-[#8b949e] font-mono bg-[#0d1117] rounded px-2 py-1">
                            $ {cmd}
                          </div>
                        ))}
                        {stage.commands_executed.length > 10 && (
                          <div className="text-xs text-[#484f58]">
                            ...and {stage.commands_executed.length - 10} more
                          </div>
                        )}
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>
          ))}
        </div>

        {/* Files summary */}
        {totalFiles > 0 && (
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-4">
            <h4 className="text-xs font-semibold text-[#8b949e] uppercase tracking-wider mb-2">
              All Files Modified ({totalFiles})
            </h4>
            <div className="space-y-0.5 max-h-40 overflow-y-auto">
              {report.stages.flatMap((s) => s.files_modified).filter((f, i, arr) => arr.indexOf(f) === i).map((file) => (
                <div key={file} className="text-xs text-[#e6edf3] font-mono">
                  {file}
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
