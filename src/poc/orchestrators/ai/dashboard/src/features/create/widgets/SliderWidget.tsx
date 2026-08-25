import type { CatalogField } from "../../../api/types";

interface Props {
  field: CatalogField;
  value: number;
  onChange: (value: number) => void;
}

export default function SliderWidget({ field, value, onChange }: Props) {
  const min = field.min ?? 0;
  const max = field.max ?? 100;
  const step = field.step ?? 1;

  return (
    <div>
      <label className="flex justify-between text-[11px] text-text-dim font-medium mb-1">
        <span>{field.label ?? field.field}</span>
        <span className="text-accent font-semibold">{value}</span>
      </label>
      <input
        type="range"
        className="w-full accent-accent h-1"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </div>
  );
}
