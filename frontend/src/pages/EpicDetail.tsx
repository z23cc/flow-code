import { useState } from "react";
import { useParams, Link } from "react-router-dom";
import useSWR from "swr";
import { Plus, GitBranch } from "lucide-react";
import CreateTaskForm from "../components/CreateTaskForm";
import TaskActions from "../components/TaskActions";

interface Task {
  id: string;
  title: string;
  status: string;
  domain?: string;
  depends_on?: string[];
}

interface TasksResponse {
  count: number;
  tasks: Task[];
}

const fetcher = (url: string) => fetch(url).then((r) => r.json());

const STATUS_COLORS: Record<string, string> = {
  todo: "bg-info/20 text-info",
  in_progress: "bg-accent/20 text-accent",
  done: "bg-success/20 text-success",
  blocked: "bg-error/20 text-error",
  skipped: "bg-text-muted/20 text-text-muted",
  failed: "bg-error/20 text-error",
};

const DOMAIN_COLORS: Record<string, string> = {
  frontend: "bg-accent/15 text-accent",
  backend: "bg-info/15 text-info",
  architecture: "bg-warning/15 text-warning",
  testing: "bg-success/15 text-success",
  docs: "bg-text-secondary/15 text-text-secondary",
  ops: "bg-error/15 text-error",
  general: "bg-text-muted/15 text-text-muted",
};

function StatusBadge({ status }: { status: string }) {
  return (
    <span
      className={`inline-block rounded-full px-2 py-0.5 text-xs font-medium ${STATUS_COLORS[status] ?? "bg-bg-tertiary text-text-secondary"}`}
    >
      {status.replace("_", " ")}
    </span>
  );
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

export default function EpicDetail() {
  const { id } = useParams<{ id: string }>();
  const [formOpen, setFormOpen] = useState(false);

  const { data, mutate } = useSWR<TasksResponse>(
    id ? `/api/v1/tasks?epic_id=${id}` : null,
    fetcher,
  );

  const tasks = data?.tasks ?? [];
  const total = tasks.length;
  const doneCount = tasks.filter((t) => t.status === "done").length;
  const blockedCount = tasks.filter((t) => t.status === "blocked").length;
  const progress = total > 0 ? Math.round((doneCount / total) * 100) : 0;

  const stats = [
    { label: "Total", value: total, color: "text-text-primary" },
    { label: "Done", value: doneCount, color: "text-success" },
    { label: "Progress", value: `${progress}%`, color: "text-accent" },
    { label: "Blocked", value: blockedCount, color: "text-error" },
  ];

  return (
    <div>
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 mb-6">
        <div>
          <h1 className="text-2xl font-bold">Epic: {id}</h1>
          <div className="flex items-center gap-3 mt-1">
            <StatusBadge status={doneCount === total && total > 0 ? "done" : "in_progress"} />
            <Link
              to={`/dag/${id}`}
              className="flex items-center gap-1 text-sm text-accent hover:text-accent-hover transition-colors"
            >
              <GitBranch size={14} />
              DAG View
            </Link>
          </div>
        </div>
        <button
          onClick={() => setFormOpen(true)}
          className="flex items-center gap-1.5 px-3 py-2 rounded-md text-sm font-medium bg-accent text-bg-primary hover:bg-accent-hover transition-colors"
        >
          <Plus size={16} />
          Create Task
        </button>
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
        <table className="w-full text-sm responsive-table">
          <thead>
            <tr className="border-b border-border bg-bg-tertiary/30">
              <th className="text-left px-4 py-3 font-medium text-text-secondary">
                ID
              </th>
              <th className="text-left px-4 py-3 font-medium text-text-secondary">
                Title
              </th>
              <th className="text-left px-4 py-3 font-medium text-text-secondary">
                Status
              </th>
              <th className="text-left px-4 py-3 font-medium text-text-secondary">
                Domain
              </th>
              <th className="text-left px-4 py-3 font-medium text-text-secondary">
                Depends On
              </th>
              <th className="text-left px-4 py-3 font-medium text-text-secondary">
                Actions
              </th>
            </tr>
          </thead>
          <tbody>
            {tasks.length === 0 ? (
              <tr>
                <td
                  colSpan={6}
                  className="px-4 py-8 text-center text-text-muted"
                >
                  No tasks yet. Create one to get started.
                </td>
              </tr>
            ) : (
              tasks.map((task) => (
                <tr
                  key={task.id}
                  className="border-b border-border last:border-b-0 hover:bg-bg-tertiary/20 transition-colors"
                >
                  <td data-label="ID" className="px-4 py-3 font-mono text-xs text-text-muted">
                    {task.id}
                  </td>
                  <td data-label="Title" className="px-4 py-3">{task.title}</td>
                  <td data-label="Status" className="px-4 py-3">
                    <StatusBadge status={task.status} />
                  </td>
                  <td data-label="Domain" className="px-4 py-3">
                    {task.domain && <DomainBadge domain={task.domain} />}
                  </td>
                  <td data-label="Depends On" className="px-4 py-3 text-xs text-text-muted">
                    {task.depends_on?.join(", ") || "--"}
                  </td>
                  <td data-label="Actions" className="px-4 py-3">
                    <TaskActions
                      taskId={task.id}
                      status={task.status}
                      onMutate={() => mutate()}
                    />
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
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
