import { Link } from "react-router-dom";
import useSWR from "swr";
import { swrFetcher } from "../lib/api";
import { LayoutDashboard, CheckCircle, Loader, Coins } from "lucide-react";
import StatsCard from "../components/StatsCard";
import ProgressRing from "../components/ProgressRing";

interface Stats {
  total_epics: number;
  open_epics: number;
  total_tasks: number;
  done_tasks: number;
  in_progress_tasks: number;
  blocked_tasks: number;
  total_tokens: number;
}

interface Epic {
  id: string;
  title: string;
  status: string;
  branch_name?: string;
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function statusBadge(status: string) {
  const colors: Record<string, string> = {
    open: "bg-info/20 text-info",
    active: "bg-accent/20 text-accent",
    in_progress: "bg-accent/20 text-accent",
    done: "bg-success/20 text-success",
    closed: "bg-success/20 text-success",
    blocked: "bg-error/20 text-error",
  };
  const cls = colors[status] ?? "bg-bg-tertiary text-text-secondary";
  return (
    <span className={`inline-block rounded px-2 py-0.5 text-xs font-medium ${cls}`}>
      {status.replace("_", " ")}
    </span>
  );
}

export default function Dashboard() {
  const { data: stats, isLoading: statsLoading } = useSWR<Stats>(
    "/stats",
    swrFetcher,
  );
  const { data: epics, isLoading: epicsLoading } = useSWR<Epic[]>(
    "/epics",
    swrFetcher,
  );

  const loading = statsLoading || epicsLoading;
  const donePercent =
    stats && stats.total_tasks > 0
      ? (stats.done_tasks / stats.total_tasks) * 100
      : 0;

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-bold mb-1">Dashboard</h1>
        <p className="text-text-secondary text-sm">
          Epic overview and task metrics.
        </p>
      </div>

      {loading ? (
        <div className="flex items-center gap-2 text-text-muted py-12 justify-center">
          <Loader size={18} className="animate-spin" />
          <span>Loading...</span>
        </div>
      ) : (
        <>
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

          {/* Progress ring + Epic grid */}
          <div className="grid grid-cols-1 lg:grid-cols-[auto_1fr] gap-8 items-start">
            <div className="flex flex-col items-center gap-2">
              <ProgressRing size={140} percentage={donePercent} />
              <p className="text-sm text-text-secondary">Task Completion</p>
            </div>

            <div>
              <h2 className="text-lg font-semibold mb-3">Epics</h2>
              {epics && epics.length > 0 ? (
                <div className="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-3">
                  {epics.map((epic) => (
                    <Link
                      key={epic.id}
                      to={`/epic/${epic.id}`}
                      className="block rounded-lg border border-border bg-bg-secondary p-4 hover:border-accent transition-colors"
                    >
                      <div className="flex items-start justify-between gap-2 mb-2">
                        <h3 className="text-sm font-medium truncate">
                          {epic.title}
                        </h3>
                        {statusBadge(epic.status)}
                      </div>
                      <p className="text-xs text-text-muted truncate">
                        {epic.id}
                      </p>
                    </Link>
                  ))}
                </div>
              ) : (
                <p className="text-text-muted text-sm">No epics yet.</p>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
