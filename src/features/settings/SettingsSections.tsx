import type { Dispatch, ReactNode, SetStateAction } from "react";
import type { RunConfig } from "../../lib/types";
import { STAGE_KEYS, STAGE_LABELS } from "./constants";

type ConfigSetter = Dispatch<SetStateAction<RunConfig>>;

/* ─── Shared primitives ───────────────────────────────────────────── */

function SectionCard({
  title,
  description,
  icon,
  children,
}: {
  title: string;
  description?: ReactNode;
  icon?: string;
  children: ReactNode;
}) {
  return (
    <div className="bg-[#161b22] border border-[#30363d] rounded-xl overflow-hidden">
      <div className="px-5 pt-5 pb-0">
        <div className="flex items-start gap-3 mb-1">
          {icon && (
            <div className="w-8 h-8 rounded-lg bg-[#21262d] flex items-center justify-center shrink-0 mt-0.5">
              <svg className="w-4 h-4 text-[#8b949e]" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round">
                <path d={icon} />
              </svg>
            </div>
          )}
          <div>
            <h3 className="text-sm font-semibold text-[#e6edf3]">{title}</h3>
            {description && <div className="text-xs text-[#8b949e] mt-1 leading-relaxed">{description}</div>}
          </div>
        </div>
      </div>
      <div className="px-5 pb-5 pt-4">{children}</div>
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
      className={`w-11 h-6 rounded-full transition-all duration-200 relative shrink-0 ${
        disabled ? "opacity-40 cursor-not-allowed" : "cursor-pointer"
      }`}
      style={{ backgroundColor: enabled ? activeColor : "#30363d" }}
    >
      <span
        className={`absolute top-0.5 w-5 h-5 bg-white rounded-full shadow-sm transition-all duration-200 ${
          enabled ? "left-[22px]" : "left-0.5"
        }`}
      />
    </button>
  );
}

function ToggleRow({
  label,
  description,
  enabled,
  onToggle,
  disabled,
  activeColor,
}: {
  label: string;
  description: string;
  enabled: boolean;
  onToggle: () => void;
  disabled?: boolean;
  activeColor?: string;
}) {
  return (
    <div className="flex items-center justify-between gap-4 py-3 border-b border-[#21262d] last:border-0">
      <div className="min-w-0">
        <label className="text-sm text-[#e6edf3]">{label}</label>
        <p className="text-xs text-[#8b949e] mt-0.5">{description}</p>
      </div>
      <Toggle enabled={enabled} onToggle={onToggle} disabled={disabled} activeColor={activeColor} />
    </div>
  );
}

function NumberField({
  label,
  value,
  min,
  max,
  onChange,
  help,
  suffix,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
  help?: string;
  suffix?: string;
}) {
  return (
    <div>
      <label className="block text-sm text-[#8b949e] mb-1.5">{label}</label>
      <div className="relative">
        <input
          type="number"
          min={min}
          max={max}
          value={value}
          onChange={(event) => onChange(parseInt(event.target.value, 10) || 0)}
          className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-lg text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] focus:ring-1 focus:ring-[#58a6ff33] transition-colors"
        />
        {suffix && (
          <span className="absolute right-3 top-1/2 -translate-y-1/2 text-xs text-[#484f58]">{suffix}</span>
        )}
      </div>
      {help && <p className="text-xs text-[#484f58] mt-1.5">{help}</p>}
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
      <label className="block text-sm text-[#8b949e] mb-1.5">{label}</label>
      <input
        type="text"
        placeholder={placeholder}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className={`w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-lg text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] focus:ring-1 focus:ring-[#58a6ff33] placeholder-[#484f58] transition-colors ${
          font || ""
        }`}
      />
      {help && <p className="text-xs text-[#484f58] mt-1.5">{help}</p>}
    </div>
  );
}

/* ─── Sections ────────────────────────────────────────────────────── */

export function AgentConfigurationSection({
  config,
  setConfig,
}: {
  config: RunConfig;
  setConfig: ConfigSetter;
}) {
  return (
    <SectionCard
      title="Agent Configuration"
      icon="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"
    >
      <div className="space-y-5">
        <div>
          <label className="block text-sm text-[#8b949e] mb-2">Agent Type</label>
          <div className="flex gap-3">
            <button
              onClick={() => setConfig((c) => ({ ...c, agent_type: "claude" }))}
              className={`flex-1 p-3 rounded-lg border text-sm font-medium transition-all ${
                config.agent_type === "claude"
                  ? "bg-[#58a6ff10] border-[#58a6ff] text-[#58a6ff] shadow-[0_0_0_1px_#58a6ff33]"
                  : "bg-[#0d1117] border-[#30363d] text-[#8b949e] hover:border-[#484f58] hover:text-[#e6edf3]"
              }`}
            >
              Claude Code
            </button>
            <button
              onClick={() => setConfig((c) => ({ ...c, agent_type: "codex" }))}
              className={`flex-1 p-3 rounded-lg border text-sm font-medium transition-all ${
                config.agent_type === "codex"
                  ? "bg-[#3fb95010] border-[#3fb950] text-[#3fb950] shadow-[0_0_0_1px_#3fb95033]"
                  : "bg-[#0d1117] border-[#30363d] text-[#8b949e] hover:border-[#484f58] hover:text-[#e6edf3]"
              }`}
            >
              Codex
            </button>
          </div>
        </div>

        <ToggleRow
          label="Auto-approve permissions"
          description={
            config.agent_type === "claude"
              ? "Uses --dangerously-skip-permissions"
              : "Uses --dangerously-bypass-approvals-and-sandbox"
          }
          enabled={config.auto_approve}
          onToggle={() => setConfig((c) => ({ ...c, auto_approve: !c.auto_approve }))}
        />

        <NumberField
          label="Max Turns per Issue"
          value={config.max_turns}
          min={1}
          max={50}
          onChange={(v) => setConfig((c) => ({ ...c, max_turns: v || 1 }))}
          help="Maximum number of agent turns for each issue"
        />
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
    <>
      <SectionCard
        title="Concurrency"
        icon="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
      >
        <div className="space-y-5">
          <NumberField
            label="Max Concurrent Agents"
            value={config.max_concurrent}
            min={1}
            max={20}
            onChange={(v) => setConfig((c) => ({ ...c, max_concurrent: v || 1 }))}
            help="Global limit across all pipeline stages"
          />

          <div>
            <label className="block text-sm text-[#8b949e] mb-2">Per-Stage Limits</label>
            <p className="text-xs text-[#484f58] mb-3">
              Override per stage (0 = use global max)
            </p>
            <div className="grid grid-cols-2 gap-3">
              {STAGE_KEYS.map((stage) => (
                <div key={stage} className="relative">
                  <label className="block text-xs text-[#8b949e] mb-1">{STAGE_LABELS[stage]}</label>
                  <input
                    type="number"
                    min={0}
                    max={20}
                    value={config.max_concurrent_by_stage[stage] || 0}
                    onChange={(event) => {
                      const value = parseInt(event.target.value, 10) || 0;
                      setConfig((c) => {
                        const next = { ...c.max_concurrent_by_stage };
                        if (value === 0) delete next[stage];
                        else next[stage] = value;
                        return { ...c, max_concurrent_by_stage: next };
                      });
                    }}
                    className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-lg text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] focus:ring-1 focus:ring-[#58a6ff33] transition-colors"
                  />
                </div>
              ))}
            </div>
          </div>

          <NumberField
            label="Poll Interval"
            value={config.poll_interval_secs}
            min={10}
            max={600}
            suffix="sec"
            onChange={(v) => setConfig((c) => ({ ...c, poll_interval_secs: v || 60 }))}
          />

          <TextField
            label="Filter by Label"
            value={config.issue_label || ""}
            placeholder="e.g., symphony, auto-fix"
            onChange={(v) => setConfig((c) => ({ ...c, issue_label: v || null }))}
            help="Only process issues with this label. Leave empty for all."
          />
        </div>
      </SectionCard>

      <SectionCard
        title="Retries & Timeouts"
        icon="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"
      >
        <div className="space-y-5">
          <div className="grid grid-cols-2 gap-4">
            <NumberField
              label="Max Retries"
              value={config.max_retries}
              min={0}
              max={5}
              onChange={(v) => setConfig((c) => ({ ...c, max_retries: v || 0 }))}
              help="Per stage (0 = disabled)"
            />
            <NumberField
              label="Base Delay"
              value={config.retry_base_delay_secs}
              min={1}
              max={300}
              suffix="sec"
              onChange={(v) =>
                setConfig((c) => ({
                  ...c,
                  retry_base_delay_secs: v || 10,
                  retry_backoff_secs: v || 10,
                }))
              }
              help="Exponential backoff base"
            />
          </div>

          <div className="grid grid-cols-2 gap-4">
            <NumberField
              label="Max Backoff"
              value={config.retry_max_backoff_secs}
              min={1}
              max={600}
              suffix="sec"
              onChange={(v) => setConfig((c) => ({ ...c, retry_max_backoff_secs: v || 300 }))}
              help="Delay cap (default: 300s)"
            />
            <NumberField
              label="Stall Timeout"
              value={config.stall_timeout_secs}
              min={0}
              max={3600}
              suffix="sec"
              onChange={(v) => setConfig((c) => ({ ...c, stall_timeout_secs: v || 0 }))}
              help="Kill idle agents (0 = off)"
            />
          </div>
        </div>
      </SectionCard>
    </>
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
    <SectionCard
      title="Dispatch Priority"
      icon="M3 4h13M3 8h9m-9 4h6m4 0l4-4m0 0l4 4m-4-4v12"
    >
      <div>
        <label className="block text-sm text-[#8b949e] mb-1.5">
          Priority Labels (highest first)
        </label>
        <input
          type="text"
          placeholder="priority:critical, priority:high, priority:medium, priority:low"
          value={config.priority_labels.join(", ")}
          onChange={(event) =>
            setConfig((c) => ({
              ...c,
              priority_labels: event.target.value
                .split(",")
                .map((v) => v.trim())
                .filter((v) => v.length > 0),
            }))
          }
          className="w-full px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-lg text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] focus:ring-1 focus:ring-[#58a6ff33] placeholder-[#484f58] transition-colors"
        />
        <p className="text-xs text-[#484f58] mt-1.5">
          Comma-separated. Issues with earlier labels are dispatched first. Unlabeled issues go last.
        </p>
      </div>
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
      description="Pause the pipeline after a stage for manual approval before continuing."
      icon="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"
    >
      <div>
        {STAGE_KEYS.map((stage) => {
          const isEnabled = config.approval_gates?.[stage] ?? false;
          return (
            <ToggleRow
              key={stage}
              label={STAGE_LABELS[stage]}
              description={`Pause after ${STAGE_LABELS[stage].toLowerCase()} completes`}
              enabled={isEnabled}
              activeColor="#d29922"
              onToggle={() =>
                setConfig((c) => {
                  const next = { ...(c.approval_gates || {}) };
                  if (isEnabled) delete next[stage];
                  else next[stage] = true;
                  return { ...c, approval_gates: next };
                })
              }
            />
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
    if (!newSkipLabel.trim()) return;
    setConfig((c) => {
      const next = { ...c.stage_skip_labels };
      if (!next[newSkipLabel.trim()]) next[newSkipLabel.trim()] = [];
      return { ...c, stage_skip_labels: next };
    });
    setNewSkipLabel("");
  }

  return (
    <SectionCard
      title="Stage Skip Labels"
      description="Map issue labels to stages that should be skipped. Only Code Review and Testing can be skipped."
      icon="M13 7l5 5m0 0l-5 5m5-5H6"
    >
      <div className="space-y-3">
        {Object.entries(config.stage_skip_labels || {}).map(([label, stages]) => (
          <div
            key={label}
            className="flex items-center gap-3 bg-[#0d1117] border border-[#21262d] rounded-lg px-3 py-2.5"
          >
            <div className="flex-1 min-w-0">
              <p className="text-sm text-[#e6edf3] font-mono">{label}</p>
              <div className="flex gap-2 mt-1.5">
                {(["code_review", "testing"] as const).map((stage) => {
                  const isActive = stages.includes(stage);
                  return (
                    <button
                      key={stage}
                      onClick={() =>
                        setConfig((c) => {
                          const next = { ...c.stage_skip_labels };
                          const current = [...(next[label] || [])];
                          if (isActive) {
                            next[label] = current.filter((v) => v !== stage);
                            if (next[label].length === 0) delete next[label];
                          } else {
                            next[label] = [...current, stage];
                          }
                          return { ...c, stage_skip_labels: next };
                        })
                      }
                      className={`text-[11px] px-2.5 py-0.5 rounded-full border transition-all ${
                        isActive
                          ? "bg-[#d2992215] border-[#d29922] text-[#d29922]"
                          : "bg-transparent border-[#30363d] text-[#484f58] hover:border-[#484f58] hover:text-[#8b949e]"
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
                setConfig((c) => {
                  const next = { ...c.stage_skip_labels };
                  delete next[label];
                  return { ...c, stage_skip_labels: next };
                })
              }
              className="text-xs text-[#f85149] hover:bg-[#f8514910] rounded-md px-2 py-1 transition-colors shrink-0"
            >
              Remove
            </button>
          </div>
        ))}

        {Object.keys(config.stage_skip_labels || {}).length === 0 && (
          <div className="text-sm text-[#484f58] py-3 text-center rounded-lg bg-[#0d1117] border border-dashed border-[#21262d]">
            No skip label mappings configured
          </div>
        )}

        <div className="flex gap-2 pt-1">
          <input
            type="text"
            placeholder="New label (e.g., skip:testing)"
            value={newSkipLabel}
            onChange={(event) => setNewSkipLabel(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") addSkipLabel();
            }}
            className="flex-1 px-3 py-2 bg-[#0d1117] border border-[#30363d] rounded-lg text-[#e6edf3] text-sm outline-none focus:border-[#58a6ff] focus:ring-1 focus:ring-[#58a6ff33] placeholder-[#484f58] font-mono transition-colors"
          />
          <button
            onClick={addSkipLabel}
            className="px-4 py-2 bg-[#21262d] text-[#8b949e] border border-[#30363d] rounded-lg text-sm hover:bg-[#30363d] hover:text-[#e6edf3] transition-colors"
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
    <SectionCard
      title="Cleanup Policy"
      icon="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
    >
      <div className="space-y-1">
        <ToggleRow
          label="Cleanup on failure"
          description="Remove workspace when an agent fails (off = keep for debugging)"
          enabled={config.cleanup_on_failure}
          onToggle={() => setConfig((c) => ({ ...c, cleanup_on_failure: !c.cleanup_on_failure }))}
        />
        <ToggleRow
          label="Cleanup on stop"
          description="Remove workspace when an agent is manually stopped"
          enabled={config.cleanup_on_stop}
          onToggle={() => setConfig((c) => ({ ...c, cleanup_on_stop: !c.cleanup_on_stop }))}
        />
      </div>
      <div className="mt-4">
        <NumberField
          label="Workspace TTL"
          value={config.workspace_ttl_days}
          min={1}
          max={365}
          suffix="days"
          onChange={(v) => setConfig((c) => ({ ...c, workspace_ttl_days: v || 7 }))}
          help="Workspaces older than this are cleaned up on startup (0 = disabled)"
        />
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
      icon="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"
    >
      <div className="space-y-4">
        <TextField
          label="after_create"
          placeholder="e.g., npm install && npm run build"
          value={config.hooks.after_create || ""}
          font="font-mono"
          onChange={(v) =>
            setConfig((c) => ({
              ...c,
              hooks: { ...c.hooks, after_create: v || null },
            }))
          }
          help="Runs after a new workspace is cloned. Failure aborts the operation."
        />

        <TextField
          label="before_run"
          placeholder="e.g., git pull origin main"
          value={config.hooks.before_run || ""}
          font="font-mono"
          onChange={(v) =>
            setConfig((c) => ({
              ...c,
              hooks: { ...c.hooks, before_run: v || null },
            }))
          }
          help="Runs before each agent attempt. Failure aborts the run."
        />

        <TextField
          label="after_run"
          placeholder="e.g., cp -r ./coverage /tmp/artifacts/"
          value={config.hooks.after_run || ""}
          font="font-mono"
          onChange={(v) =>
            setConfig((c) => ({
              ...c,
              hooks: { ...c.hooks, after_run: v || null },
            }))
          }
          help="Runs after each agent attempt. Failure is logged but ignored."
        />

        <TextField
          label="before_remove"
          placeholder="e.g., tar czf /tmp/workspace-backup.tar.gz ."
          value={config.hooks.before_remove || ""}
          font="font-mono"
          onChange={(v) =>
            setConfig((c) => ({
              ...c,
              hooks: { ...c.hooks, before_remove: v || null },
            }))
          }
          help="Runs before workspace deletion. Failure is logged but ignored."
        />

        <NumberField
          label="Hook Timeout"
          value={config.hooks.timeout_secs}
          min={5}
          max={600}
          suffix="sec"
          onChange={(v) =>
            setConfig((c) => ({
              ...c,
              hooks: { ...c.hooks, timeout_secs: v || 60 },
            }))
          }
          help="Maximum time each hook is allowed to run before being killed"
        />
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
    <SectionCard
      title="Notifications"
      icon="M15 17h5l-1.405-1.405A2.032 2.032 0 0118 14.158V11a6.002 6.002 0 00-4-5.659V5a2 2 0 10-4 0v.341C7.67 6.165 6 8.388 6 11v3.159c0 .538-.214 1.055-.595 1.436L4 17h5m6 0v1a3 3 0 11-6 0v-1m6 0H9"
    >
      <div>
        <ToggleRow
          label="Enable Notifications"
          description="Get notified when pipelines complete or fail"
          enabled={config.notifications_enabled}
          onToggle={() => setConfig((c) => ({ ...c, notifications_enabled: !c.notifications_enabled }))}
        />
        <ToggleRow
          label="Notification Sound"
          description="Play a sound with notifications"
          enabled={config.notification_sound && config.notifications_enabled}
          disabled={!config.notifications_enabled}
          onToggle={() => setConfig((c) => ({ ...c, notification_sound: !c.notification_sound }))}
        />
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
          Customize the prompt sent to agents at each stage. Variables:{" "}
          <code className="text-[#7ee787] bg-[#0d1117] px-1 py-0.5 rounded text-[10px]">{"{{issue_number}}"}</code>{" "}
          <code className="text-[#7ee787] bg-[#0d1117] px-1 py-0.5 rounded text-[10px]">{"{{issue_title}}"}</code>{" "}
          <code className="text-[#7ee787] bg-[#0d1117] px-1 py-0.5 rounded text-[10px]">{"{{issue_body}}"}</code>{" "}
          <code className="text-[#7ee787] bg-[#0d1117] px-1 py-0.5 rounded text-[10px]">{"{{repo}}"}</code>{" "}
          <code className="text-[#7ee787] bg-[#0d1117] px-1 py-0.5 rounded text-[10px]">{"{{attempt}}"}</code>{" "}
          <code className="text-[#7ee787] bg-[#0d1117] px-1 py-0.5 rounded text-[10px]">{"{{previous_error}}"}</code>
        </>
      }
      icon="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
    >
      <div className="space-y-2">
        {STAGE_KEYS.map((stage) => {
          const isExpanded = expandedStage === stage;
          const hasCustomPrompt = !!(config.stage_prompts[stage] && config.stage_prompts[stage].trim());

          return (
            <div key={stage} className="border border-[#21262d] rounded-lg overflow-hidden">
              <button
                onClick={() => setExpandedStage(isExpanded ? null : stage)}
                className="w-full flex items-center justify-between px-4 py-3 bg-[#0d1117] hover:bg-[#161b22] transition-colors text-left"
              >
                <div className="flex items-center gap-2">
                  <span className="text-sm text-[#e6edf3]">{STAGE_LABELS[stage]}</span>
                  {hasCustomPrompt && (
                    <span className="px-1.5 py-0.5 text-[10px] bg-[#58a6ff15] text-[#58a6ff] rounded-full font-medium">
                      Custom
                    </span>
                  )}
                </div>
                <svg
                  className={`w-4 h-4 text-[#484f58] transition-transform duration-200 ${isExpanded ? "rotate-180" : ""}`}
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                  strokeWidth={2}
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <path d="M19 9l-7 7-7-7" />
                </svg>
              </button>
              {isExpanded && (
                <div className="p-4 bg-[#0d1117] border-t border-[#21262d]">
                  <textarea
                    value={config.stage_prompts[stage] ?? defaultPrompts[stage] ?? ""}
                    onChange={(event) =>
                      setConfig((c) => ({
                        ...c,
                        stage_prompts: { ...c.stage_prompts, [stage]: event.target.value },
                      }))
                    }
                    rows={12}
                    className="w-full px-3 py-2 bg-[#161b22] border border-[#30363d] rounded-lg text-[#e6edf3] text-xs font-mono outline-none focus:border-[#58a6ff] focus:ring-1 focus:ring-[#58a6ff33] resize-y leading-relaxed transition-colors"
                    placeholder={defaultPrompts[stage] ?? ""}
                  />
                  <div className="flex justify-end mt-2">
                    <button
                      onClick={() =>
                        setConfig((c) => {
                          const next = { ...c.stage_prompts };
                          delete next[stage];
                          return { ...c, stage_prompts: next };
                        })
                      }
                      className="px-3 py-1.5 text-xs text-[#d29922] bg-[#21262d] border border-[#30363d] rounded-lg hover:bg-[#30363d] transition-colors"
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
        className="px-6 py-2 bg-[#238636] text-white rounded-lg text-sm font-medium hover:bg-[#2ea043] transition-colors shadow-sm"
      >
        Save Settings
      </button>
      {saved && (
        <span className="text-sm text-[#3fb950] flex items-center gap-1.5">
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
            <path d="M5 13l4 4L19 7" />
          </svg>
          Saved
        </span>
      )}
      {error && (
        <span className="text-sm text-[#f85149] flex items-center gap-1.5">
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="10" />
            <line x1="15" y1="9" x2="9" y2="15" />
            <line x1="9" y1="9" x2="15" y2="15" />
          </svg>
          {error}
        </span>
      )}
    </div>
  );
}
