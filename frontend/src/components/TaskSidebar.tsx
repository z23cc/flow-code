import { useEffect, useCallback } from "react";
import { Link } from "react-router-dom";
import useSWR from "swr";
import { X, Play, Ban, RotateCcw, SkipForward, Clock, GitBranch } from "lucide-react";
import { toast } from "sonner";
import { apiPost, swrFetcher } from "../lib/api";
import { useTaskSidebar } from "./TaskSidebarContext";

interface TaskDetail {
  id: string;
  title: string;
  status: string;
  epic_id?: string;
  description?: string;
  depends_on?: string[];
  duration_seconds?: number;
  domain?: string;
  evidence?: Record<string, unknown>;
}

const STATUS_COLORS: Record<string, string> = {
  todo: "bg-info/20 text-info",
  in_progress: "bg-accent/20 text-accent",
  done: "bg-success/20 text-success",
  blocked: "bg-error/20 text-error",
  skipped: "bg-text-muted/20 text-text-muted",
  failed: "bg-error/20 text-error",
};

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}

export default function TaskSidebar() {
  const { isOpen, taskId, close } = useTaskSidebar();

  const { data: task, mutate } = useSWR<TaskDetail>(
    isOpen && taskId ? `/tasks/${taskId}` : null,
    swrFetcher,
  );

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape" && isOpen) close();
    },
    [isOpen, close],
  );

  useEffect(() => {
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  const runAction = async (action: string) => {
    if (!taskId) return;
    try {
      await apiPost(`/tasks/${taskId}/${action}`, {});
      toast.success(`Task ${action} successful`);
      mutate();
    } catch (err) {
      toast.error(`Failed to ${action} task: ${err instanceof Error ? err.message : "unknown error"}`);
    }
  };

  const btn = "flex items-center gap-1.5 px-3 py-1.5 rounded text-xs font-medium transition-colors cursor-pointer";

  return (
    <>
      {/* Backdrop for mobile */}
      {isOpen && (
        <div
          className="fixed inset-0 bg-black/50 z-40 md:hidden"
          onClick={close}
        />
      )}

      {/* Sidebar drawer */}
      <div
        className={`fixed inset-y-0 right-0 z-50 bg-bg-secondary border-l border-border shadow-xl flex flex-col transition-transform duration-200 ease-in-out
          w-full max-w-[100vw] md:w-[400px] md:max-w-[400px]
          ${isOpen ? "translate-x-0" : "translate-x-full"}`}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-3 border-b border-border shrink-0">
          <h2 className="text-sm font-semibold text-text-primary truncate">
            {task?.title ?? "Loading..."}
          </h2>
          <button
            onClick={close}
            aria-label="Close task details"
            className="p-1 rounded hover:bg-bg-tertiary text-text-muted hover:text-text-primary transition-colors min-h-[44px] min-w-[44px] flex items-center justify-center"
          >
            <X size={18} />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-auto p-5 space-y-5">
          {task ? (
            <>
              {/* Status + Domain */}
              <div className="flex items-center gap-2 flex-wrap">
                <span
                  className={`inline-block rounded-full px-2.5 py-0.5 text-xs font-medium ${STATUS_COLORS[task.status] ?? "bg-bg-tertiary text-text-secondary"}`}
                >
                  {task.status.replace("_", " ")}
                </span>
                {task.domain && (
                  <span className="inline-block rounded-full px-2.5 py-0.5 text-xs font-medium bg-bg-tertiary text-text-secondary">
                    {task.domain}
                  </span>
                )}
              </div>

              {/* Epic link */}
              {task.epic_id && (
                <div className="text-sm">
                  <span className="text-text-muted">Epic: </span>
                  <Link
                    to={`/epic/${task.epic_id}`}
                    className="text-accent hover:underline"
                    onClick={close}
                  >
                    {task.epic_id}
                  </Link>
                </div>
              )}

              {/* Description */}
              {task.description && (
                <div>
                  <h3 className="text-xs font-medium text-text-muted mb-1.5">Description</h3>
                  <p className="text-sm text-text-secondary whitespace-pre-wrap">{task.description}</p>
                </div>
              )}

              {/* Dependencies */}
              {task.depends_on && task.depends_on.length > 0 && (
                <div>
                  <h3 className="text-xs font-medium text-text-muted mb-1.5 flex items-center gap-1">
                    <GitBranch size={12} /> Dependencies
                  </h3>
                  <ul className="space-y-1">
                    {task.depends_on.map((dep) => (
                      <li key={dep} className="text-sm text-text-secondary">
                        {dep}
                      </li>
                    ))}
                  </ul>
                </div>
              )}

              {/* Duration */}
              {task.duration_seconds != null && task.duration_seconds > 0 && (
                <div className="flex items-center gap-1.5 text-sm text-text-secondary">
                  <Clock size={14} className="text-text-muted" />
                  {formatDuration(task.duration_seconds)}
                </div>
              )}

              {/* Evidence */}
              {task.evidence && Object.keys(task.evidence).length > 0 && (
                <div>
                  <h3 className="text-xs font-medium text-text-muted mb-1.5">Evidence</h3>
                  <pre className="text-xs text-text-secondary bg-bg-primary rounded p-3 overflow-auto max-h-48">
                    {JSON.stringify(task.evidence, null, 2)}
                  </pre>
                </div>
              )}

              {/* Action buttons */}
              <div className="pt-2 border-t border-border">
                <h3 className="text-xs font-medium text-text-muted mb-2">Actions</h3>
                <div className="flex flex-wrap gap-2">
                  {(task.status === "todo" || task.status === "blocked") && (
                    <button
                      className={`${btn} bg-success/20 text-success hover:bg-success/30`}
                      onClick={() => runAction("start")}
                    >
                      <Play size={12} /> Start
                    </button>
                  )}
                  {task.status === "in_progress" && (
                    <button
                      className={`${btn} bg-error/20 text-error hover:bg-error/30`}
                      onClick={() => runAction("block")}
                    >
                      <Ban size={12} /> Block
                    </button>
                  )}
                  {(task.status === "blocked" || task.status === "failed") && (
                    <button
                      className={`${btn} bg-warning/20 text-warning hover:bg-warning/30`}
                      onClick={() => runAction("restart")}
                    >
                      <RotateCcw size={12} /> Restart
                    </button>
                  )}
                  {task.status !== "done" && task.status !== "skipped" && (
                    <button
                      className={`${btn} bg-text-muted/20 text-text-muted hover:bg-text-muted/30`}
                      onClick={() => runAction("skip")}
                    >
                      <SkipForward size={12} /> Skip
                    </button>
                  )}
                </div>
              </div>
            </>
          ) : (
            <div className="space-y-3">
              <div className="h-4 w-24 bg-bg-tertiary rounded animate-pulse" />
              <div className="h-3 w-full bg-bg-tertiary rounded animate-pulse" />
              <div className="h-3 w-3/4 bg-bg-tertiary rounded animate-pulse" />
            </div>
          )}
        </div>
      </div>
    </>
  );
}
