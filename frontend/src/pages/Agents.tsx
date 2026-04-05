import { useEffect, useRef, useState, useCallback } from "react";
import { connectEvents } from "../lib/ws";

interface FlowEvent {
  timestamp: string;
  event: {
    type: string;
    data: { task_id?: string; epic_id?: string; [key: string]: unknown };
  };
}

interface LogEntry {
  time: string;
  type: string;
  taskId: string;
  message: string;
}

function formatTime(ts: string): string {
  try {
    const d = new Date(ts);
    return d.toLocaleTimeString("en-US", { hour12: false });
  } catch {
    return ts;
  }
}

function typeColor(type: string): string {
  if (type === "TaskCompleted") return "text-success";
  if (type === "TaskFailed") return "text-error";
  if (type === "TaskStarted") return "text-warning";
  return "text-text-muted";
}

export default function Agents() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [filter, setFilter] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);

  const handleEvent = useCallback((raw: unknown) => {
    const ev = raw as FlowEvent;
    const entry: LogEntry = {
      time: formatTime(ev.timestamp),
      type: ev.event.type,
      taskId: ev.event.data?.task_id ?? "",
      message: JSON.stringify(ev.event.data),
    };
    setLogs((prev) => [...prev, entry]);
  }, []);

  useEffect(() => {
    const ws = connectEvents(handleEvent);
    return () => ws.close();
  }, [handleEvent]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  const filtered = logs.filter((l) => {
    if (!filter) return true;
    const q = filter.toLowerCase();
    return (
      l.taskId.toLowerCase().includes(q) ||
      l.type.toLowerCase().includes(q)
    );
  });

  const singleTask =
    filter && new Set(filtered.map((l) => l.taskId)).size === 1
      ? filtered[0]?.taskId
      : null;

  return (
    <div className="flex flex-col h-full gap-4">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Agents</h1>
        <span className="text-xs text-text-muted">
          {logs.length} event{logs.length !== 1 && "s"}
        </span>
      </div>

      <input
        type="text"
        placeholder="Filter by task ID or event type..."
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        className="w-full px-3 py-2 rounded-md bg-bg-secondary border border-border text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-border-accent"
      />

      {singleTask && (
        <div className="flex gap-2">
          <button className="px-3 py-1.5 rounded-md bg-success/20 text-success text-xs font-medium hover:bg-success/30 transition-colors">
            Retry
          </button>
          <button className="px-3 py-1.5 rounded-md bg-warning/20 text-warning text-xs font-medium hover:bg-warning/30 transition-colors">
            Skip
          </button>
          <button className="px-3 py-1.5 rounded-md bg-error/20 text-error text-xs font-medium hover:bg-error/30 transition-colors">
            Block
          </button>
        </div>
      )}

      <div className="flex-1 overflow-auto rounded-md bg-bg-secondary border border-border p-3 font-mono text-xs leading-relaxed">
        {filtered.length === 0 ? (
          <p className="text-text-muted">
            {logs.length === 0
              ? "Waiting for events..."
              : "No events match filter."}
          </p>
        ) : (
          filtered.map((entry, i) => (
            <div key={i} className="flex gap-2">
              <span className="text-text-muted shrink-0">[{entry.time}]</span>
              <span className={`shrink-0 ${typeColor(entry.type)}`}>
                [{entry.type}]
              </span>
              {entry.taskId && (
                <span className="text-accent shrink-0">[{entry.taskId}]</span>
              )}
              <span className="text-text-secondary truncate">
                {entry.message}
              </span>
            </div>
          ))
        )}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
