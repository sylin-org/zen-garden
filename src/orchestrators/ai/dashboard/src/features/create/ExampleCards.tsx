import type { CatalogExample } from "../../api/types";

interface Props {
  examples: CatalogExample[];
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
            // Flatten the nested payload to dotted paths for form population
            const flat = flattenToDotted(ex.payload as Record<string, unknown>);
            onSelect(flat);
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

function flattenToDotted(
  obj: Record<string, unknown>,
  prefix = "",
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (value && typeof value === "object" && !Array.isArray(value)) {
      Object.assign(result, flattenToDotted(value as Record<string, unknown>, path));
    } else {
      result[path] = value;
    }
  }
  return result;
}
