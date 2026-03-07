import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { RunConfig } from "../App";

export function Settings() {
  const [config, setConfig] = useState<RunConfig>({
    agent_type: "claude",
    auto_approve: true,
    max_concurrent: 3,
    poll_interval_secs: 60,
    issue_label: null,
    max_turns: 1,
  });
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadConfig();
  }, []);

  async function loadConfig() {
    try {
      const status = await invoke<{ config: RunConfig }>("get_status");
      setConfig(status.config);
    } catch (_) {}
  }

  async function saveConfig() {
    setError(null);
    setSaved(false);
    try {
      await invoke("update_config", { config });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="flex-1 overflow-auto p-6">
      <div className="max-w-2xl mx-auto">
        <h2 className="text-2xl font-bold text-[#e6edf3] mb-6">Settings</h2>

        <div className="space-y-6">
          {/* Agent Type */}
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-5">
            <h3 className="text-sm font-medium text-[#e6edf3] mb-4">Agent Configuration</h3>

            <div className="space-y-4">
              <div>
                <label className="block text-sm text-[#8b949e] mb-2">Agent Type</label>
                <div className="flex gap-3">
                  <button
                    onClick={() => setConfig({ ...config, agent_type: "claude" })}
                    className={`flex-1 p-3 rounded-lg border text-sm font-medium transition-colors ${
                      config.agent_type === "claude"
                        ? "bg-[#58a6ff15] border-[#58a6ff] text-[#58a6ff]"
                        : "bg-[#0d1117] border-[#30363d] text-[#8b949e] hover:border-[#484f58]"
                    }`}
                  >
                    Claude Code
                  </button>
                  <button
                    onClick={() => setConfig({ ...config, agent_type: "codex" })}
                    className={`flex-1 p-3 rounded-lg border text-sm font-medium transition-colors ${
                      config.agent_type === "codex"
                        ? "bg-[#3fb95015] border-[#3fb950] text-[#3fb950]"
                        : "bg-[#0d1117] border-[#30363d] text-[#8b949e] hover:border-[#484f58]"
                    }`}
                  >
                    Codex
                  </button>
                </div>
              </div>

              <div className="flex items-center justify-between">
                <div>
                  <label className="text-sm text-[#e6edf3]">Auto-approve permissions</label>
                  <p className="text-xs text-[#8b949e] mt-0.5">
                    {config.agent_type === "claude"
                      ? "Uses --dangerously-skip-permissions"
                      : "Uses --full-auto mode"}
                  </p>
                </div>
                <button
                  onClick={() => setConfig({ ...config, auto_approve: !config.auto_approve })}
                  className={`w-12 h-6 rounded-full transition-colors relative ${
                    config.auto_approve ? "bg-[#58a6ff]" : "bg-[#30363d]"
                  }`}
                >
                  <span
                    className={`absolute top-0.5 w-5 h-5 bg-white rounded-full transition-transform ${
                      config.auto_approve ? "left-[26px]" : "left-0.5"
                    }`}
                  />
                </button>
              </div>

              <div>
                <label className="block text-sm text-[#8b949e] mb-2">Max Turns per Issue</label>
                <input
                  type="number"
                  min={1}
                  max={50}
                  value={config.max_turns}
                  onChange={(e) => setConfig({ ...config, max_turns: parseInt(e.target.value) || 1 })}
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff]"
                />
              </div>
            </div>
          </div>

          {/* Orchestrator */}
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-5">
            <h3 className="text-sm font-medium text-[#e6edf3] mb-4">Orchestrator</h3>

            <div className="space-y-4">
              <div>
                <label className="block text-sm text-[#8b949e] mb-2">Max Concurrent Agents</label>
                <input
                  type="number"
                  min={1}
                  max={20}
                  value={config.max_concurrent}
                  onChange={(e) =>
                    setConfig({ ...config, max_concurrent: parseInt(e.target.value) || 1 })
                  }
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff]"
                />
              </div>

              <div>
                <label className="block text-sm text-[#8b949e] mb-2">Poll Interval (seconds)</label>
                <input
                  type="number"
                  min={10}
                  max={600}
                  value={config.poll_interval_secs}
                  onChange={(e) =>
                    setConfig({ ...config, poll_interval_secs: parseInt(e.target.value) || 60 })
                  }
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff]"
                />
              </div>

              <div>
                <label className="block text-sm text-[#8b949e] mb-2">
                  Filter by Label (optional)
                </label>
                <input
                  type="text"
                  placeholder="e.g., symphony, auto-fix"
                  value={config.issue_label || ""}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      issue_label: e.target.value || null,
                    })
                  }
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] placeholder-[#484f58]"
                />
                <p className="text-xs text-[#8b949e] mt-1">
                  Only process issues with this label. Leave empty for all issues.
                </p>
              </div>
            </div>
          </div>

          {/* Save */}
          <div className="flex items-center gap-3">
            <button
              onClick={saveConfig}
              className="px-6 py-2 bg-[#58a6ff] text-white rounded-lg text-sm font-medium hover:bg-[#79b8ff] transition-colors"
            >
              Save Settings
            </button>
            {saved && <span className="text-sm text-[#3fb950]">Saved!</span>}
            {error && <span className="text-sm text-[#f85149]">{error}</span>}
          </div>
        </div>
      </div>
    </div>
  );
}
