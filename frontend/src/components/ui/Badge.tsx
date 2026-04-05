const statusColors = {
  todo: "bg-status-todo/20 text-status-todo",
  progress: "bg-status-progress/20 text-status-progress",
  done: "bg-status-done/20 text-status-done",
  blocked: "bg-status-blocked/20 text-status-blocked",
  skipped: "bg-status-skipped/20 text-status-skipped",
} as const;

interface BadgeProps {
  status: keyof typeof statusColors;
  label?: string;
  className?: string;
}

export default function Badge({ status, label, className = "" }: BadgeProps) {
  const displayLabel = label ?? status.charAt(0).toUpperCase() + status.slice(1);
  return (
    <span
      className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium ${statusColors[status]} ${className}`}
    >
      {displayLabel}
    </span>
  );
}
