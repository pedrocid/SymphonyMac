import { SettingsView } from "../features/settings/SettingsView";
import { useSettingsController } from "../features/settings/useSettingsController";

export function Settings() {
  const controller = useSettingsController();

  return (
    <SettingsView
      config={controller.config}
      setConfig={controller.setConfig}
      saved={controller.saved}
      error={controller.error}
      workspaces={controller.workspaces}
      wsLoading={controller.wsLoading}
      wsMessage={controller.wsMessage}
      defaultPrompts={controller.defaultPrompts}
      expandedStage={controller.expandedStage}
      setExpandedStage={controller.setExpandedStage}
      newSkipLabel={controller.newSkipLabel}
      setNewSkipLabel={controller.setNewSkipLabel}
      saveConfig={controller.saveConfig}
      refreshWorkspaces={controller.refreshWorkspaces}
      cleanupSingle={controller.cleanupSingle}
      cleanupAll={controller.cleanupAll}
      cleanupOld={controller.cleanupOld}
      totalWorkspaceSizeDisplay={controller.totalWorkspaceSizeDisplay}
    />
  );
}
