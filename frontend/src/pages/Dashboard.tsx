import { useState, useEffect, useRef } from "react";
import { Link } from "react-router-dom";
import useSWR from "swr";
import { swrFetcher, apiPost } from "../lib/api";
import { connectEvents, type EventConnection } from "../lib/ws";
import type { TimestampedEvent, TaskStatusChanged } from "../lib/types";
import { LayoutDashboard, CheckCircle, Loader, Coins, Plus } from "lucide-react";
import StatsCard from "../components/StatsCard";
import Skeleton from "../components/ui/Skeleton";
import Badge from "../components/ui/Badge";

interface Stats {
  total_epics: number;
  open_epics: number;
  total_tasks: number;
  done_tasks: number;
  in_progress_tasks: number;
  blocked_tasks: number;
  total_tokens: number;
}

interface EpicTask {
  status: string;
}

interface Epic {
  id: string;
  title: string;
  status: string;
  branch_name?: string;
  tasks?: EpicTask[];
}

interface ActivityEvent {
  id: number;
  timestamp: string;
  type: string;
  summary: string;
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function epicStatusKey(status: string): "todo" | "progress" | "done" | "blocked" {
  if (status === "done" || status === "closed") return "done";
  if (status === "active" || status === "in_progress") return "progress";
  if (status === "blocked") return "blocked";
  return "todo";
}

function ProgressBar({ tasks }: { tasks?: EpicTask[] }) {
  if (!tasks || tasks.length === 0) {
    return <div className="h-2 w-full rounded-full bg-bg-tertiary" />;
  }
  const total = tasks.length;
  const counts: Record<string, number> = {};
  for (const t of tasks) {
    counts[t.status] = (counts[t.status] || 0) + 1;
  }
  const segments = [
    { key: "done", count: counts["done"] || 0, color: "bg-success" },
    { key: "in_progress", count: counts["in_progress"] || 0, color: "bg-accent" },
    { key: "blocked", count: counts["blocked"] || 0, color: "bg-error" },
    { key: "todo", count: counts["todo"] || 0, color: "bg-bg-tertiary" },
  ];

  return (
    <div className="flex h-2 w-full rounded-full overflow-hidden bg-bg-tertiary">
      {segments.map(
        (seg) =>
          seg.count > 0 && (
            <div
              key={seg.key}
              className={`${seg.color} transition-all duration-300`}
              style={{ width: `${(seg.count / total) * 100}%` }}
            />
          ),
      )}
    </div>
  );
}

function DashboardSkeleton() {
  return (
    <div className="space-y-8">
      <div>
        <Skeleton variant="text" width="180px" height="28px" />
        <Skeleton variant="text" width="220px" height="16px" className="mt-2" />
      </div>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <Skeleton key={i} variant="card" height="100px" />
        ))}
      </div>
      <div className="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-4">
        {Array.from({ length: 3 }).map((_, i) => (
          <Skeleton key={i} variant="card" height="140px" />
        ))}
      </div>
    </div>
  );
}

function formatEventTime(ts: string): string {
  try {
    const d = new Date(ts);
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } catch {
    return "";
  }
}

function eventSummary(raw: TimestampedEvent): string {
  const { type, data } = raw.event;
  if (type === "TaskStatusChanged" && data && "task_id" in data) {
    const d = data as TaskStatusChanged;
    return `Task ${d.task_id}: ${d.from_status} -> ${d.to_status}`;
  }
  if (type === "AgentLog" && data && "message" in data) {
    return `[${(data as { agent_id: string }).agent_id}] ${(data as { message: string }).message}`;
  }
  if (type === "EpicUpdated" && data && "epic_id" in data) {
    return `Epic ${(data as { epic_id: string }).epic_id} updated`;
  }
  if (type === "Heartbeat") return "heartbeat";
  return type;
}

export default function Dashboard() {
  const { data: stats, isLoading: statsLoading } = useSWR<Stats>(
    "/stats",
    swrFetcher,
  );
  const { data: epics, isLoading: epicsLoading, mutate: mutateEpics } = useSWR<Epic[]>(
    "/epics",
    swrFetcher,
  );

  const [creating, setCreating] = useState(false);
  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const eventIdRef = useRef(0);

  // Activity timeline via WS
  const connRef = useRef<EventConnection | null>(null);
  useEffect(() => {
    const conn = connectEvents((raw) => {
      const stamped = raw as TimestampedEvent;
      if (stamped?.event?.type === "Heartbeat") return;
      const summary = eventSummary(stamped);
      const id = ++eventIdRef.current;
      setEvents((prev) => [
        { id, timestamp: stamped.timestamp, type: stamped.event.type, summary },
        ...prev.slice(0, 19),
      ]);
    });
    connRef.current = conn;
    return () => conn.close();
  }, []);

  const loading = statsLoading || epicsLoading;
  const donePercent =
    stats && stats.total_tasks > 0
      ? (stats.done_tasks / stats.total_tasks) * 100
      : 0;

  async function handleCreateEpic() {
    const title = window.prompt("Epic title:");
    if (!title?.trim()) return;
    setCreating(true);
    try {
      await apiPost("/epics/create", { title: title.trim() });
      mutateEpics();
    } catch (err) {
      console.error("Failed to create epic:", err);
    } finally {
      setCreating(false);
    }
  }

  if (loading) return <DashboardSkeleton />;

  return (
    <div className="space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold mb-1">Dashboard</h1>
          <p className="text-text-secondary text-sm">
            Epic overview and task metrics.
          </p>
        </div>
        <button
          onClick={handleCreateEpic}
          disabled={creating}
          className="flex items-center gap-1.5 px-3 py-2 rounded-md text-sm font-medium bg-accent text-bg-primary hover:bg-accent-hover transition-colors disabled:opacity-50"
        >
          <Plus size={16} />
          {creating ? "Creating..." : "Create Epic"}
        </button>
      </div>

      {/* Stats row */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <StatsCard
          icon={LayoutDashboard}
          value={stats?.total_epics ?? 0}
          label="Total Epics"
          trend={stats ? `${stats.open_epics} open` : undefined}
        />
        <StatsCard
          icon={CheckCircle}
          value={`${Math.round(donePercent)}%`}
          label="Tasks Done"
          trend={
            stats
              ? `${stats.done_tasks} / ${stats.total_tasks}`
              : undefined
          }
        />
        <StatsCard
          icon={Loader}
          value={stats?.in_progress_tasks ?? 0}
          label="In Progress"
          trend={
            stats && stats.blocked_tasks > 0
              ? `${stats.blocked_tasks} blocked`
              : undefined
          }
        />
        <StatsCard
          icon={Coins}
          value={stats ? formatTokens(stats.total_tokens) : "0"}
          label="Tokens Used"
        />
      </div>

      {/* Epic cards grid */}
      {epics && epics.length > 0 ? (
        <div>
          <h2 className="text-lg font-semibold mb-3">Epics</h2>
          <div className="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-4">
            {epics.map((epic) => (
              <Link
                key={epic.id}
                to={`/epic/${epic.id}`}
                className="block rounded-lg border border-border bg-bg-secondary p-4 hover:border-accent transition-colors"
              >
                <div className="flex items-start justify-between gap-2 mb-3">
                  <h3 className="text-[15px] font-semibold truncate">{epic.title}</h3>
                  <Badge status={epicStatusKey(epic.status)} label={epic.status.replace("_", " ")} />
                </div>
                <ProgressBar tasks={epic.tasks} />
                <p className="text-xs text-text-muted mt-2 truncate font-mono">
                  {epic.id}
                </p>
              </Link>
            ))}
          </div>
        </div>
      ) : (
        <div className="flex flex-col items-center justify-center py-16 text-center">
          <LayoutDashboard size={48} className="text-text-muted mb-4" />
          <h2 className="text-lg font-semibold mb-2">Create your first Epic to get started</h2>
          <p className="text-sm text-text-muted mb-6 max-w-md">
            Epics organize your tasks into focused work streams. Create one to begin tracking progress.
          </p>
          <button
            onClick={handleCreateEpic}
            disabled={creating}
            className="flex items-center gap-1.5 px-4 py-2.5 rounded-md text-sm font-medium bg-accent text-bg-primary hover:bg-accent-hover transition-colors disabled:opacity-50"
          >
            <Plus size={16} />
            {creating ? "Creating..." : "Create Epic"}
          </button>
        </div>
      )}

      {/* Activity timeline */}
      {events.length > 0 && (
        <div>
          <h2 className="text-lg font-semibold mb-3">Activity</h2>
          <div className="rounded-lg border border-border bg-bg-secondary overflow-hidden">
            <div className="max-h-64 overflow-y-auto divide-y divide-border">
              {events.map((ev) => (
                <div key={ev.id} className="flex items-center gap-3 px-4 py-2 text-xs">
                  <span className="text-text-muted font-mono shrink-0 w-14">
                    {formatEventTime(ev.timestamp)}
                  </span>
                  <span className="text-text-secondary truncate">{ev.summary}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
