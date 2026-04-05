import { memo } from "react";
import { Handle, Position, type NodeProps } from "@xyflow/react";

const STATUS_COLORS: Record<string, string> = {
  done: "#10b981",
  in_progress: "#f59e0b",
  todo: "#6b7280",
  blocked: "#ef4444",
};

const STATUS_ICONS: Record<string, string> = {
  done: "\u2713",
  in_progress: "\u25b6",
  todo: "\u25cb",
  blocked: "\u2715",
};

export type TaskNodeData = {
  title: string;
  status: string;
  domain?: string;
  estimated_seconds?: number | null;
};

/** Map estimated_seconds to node width: null/0 -> 120px, min 80px, max 160px */
function nodeWidth(est: number | null | undefined): number {
  if (est == null || est <= 0) return 120;
  // Scale: 60s -> 80px, 300s -> 120px, 600s+ -> 160px
  const clamped = Math.max(80, Math.min(160, 80 + (est / 600) * 80));
  return Math.round(clamped);
}

function TaskNode({ data }: NodeProps) {
  const { title, status, domain, estimated_seconds } = data as TaskNodeData;
  const color = STATUS_COLORS[status] ?? STATUS_COLORS.todo;
  const icon = STATUS_ICONS[status] ?? STATUS_ICONS.todo;
  const label = title.length > 20 ? title.slice(0, 20) + "\u2026" : title;
  const width = nodeWidth(estimated_seconds);
  const isInProgress = status === "in_progress";

  return (
    <>
      <Handle type="target" position={Position.Top} />
      <div
        className={`rounded-lg border px-3 py-2 shadow-sm bg-bg-primary text-center ${isInProgress ? "animate-node-pulse" : ""}`}
        style={{ borderColor: color, borderWidth: 2, width: `${width}px` }}
      >
        <div className="flex items-center gap-1.5 justify-center">
          <span style={{ color }} className="text-sm font-bold">
            {icon}
          </span>
          <span className="text-sm font-medium text-text-primary truncate">
            {label}
          </span>
        </div>
        {domain && (
          <span className="text-xs text-text-secondary">{domain}</span>
        )}
      </div>
      <Handle type="source" position={Position.Bottom} />
    </>
  );
}

export default memo(TaskNode);
