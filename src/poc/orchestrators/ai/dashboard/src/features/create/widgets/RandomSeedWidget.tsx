import { useCallback } from "react";
import type { CatalogField } from "../../../api/types";

interface Props {
  field: CatalogField;
  value: number;
  onChange: (value: number) => void;
}

/**
 * Seed input paired with a dice button that generates a uniformly
 * random integer in `[field.min, field.max]`. Used for generative
 * seeds where the user wants reproducibility by default but one-click
 * randomization on demand.
 */
export default function RandomSeedWidget({ field, value, onChange }: Props) {
  const min = Number.isFinite(field.min) ? (field.min as number) : 0;
  const max = Number.isFinite(field.max) ? (field.max as number) : 0xffffffff;

  const reroll = useCallback(() => {
    onChange(randomInRange(min, max));
  }, [min, max, onChange]);

  return (
    <div>
      {field.label && (
        <label className="block text-[11px] text-text-dim font-medium mb-1">
          {field.label}
        </label>
      )}
      <div className="flex items-stretch gap-1.5">
        <input
          type="number"
          className="flex-1 px-2.5 py-2 bg-surface-2 border border-border rounded-md text-[12px]
                     text-text outline-none focus:border-accent font-mono"
          min={min}
          max={max}
          step={field.step ?? 1}
          value={Number.isFinite(value) ? value : 0}
          onChange={(e) => {
            const next = Number(e.target.value);
            if (Number.isFinite(next)) onChange(next);
          }}
        />
        <button
          type="button"
          onClick={reroll}
          title="Roll a new random seed"
          aria-label="Roll a new random seed"
          className="px-2.5 py-2 bg-surface-2 border border-border rounded-md
                     hover:border-accent hover:text-accent text-text-dim
                     transition-colors text-[14px] leading-none select-none"
        >
          <DiceIcon />
        </button>
      </div>
    </div>
  );
}

function DiceIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
      <circle cx="8" cy="8" r="1" fill="currentColor" />
      <circle cx="16" cy="8" r="1" fill="currentColor" />
      <circle cx="12" cy="12" r="1" fill="currentColor" />
      <circle cx="8" cy="16" r="1" fill="currentColor" />
      <circle cx="16" cy="16" r="1" fill="currentColor" />
    </svg>
  );
}

/**
 * Returns a uniformly random integer in `[min, max]` inclusive.
 * Uses `crypto.getRandomValues` when available for better entropy,
 * falling back to `Math.random` in environments without it.
 */
function randomInRange(min: number, max: number): number {
  const lo = Math.ceil(min);
  const hi = Math.floor(max);
  if (hi < lo) return lo;
  const span = hi - lo + 1;

  if (typeof crypto !== "undefined" && typeof crypto.getRandomValues === "function") {
    // Use a full u32 draw — sufficient for any realistic seed range.
    const buf = new Uint32Array(1);
    crypto.getRandomValues(buf);
    return lo + (buf[0] % span);
  }

  return lo + Math.floor(Math.random() * span);
}
