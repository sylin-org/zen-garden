import type { CatalogField } from "../../../api/types";

interface Props {
  field: CatalogField;
  value: unknown;
  onChange: (value: unknown) => void;
}

export default function SelectWidget({ field, value, onChange }: Props) {
  const options = field.options ?? [];
  const autoDefault = field.auto?.default;

  return (
    <div>
      {field.label && (
        <label className="block text-[11px] text-text-dim font-medium mb-1">
          {field.label}
        </label>
      )}
      <select
        className="w-full px-2.5 py-2 bg-surface-2 border border-border rounded-md text-[12px]
                   text-text outline-none focus:border-accent"
        value={String(value ?? "")}
        onChange={(e) => {
          const raw = e.target.value;
          if (raw === "__auto__") {
            onChange(undefined);
            return;
          }
          // Attempt numeric parse if options are numeric
          const num = Number(raw);
          onChange(isNaN(num) ? raw : num);
        }}
      >
        {autoDefault && (
          <option value="__auto__">
            Auto ({field.auto?.description ?? autoDefault})
          </option>
        )}
        {options.map((opt) => (
          <option key={String(opt)} value={String(opt)}>
            {String(opt)}
          </option>
        ))}
      </select>
    </div>
  );
}
