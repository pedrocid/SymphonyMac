import { useState, type Dispatch, type SetStateAction } from "react";
import type { RunConfig, WorkspaceInfo } from "../../lib/types";
import {
  AgentConfigurationSection,
  ApprovalGatesSection,
  LifecycleHooksSection,
  NotificationsSection,
  OrchestratorSection,
  PriorityLabelsSection,
  PromptTemplatesSection,
  SettingsSaveBar,
  StageSkipLabelsSection,
  WorkspaceCleanupSection,
} from "./SettingsSections";
import { WorkspacesSection } from "./WorkspacesSection";

type SettingsTab = "general" | "pipeline" | "prompts" | "workspaces" | "advanced";

const TABS: { key: SettingsTab; label: string; icon: string }[] = [
  { key: "general", label: "General", icon: "M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z" },
  { key: "pipeline", label: "Pipeline", icon: "M13 10V3L4 14h7v7l9-11h-7z" },
  { key: "prompts", label: "Prompts", icon: "M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" },
  { key: "workspaces", label: "Workspaces", icon: "M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" },
  { key: "advanced", label: "Advanced", icon: "M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 110-4m0 4v2m0-6V4" },
];

interface SettingsViewProps {
  config: RunConfig;
  setConfig: Dispatch<SetStateAction<RunConfig>>;
  saved: boolean;
  error: string | null;
  workspaces: WorkspaceInfo[];
  wsLoading: boolean;
  wsMessage: string | null;
  defaultPrompts: Record<string, string>;
  expandedStage: string | null;
  setExpandedStage: Dispatch<SetStateAction<string | null>>;
  newSkipLabel: string;
  setNewSkipLabel: Dispatch<SetStateAction<string>>;
  saveConfig: () => void;
  refreshWorkspaces: () => void;
  cleanupSingle: (path: string) => void;
  cleanupAll: () => void;
  cleanupOld: () => void;
  totalWorkspaceSizeDisplay: string;
}

export function SettingsView({
  config,
  setConfig,
  saved,
  error,
  workspaces,
  wsLoading,
  wsMessage,
  defaultPrompts,
  expandedStage,
  setExpandedStage,
  newSkipLabel,
  setNewSkipLabel,
  saveConfig,
  refreshWorkspaces,
  cleanupSingle,
  cleanupAll,
  cleanupOld,
  totalWorkspaceSizeDisplay,
}: SettingsViewProps) {
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {/* Header with tabs */}
      <div className="border-b border-[#30363d] bg-[#161b22]">
        <div className="max-w-3xl mx-auto px-6">
          <h2 className="text-xl font-semibold text-[#e6edf3] pt-5 pb-4">Settings</h2>
          <div className="flex gap-1">
            {TABS.map((tab) => (
              <button
                key={tab.key}
                onClick={() => setActiveTab(tab.key)}
                className={`flex items-center gap-2 px-4 py-2.5 text-sm font-medium rounded-t-lg transition-colors border-b-2 -mb-[1px] ${
                  activeTab === tab.key
                    ? "text-[#e6edf3] border-[#58a6ff] bg-[#0d1117]"
                    : "text-[#8b949e] border-transparent hover:text-[#e6edf3] hover:border-[#30363d]"
                }`}
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round">
                  <path d={tab.icon} />
                </svg>
                {tab.label}
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* Content area */}
      <div className="flex-1 overflow-auto">
        <div className="max-w-3xl mx-auto px-6 py-6">
          <div className="space-y-5">
            {activeTab === "general" && (
              <>
                <AgentConfigurationSection config={config} setConfig={setConfig} />
                <NotificationsSection config={config} setConfig={setConfig} />
              </>
            )}

            {activeTab === "pipeline" && (
              <>
                <OrchestratorSection config={config} setConfig={setConfig} />
                <PriorityLabelsSection config={config} setConfig={setConfig} />
                <ApprovalGatesSection config={config} setConfig={setConfig} />
                <StageSkipLabelsSection
                  config={config}
                  setConfig={setConfig}
                  newSkipLabel={newSkipLabel}
                  setNewSkipLabel={setNewSkipLabel}
                />
              </>
            )}

            {activeTab === "prompts" && (
              <PromptTemplatesSection
                config={config}
                setConfig={setConfig}
                defaultPrompts={defaultPrompts}
                expandedStage={expandedStage}
                setExpandedStage={setExpandedStage}
              />
            )}

            {activeTab === "workspaces" && (
              <>
                <WorkspaceCleanupSection config={config} setConfig={setConfig} />
                <WorkspacesSection
                  workspaces={workspaces}
                  wsLoading={wsLoading}
                  wsMessage={wsMessage}
                  totalSizeDisplay={totalWorkspaceSizeDisplay}
                  workspaceTtlDays={config.workspace_ttl_days}
                  onRefresh={refreshWorkspaces}
                  onCleanupOld={cleanupOld}
                  onCleanupAll={cleanupAll}
                  onCleanupSingle={cleanupSingle}
                />
              </>
            )}

            {activeTab === "advanced" && (
              <LifecycleHooksSection config={config} setConfig={setConfig} />
            )}
          </div>
        </div>
      </div>

      {/* Sticky save bar */}
      <div className="border-t border-[#30363d] bg-[#161b22] px-6 py-3">
        <div className="max-w-3xl mx-auto">
          <SettingsSaveBar saved={saved} error={error} onSave={saveConfig} />
        </div>
      </div>
    </div>
  );
}
