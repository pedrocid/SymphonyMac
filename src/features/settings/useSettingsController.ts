import { useEffect, useState } from "react";
import {
  cleanupAllWorkspaces,
  cleanupOldWorkspaces,
  cleanupSingleWorkspace,
  getConfig,
  getDefaultPrompts,
  listWorkspaces,
  updateConfig,
} from "../../lib/api";
import { formatBytes } from "../../lib/formatters";
import type { RunConfig, WorkspaceInfo } from "../../lib/types";
import { DEFAULT_RUN_CONFIG } from "./constants";

export function useSettingsController() {
  const [config, setConfig] = useState<RunConfig>(DEFAULT_RUN_CONFIG);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceInfo[]>([]);
  const [wsLoading, setWsLoading] = useState(false);
  const [wsMessage, setWsMessage] = useState<string | null>(null);
  const [defaultPrompts, setDefaultPrompts] = useState<Record<string, string>>({});
  const [expandedStage, setExpandedStage] = useState<string | null>(null);
  const [newSkipLabel, setNewSkipLabel] = useState("");

  useEffect(() => {
    void loadInitialData();
  }, []);

  function showSavedMessage() {
    setSaved(true);
    window.setTimeout(() => setSaved(false), 2000);
  }

  function showWorkspaceMessage(message: string) {
    setWsMessage(message);
    window.setTimeout(() => setWsMessage(null), 2000);
  }

  async function loadInitialData() {
    setWsLoading(true);

    const [configResult, workspacesResult, promptsResult] = await Promise.allSettled([
      getConfig(),
      listWorkspaces(),
      getDefaultPrompts(),
    ]);

    if (configResult.status === "fulfilled") {
      setConfig(configResult.value);
    } else {
      setError((currentError) => currentError ?? String(configResult.reason));
    }

    if (workspacesResult.status === "fulfilled") {
      setWorkspaces(workspacesResult.value);
    } else {
      setError((currentError) => currentError ?? String(workspacesResult.reason));
    }

    if (promptsResult.status === "fulfilled") {
      setDefaultPrompts(promptsResult.value);
    }

    setWsLoading(false);
  }

  async function saveConfig() {
    setError(null);
    setSaved(false);

    try {
      await updateConfig(config);
      showSavedMessage();
    } catch (saveError) {
      setError(String(saveError));
    }
  }

  async function refreshWorkspaces() {
    setWsLoading(true);

    try {
      const nextWorkspaces = await listWorkspaces();
      setWorkspaces(nextWorkspaces);
    } catch (workspaceError) {
      setError(String(workspaceError));
    } finally {
      setWsLoading(false);
    }
  }

  async function cleanupSingle(path: string) {
    try {
      await cleanupSingleWorkspace(path);
      showWorkspaceMessage("Workspace removed");
      await refreshWorkspaces();
    } catch (cleanupError) {
      setError(String(cleanupError));
    }
  }

  async function cleanupAll() {
    try {
      const removedCount = await cleanupAllWorkspaces();
      showWorkspaceMessage(`Removed ${removedCount} workspace${removedCount !== 1 ? "s" : ""}`);
      await refreshWorkspaces();
    } catch (cleanupError) {
      setError(String(cleanupError));
    }
  }

  async function cleanupOld() {
    try {
      const removedCount = await cleanupOldWorkspaces(config.workspace_ttl_days);
      showWorkspaceMessage(
        removedCount > 0
          ? `Removed ${removedCount} old workspace${removedCount !== 1 ? "s" : ""}`
          : "No old workspaces to remove",
      );
      await refreshWorkspaces();
    } catch (cleanupError) {
      setError(String(cleanupError));
    }
  }

  const totalWorkspaceSize = workspaces.reduce((sum, workspace) => sum + workspace.size_bytes, 0);

  return {
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
    totalWorkspaceSizeDisplay: formatBytes(totalWorkspaceSize),
  };
}
