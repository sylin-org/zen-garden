import type { WorkspaceExample } from "../../api/types";

interface Props {
  examples: WorkspaceExample[];
  onSelect: (payload: Record<string, unknown>) => void;
  /** Hide cards when the form already has user input. */
  hidden?: boolean;
}

const MAX_VISIBLE = 3;

export default function ExampleCards({ examples, onSelect, hidden }: Props) {
  if (examples.length === 0 || hidden) return null;

  const visible = examples.slice(0, MAX_VISIBLE);

  return (
    <div className="flex gap-2 mb-4">
      {visible.map((ex, i) => (
        <button
          key={i}
          onClick={() => {
            // Pass the example payload directly — the form deep-merges it.
            onSelect(ex.payload);
          }}
          className="flex-1 p-3 rounded-lg bg-surface-2 border border-border
                     hover:border-accent hover:bg-accent-bg transition-colors
                     text-left group min-w-0"
        >
          <div className="text-[11px] font-medium text-text-dim group-hover:text-accent truncate">
            {ex.label}
          </div>
          {ex.description && (
            <div className="text-[10px] text-text-dimmer mt-0.5 truncate">
              {ex.description}
            </div>
          )}
        </button>
      ))}
      {examples.length > MAX_VISIBLE && (
        <div className="flex items-center text-[10px] text-text-dimmer px-2">
          +{examples.length - MAX_VISIBLE} more
        </div>
      )}
    </div>
  );
}

