import type { LucideIcon } from "lucide-react";

interface StatsCardProps {
  icon: LucideIcon;
  value: string | number;
  label: string;
  trend?: string;
}

export default function StatsCard({
  icon: Icon,
  value,
  label,
  trend,
}: StatsCardProps) {
  return (
    <div className="rounded-lg border border-border bg-bg-secondary p-5 flex items-start gap-4">
      <div className="rounded-md bg-accent/10 p-2.5 text-accent">
        <Icon size={22} />
      </div>
      <div className="flex-1 min-w-0">
        <p className="text-2xl font-bold tracking-tight">{value}</p>
        <p className="text-sm text-text-secondary mt-0.5">{label}</p>
        {trend && (
          <p className="text-xs text-text-muted mt-1">{trend}</p>
        )}
      </div>
    </div>
  );
}
