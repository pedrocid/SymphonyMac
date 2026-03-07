import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface AgentLogLine {
  run_id: string;
  timestamp: string;
  line: string;
}

export function LogViewer({ runId, onClose }: { runId: string; onClose: () => void }) {
  const [logs, setLogs] = useState<string[]>([]);
  const [autoScroll, setAutoScroll] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Load existing logs
    invoke<string[]>("get_agent_logs", { runId }).then((result) => {
      setLogs(result);
    });

    // Listen for new log lines
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

  function handleScroll() {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    const isAtBottom = scrollHeight - scrollTop - clientHeight < 50;
    setAutoScroll(isAtBottom);
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between p-3 border-b border-[#30363d]">
        <div className="flex items-center gap-2">
          <h3 className="text-sm font-medium text-[#e6edf3]">Agent Logs</h3>
          <span className="text-xs text-[#8b949e]">({logs.length} lines)</span>
        </div>
        <div className="flex items-center gap-2">
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

      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="flex-1 overflow-auto p-3 log-viewer bg-[#0d1117]"
      >
        {logs.length === 0 ? (
          <div className="text-[#484f58] text-center py-8">Waiting for output...</div>
        ) : (
          logs.map((line, i) => (
            <div
              key={i}
              className={`${
                line.startsWith("[stderr]")
                  ? "text-[#f85149]"
                  : "text-[#8b949e]"
              } hover:bg-[#161b22]`}
            >
              {line}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
