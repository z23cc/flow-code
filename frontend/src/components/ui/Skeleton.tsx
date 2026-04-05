const variantClasses = {
  text: "h-4 w-full rounded",
  card: "h-32 w-full rounded-[var(--radius-md)]",
  circle: "rounded-full",
} as const;

interface SkeletonProps {
  variant?: keyof typeof variantClasses;
  width?: string | number;
  height?: string | number;
  className?: string;
}

export default function Skeleton({
  variant = "text",
  width,
  height,
  className = "",
}: SkeletonProps) {
  return (
    <div
      className={`bg-bg-tertiary animate-skeleton-pulse ${variantClasses[variant]} ${className}`}
      style={{ width, height }}
    />
  );
}
