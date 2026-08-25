import type { CatalogField } from "../../../api/types";

interface Props {
  field: CatalogField;
  value: boolean;
  onChange: (value: boolean) => void;
}

export default function ToggleWidget({ field, value, onChange }: Props) {
  return (
    <div className="flex items-center justify-between">
      {field.label && (
        <label className="text-[11px] text-text-dim font-medium">
          {field.label}
        </label>
      )}
      <button
        type="button"
        role="switch"
        aria-checked={value}
        onClick={() => onChange(!value)}
        className={[
          "relative w-9 h-5 rounded-full transition-colors",
          value ? "bg-accent" : "bg-surface-3",
        ].join(" ")}
      >
        <span
          className={[
            "absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform",
            value ? "translate-x-[18px]" : "translate-x-0.5",
          ].join(" ")}
        />
      </button>
    </div>
  );
}
