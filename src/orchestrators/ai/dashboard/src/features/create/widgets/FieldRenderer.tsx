import type { CatalogField } from "../../../api/types";
import TextareaWidget from "./TextareaWidget";
import SliderWidget from "./SliderWidget";
import SelectWidget from "./SelectWidget";
import NumberWidget from "./NumberWidget";
import ToggleWidget from "./ToggleWidget";

interface Props {
  field: CatalogField;
  value: unknown;
  onChange: (value: unknown) => void;
}

/** Renders the appropriate widget for a catalog field descriptor. */
export default function FieldRenderer({ field, value, onChange }: Props) {
  const widget = field.widget ?? inferWidget(field);

  switch (widget) {
    case "textarea":
      return (
        <TextareaWidget
          field={field}
          value={(value as string) ?? ""}
          onChange={onChange}
        />
      );
    case "slider":
      return (
        <SliderWidget
          field={field}
          value={(value as number) ?? field.default ?? field.min ?? 0}
          onChange={onChange}
        />
      );
    case "select":
      return (
        <SelectWidget
          field={field}
          value={value}
          onChange={onChange}
        />
      );
    case "number":
      return (
        <NumberWidget
          field={field}
          value={(value as number) ?? field.default ?? 0}
          onChange={onChange}
        />
      );
    case "toggle":
      return (
        <ToggleWidget
          field={field}
          value={(value as boolean) ?? false}
          onChange={onChange}
        />
      );
    case "hidden":
      return null;
    case "file":
      // File widgets are handled separately via media_inputs
      return null;
    default:
      // Fallback: render as textarea
      return (
        <TextareaWidget
          field={field}
          value={String(value ?? "")}
          onChange={onChange}
        />
      );
  }
}

function inferWidget(field: CatalogField): string {
  if (field.options && field.options.length > 0) return "select";
  if (field.field_type === "boolean") return "toggle";
  if (field.field_type === "number" || field.field_type === "integer") {
    if (field.min !== undefined && field.max !== undefined) return "slider";
    return "number";
  }
  return "textarea";
}
