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

type TaskNodeData = {
  title: string;
  status: string;
  domain?: string;
};

function TaskNode({ data }: NodeProps) {
  const { title, status, domain } = data as TaskNodeData;
  const color = STATUS_COLORS[status] ?? STATUS_COLORS.todo;
  const icon = STATUS_ICONS[status] ?? STATUS_ICONS.todo;
  const label = title.length > 20 ? title.slice(0, 20) + "\u2026" : title;

  return (
    <>
      <Handle type="target" position={Position.Top} />
      <div
        className="rounded-lg border px-3 py-2 shadow-sm bg-bg-primary min-w-[140px] text-center"
        style={{ borderColor: color, borderWidth: 2 }}
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
