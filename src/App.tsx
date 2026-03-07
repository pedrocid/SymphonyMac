import { useState } from "react";
import "./App.css";
import { RepoSelector } from "./components/RepoSelector";
import { IssueList } from "./components/IssueList";
import { Dashboard } from "./components/Dashboard";
import { ActiveAgents } from "./components/ActiveAgents";
import { Settings } from "./components/Settings";
import { LogViewer } from "./components/LogViewer";
import { PipelineReportView } from "./components/PipelineReportView";

export type View = "repos" | "issues" | "dashboard" | "agents" | "settings";

export interface RunConfig {
  agent_type: string;
  auto_approve: boolean;
  max_concurrent: number;
  poll_interval_secs: number;
  issue_label: string | null;
  max_turns: number;
  notifications_enabled: boolean;
  notification_sound: boolean;
}

function App() {
  const [view, setView] = useState<View>("repos");
  const [selectedRepo, setSelectedRepo] = useState<string | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [reportRunId, setReportRunId] = useState<string | null>(null);

  return (
    <div className="flex h-screen bg-[#0d1117]">
      {/* Sidebar */}
      <div className="w-56 bg-[#161b22] border-r border-[#30363d] flex flex-col">
        <div className="p-4 border-b border-[#30363d]">
          <h1 className="text-lg font-bold text-[#e6edf3] flex items-center gap-2">
            <span className="text-xl">&#9835;</span> Symphony
          </h1>
          <p className="text-xs text-[#8b949e] mt-1">Agent Orchestrator</p>
        </div>

        <nav className="flex-1 p-2">
          <NavItem
            label="Repositories"
            active={view === "repos"}
            onClick={() => setView("repos")}
            icon="&#128193;"
          />
          <NavItem
            label="Issues"
            active={view === "issues"}
            onClick={() => setView("issues")}
            icon="&#128196;"
            disabled={!selectedRepo}
          />
          <NavItem
            label="Dashboard"
            active={view === "dashboard"}
            onClick={() => setView("dashboard")}
            icon="&#9632;"
          />
          <NavItem
            label="Active Agents"
            active={view === "agents"}
            onClick={() => setView("agents")}
            icon="&#9889;"
          />
          <NavItem
            label="Settings"
            active={view === "settings"}
            onClick={() => setView("settings")}
            icon="&#9881;"
          />
        </nav>

        {selectedRepo && (
          <div className="p-3 border-t border-[#30363d]">
            <p className="text-xs text-[#8b949e]">Active repo</p>
            <p className="text-sm text-[#58a6ff] truncate">{selectedRepo}</p>
          </div>
        )}
      </div>

      {/* Main content */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {view === "repos" && (
          <RepoSelector
            onSelect={(repo) => {
              setSelectedRepo(repo);
              setView("issues");
            }}
          />
        )}
        {view === "issues" && selectedRepo && (
          <IssueList
            repo={selectedRepo}
            onRunStarted={() => setView("dashboard")}
          />
        )}
        {view === "dashboard" && (
          <Dashboard
            onViewLogs={(runId) => { setReportRunId(null); setSelectedRunId(runId); }}
            onViewReport={(runId) => { setSelectedRunId(null); setReportRunId(runId); }}
          />
        )}
        {view === "agents" && (
          <ActiveAgents
            onViewLogs={(runId) => setSelectedRunId(runId)}
          />
        )}
        {view === "settings" && <Settings />}
      </div>

      {/* Report panel (slides in) */}
      {reportRunId && (
        <div className="w-[500px] bg-[#161b22] border-l border-[#30363d] flex flex-col">
          <PipelineReportView
            runId={reportRunId}
            onClose={() => setReportRunId(null)}
            onViewLogs={(runId) => { setReportRunId(null); setSelectedRunId(runId); }}
          />
        </div>
      )}

      {/* Log panel (slides in) */}
      {selectedRunId && (
        <div className="w-[500px] bg-[#161b22] border-l border-[#30363d] flex flex-col">
          <LogViewer
            runId={selectedRunId}
            onClose={() => setSelectedRunId(null)}
          />
        </div>
      )}
    </div>
  );
}

function NavItem({
  label,
  active,
  onClick,
  icon,
  disabled,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  icon: string;
  disabled?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`w-full text-left px-3 py-2 rounded-md text-sm flex items-center gap-2 mb-1 transition-colors
        ${active
          ? "bg-[#21262d] text-[#e6edf3]"
          : disabled
            ? "text-[#484f58] cursor-not-allowed"
            : "text-[#8b949e] hover:bg-[#21262d] hover:text-[#e6edf3]"
        }`}
    >
      <span dangerouslySetInnerHTML={{ __html: icon }} />
      {label}
    </button>
  );
}

export default App;
