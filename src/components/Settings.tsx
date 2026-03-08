import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { RunConfig } from "../App";

interface WorkspaceInfo {
  name: string;
  path: string;
  size_bytes: number;
  size_display: string;
  modified_at: string;
  age_days: number;
}

const STAGE_KEYS = ["implement", "code_review", "testing", "merge"] as const;
const STAGE_LABELS: Record<string, string> = {
  implement: "Implement",
  code_review: "Code Review",
  testing: "Testing",
  merge: "Merge",
};

export function Settings() {
  const [config, setConfig] = useState<RunConfig>({
    agent_type: "claude",
    auto_approve: true,
    max_concurrent: 3,
    poll_interval_secs: 60,
    issue_label: null,
    max_turns: 1,
    notifications_enabled: true,
    notification_sound: true,
    max_retries: 1,
    retry_backoff_secs: 10,
    cleanup_on_failure: false,
    cleanup_on_stop: false,
    workspace_ttl_days: 7,
    max_concurrent_by_stage: {},
    stage_prompts: {},
    hooks: {
      after_create: null,
      before_run: null,
      after_run: null,
      before_remove: null,
      timeout_secs: 60,
    },
    priority_labels: ["priority:critical", "priority:high", "priority:medium", "priority:low"],
  });
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceInfo[]>([]);
  const [wsLoading, setWsLoading] = useState(false);
  const [wsMessage, setWsMessage] = useState<string | null>(null);
  const [defaultPrompts, setDefaultPrompts] = useState<Record<string, string>>({});
  const [expandedStage, setExpandedStage] = useState<string | null>(null);

  useEffect(() => {
    loadConfig();
    loadWorkspaces();
    loadDefaultPrompts();
  }, []);

  async function loadDefaultPrompts() {
    try {
      const defaults = await invoke<Record<string, string>>("get_default_prompts");
      setDefaultPrompts(defaults);
    } catch (_) {}
  }

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

  async function loadWorkspaces() {
    setWsLoading(true);
    try {
      const result = await invoke<WorkspaceInfo[]>("list_workspaces");
      setWorkspaces(result);
    } catch (_) {}
    setWsLoading(false);
  }

  async function cleanupSingle(path: string) {
    try {
      await invoke("cleanup_single_workspace", { path });
      setWsMessage("Workspace removed");
      setTimeout(() => setWsMessage(null), 2000);
      loadWorkspaces();
    } catch (e) {
      setError(String(e));
    }
  }

  async function cleanupAll() {
    try {
      const removed = await invoke<number>("cleanup_all_workspaces");
      setWsMessage(`Removed ${removed} workspace${removed !== 1 ? "s" : ""}`);
      setTimeout(() => setWsMessage(null), 2000);
      loadWorkspaces();
    } catch (e) {
      setError(String(e));
    }
  }

  async function cleanupOld() {
    try {
      const removed = await invoke<number>("cleanup_old_workspaces", {
        maxAgeDays: config.workspace_ttl_days,
      });
      setWsMessage(
        removed > 0
          ? `Removed ${removed} old workspace${removed !== 1 ? "s" : ""}`
          : "No old workspaces to remove"
      );
      setTimeout(() => setWsMessage(null), 2000);
      loadWorkspaces();
    } catch (e) {
      setError(String(e));
    }
  }

  function formatAge(days: number): string {
    if (days < 1) return "< 1 day";
    if (days < 2) return "1 day";
    return `${Math.floor(days)} days`;
  }

  const totalSize = workspaces.reduce((sum, ws) => sum + ws.size_bytes, 0);
  const totalSizeDisplay =
    totalSize < 1024 * 1024
      ? `${(totalSize / 1024).toFixed(1)} KB`
      : totalSize < 1024 * 1024 * 1024
        ? `${(totalSize / (1024 * 1024)).toFixed(1)} MB`
        : `${(totalSize / (1024 * 1024 * 1024)).toFixed(2)} GB`;

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
                      : "Uses --dangerously-bypass-approvals-and-sandbox"}
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
                <label className="block text-sm text-[#8b949e] mb-2">Per-Stage Concurrency Limits</label>
                <p className="text-xs text-[#8b949e] mb-3">
                  Set per-stage limits (0 = use global max). Stages without limits fall back to the global setting.
                </p>
                <div className="grid grid-cols-2 gap-3">
                  {(["implement", "code_review", "testing", "merge"] as const).map((stage) => (
                    <div key={stage}>
                      <label className="block text-xs text-[#8b949e] mb-1 capitalize">
                        {stage.replace("_", " ")}
                      </label>
                      <input
                        type="number"
                        min={0}
                        max={20}
                        value={config.max_concurrent_by_stage[stage] || 0}
                        onChange={(e) => {
                          const val = parseInt(e.target.value) || 0;
                          const updated = { ...config.max_concurrent_by_stage };
                          if (val === 0) {
                            delete updated[stage];
                          } else {
                            updated[stage] = val;
                          }
                          setConfig({ ...config, max_concurrent_by_stage: updated });
                        }}
                        className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff]"
                      />
                    </div>
                  ))}
                </div>
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
                <label className="block text-sm text-[#8b949e] mb-2">Max Retries per Stage</label>
                <input
                  type="number"
                  min={0}
                  max={5}
                  value={config.max_retries}
                  onChange={(e) =>
                    setConfig({ ...config, max_retries: parseInt(e.target.value) || 0 })
                  }
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff]"
                />
                <p className="text-xs text-[#8b949e] mt-1">
                  Number of automatic retries when a pipeline stage fails. Set to 0 to disable.
                </p>
              </div>

              <div>
                <label className="block text-sm text-[#8b949e] mb-2">Retry Backoff (seconds)</label>
                <input
                  type="number"
                  min={1}
                  max={300}
                  value={config.retry_backoff_secs}
                  onChange={(e) =>
                    setConfig({ ...config, retry_backoff_secs: parseInt(e.target.value) || 10 })
                  }
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff]"
                />
                <p className="text-xs text-[#8b949e] mt-1">
                  Delay in seconds before retrying a failed stage.
                </p>
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

          {/* Priority Labels */}
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-5">
            <h3 className="text-sm font-medium text-[#e6edf3] mb-4">Dispatch Priority</h3>
            <div>
              <label className="block text-sm text-[#8b949e] mb-2">
                Priority Labels (highest priority first)
              </label>
              <input
                type="text"
                placeholder="priority:critical, priority:high, priority:medium, priority:low"
                value={config.priority_labels.join(", ")}
                onChange={(e) =>
                  setConfig({
                    ...config,
                    priority_labels: e.target.value
                      .split(",")
                      .map((s) => s.trim())
                      .filter((s) => s.length > 0),
                  })
                }
                className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] placeholder-[#484f58]"
              />
              <p className="text-xs text-[#8b949e] mt-1">
                Comma-separated list of labels. Issues with labels listed first are dispatched first.
                Issues without any priority label are dispatched last. Within the same priority,
                older issues are dispatched first.
              </p>
            </div>
          </div>

          {/* Workspace Cleanup */}
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-5">
            <h3 className="text-sm font-medium text-[#e6edf3] mb-4">Workspace Cleanup</h3>

            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div>
                  <label className="text-sm text-[#e6edf3]">Cleanup on failure</label>
                  <p className="text-xs text-[#8b949e] mt-0.5">
                    Remove workspace when an agent fails (off = keep for debugging)
                  </p>
                </div>
                <button
                  onClick={() =>
                    setConfig({ ...config, cleanup_on_failure: !config.cleanup_on_failure })
                  }
                  className={`w-12 h-6 rounded-full transition-colors relative ${
                    config.cleanup_on_failure ? "bg-[#58a6ff]" : "bg-[#30363d]"
                  }`}
                >
                  <span
                    className={`absolute top-0.5 w-5 h-5 bg-white rounded-full transition-transform ${
                      config.cleanup_on_failure ? "left-[26px]" : "left-0.5"
                    }`}
                  />
                </button>
              </div>

              <div className="flex items-center justify-between">
                <div>
                  <label className="text-sm text-[#e6edf3]">Cleanup on stop</label>
                  <p className="text-xs text-[#8b949e] mt-0.5">
                    Remove workspace when an agent is manually stopped
                  </p>
                </div>
                <button
                  onClick={() =>
                    setConfig({ ...config, cleanup_on_stop: !config.cleanup_on_stop })
                  }
                  className={`w-12 h-6 rounded-full transition-colors relative ${
                    config.cleanup_on_stop ? "bg-[#58a6ff]" : "bg-[#30363d]"
                  }`}
                >
                  <span
                    className={`absolute top-0.5 w-5 h-5 bg-white rounded-full transition-transform ${
                      config.cleanup_on_stop ? "left-[26px]" : "left-0.5"
                    }`}
                  />
                </button>
              </div>

              <div>
                <label className="block text-sm text-[#8b949e] mb-2">
                  Workspace TTL (days)
                </label>
                <input
                  type="number"
                  min={1}
                  max={365}
                  value={config.workspace_ttl_days}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      workspace_ttl_days: parseInt(e.target.value) || 7,
                    })
                  }
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff]"
                />
                <p className="text-xs text-[#8b949e] mt-1">
                  Workspaces older than this are cleaned up on app startup. Set to 0 to disable.
                </p>
              </div>
            </div>
          </div>

          {/* Lifecycle Hooks */}
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-5">
            <h3 className="text-sm font-medium text-[#e6edf3] mb-1">Lifecycle Hooks</h3>
            <p className="text-xs text-[#8b949e] mb-4">
              Shell commands executed at key points in the workspace lifecycle.
              Commands run in the workspace directory.
            </p>

            <div className="space-y-4">
              <div>
                <label className="block text-sm text-[#8b949e] mb-1">after_create</label>
                <input
                  type="text"
                  placeholder="e.g., npm install && npm run build"
                  value={config.hooks.after_create || ""}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      hooks: { ...config.hooks, after_create: e.target.value || null },
                    })
                  }
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] placeholder-[#484f58] font-mono"
                />
                <p className="text-xs text-[#8b949e] mt-1">
                  Runs after a new workspace is cloned. Failure aborts the operation.
                </p>
              </div>

              <div>
                <label className="block text-sm text-[#8b949e] mb-1">before_run</label>
                <input
                  type="text"
                  placeholder="e.g., git pull origin main"
                  value={config.hooks.before_run || ""}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      hooks: { ...config.hooks, before_run: e.target.value || null },
                    })
                  }
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] placeholder-[#484f58] font-mono"
                />
                <p className="text-xs text-[#8b949e] mt-1">
                  Runs before each agent attempt. Failure aborts the run.
                </p>
              </div>

              <div>
                <label className="block text-sm text-[#8b949e] mb-1">after_run</label>
                <input
                  type="text"
                  placeholder="e.g., cp -r ./coverage /tmp/artifacts/"
                  value={config.hooks.after_run || ""}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      hooks: { ...config.hooks, after_run: e.target.value || null },
                    })
                  }
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] placeholder-[#484f58] font-mono"
                />
                <p className="text-xs text-[#8b949e] mt-1">
                  Runs after each agent attempt (success or failure). Failure is logged but ignored.
                </p>
              </div>

              <div>
                <label className="block text-sm text-[#8b949e] mb-1">before_remove</label>
                <input
                  type="text"
                  placeholder="e.g., tar czf /tmp/workspace-backup.tar.gz ."
                  value={config.hooks.before_remove || ""}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      hooks: { ...config.hooks, before_remove: e.target.value || null },
                    })
                  }
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] placeholder-[#484f58] font-mono"
                />
                <p className="text-xs text-[#8b949e] mt-1">
                  Runs before workspace deletion. Failure is logged but ignored.
                </p>
              </div>

              <div>
                <label className="block text-sm text-[#8b949e] mb-2">Hook Timeout (seconds)</label>
                <input
                  type="number"
                  min={5}
                  max={600}
                  value={config.hooks.timeout_secs}
                  onChange={(e) =>
                    setConfig({
                      ...config,
                      hooks: { ...config.hooks, timeout_secs: parseInt(e.target.value) || 60 },
                    })
                  }
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff]"
                />
                <p className="text-xs text-[#8b949e] mt-1">
                  Maximum time each hook is allowed to run before being killed.
                </p>
              </div>
            </div>
          </div>

          {/* Notifications */}
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-5">
            <h3 className="text-sm font-medium text-[#e6edf3] mb-4">Notifications</h3>

            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div>
                  <label className="text-sm text-[#e6edf3]">Enable Notifications</label>
                  <p className="text-xs text-[#8b949e] mt-0.5">
                    Get notified when pipelines complete or fail
                  </p>
                </div>
                <button
                  onClick={() =>
                    setConfig({ ...config, notifications_enabled: !config.notifications_enabled })
                  }
                  className={`w-12 h-6 rounded-full transition-colors relative ${
                    config.notifications_enabled ? "bg-[#58a6ff]" : "bg-[#30363d]"
                  }`}
                >
                  <span
                    className={`absolute top-0.5 w-5 h-5 bg-white rounded-full transition-transform ${
                      config.notifications_enabled ? "left-[26px]" : "left-0.5"
                    }`}
                  />
                </button>
              </div>

              <div className="flex items-center justify-between">
                <div>
                  <label className="text-sm text-[#e6edf3]">Notification Sound</label>
                  <p className="text-xs text-[#8b949e] mt-0.5">
                    Play a sound with notifications
                  </p>
                </div>
                <button
                  onClick={() =>
                    setConfig({ ...config, notification_sound: !config.notification_sound })
                  }
                  disabled={!config.notifications_enabled}
                  className={`w-12 h-6 rounded-full transition-colors relative ${
                    config.notification_sound && config.notifications_enabled
                      ? "bg-[#58a6ff]"
                      : "bg-[#30363d]"
                  } ${!config.notifications_enabled ? "opacity-50 cursor-not-allowed" : ""}`}
                >
                  <span
                    className={`absolute top-0.5 w-5 h-5 bg-white rounded-full transition-transform ${
                      config.notification_sound && config.notifications_enabled
                        ? "left-[26px]"
                        : "left-0.5"
                    }`}
                  />
                </button>
              </div>
            </div>
          </div>

          {/* Prompt Templates */}
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-5">
            <h3 className="text-sm font-medium text-[#e6edf3] mb-2">Prompt Templates</h3>
            <p className="text-xs text-[#8b949e] mb-4">
              Customize the prompt sent to agents at each pipeline stage. Use template variables:{" "}
              <code className="text-[#7ee787]">{"{{issue_number}}"}</code>,{" "}
              <code className="text-[#7ee787]">{"{{issue_title}}"}</code>,{" "}
              <code className="text-[#7ee787]">{"{{issue_body}}"}</code>,{" "}
              <code className="text-[#7ee787]">{"{{repo}}"}</code>,{" "}
              <code className="text-[#7ee787]">{"{{attempt}}"}</code>,{" "}
              <code className="text-[#7ee787]">{"{{previous_error}}"}</code>
            </p>

            <div className="space-y-2">
              {STAGE_KEYS.map((stage) => {
                const isExpanded = expandedStage === stage;
                const hasCustom = !!(config.stage_prompts[stage] && config.stage_prompts[stage].trim());
                return (
                  <div key={stage} className="border border-[#30363d] rounded-lg overflow-hidden">
                    <button
                      onClick={() => setExpandedStage(isExpanded ? null : stage)}
                      className="w-full flex items-center justify-between px-4 py-3 bg-[#0d1117] hover:bg-[#161b22] transition-colors text-left"
                    >
                      <div className="flex items-center gap-2">
                        <span className="text-sm text-[#e6edf3]">{STAGE_LABELS[stage]}</span>
                        {hasCustom && (
                          <span className="px-1.5 py-0.5 text-[10px] bg-[#58a6ff20] text-[#58a6ff] rounded">
                            Custom
                          </span>
                        )}
                      </div>
                      <span className="text-[#8b949e] text-xs">{isExpanded ? "▲" : "▼"}</span>
                    </button>
                    {isExpanded && (
                      <div className="p-4 bg-[#0d1117] border-t border-[#30363d]">
                        <textarea
                          value={config.stage_prompts[stage] ?? defaultPrompts[stage] ?? ""}
                          onChange={(e) =>
                            setConfig({
                              ...config,
                              stage_prompts: { ...config.stage_prompts, [stage]: e.target.value },
                            })
                          }
                          rows={12}
                          className="w-full px-3 py-2 bg-[#161b22] border border-[#30363d] rounded-md text-[#e6edf3] text-xs font-mono outline-none focus:border-[#58a6ff] resize-y leading-relaxed"
                          placeholder={defaultPrompts[stage] ?? ""}
                        />
                        <div className="flex justify-end mt-2">
                          <button
                            onClick={() => {
                              const updated = { ...config.stage_prompts };
                              delete updated[stage];
                              setConfig({ ...config, stage_prompts: updated });
                            }}
                            className="px-3 py-1.5 text-xs text-[#d29922] bg-[#21262d] border border-[#30363d] rounded-md hover:bg-[#30363d] transition-colors"
                          >
                            Reset to Default
                          </button>
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}
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

          {/* Workspaces */}
          <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-5">
            <div className="flex items-center justify-between mb-4">
              <div>
                <h3 className="text-sm font-medium text-[#e6edf3]">Workspaces</h3>
                <p className="text-xs text-[#8b949e] mt-0.5">
                  {workspaces.length} workspace{workspaces.length !== 1 ? "s" : ""} &middot; {totalSizeDisplay} total
                </p>
              </div>
              <div className="flex gap-2">
                <button
                  onClick={cleanupOld}
                  disabled={workspaces.length === 0}
                  className="px-3 py-1.5 bg-[#21262d] text-[#d29922] border border-[#30363d] rounded-md text-xs hover:bg-[#30363d] transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Clean old ({config.workspace_ttl_days}d+)
                </button>
                <button
                  onClick={cleanupAll}
                  disabled={workspaces.length === 0}
                  className="px-3 py-1.5 bg-[#21262d] text-[#f85149] border border-[#30363d] rounded-md text-xs hover:bg-[#30363d] transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Clean all
                </button>
                <button
                  onClick={loadWorkspaces}
                  className="px-3 py-1.5 bg-[#21262d] text-[#8b949e] border border-[#30363d] rounded-md text-xs hover:bg-[#30363d] transition-colors"
                >
                  Refresh
                </button>
              </div>
            </div>

            {wsMessage && (
              <div className="mb-3 text-sm text-[#3fb950]">{wsMessage}</div>
            )}

            {wsLoading ? (
              <div className="text-sm text-[#8b949e] py-4 text-center">Loading...</div>
            ) : workspaces.length === 0 ? (
              <div className="text-sm text-[#484f58] py-4 text-center">No workspaces found</div>
            ) : (
              <div className="space-y-2 max-h-80 overflow-y-auto">
                {workspaces.map((ws) => (
                  <div
                    key={ws.path}
                    className="flex items-center justify-between bg-[#0d1117] border border-[#30363d] rounded-lg px-3 py-2"
                  >
                    <div className="flex-1 min-w-0">
                      <p className="text-sm text-[#e6edf3] truncate">{ws.name}</p>
                      <p className="text-xs text-[#8b949e]">
                        {ws.size_display} &middot; {formatAge(ws.age_days)} old
                      </p>
                    </div>
                    <button
                      onClick={() => cleanupSingle(ws.path)}
                      className="ml-3 px-2 py-1 text-xs text-[#f85149] hover:bg-[#f8514915] rounded transition-colors shrink-0"
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
