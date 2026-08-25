import type { WorkspaceExample } from "../../api/types";

interface Props {
  examples: WorkspaceExample[];
  onSelect: (payload: Record<string, unknown>) => void;
  hidden?: boolean;
}

export default function ExampleCards({ examples, onSelect, hidden }: Props) {
  if (examples.length === 0 || hidden) return null;

  return (
    <div className="mt-4 pt-3 border-t border-border">
      <div className="text-[10px] uppercase tracking-wider text-text-dimmer font-semibold mb-2">
        Try an example
      </div>
      <div className="flex flex-wrap gap-2">
        {examples.map((ex, i) => (
          <button
            key={i}
            onClick={() => onSelect(ex.payload)}
            title={ex.description}
            className="px-3 py-1.5 rounded-full bg-surface-2 border border-border
                       hover:border-accent hover:bg-accent-bg hover:text-accent
                       transition-colors text-[11px] text-text-dim"
          >
            {ex.label}
          </button>
        ))}
      </div>
    </div>
  );
}
