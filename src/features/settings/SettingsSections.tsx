import type { Dispatch, ReactNode, SetStateAction } from "react";
import type { RunConfig } from "../../lib/types";
import { STAGE_KEYS, STAGE_LABELS } from "./constants";

type ConfigSetter = Dispatch<SetStateAction<RunConfig>>;

function SectionCard({
  title,
  description,
  children,
}: {
  title: string;
  description?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="bg-[#161b22] border border-[#30363d] rounded-lg p-5">
      <h3 className="text-sm font-medium text-[#e6edf3] mb-1">{title}</h3>
      {description ? <div className="text-xs text-[#8b949e] mb-4">{description}</div> : null}
      {children}
    </div>
  );
}

function Toggle({
  enabled,
  onToggle,
  disabled,
  activeColor = "#58a6ff",
}: {
  enabled: boolean;
  onToggle: () => void;
  disabled?: boolean;
  activeColor?: string;
}) {
  return (
    <button
      onClick={onToggle}
      disabled={disabled}
      className={`w-12 h-6 rounded-full transition-colors relative ${
        enabled ? `bg-[${activeColor}]` : "bg-[#30363d]"
      } ${disabled ? "opacity-50 cursor-not-allowed" : ""}`}
      style={{ backgroundColor: enabled ? activeColor : "#30363d" }}
    >
      <span
        className={`absolute top-0.5 w-5 h-5 bg-white rounded-full transition-transform ${
          enabled ? "left-[26px]" : "left-0.5"
        }`}
      />
    </button>
  );
}

export function AgentConfigurationSection({
  config,
  setConfig,
}: {
  config: RunConfig;
  setConfig: ConfigSetter;
}) {
  return (
    <SectionCard title="Agent Configuration">
      <div className="space-y-4">
        <div>
          <label className="block text-sm text-[#8b949e] mb-2">Agent Type</label>
          <div className="flex gap-3">
            <button
              onClick={() => setConfig((currentConfig) => ({ ...currentConfig, agent_type: "claude" }))}
              className={`flex-1 p-3 rounded-lg border text-sm font-medium transition-colors ${
                config.agent_type === "claude"
                  ? "bg-[#58a6ff15] border-[#58a6ff] text-[#58a6ff]"
                  : "bg-[#0d1117] border-[#30363d] text-[#8b949e] hover:border-[#484f58]"
              }`}
            >
              Claude Code
            </button>
            <button
              onClick={() => setConfig((currentConfig) => ({ ...currentConfig, agent_type: "codex" }))}
              className={`flex-1 p-3 rounded-lg border text-sm font-medium transition-colors ${
                config.agent_type === "codex"
                  ? "bg-[#3fb95015] border-[#3fb950] text-[#3fb950]"
                  : "bg-[#0d1117] border-[#30363d] text-[#8b949e] hover:border-[#484f58]"
              }`}
            >
              Codex
            </button>
            <button
              onClick={() => setConfig((currentConfig) => ({ ...currentConfig, agent_type: "custom" }))}
              className={`flex-1 p-3 rounded-lg border text-sm font-medium transition-colors ${
                config.agent_type === "custom"
                  ? "bg-[#d2a8ff15] border-[#d2a8ff] text-[#d2a8ff]"
                  : "bg-[#0d1117] border-[#30363d] text-[#8b949e] hover:border-[#484f58]"
              }`}
            >
              Custom
            </button>
          </div>
          {config.agent_type === "custom" && (
            <div className="mt-3">
              <label className="block text-sm text-[#8b949e] mb-1">Command template</label>
              <input
                type="text"
                value={config.custom_agent_command}
                onChange={(event) =>
                  setConfig((currentConfig) => ({
                    ...currentConfig,
                    custom_agent_command: event.target.value,
                  }))
                }
                placeholder="e.g. aider --yes-always {{prompt}}"
                className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm font-mono outline-none focus:border-[#d2a8ff] placeholder:text-[#484f58]"
              />
              <p className="text-xs text-[#8b949e] mt-1">
                Use <code className="text-[#d2a8ff]">{"{{prompt}}"}</code> where the prompt should be inserted. If omitted, it is appended as the last argument.
              </p>
            </div>
          )}
        </div>

        <div className="flex items-center justify-between">
          <div>
            <label className="text-sm text-[#e6edf3]">Auto-approve permissions</label>
            <p className="text-xs text-[#8b949e] mt-0.5">
              {config.agent_type === "claude"
                ? "Uses --dangerously-skip-permissions"
                : config.agent_type === "codex"
                  ? "Uses --dangerously-bypass-approvals-and-sandbox"
                  : "Not applicable for custom agents"}
            </p>
          </div>
          <Toggle
            enabled={config.auto_approve}
            onToggle={() =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                auto_approve: !currentConfig.auto_approve,
              }))
            }
          />
        </div>

        <div>
          <label className="block text-sm text-[#8b949e] mb-2">Max Turns per Issue</label>
          <input
            type="number"
            min={1}
            max={50}
            value={config.max_turns}
            onChange={(event) =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                max_turns: parseInt(event.target.value, 10) || 1,
              }))
            }
            className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff]"
          />
        </div>
      </div>
    </SectionCard>
  );
}

export function OrchestratorSection({
  config,
  setConfig,
}: {
  config: RunConfig;
  setConfig: ConfigSetter;
}) {
  return (
    <SectionCard title="Orchestrator">
      <div className="space-y-4">
        <div>
          <label className="block text-sm text-[#8b949e] mb-2">Max Concurrent Agents</label>
          <input
            type="number"
            min={1}
            max={20}
            value={config.max_concurrent}
            onChange={(event) =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                max_concurrent: parseInt(event.target.value, 10) || 1,
              }))
            }
            className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff]"
          />
        </div>

        <div>
          <label className="block text-sm text-[#8b949e] mb-2">Per-Stage Concurrency Limits</label>
          <p className="text-xs text-[#8b949e] mb-3">
            Set per-stage limits (0 = use global max). Stages without limits fall back to the global
            setting.
          </p>
          <div className="grid grid-cols-2 gap-3">
            {STAGE_KEYS.map((stage) => (
              <div key={stage}>
                <label className="block text-xs text-[#8b949e] mb-1 capitalize">
                  {stage.replace("_", " ")}
                </label>
                <input
                  type="number"
                  min={0}
                  max={20}
                  value={config.max_concurrent_by_stage[stage] || 0}
                  onChange={(event) => {
                    const value = parseInt(event.target.value, 10) || 0;
                    setConfig((currentConfig) => {
                      const nextStageLimits = { ...currentConfig.max_concurrent_by_stage };
                      if (value === 0) {
                        delete nextStageLimits[stage];
                      } else {
                        nextStageLimits[stage] = value;
                      }
                      return { ...currentConfig, max_concurrent_by_stage: nextStageLimits };
                    });
                  }}
                  className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff]"
                />
              </div>
            ))}
          </div>
        </div>

        <NumberField
          label="Poll Interval (seconds)"
          value={config.poll_interval_secs}
          min={10}
          max={600}
          onChange={(value) =>
            setConfig((currentConfig) => ({ ...currentConfig, poll_interval_secs: value || 60 }))
          }
        />

        <div>
          <NumberField
            label="Max Retries per Stage"
            value={config.max_retries}
            min={0}
            max={5}
            onChange={(value) =>
              setConfig((currentConfig) => ({ ...currentConfig, max_retries: value || 0 }))
            }
          />
          <p className="text-xs text-[#8b949e] mt-1">
            Number of automatic retries when a pipeline stage fails. Set to 0 to disable.
          </p>
        </div>

        <div>
          <NumberField
            label="Retry Base Delay (seconds)"
            value={config.retry_base_delay_secs}
            min={1}
            max={300}
            onChange={(value) =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                retry_base_delay_secs: value || 10,
                retry_backoff_secs: value || 10,
              }))
            }
          />
          <p className="text-xs text-[#8b949e] mt-1">
            Base delay for exponential backoff (delay = base × 2^(attempt−1)).
          </p>
        </div>

        <div>
          <NumberField
            label="Max Retry Backoff (seconds)"
            value={config.retry_max_backoff_secs}
            min={1}
            max={600}
            onChange={(value) =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                retry_max_backoff_secs: value || 300,
              }))
            }
          />
          <p className="text-xs text-[#8b949e] mt-1">
            Maximum delay cap in seconds (default: 300s / 5 minutes).
          </p>
        </div>

        <div>
          <NumberField
            label="Stall Timeout (seconds)"
            value={config.stall_timeout_secs}
            min={0}
            max={3600}
            onChange={(value) =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                stall_timeout_secs: value || 0,
              }))
            }
          />
          <p className="text-xs text-[#8b949e] mt-1">
            Kill agents that produce no output for this duration. Set to 0 to disable. Default: 300s
            (5 minutes).
          </p>
        </div>

        <div>
          <label className="block text-sm text-[#8b949e] mb-2">Filter by Label (optional)</label>
          <input
            type="text"
            placeholder="e.g., symphony, auto-fix"
            value={config.issue_label || ""}
            onChange={(event) =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                issue_label: event.target.value || null,
              }))
            }
            className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] placeholder-[#484f58]"
          />
          <p className="text-xs text-[#8b949e] mt-1">
            Only process issues with this label. Leave empty for all issues.
          </p>
        </div>
      </div>
    </SectionCard>
  );
}

export function PriorityLabelsSection({
  config,
  setConfig,
}: {
  config: RunConfig;
  setConfig: ConfigSetter;
}) {
  return (
    <SectionCard title="Dispatch Priority">
      <label className="block text-sm text-[#8b949e] mb-2">
        Priority Labels (highest priority first)
      </label>
      <input
        type="text"
        placeholder="priority:critical, priority:high, priority:medium, priority:low"
        value={config.priority_labels.join(", ")}
        onChange={(event) =>
          setConfig((currentConfig) => ({
            ...currentConfig,
            priority_labels: event.target.value
              .split(",")
              .map((value) => value.trim())
              .filter((value) => value.length > 0),
          }))
        }
        className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] placeholder-[#484f58]"
      />
      <p className="text-xs text-[#8b949e] mt-1">
        Comma-separated list of labels. Issues with labels listed first are dispatched first. Issues
        without any priority label are dispatched last. Within the same priority, older issues are
        dispatched first.
      </p>
    </SectionCard>
  );
}

export function ApprovalGatesSection({
  config,
  setConfig,
}: {
  config: RunConfig;
  setConfig: ConfigSetter;
}) {
  return (
    <SectionCard
      title="Human-in-the-Loop"
      description="Enable approval gates to pause the pipeline after a stage completes and wait for your explicit approval before advancing. All gates are off by default (fully automatic)."
    >
      <div className="space-y-3">
        {STAGE_KEYS.map((stage) => {
          const isEnabled = config.approval_gates?.[stage] ?? false;
          return (
            <div key={stage} className="flex items-center justify-between">
              <div>
                <label className="text-sm text-[#e6edf3]">{STAGE_LABELS[stage]}</label>
                <p className="text-xs text-[#8b949e] mt-0.5">
                  Pause after {STAGE_LABELS[stage].toLowerCase()} completes
                </p>
              </div>
              <Toggle
                enabled={isEnabled}
                activeColor="#d29922"
                onToggle={() =>
                  setConfig((currentConfig) => {
                    const nextApprovalGates = { ...(currentConfig.approval_gates || {}) };
                    if (isEnabled) {
                      delete nextApprovalGates[stage];
                    } else {
                      nextApprovalGates[stage] = true;
                    }
                    return { ...currentConfig, approval_gates: nextApprovalGates };
                  })
                }
              />
            </div>
          );
        })}
      </div>
    </SectionCard>
  );
}

export function StageSkipLabelsSection({
  config,
  setConfig,
  newSkipLabel,
  setNewSkipLabel,
}: {
  config: RunConfig;
  setConfig: ConfigSetter;
  newSkipLabel: string;
  setNewSkipLabel: Dispatch<SetStateAction<string>>;
}) {
  function addSkipLabel() {
    if (!newSkipLabel.trim()) {
      return;
    }

    setConfig((currentConfig) => {
      const nextSkipLabels = { ...currentConfig.stage_skip_labels };
      if (!nextSkipLabels[newSkipLabel.trim()]) {
        nextSkipLabels[newSkipLabel.trim()] = [];
      }
      return { ...currentConfig, stage_skip_labels: nextSkipLabels };
    });
    setNewSkipLabel("");
  }

  return (
    <SectionCard
      title="Stage Skip Labels"
      description="Map issue labels to pipeline stages that should be skipped. Only Code Review and Testing can be skipped; Implement and Merge are always required."
    >
      <div className="space-y-3">
        {Object.entries(config.stage_skip_labels || {}).map(([label, stages]) => (
          <div
            key={label}
            className="flex items-center gap-3 bg-[#0d1117] border border-[#30363d] rounded-lg px-3 py-2"
          >
            <div className="flex-1 min-w-0">
              <p className="text-sm text-[#e6edf3] font-mono">{label}</p>
              <div className="flex gap-2 mt-1">
                {(["code_review", "testing"] as const).map((stage) => {
                  const isActive = stages.includes(stage);
                  return (
                    <button
                      key={stage}
                      onClick={() =>
                        setConfig((currentConfig) => {
                          const nextSkipLabels = { ...currentConfig.stage_skip_labels };
                          const currentStages = [...(nextSkipLabels[label] || [])];
                          if (isActive) {
                            nextSkipLabels[label] = currentStages.filter((value) => value !== stage);
                            if (nextSkipLabels[label].length === 0) {
                              delete nextSkipLabels[label];
                            }
                          } else {
                            nextSkipLabels[label] = [...currentStages, stage];
                          }
                          return { ...currentConfig, stage_skip_labels: nextSkipLabels };
                        })
                      }
                      className={`text-[10px] px-2 py-0.5 rounded-full border transition-colors ${
                        isActive
                          ? "bg-[#d2992215] border-[#d29922] text-[#d29922]"
                          : "bg-[#0d1117] border-[#30363d] text-[#484f58] hover:border-[#484f58]"
                      }`}
                    >
                      {stage.replace("_", " ")}
                    </button>
                  );
                })}
              </div>
            </div>
            <button
              onClick={() =>
                setConfig((currentConfig) => {
                  const nextSkipLabels = { ...currentConfig.stage_skip_labels };
                  delete nextSkipLabels[label];
                  return { ...currentConfig, stage_skip_labels: nextSkipLabels };
                })
              }
              className="text-xs text-[#f85149] hover:bg-[#f8514915] rounded px-2 py-1 transition-colors shrink-0"
            >
              Remove
            </button>
          </div>
        ))}

        {Object.keys(config.stage_skip_labels || {}).length === 0 && (
          <div className="text-sm text-[#484f58] py-2 text-center">No skip label mappings</div>
        )}

        <div className="flex gap-2 mt-2">
          <input
            type="text"
            placeholder="New label (e.g., skip:testing)"
            value={newSkipLabel}
            onChange={(event) => setNewSkipLabel(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                addSkipLabel();
              }
            }}
            className="flex-1 px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] placeholder-[#484f58] font-mono"
          />
          <button
            onClick={addSkipLabel}
            className="px-3 py-2 bg-[#21262d] text-[#8b949e] border border-[#30363d] rounded-md text-sm hover:bg-[#30363d] transition-colors"
          >
            Add
          </button>
        </div>
      </div>
    </SectionCard>
  );
}

export function WorkspaceCleanupSection({
  config,
  setConfig,
}: {
  config: RunConfig;
  setConfig: ConfigSetter;
}) {
  return (
    <SectionCard title="Workspace Cleanup">
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <label className="text-sm text-[#e6edf3]">Cleanup on failure</label>
            <p className="text-xs text-[#8b949e] mt-0.5">
              Remove workspace when an agent fails (off = keep for debugging)
            </p>
          </div>
          <Toggle
            enabled={config.cleanup_on_failure}
            onToggle={() =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                cleanup_on_failure: !currentConfig.cleanup_on_failure,
              }))
            }
          />
        </div>

        <div className="flex items-center justify-between">
          <div>
            <label className="text-sm text-[#e6edf3]">Cleanup on stop</label>
            <p className="text-xs text-[#8b949e] mt-0.5">
              Remove workspace when an agent is manually stopped
            </p>
          </div>
          <Toggle
            enabled={config.cleanup_on_stop}
            onToggle={() =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                cleanup_on_stop: !currentConfig.cleanup_on_stop,
              }))
            }
          />
        </div>

        <div>
          <NumberField
            label="Workspace TTL (days)"
            value={config.workspace_ttl_days}
            min={1}
            max={365}
            onChange={(value) =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                workspace_ttl_days: value || 7,
              }))
            }
          />
          <p className="text-xs text-[#8b949e] mt-1">
            Workspaces older than this are cleaned up on app startup. Set to 0 to disable.
          </p>
        </div>
      </div>
    </SectionCard>
  );
}

export function LifecycleHooksSection({
  config,
  setConfig,
}: {
  config: RunConfig;
  setConfig: ConfigSetter;
}) {
  return (
    <SectionCard
      title="Lifecycle Hooks"
      description="Shell commands executed at key points in the workspace lifecycle. Commands run in the workspace directory."
    >
      <div className="space-y-4">
        <TextField
          label="after_create"
          placeholder="e.g., npm install && npm run build"
          value={config.hooks.after_create || ""}
          font="font-mono"
          onChange={(value) =>
            setConfig((currentConfig) => ({
              ...currentConfig,
              hooks: { ...currentConfig.hooks, after_create: value || null },
            }))
          }
          help="Runs after a new workspace is cloned. Failure aborts the operation."
        />

        <TextField
          label="before_run"
          placeholder="e.g., git pull origin main"
          value={config.hooks.before_run || ""}
          font="font-mono"
          onChange={(value) =>
            setConfig((currentConfig) => ({
              ...currentConfig,
              hooks: { ...currentConfig.hooks, before_run: value || null },
            }))
          }
          help="Runs before each agent attempt. Failure aborts the run."
        />

        <TextField
          label="after_run"
          placeholder="e.g., cp -r ./coverage /tmp/artifacts/"
          value={config.hooks.after_run || ""}
          font="font-mono"
          onChange={(value) =>
            setConfig((currentConfig) => ({
              ...currentConfig,
              hooks: { ...currentConfig.hooks, after_run: value || null },
            }))
          }
          help="Runs after each agent attempt (success or failure). Failure is logged but ignored."
        />

        <TextField
          label="before_remove"
          placeholder="e.g., tar czf /tmp/workspace-backup.tar.gz ."
          value={config.hooks.before_remove || ""}
          font="font-mono"
          onChange={(value) =>
            setConfig((currentConfig) => ({
              ...currentConfig,
              hooks: { ...currentConfig.hooks, before_remove: value || null },
            }))
          }
          help="Runs before workspace deletion. Failure is logged but ignored."
        />

        <div>
          <NumberField
            label="Hook Timeout (seconds)"
            value={config.hooks.timeout_secs}
            min={5}
            max={600}
            onChange={(value) =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                hooks: { ...currentConfig.hooks, timeout_secs: value || 60 },
              }))
            }
          />
          <p className="text-xs text-[#8b949e] mt-1">
            Maximum time each hook is allowed to run before being killed.
          </p>
        </div>
      </div>
    </SectionCard>
  );
}

export function NotificationsSection({
  config,
  setConfig,
}: {
  config: RunConfig;
  setConfig: ConfigSetter;
}) {
  return (
    <SectionCard title="Notifications">
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <label className="text-sm text-[#e6edf3]">Enable Notifications</label>
            <p className="text-xs text-[#8b949e] mt-0.5">
              Get notified when pipelines complete or fail
            </p>
          </div>
          <Toggle
            enabled={config.notifications_enabled}
            onToggle={() =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                notifications_enabled: !currentConfig.notifications_enabled,
              }))
            }
          />
        </div>

        <div className="flex items-center justify-between">
          <div>
            <label className="text-sm text-[#e6edf3]">Notification Sound</label>
            <p className="text-xs text-[#8b949e] mt-0.5">Play a sound with notifications</p>
          </div>
          <Toggle
            enabled={config.notification_sound && config.notifications_enabled}
            disabled={!config.notifications_enabled}
            onToggle={() =>
              setConfig((currentConfig) => ({
                ...currentConfig,
                notification_sound: !currentConfig.notification_sound,
              }))
            }
          />
        </div>
      </div>
    </SectionCard>
  );
}

export function PromptTemplatesSection({
  config,
  setConfig,
  defaultPrompts,
  expandedStage,
  setExpandedStage,
}: {
  config: RunConfig;
  setConfig: ConfigSetter;
  defaultPrompts: Record<string, string>;
  expandedStage: string | null;
  setExpandedStage: Dispatch<SetStateAction<string | null>>;
}) {
  return (
    <SectionCard
      title="Prompt Templates"
      description={
        <>
          Customize the prompt sent to agents at each pipeline stage. Use template variables:{" "}
          <code className="text-[#7ee787]">{"{{issue_number}}"}</code>,{" "}
          <code className="text-[#7ee787]">{"{{issue_title}}"}</code>,{" "}
          <code className="text-[#7ee787]">{"{{issue_body}}"}</code>,{" "}
          <code className="text-[#7ee787]">{"{{repo}}"}</code>,{" "}
          <code className="text-[#7ee787]">{"{{attempt}}"}</code>,{" "}
          <code className="text-[#7ee787]">{"{{previous_error}}"}</code>
        </>
      }
    >
      <div className="space-y-2">
        {STAGE_KEYS.map((stage) => {
          const isExpanded = expandedStage === stage;
          const hasCustomPrompt = !!(config.stage_prompts[stage] && config.stage_prompts[stage].trim());

          return (
            <div key={stage} className="border border-[#30363d] rounded-lg overflow-hidden">
              <button
                onClick={() => setExpandedStage(isExpanded ? null : stage)}
                className="w-full flex items-center justify-between px-4 py-3 bg-[#0d1117] hover:bg-[#161b22] transition-colors text-left"
              >
                <div className="flex items-center gap-2">
                  <span className="text-sm text-[#e6edf3]">{STAGE_LABELS[stage]}</span>
                  {hasCustomPrompt && (
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
                    onChange={(event) =>
                      setConfig((currentConfig) => ({
                        ...currentConfig,
                        stage_prompts: {
                          ...currentConfig.stage_prompts,
                          [stage]: event.target.value,
                        },
                      }))
                    }
                    rows={12}
                    className="w-full px-3 py-2 bg-[#161b22] border border-[#30363d] rounded-md text-[#e6edf3] text-xs font-mono outline-none focus:border-[#58a6ff] resize-y leading-relaxed"
                    placeholder={defaultPrompts[stage] ?? ""}
                  />
                  <div className="flex justify-end mt-2">
                    <button
                      onClick={() =>
                        setConfig((currentConfig) => {
                          const nextStagePrompts = { ...currentConfig.stage_prompts };
                          delete nextStagePrompts[stage];
                          return { ...currentConfig, stage_prompts: nextStagePrompts };
                        })
                      }
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
    </SectionCard>
  );
}

export function SettingsSaveBar({
  saved,
  error,
  onSave,
}: {
  saved: boolean;
  error: string | null;
  onSave: () => void;
}) {
  return (
    <div className="flex items-center gap-3">
      <button
        onClick={onSave}
        className="px-6 py-2 bg-[#58a6ff] text-white rounded-lg text-sm font-medium hover:bg-[#79b8ff] transition-colors"
      >
        Save Settings
      </button>
      {saved && <span className="text-sm text-[#3fb950]">Saved!</span>}
      {error && <span className="text-sm text-[#f85149]">{error}</span>}
    </div>
  );
}

function NumberField({
  label,
  value,
  min,
  max,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
}) {
  return (
    <div>
      <label className="block text-sm text-[#8b949e] mb-2">{label}</label>
      <input
        type="number"
        min={min}
        max={max}
        value={value}
        onChange={(event) => onChange(parseInt(event.target.value, 10) || 0)}
        className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff]"
      />
    </div>
  );
}

function TextField({
  label,
  value,
  placeholder,
  font,
  onChange,
  help,
}: {
  label: string;
  value: string;
  placeholder: string;
  font?: string;
  onChange: (value: string) => void;
  help?: string;
}) {
  return (
    <div>
      <label className="block text-sm text-[#8b949e] mb-1">{label}</label>
      <input
        type="text"
        placeholder={placeholder}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className={`w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-md text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] placeholder-[#484f58] ${
          font || ""
        }`}
      />
      {help && <p className="text-xs text-[#8b949e] mt-1">{help}</p>}
    </div>
  );
}
