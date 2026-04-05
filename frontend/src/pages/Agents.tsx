import { useEffect, useRef, useState, useCallback, useMemo } from "react";
import { connectEvents, type EventConnection } from "../lib/ws";
import type { AgentLog } from "../lib/types";
import { apiPost } from "../lib/api";
import { toast } from "sonner";
import Button from "../components/ui/Button";

interface LogEntry {
  time: string;
  agentId: string;
  taskId: string;
  level: string;
  message: string;
}

interface AgentHealth {
  agentId: string;
  lastHeartbeat: number; // epoch ms
}

function formatTime(ts: string): string {
  try {
    const d = new Date(ts);
    return d.toLocaleTimeString("en-US", { hour12: false });
  } catch {
    return ts;
  }
}

function levelColor(level: string): string {
  switch (level.toLowerCase()) {
    case "warn":
    case "warning":
      return "text-status-skipped";
    case "error":
      return "text-status-blocked";
    default:
      return "text-text-secondary";
  }
}

function levelBadgeClass(level: string): string {
  switch (level.toLowerCase()) {
    case "warn":
    case "warning":
      return "bg-status-skipped/20 text-status-skipped";
    case "error":
      return "bg-status-blocked/20 text-status-blocked";
    default:
      return "bg-bg-tertiary text-text-secondary";
  }
}

function healthColor(lastHeartbeat: number): string {
  const elapsed = Date.now() - lastHeartbeat;
  if (elapsed < 60_000) return "bg-status-done";
  if (elapsed < 120_000) return "bg-status-skipped";
  return "bg-status-blocked";
}

function healthLabel(lastHeartbeat: number): string {
  const elapsed = Math.floor((Date.now() - lastHeartbeat) / 1000);
  if (elapsed < 60) return `${elapsed}s ago`;
  return `${Math.floor(elapsed / 60)}m ${elapsed % 60}s ago`;
}

export default function Agents() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [agents, setAgents] = useState<Map<string, AgentHealth>>(new Map());
  const [filterAgent, setFilterAgent] = useState("");
  const [filterTask, setFilterTask] = useState("");
  const [filterLevels, setFilterLevels] = useState<Set<string>>(
    new Set(["info", "warn", "error"]),
  );
  const [searchText, setSearchText] = useState("");
  const [autoScroll, setAutoScroll] = useState(true);
  const [hasNewMessages, setHasNewMessages] = useState(false);
  const [actionLoading, setActionLoading] = useState<string | null>(null);
  const [, setTick] = useState(0);

  const bottomRef = useRef<HTMLDivElement>(null);
  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const connRef = useRef<EventConnection | null>(null);

  // Tick every 10s to update health labels
  useEffect(() => {
    const interval = setInterval(() => setTick((t) => t + 1), 10_000);
    return () => clearInterval(interval);
  }, []);

  // WS connection
  useEffect(() => {
    const conn = connectEvents();
    connRef.current = conn;

    conn.on<AgentLog>("AgentLog", (data, timestamp) => {
      const entry: LogEntry = {
        time: formatTime(timestamp),
        agentId: data.agent_id,
        taskId: data.task_id,
        level: data.level,
        message: data.message,
      };
      setLogs((prev) => [...prev, entry]);
    });

    conn.on("Heartbeat", (_data, timestamp) => {
      setAgents((prev) => {
        const next = new Map(prev);
        // Use a generic agent id from heartbeat; if no agent_id, use "system"
        const agentId = "system";
        next.set(agentId, {
          agentId,
          lastHeartbeat: new Date(timestamp).getTime(),
        });
        return next;
      });
    });

    return () => conn.close();
  }, []);

  // Auto-scroll logic
  useEffect(() => {
    if (autoScroll) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
      setHasNewMessages(false);
    } else if (logs.length > 0) {
      setHasNewMessages(true);
    }
  }, [logs, autoScroll]);

  const handleScroll = useCallback(() => {
    const el = scrollContainerRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    setAutoScroll(atBottom);
    if (atBottom) setHasNewMessages(false);
  }, []);

  // Unique agents and tasks for filter dropdowns
  const uniqueAgents = useMemo(
    () => [...new Set(logs.map((l) => l.agentId).filter(Boolean))],
    [logs],
  );
  const uniqueTasks = useMemo(
    () => [...new Set(logs.map((l) => l.taskId).filter(Boolean))],
    [logs],
  );

  // Filtered logs
  const filtered = useMemo(() => {
    return logs.filter((l) => {
      if (filterAgent && l.agentId !== filterAgent) return false;
      if (filterTask && l.taskId !== filterTask) return false;
      if (!filterLevels.has(l.level.toLowerCase())) return false;
      if (
        searchText &&
        !l.message.toLowerCase().includes(searchText.toLowerCase())
      )
        return false;
      return true;
    });
  }, [logs, filterAgent, filterTask, filterLevels, searchText]);

  const hasFilters = filterAgent || filterTask || searchText || filterLevels.size < 3;

  function clearFilters() {
    setFilterAgent("");
    setFilterTask("");
    setSearchText("");
    setFilterLevels(new Set(["info", "warn", "error"]));
  }

  function toggleLevel(level: string) {
    setFilterLevels((prev) => {
      const next = new Set(prev);
      if (next.has(level)) next.delete(level);
      else next.add(level);
      return next;
    });
  }

  async function taskAction(
    taskId: string,
    action: "restart" | "skip" | "block",
  ) {
    const key = `${taskId}-${action}`;
    setActionLoading(key);
    try {
      const body =
        action === "restart"
          ? undefined
          : { reason: `${action === "skip" ? "Skipped" : "Blocked"} from console` };
      await apiPost(`/tasks/${taskId}/${action}`, body);
      toast.success(`Task ${taskId} ${action === "restart" ? "restarted" : action === "skip" ? "skipped" : "blocked"}`);
    } catch (err) {
      toast.error(
        `Failed to ${action} task ${taskId}: ${err instanceof Error ? err.message : "Unknown error"}`,
      );
    } finally {
      setActionLoading(null);
    }
  }

  // Tasks that appear in filtered logs for action buttons
  const visibleTasks = useMemo(
    () => [...new Set(filtered.map((l) => l.taskId).filter(Boolean))],
    [filtered],
  );

  if (logs.length === 0) {
    return (
      <div className="flex flex-col h-full items-center justify-center gap-4 text-text-muted">
        <div className="text-4xl">&#x1f4e1;</div>
        <p className="text-sm">
          Waiting for agents to connect. Start work to see activity here.
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full gap-3">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Agent Console</h1>
        <span className="text-xs text-text-muted">
          {filtered.length}/{logs.length} event{logs.length !== 1 && "s"}
        </span>
      </div>

      {/* Agent health indicators */}
      {agents.size > 0 && (
        <div className="flex gap-3 flex-wrap">
          {[...agents.values()].map((a) => (
            <div
              key={a.agentId}
              className="flex items-center gap-2 px-3 py-1.5 rounded-md bg-bg-secondary border border-border text-xs"
            >
              <span
                className={`w-2 h-2 rounded-full ${healthColor(a.lastHeartbeat)}`}
              />
              <span className="text-text-primary font-medium">{a.agentId}</span>
              <span className="text-text-muted">{healthLabel(a.lastHeartbeat)}</span>
            </div>
          ))}
        </div>
      )}

      {/* Filter toolbar */}
      <div className="flex items-center gap-2 flex-wrap">
        <select
          value={filterAgent}
          onChange={(e) => setFilterAgent(e.target.value)}
          className="px-2 py-1.5 rounded-md bg-bg-secondary border border-border text-xs text-text-primary"
        >
          <option value="">All agents</option>
          {uniqueAgents.map((a) => (
            <option key={a} value={a}>
              {a}
            </option>
          ))}
        </select>

        <select
          value={filterTask}
          onChange={(e) => setFilterTask(e.target.value)}
          className="px-2 py-1.5 rounded-md bg-bg-secondary border border-border text-xs text-text-primary"
        >
          <option value="">All tasks</option>
          {uniqueTasks.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>

        {(["info", "warn", "error"] as const).map((level) => (
          <label
            key={level}
            className="flex items-center gap-1 text-xs cursor-pointer"
          >
            <input
              type="checkbox"
              checked={filterLevels.has(level)}
              onChange={() => toggleLevel(level)}
              className="rounded border-border"
            />
            <span className={levelColor(level)}>
              {level.charAt(0).toUpperCase() + level.slice(1)}
            </span>
          </label>
        ))}

        <input
          type="text"
          placeholder="Search messages..."
          value={searchText}
          onChange={(e) => setSearchText(e.target.value)}
          className="flex-1 min-w-[120px] px-2 py-1.5 rounded-md bg-bg-secondary border border-border text-xs text-text-primary placeholder:text-text-muted focus:outline-none focus:border-border-accent"
        />

        {hasFilters && (
          <button
            onClick={clearFilters}
            className="px-2 py-1.5 rounded-md text-xs text-text-muted hover:text-text-primary hover:bg-bg-tertiary transition-colors"
          >
            Clear filters
          </button>
        )}
      </div>

      {/* Task action buttons */}
      {visibleTasks.length > 0 && visibleTasks.length <= 5 && (
        <div className="flex gap-2 flex-wrap">
          {visibleTasks.map((taskId) => (
            <div
              key={taskId}
              className="flex items-center gap-1.5 px-2 py-1 rounded-md bg-bg-secondary border border-border"
            >
              <span className="text-xs text-accent font-medium mr-1">
                {taskId}
              </span>
              <Button
                size="sm"
                variant="ghost"
                loading={actionLoading === `${taskId}-restart`}
                onClick={() => taskAction(taskId, "restart")}
                className="text-status-done hover:bg-status-done/10"
              >
                Restart
              </Button>
              <Button
                size="sm"
                variant="ghost"
                loading={actionLoading === `${taskId}-skip`}
                onClick={() => taskAction(taskId, "skip")}
                className="text-status-skipped hover:bg-status-skipped/10"
              >
                Skip
              </Button>
              <Button
                size="sm"
                variant="ghost"
                loading={actionLoading === `${taskId}-block`}
                onClick={() => taskAction(taskId, "block")}
                className="text-status-blocked hover:bg-status-blocked/10"
              >
                Block
              </Button>
            </div>
          ))}
        </div>
      )}

      {/* Log viewer */}
      <div
        ref={scrollContainerRef}
        onScroll={handleScroll}
        className="relative flex-1 overflow-auto rounded-md bg-bg-secondary border border-border p-3 font-mono text-xs leading-relaxed"
      >
        {filtered.length === 0 ? (
          <p className="text-text-muted">No events match filters.</p>
        ) : (
          filtered.map((entry, i) => (
            <div key={i} className="flex gap-2 py-0.5 hover:bg-bg-tertiary/50">
              <span className="text-text-muted shrink-0">[{entry.time}]</span>
              <span
                className={`shrink-0 inline-flex items-center rounded px-1.5 py-0 text-[10px] font-medium ${levelBadgeClass(entry.level)}`}
              >
                {entry.level.toUpperCase()}
              </span>
              {entry.agentId && (
                <span className="text-accent shrink-0">[{entry.agentId}]</span>
              )}
              {entry.taskId && (
                <span className="text-status-progress shrink-0">
                  [{entry.taskId}]
                </span>
              )}
              <span className={`truncate ${levelColor(entry.level)}`}>
                {entry.message}
              </span>
            </div>
          ))
        )}
        <div ref={bottomRef} />

        {/* New messages indicator */}
        {hasNewMessages && (
          <button
            onClick={() => {
              bottomRef.current?.scrollIntoView({ behavior: "smooth" });
              setAutoScroll(true);
              setHasNewMessages(false);
            }}
            className="sticky bottom-2 left-1/2 -translate-x-1/2 px-3 py-1.5 rounded-full bg-accent text-white text-xs font-medium shadow-lg hover:bg-accent-hover transition-colors"
          >
            New messages ↓
          </button>
        )}
      </div>

      {/* Client status section */}
      <div className="rounded-md bg-bg-secondary border border-border">
        <div className="px-3 py-2 border-b border-border">
          <h2 className="text-xs font-semibold text-text-muted uppercase tracking-wider">
            Connected Clients
          </h2>
        </div>
        <table className="w-full text-xs">
          <thead>
            <tr className="text-text-muted">
              <th className="text-left px-3 py-1.5 font-medium">Client</th>
              <th className="text-left px-3 py-1.5 font-medium">
                Connected Since
              </th>
              <th className="text-left px-3 py-1.5 font-medium">
                Last Heartbeat
              </th>
              <th className="text-left px-3 py-1.5 font-medium">Status</th>
            </tr>
          </thead>
          <tbody>
            <tr className="text-text-primary border-t border-border">
              <td className="px-3 py-1.5 font-medium">Claude Code</td>
              <td className="px-3 py-1.5 text-text-secondary">--</td>
              <td className="px-3 py-1.5 text-text-secondary">
                {agents.size > 0
                  ? healthLabel([...agents.values()][0].lastHeartbeat)
                  : "--"}
              </td>
              <td className="px-3 py-1.5">
                <span className="inline-flex items-center gap-1.5">
                  <span
                    className={`w-2 h-2 rounded-full ${
                      agents.size > 0
                        ? healthColor([...agents.values()][0].lastHeartbeat)
                        : "bg-text-muted"
                    }`}
                  />
                  {agents.size > 0 ? "Connected" : "Waiting"}
                </span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
  );
}
