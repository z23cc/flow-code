import { useMemo } from "react";

interface WaveTimelineProps {
  nodes: Array<{ id: string; layer: number; status: string }>;
}

const WAVE_COLORS = [
  "var(--color-status-progress)",
  "var(--color-success)",
  "var(--color-warning)",
  "var(--color-info)",
  "#a855f7",
  "#ec4899",
];

export default function WaveTimeline({ nodes }: WaveTimelineProps) {
  const waves = useMemo(() => {
    if (nodes.length === 0) return [];
    const grouped = new Map<number, string[]>();
    for (const n of nodes) {
      const list = grouped.get(n.layer) ?? [];
      list.push(n.id);
      grouped.set(n.layer, list);
    }
    const sorted = Array.from(grouped.entries()).sort((a, b) => a[0] - b[0]);
    return sorted.map(([idx, ids]) => ({ wave: idx + 1, taskIds: ids }));
  }, [nodes]);

  if (waves.length === 0) return null;

  return (
    <div className="mt-4 px-2">
      <h3 className="text-sm font-medium text-text-secondary mb-2">
        Wave Timeline
      </h3>
      <div className="flex items-center gap-0 overflow-x-auto">
        {waves.map((w, i) => (
          <div key={w.wave} className="flex items-center">
            {i > 0 && (
              <div className="flex flex-col items-center mx-1">
                <div
                  className="w-px h-6"
                  style={{ background: "var(--color-border)" }}
                />
                <span className="text-[10px] text-text-muted whitespace-nowrap">
                  CP
                </span>
                <div
                  className="w-px h-6"
                  style={{ background: "var(--color-border)" }}
                />
              </div>
            )}
            <div
              className="rounded-md px-3 py-2 min-w-[80px]"
              style={{
                background: WAVE_COLORS[i % WAVE_COLORS.length],
                opacity: 0.85,
              }}
            >
              <div className="text-xs font-bold text-white mb-0.5">
                Wave {w.wave}
              </div>
              <div className="text-[10px] text-white/80 leading-tight">
                {w.taskIds.join(", ")}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
