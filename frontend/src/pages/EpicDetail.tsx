import { useState } from "react";
import { useParams, Link } from "react-router-dom";
import useSWR from "swr";
import { Plus, GitBranch, Copy } from "lucide-react";
import { toast } from "sonner";
import { swrFetcher } from "../lib/api";
import CreateTaskForm from "../components/CreateTaskForm";
import TaskActions from "../components/TaskActions";
import Badge from "../components/ui/Badge";
import Table from "../components/ui/Table";
import Skeleton from "../components/ui/Skeleton";

interface Task {
  id: string;
  title: string;
  status: string;
  domain?: string;
  depends_on?: string[];
  duration_seconds?: number;
}

interface TasksResponse {
  count: number;
  tasks: Task[];
}

interface EpicInfo {
  id: string;
  title: string;
  status: string;
  plan_reviewed?: boolean;
}

const DOMAIN_COLORS: Record<string, string> = {
  frontend: "bg-accent/15 text-accent",
  backend: "bg-info/15 text-info",
  architecture: "bg-warning/15 text-warning",
  testing: "bg-success/15 text-success",
  docs: "bg-text-secondary/15 text-text-secondary",
  ops: "bg-error/15 text-error",
  general: "bg-text-muted/15 text-text-muted",
};

function statusKey(s: string): "todo" | "progress" | "done" | "blocked" | "skipped" {
  if (s === "in_progress") return "progress";
  if (s === "done" || s === "blocked" || s === "skipped" || s === "todo") return s as "todo" | "done" | "blocked" | "skipped";
  return "todo";
}

function DomainBadge({ domain }: { domain: string }) {
  return (
    <span
      className={`inline-block rounded-full px-2 py-0.5 text-xs font-medium ${DOMAIN_COLORS[domain] ?? DOMAIN_COLORS.general}`}
    >
      {domain}
    </span>
  );
}

function formatDuration(seconds?: number): string {
  if (!seconds) return "--";
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}

function EpicDetailSkeleton() {
  return (
    <div className="space-y-6">
      <div className="flex justify-between items-start">
        <div>
          <Skeleton variant="text" width="300px" height="28px" />
          <Skeleton variant="text" width="120px" height="20px" className="mt-2" />
        </div>
        <Skeleton variant="text" width="120px" height="36px" />
      </div>
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <Skeleton key={i} variant="card" height="80px" />
        ))}
      </div>
      <Skeleton variant="card" height="300px" />
    </div>
  );
}

export default function EpicDetail() {
  const { id } = useParams<{ id: string }>();
  const [formOpen, setFormOpen] = useState(false);

  const { data, isLoading, mutate } = useSWR<TasksResponse>(
    id ? `/tasks?epic_id=${id}` : null,
    swrFetcher,
  );

  const { data: epicInfo } = useSWR<EpicInfo>(
    id ? `/epics/${id}` : null,
    swrFetcher,
  );

  if (isLoading) return <EpicDetailSkeleton />;

  const tasks = data?.tasks ?? [];
  const total = tasks.length;
  const doneCount = tasks.filter((t) => t.status === "done").length;
  const blockedCount = tasks.filter((t) => t.status === "blocked").length;
  const inProgressCount = tasks.filter((t) => t.status === "in_progress").length;
  const progress = total > 0 ? Math.round((doneCount / total) * 100) : 0;

  const workCommand = `/flow-code:work ${id ?? ""}`;

  const stats = [
    { label: "Total", value: total, color: "text-text-primary" },
    { label: "Done", value: doneCount, color: "text-success" },
    { label: "Progress", value: `${progress}%`, color: "text-accent" },
    { label: "Blocked", value: blockedCount, color: "text-error" },
  ];

  async function handleCopyCommand() {
    if (!id) return;
    try {
      await navigator.clipboard.writeText(workCommand);
      toast.success("Command copied", {
        description: "Run it in your Claude Code terminal to execute this epic.",
      });
    } catch {
      toast.error("Copy failed — select and copy manually");
    }
  }

  const columns = [
    {
      key: "title",
      header: "Title",
      sortable: true,
      sortValue: (t: Task) => t.title,
      render: (t: Task) => (
        <div>
          <span className="font-medium">{t.title}</span>
          <span className="block text-xs text-text-muted font-mono mt-0.5">{t.id}</span>
        </div>
      ),
    },
    {
      key: "status",
      header: "Status",
      sortable: true,
      sortValue: (t: Task) => t.status,
      render: (t: Task) => <Badge status={statusKey(t.status)} label={t.status.replace("_", " ")} />,
    },
    {
      key: "domain",
      header: "Domain",
      sortable: true,
      sortValue: (t: Task) => t.domain ?? "",
      render: (t: Task) => (t.domain ? <DomainBadge domain={t.domain} /> : <span className="text-text-muted">--</span>),
    },
    {
      key: "deps",
      header: "Deps",
      render: (t: Task) => (
        <span className="text-xs text-text-muted">
          {t.depends_on && t.depends_on.length > 0 ? t.depends_on.length : "--"}
        </span>
      ),
    },
    {
      key: "duration",
      header: "Duration",
      sortable: true,
      sortValue: (t: Task) => t.duration_seconds ?? 0,
      render: (t: Task) => (
        <span className="text-xs text-text-muted font-mono">{formatDuration(t.duration_seconds)}</span>
      ),
    },
    {
      key: "actions",
      header: "Actions",
      render: (t: Task) => (
        <TaskActions taskId={t.id} status={t.status} onMutate={() => mutate()} />
      ),
    },
  ];

  return (
    <div>
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 mb-6">
        <div>
          <h1 className="text-2xl font-bold">
            {epicInfo?.title ?? `Epic: ${id}`}
          </h1>
          <div className="flex items-center gap-3 mt-1">
            <Badge status={statusKey(epicInfo?.status ?? (doneCount === total && total > 0 ? "done" : "in_progress"))} />
            <Link
              to={`/dag/${id}`}
              className="flex items-center gap-1 text-sm text-accent hover:text-accent-hover transition-colors"
            >
              <GitBranch size={14} />
              DAG View
            </Link>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={handleCopyCommand}
            disabled={!id}
            title={workCommand}
            className="flex items-center gap-1.5 px-3 py-2 rounded-md text-sm font-medium bg-bg-tertiary text-text-primary border border-border hover:border-accent transition-colors disabled:opacity-50 font-mono"
          >
            <Copy size={16} />
            <span className="hidden sm:inline">Copy:&nbsp;</span>
            {workCommand}
          </button>
          <button
            onClick={() => setFormOpen(true)}
            className="flex items-center gap-1.5 px-3 py-2 rounded-md text-sm font-medium bg-accent text-bg-primary hover:bg-accent-hover transition-colors"
          >
            <Plus size={16} />
            Create Task
          </button>
        </div>
      </div>

      {/* Execution explainer */}
      <div className="mb-4 text-xs text-text-muted">
        Agent execution happens in your Claude Code terminal. This web UI is for browsing and data management.
      </div>

      {/* Stats row */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
        {stats.map((s) => (
          <div
            key={s.label}
            className="rounded-lg border border-border bg-bg-secondary p-4"
          >
            <p className="text-xs text-text-muted uppercase tracking-wide">
              {s.label}
            </p>
            <p className={`text-2xl font-bold mt-1 ${s.color}`}>{s.value}</p>
          </div>
        ))}
      </div>

      {/* Task table */}
      <div className="rounded-lg border border-border bg-bg-secondary overflow-hidden">
        {tasks.length === 0 ? (
          <div className="px-4 py-12 text-center text-text-muted">
            No tasks yet. Create one to get started.
          </div>
        ) : (
          <Table
            columns={columns}
            data={tasks}
            keyExtractor={(t) => t.id}
          />
        )}
      </div>

      {/* Create task dialog */}
      {id && (
        <CreateTaskForm
          epicId={id}
          open={formOpen}
          onClose={() => setFormOpen(false)}
          onCreated={() => mutate()}
        />
      )}
    </div>
  );
}
