interface Props {
  lines?: number;
  className?: string;
}

/** Animated loading skeleton. */
export default function Skeleton({ lines = 3, className }: Props) {
  return (
    <div className={`space-y-2 ${className ?? ""}`}>
      {Array.from({ length: lines }).map((_, i) => (
        <div
          key={i}
          className="h-3 bg-surface-3 rounded animate-pulse"
          style={{ width: `${70 + Math.random() * 30}%` }}
        />
      ))}
    </div>
  );
}

/** Card-shaped skeleton for grid layouts. */
export function SkeletonCard() {
  return (
    <div className="p-4 rounded-lg border border-border bg-surface animate-pulse">
      <div className="h-4 bg-surface-3 rounded w-2/3 mb-3" />
      <div className="h-3 bg-surface-3 rounded w-full mb-2" />
      <div className="h-3 bg-surface-3 rounded w-4/5" />
    </div>
  );
}
