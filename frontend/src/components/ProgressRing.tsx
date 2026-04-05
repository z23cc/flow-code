interface ProgressRingProps {
  size?: number;
  percentage: number;
  strokeWidth?: number;
  color?: string;
  trackColor?: string;
}

export default function ProgressRing({
  size = 120,
  percentage,
  strokeWidth = 8,
  color = "var(--color-accent)",
  trackColor = "var(--color-bg-tertiary)",
}: ProgressRingProps) {
  const radius = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const clamped = Math.min(100, Math.max(0, percentage));
  const offset = circumference - (clamped / 100) * circumference;

  return (
    <svg width={size} height={size} className="block">
      <circle
        cx={size / 2}
        cy={size / 2}
        r={radius}
        fill="none"
        stroke={trackColor}
        strokeWidth={strokeWidth}
      />
      <circle
        cx={size / 2}
        cy={size / 2}
        r={radius}
        fill="none"
        stroke={color}
        strokeWidth={strokeWidth}
        strokeDasharray={circumference}
        strokeDashoffset={offset}
        strokeLinecap="round"
        transform={`rotate(-90 ${size / 2} ${size / 2})`}
        className="transition-[stroke-dashoffset] duration-500"
      />
      <text
        x="50%"
        y="50%"
        dominantBaseline="central"
        textAnchor="middle"
        className="fill-text-primary text-2xl font-bold"
        style={{ fontSize: size * 0.22 }}
      >
        {Math.round(clamped)}%
      </text>
    </svg>
  );
}
