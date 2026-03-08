import type { Dispatch, SetStateAction } from "react";
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
  return (
    <div className="flex-1 overflow-auto p-6">
      <div className="max-w-2xl mx-auto">
        <h2 className="text-2xl font-bold text-[#e6edf3] mb-6">Settings</h2>

        <div className="space-y-6">
          <AgentConfigurationSection config={config} setConfig={setConfig} />
          <OrchestratorSection config={config} setConfig={setConfig} />
          <PriorityLabelsSection config={config} setConfig={setConfig} />
          <ApprovalGatesSection config={config} setConfig={setConfig} />
          <StageSkipLabelsSection
            config={config}
            setConfig={setConfig}
            newSkipLabel={newSkipLabel}
            setNewSkipLabel={setNewSkipLabel}
          />
          <WorkspaceCleanupSection config={config} setConfig={setConfig} />
          <LifecycleHooksSection config={config} setConfig={setConfig} />
          <NotificationsSection config={config} setConfig={setConfig} />
          <PromptTemplatesSection
            config={config}
            setConfig={setConfig}
            defaultPrompts={defaultPrompts}
            expandedStage={expandedStage}
            setExpandedStage={setExpandedStage}
          />
          <SettingsSaveBar saved={saved} error={error} onSave={saveConfig} />
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
        </div>
      </div>
    </div>
  );
}
