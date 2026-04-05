import { useCallback } from "react";

const API_BASE = "/api/v1/tasks";

interface TaskActionsProps {
  taskId: string;
  status: string;
  onMutate: () => void;
}

async function postAction(endpoint: string, body: Record<string, string>) {
  const res = await fetch(`${API_BASE}/${endpoint}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`Failed to ${endpoint} task`);
  return res.json();
}

export default function TaskActions({ taskId, status, onMutate }: TaskActionsProps) {
  const act = useCallback(
    async (endpoint: string) => {
      await postAction(endpoint, { id: taskId });
      onMutate();
    },
    [taskId, onMutate],
  );

  const btn =
    "px-2.5 py-1 rounded text-xs font-medium transition-colors cursor-pointer";

  switch (status) {
    case "todo":
      return (
        <button
          className={`${btn} bg-success/20 text-success hover:bg-success/30`}
          onClick={() => act("start")}
        >
          Start
        </button>
      );
    case "in_progress":
      return (
        <div className="flex gap-1.5">
          <button
            className={`${btn} bg-success/20 text-success hover:bg-success/30`}
            onClick={() => act("done")}
          >
            Done
          </button>
          <button
            className={`${btn} bg-error/20 text-error hover:bg-error/30`}
            onClick={() => act("block")}
          >
            Block
          </button>
        </div>
      );
    case "blocked":
    case "failed":
      return (
        <button
          className={`${btn} bg-warning/20 text-warning hover:bg-warning/30`}
          onClick={() => act("restart")}
        >
          Restart
        </button>
      );
    default:
      return <span className="text-xs text-text-muted">--</span>;
  }
}
