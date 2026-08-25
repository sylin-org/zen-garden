import type { CatalogField } from "../../../api/types";

interface Props {
  field: CatalogField;
  value: string;
  onChange: (value: string) => void;
}

export default function TextareaWidget({ field, value, onChange }: Props) {
  return (
    <div>
      {field.label && (
        <label className="block text-[11px] text-text-dim font-medium mb-1">
          {field.label}
          {field.required && <span className="text-red ml-1">*</span>}
        </label>
      )}
      <textarea
        className="w-full p-3 bg-surface-2 border border-border rounded-lg text-[13px] text-text
                   placeholder:text-text-dimmer outline-none focus:border-accent transition-colors
                   resize-y min-h-[80px]"
        placeholder={field.placeholder}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        rows={field.required ? 4 : 2}
      />
    </div>
  );
}
