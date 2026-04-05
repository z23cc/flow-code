import { useEffect } from "react";
import useSWR from "swr";
import { X } from "lucide-react";
import { swrFetcher } from "../lib/api";

interface Evidence {
  commits?: string[];
  tests?: string[];
  prs?: string[];
  files_changed?: number;
  insertions?: number;
  deletions?: number;
  review_iterations?: number;
  workspace_changes?: unknown;
}

interface TaskDetail {
  id: string;
  title: string;
  status: string;
  domain?: string;
  duration_seconds?: number | null;
  evidence?: Evidence | null;
}

interface EvidenceModalProps {
  taskId: string | null;
  open: boolean;
  onClose: () => void;
}

function formatDuration(seconds?: number | null): string {
  if (!seconds && seconds !== 0) return "--";
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}

export default function EvidenceModal({ taskId, open, onClose }: EvidenceModalProps) {
  const { data, error, isLoading } = useSWR<TaskDetail>(
    open && taskId ? `/tasks/${taskId}` : null,
    swrFetcher,
  );

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;

  const evidence = data?.evidence ?? null;
  const hasEvidence =
    evidence &&
    ((evidence.commits?.length ?? 0) > 0 ||
      (evidence.tests?.length ?? 0) > 0 ||
      (evidence.files_changed ?? 0) > 0 ||
      evidence.workspace_changes);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      onClick={onClose}
    >
      <div
        className="relative max-h-[85vh] w-full max-w-2xl overflow-y-auto rounded-lg border border-border bg-bg-secondary shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="sticky top-0 flex items-center justify-between border-b border-border bg-bg-secondary px-4 py-3">
          <div className="min-w-0">
            <h2 className="truncate text-base font-semibold text-text-primary">
              {data?.title ?? "Task details"}
            </h2>
            <p className="mt-0.5 truncate font-mono text-xs text-text-muted">
              {taskId}
            </p>
          </div>
          <button
            onClick={onClose}
            aria-label="Close"
            className="ml-3 rounded p-1 text-text-muted hover:bg-bg-tertiary hover:text-text-primary transition-colors"
          >
            <X size={18} />
          </button>
        </div>

        {/* Body */}
        <div className="p-4">
          {isLoading && (
            <div className="py-8 text-center text-sm text-text-muted">Loading…</div>
          )}
          {error && (
            <div className="rounded-md border border-error/40 bg-error/10 p-3 text-sm text-error">
              Failed to load task details.
            </div>
          )}
          {data && !isLoading && !error && (
            <div className="space-y-4">
              {/* Summary row */}
              <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
                <div>
                  <p className="text-xs uppercase tracking-wide text-text-muted">Status</p>
                  <p className="mt-0.5 text-sm font-medium text-text-primary">
                    {data.status.replace("_", " ")}
                  </p>
                </div>
                <div>
                  <p className="text-xs uppercase tracking-wide text-text-muted">Duration</p>
                  <p className="mt-0.5 font-mono text-sm text-text-primary">
                    {formatDuration(data.duration_seconds)}
                  </p>
                </div>
                {data.domain && (
                  <div>
                    <p className="text-xs uppercase tracking-wide text-text-muted">Domain</p>
                    <p className="mt-0.5 text-sm text-text-primary">{data.domain}</p>
                  </div>
                )}
              </div>

              {/* Evidence details */}
              {!hasEvidence ? (
                <div className="rounded-md border border-border bg-bg-tertiary p-3 text-sm text-text-muted">
                  No evidence recorded for this task yet.
                </div>
              ) : (
                <div className="space-y-3">
                  {(evidence?.files_changed != null ||
                    evidence?.insertions != null ||
                    evidence?.deletions != null) && (
                    <div>
                      <p className="text-xs uppercase tracking-wide text-text-muted mb-1">
                        Diff stats
                      </p>
                      <div className="flex gap-4 text-sm font-mono">
                        {evidence?.files_changed != null && (
                          <span className="text-text-primary">
                            {evidence.files_changed} files
                          </span>
                        )}
                        {evidence?.insertions != null && (
                          <span className="text-success">+{evidence.insertions}</span>
                        )}
                        {evidence?.deletions != null && (
                          <span className="text-error">-{evidence.deletions}</span>
                        )}
                      </div>
                    </div>
                  )}

                  {evidence?.commits && evidence.commits.length > 0 && (
                    <div>
                      <p className="text-xs uppercase tracking-wide text-text-muted mb-1">
                        Commits
                      </p>
                      <ul className="space-y-0.5">
                        {evidence.commits.map((c) => (
                          <li key={c} className="font-mono text-xs text-text-primary">
                            {c}
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}

                  {evidence?.tests && evidence.tests.length > 0 && (
                    <div>
                      <p className="text-xs uppercase tracking-wide text-text-muted mb-1">
                        Tests run
                      </p>
                      <ul className="space-y-0.5">
                        {evidence.tests.map((t) => (
                          <li key={t} className="font-mono text-xs text-text-primary">
                            {t}
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}

                  {evidence?.review_iterations != null && (
                    <div>
                      <p className="text-xs uppercase tracking-wide text-text-muted mb-1">
                        Review iterations
                      </p>
                      <p className="text-sm text-text-primary">{evidence.review_iterations}</p>
                    </div>
                  )}

                  {evidence?.workspace_changes != null && (
                    <div>
                      <p className="text-xs uppercase tracking-wide text-text-muted mb-1">
                        Workspace changes
                      </p>
                      <pre className="overflow-x-auto rounded-md bg-bg-tertiary p-2 text-xs text-text-primary">
                        {JSON.stringify(evidence.workspace_changes, null, 2)}
                      </pre>
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
