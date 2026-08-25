import type { CatalogField } from "../../../api/types";

interface Props {
  field: CatalogField;
  value: number;
  onChange: (value: number) => void;
}

export default function NumberWidget({ field, value, onChange }: Props) {
  return (
    <div>
      {field.label && (
        <label className="block text-[11px] text-text-dim font-medium mb-1">
          {field.label}
        </label>
      )}
      <input
        type="number"
        className="w-full px-2.5 py-2 bg-surface-2 border border-border rounded-md text-[12px]
                   text-text outline-none focus:border-accent"
        min={field.min}
        max={field.max}
        step={field.step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </div>
  );
}
