import { useState, useEffect, useRef, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AgentLogLine } from "../contracts";

export function LogViewer({ runId, onClose }: { runId: string; onClose: () => void }) {
  const [logs, setLogs] = useState<string[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");
  const [exportMsg, setExportMsg] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    invoke<string[]>("get_agent_logs", { runId }).then((result) => {
      setLogs(result);
    });

    const unlisten = listen<AgentLogLine>("agent-log", (event) => {
      if (event.payload.run_id === runId) {
        setLogs((prev) => [...prev, event.payload.line]);
      }
    });

    return () => {
      unlisten.then((f) => f());
    };
  }, [runId]);

  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [logs, autoScroll]);

  const filteredLogs = useMemo(() => {
    if (!searchQuery.trim()) return logs;
    const q = searchQuery.toLowerCase();
    return logs.filter((line) => line.toLowerCase().includes(q));
  }, [logs, searchQuery]);

  function handleScroll() {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    const isAtBottom = scrollHeight - scrollTop - clientHeight < 50;
    setAutoScroll(isAtBottom);
  }

  async function handleExport(format: "txt" | "json") {
    try {
      const content =
        format === "txt"
          ? await invoke<string>("export_logs_text", { runId })
          : await invoke<string>("export_logs_json", { runId });

      await navigator.clipboard.writeText(content);
      setExportMsg(`Copied as ${format.toUpperCase()}`);
      setTimeout(() => setExportMsg(null), 2000);
    } catch {
      setExportMsg("Export failed");
      setTimeout(() => setExportMsg(null), 2000);
    }
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between p-3 border-b border-[#30363d]">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-medium text-[#e6edf3]">Agent Logs</h3>
          <span className="text-xs text-[#8b949e]">
            ({filteredLogs.length}{searchQuery ? `/${logs.length}` : ""} lines)
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => handleExport("txt")}
            className="text-xs px-2 py-1 rounded text-[#8b949e] hover:text-[#e6edf3] hover:bg-[#21262d]"
            title="Copy as plain text"
          >
            TXT
          </button>
          <button
            onClick={() => handleExport("json")}
            className="text-xs px-2 py-1 rounded text-[#8b949e] hover:text-[#e6edf3] hover:bg-[#21262d]"
            title="Copy as JSON"
          >
            JSON
          </button>
          <button
            onClick={() => {
              setAutoScroll(true);
              if (scrollRef.current) {
                scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
              }
            }}
            className={`text-xs px-2 py-1 rounded ${
              autoScroll
                ? "bg-[#58a6ff26] text-[#58a6ff]"
                : "text-[#8b949e] hover:text-[#e6edf3]"
            }`}
          >
            Auto-scroll
          </button>
          <button
            onClick={onClose}
            className="text-[#8b949e] hover:text-[#e6edf3] text-lg leading-none"
          >
            &times;
          </button>
        </div>
      </div>

      {/* Search bar */}
      <div className="px-3 py-2 border-b border-[#30363d]">
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          placeholder="Search logs..."
          className="w-full bg-[#0d1117] border border-[#30363d] rounded px-2 py-1 text-sm text-[#e6edf3] placeholder-[#484f58] focus:outline-none focus:border-[#58a6ff]"
        />
      </div>

      {/* Export feedback */}
      {exportMsg && (
        <div className="px-3 py-1 text-xs text-[#3fb950] bg-[#3fb95015]">
          {exportMsg}
        </div>
      )}

      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="flex-1 overflow-auto p-3 log-viewer bg-[#0d1117]"
      >
        {filteredLogs.length === 0 ? (
          <div className="text-[#484f58] text-center py-8">
            {searchQuery ? "No matching lines" : "Waiting for output..."}
          </div>
        ) : (
          filteredLogs.map((line, i) => (
            <div
              key={i}
              className={`${
                line.startsWith("[stderr]")
                  ? "text-[#f85149]"
                  : "text-[#8b949e]"
              } hover:bg-[#161b22]`}
            >
              {searchQuery ? highlightMatch(line, searchQuery) : line}
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function highlightMatch(text: string, query: string) {
  if (!query) return text;
  const idx = text.toLowerCase().indexOf(query.toLowerCase());
  if (idx === -1) return text;
  return (
    <>
      {text.slice(0, idx)}
      <span className="bg-[#e3b341] text-[#0d1117] rounded-sm px-[1px]">
        {text.slice(idx, idx + query.length)}
      </span>
      {text.slice(idx + query.length)}
    </>
  );
}
