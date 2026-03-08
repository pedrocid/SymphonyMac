import { formatCompactNumber, formatElapsed } from "../../lib/formatters";
import type { OrchestratorStatus } from "../../lib/types";

export function DashboardMetricsBar({ status }: { status: OrchestratorStatus }) {
  const totalInputTokens = status.total_input_tokens ?? 0;
  const totalOutputTokens = status.total_output_tokens ?? 0;
  const totalCostUsd = status.total_cost_usd ?? 0;
  const totalRuntimeSecs = status.total_runtime_secs ?? 0;

  if (totalCostUsd <= 0 && totalInputTokens <= 0 && totalOutputTokens <= 0) {
    return null;
  }

  return (
    <div className="mx-6 mt-3 flex items-center gap-6 bg-[#161b22] border border-[#30363d] rounded-lg px-4 py-2 text-xs shrink-0">
      <div className="flex items-center gap-1.5 text-[#8b949e]">
        <span className="text-[#e6edf3] font-medium">
          {formatCompactNumber(totalInputTokens + totalOutputTokens)}
        </span>
        <span>tokens</span>
      </div>
      <div className="flex items-center gap-1.5 text-[#8b949e]">
        <span className="text-[#e6edf3] font-medium">{formatCompactNumber(totalInputTokens)}</span>
        <span>in</span>
        <span className="text-[#484f58]">/</span>
        <span className="text-[#e6edf3] font-medium">{formatCompactNumber(totalOutputTokens)}</span>
        <span>out</span>
      </div>
      <div className="flex items-center gap-1.5 text-[#8b949e]">
        <span className="text-[#3fb950] font-medium">${totalCostUsd.toFixed(4)}</span>
        <span>cost</span>
      </div>
      <div className="flex items-center gap-1.5 text-[#8b949e]">
        <span className="text-[#e6edf3] font-medium">
          {formatElapsed("", null, totalRuntimeSecs)}
        </span>
        <span>runtime</span>
      </div>
    </div>
  );
}
