import type { CatalogField } from "../../../api/types";
import TextareaWidget from "./TextareaWidget";
import SliderWidget from "./SliderWidget";
import SelectWidget from "./SelectWidget";
import NumberWidget from "./NumberWidget";
import ToggleWidget from "./ToggleWidget";
import DialogueWidget from "./DialogueWidget";
import RandomSeedWidget from "./RandomSeedWidget";

interface Props {
  field: CatalogField;
  value: unknown;
  onChange: (value: unknown) => void;
  streamingText?: string;
}

export default function FieldRenderer({ field, value, onChange, streamingText }: Props) {
  const widget = field.widget ?? inferWidget(field);

  switch (widget) {
    case "textarea":
      return <TextareaWidget field={field} value={(value as string) ?? ""} onChange={onChange} />;
    case "slider":
      return <SliderWidget field={field} value={(value as number) ?? field.min ?? 0} onChange={onChange} />;
    case "select":
      return <SelectWidget field={field} value={value} onChange={onChange} />;
    case "number":
      return <NumberWidget field={field} value={(value as number) ?? 0} onChange={onChange} />;
    case "random_seed":
      return (
        <RandomSeedWidget
          field={field}
          value={(value as number) ?? field.min ?? 0}
          onChange={onChange}
        />
      );
    case "toggle":
      return <ToggleWidget field={field} value={(value as boolean) ?? false} onChange={onChange} />;
    case "dialogue":
      return (
        <DialogueWidget
          value={(value as { user: string; assistant: string }[]) ?? []}
          streamingText={streamingText}
          onChange={onChange}
        />
      );
    case "hidden":
      return null;
    case "file":
      return null;
    default:
      return <TextareaWidget field={field} value={String(value ?? "")} onChange={onChange} />;
  }
}

function inferWidget(field: CatalogField): string {
  if (field.field_type === "dialogue") return "dialogue";
  if (field.options && field.options.length > 0) return "select";
  if (field.field_type === "boolean") return "toggle";
  if (field.field_type === "number" || field.field_type === "integer") {
    if (field.min !== undefined && field.max !== undefined) return "slider";
    return "number";
  }
  return "textarea";
}
