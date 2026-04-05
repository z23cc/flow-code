import { useEffect, useState } from "react";
import useSWR from "swr";
import { toast } from "sonner";
import { swrFetcher, apiPost, ApiRequestError } from "../lib/api";
import { connectEvents } from "../lib/ws";

interface Approval {
  id: string;
  task_id: string;
  kind: "file_access" | "mutation" | "generic";
  payload: unknown;
  status: "pending" | "approved" | "rejected";
  created_at: number;
  resolved_at: number | null;
  resolver: string | null;
  reason: string | null;
}

function kindBadge(kind: string): { color: string; label: string } {
  switch (kind) {
    case "file_access":
      return { color: "bg-info/20 text-info", label: "File access" };
    case "mutation":
      return { color: "bg-purple-500/20 text-purple-400", label: "Mutation" };
    case "generic":
      return { color: "bg-bg-tertiary text-text-secondary", label: "Generic" };
    default:
      return { color: "bg-bg-tertiary text-text-muted", label: kind };
  }
}

function formatTimestamp(epochSeconds: number): string {
  return new Date(epochSeconds * 1000).toLocaleString();
}

export default function Approvals() {
  const { data, error, isLoading, mutate } = useSWR<Approval[]>(
    "/approvals?status=pending",
    swrFetcher,
    { refreshInterval: 5000 },
  );

  const [rejectingId, setRejectingId] = useState<string | null>(null);
  const [rejectReason, setRejectReason] = useState("");
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  useEffect(() => {
    const conn = connectEvents();
    const refresh = () => {
      mutate();
    };
    conn.on("ApprovalCreated", refresh);
    conn.on("ApprovalResolved", refresh);
    return () => conn.close();
  }, [mutate]);

  const approvals = data ?? [];

  async function handleApprove(id: string) {
    setBusyId(id);
    try {
      await apiPost(`/approvals/${id}/approve`, {});
      toast.success(`Approved ${id}`);
      mutate();
    } catch (e) {
      const msg =
        e instanceof ApiRequestError ? e.serverMessage : String(e);
      toast.error(`Approve failed: ${msg}`);
    } finally {
      setBusyId(null);
    }
  }

  async function handleReject(id: string) {
    setBusyId(id);
    try {
      await apiPost(`/approvals/${id}/reject`, {
        reason: rejectReason || null,
      });
      toast.success(`Rejected ${id}`);
      setRejectingId(null);
      setRejectReason("");
      mutate();
    } catch (e) {
      const msg =
        e instanceof ApiRequestError ? e.serverMessage : String(e);
      toast.error(`Reject failed: ${msg}`);
    } finally {
      setBusyId(null);
    }
  }

  return (
    <div className="flex flex-col h-full gap-4">
      <h1 className="text-2xl font-bold">Approvals</h1>

      {isLoading && <p className="text-text-muted text-sm">Loading...</p>}
      {error && (
        <p className="text-error text-sm">Failed to load approvals.</p>
      )}

      {!isLoading && approvals.length === 0 && (
        <div className="flex-1 flex flex-col items-center justify-center gap-3 text-center">
          <div className="text-4xl opacity-40">OK</div>
          <p className="text-text-primary text-sm font-medium">
            No pending approvals
          </p>
          <p className="text-text-muted text-xs max-w-md">
            Workers will request approval here when they need access to files
            outside their owned set or want to mutate the task DAG.
          </p>
        </div>
      )}

      <div className="flex-1 overflow-auto space-y-3">
        {approvals.map((approval) => {
          const badge = kindBadge(approval.kind);
          const isExpanded = expandedId === approval.id;
          const isRejecting = rejectingId === approval.id;
          const isBusy = busyId === approval.id;
          return (
            <div
              key={approval.id}
              className="rounded-md border border-border bg-bg-secondary p-4 space-y-3"
            >
              <div className="flex items-center gap-2 flex-wrap">
                <span
                  className={`px-2 py-0.5 rounded text-xs font-medium ${badge.color}`}
                >
                  {badge.label}
                </span>
                <span className="px-2 py-0.5 rounded text-xs font-medium bg-accent/20 text-accent">
                  {approval.task_id}
                </span>
                <span className="text-xs text-text-muted ml-auto">
                  {formatTimestamp(approval.created_at)}
                </span>
              </div>

              <div>
                <button
                  type="button"
                  onClick={() =>
                    setExpandedId(isExpanded ? null : approval.id)
                  }
                  className="text-xs text-text-muted hover:text-text-primary"
                >
                  {isExpanded ? "Hide" : "Show"} payload
                </button>
                {isExpanded && (
                  <pre className="mt-2 p-2 rounded bg-bg-tertiary text-xs text-text-primary overflow-auto">
                    {JSON.stringify(approval.payload, null, 2)}
                  </pre>
                )}
              </div>

              {isRejecting ? (
                <div className="space-y-2">
                  <textarea
                    value={rejectReason}
                    onChange={(e) => setRejectReason(e.target.value)}
                    placeholder="Reason (optional)..."
                    className="w-full px-3 py-2 rounded-md bg-bg-tertiary border border-border text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-border-accent"
                    rows={2}
                  />
                  <div className="flex gap-2">
                    <button
                      type="button"
                      disabled={isBusy}
                      onClick={() => handleReject(approval.id)}
                      className="px-3 py-1.5 rounded-md bg-error/20 text-error text-sm font-medium hover:bg-error/30 disabled:opacity-50"
                    >
                      Confirm reject
                    </button>
                    <button
                      type="button"
                      disabled={isBusy}
                      onClick={() => {
                        setRejectingId(null);
                        setRejectReason("");
                      }}
                      className="px-3 py-1.5 rounded-md bg-bg-tertiary text-text-secondary text-sm hover:bg-bg-tertiary/70 disabled:opacity-50"
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              ) : (
                <div className="flex gap-2">
                  <button
                    type="button"
                    disabled={isBusy}
                    onClick={() => handleApprove(approval.id)}
                    className="px-3 py-1.5 rounded-md bg-success/20 text-success text-sm font-medium hover:bg-success/30 disabled:opacity-50"
                  >
                    Approve
                  </button>
                  <button
                    type="button"
                    disabled={isBusy}
                    onClick={() => {
                      setRejectingId(approval.id);
                      setRejectReason("");
                    }}
                    className="px-3 py-1.5 rounded-md bg-error/20 text-error text-sm font-medium hover:bg-error/30 disabled:opacity-50"
                  >
                    Reject
                  </button>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
