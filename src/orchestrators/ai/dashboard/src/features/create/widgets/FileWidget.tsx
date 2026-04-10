import { useCallback, useState, useRef } from "react";
import type { MediaInput } from "../../../api/types";

interface Props {
  mediaInput: MediaInput;
  onFileSelected: (file: File) => void;
  selectedFile?: File | null;
}

export default function FileWidget({ mediaInput, onFileSelected, selectedFile }: Props) {
  const [dragOver, setDragOver] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const accept = mediaInput.accepted_types.join(",");

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setDragOver(false);
      const file = e.dataTransfer.files[0];
      if (file) onFileSelected(file);
    },
    [onFileSelected],
  );

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) onFileSelected(file);
    },
    [onFileSelected],
  );

  return (
    <div>
      <label className="block text-[11px] text-text-dim font-medium mb-1">
        {fieldLabel(mediaInput.field)}
        <span className="text-red ml-1">*</span>
      </label>
      <div
        className={[
          "border-2 border-dashed rounded-lg p-6 text-center cursor-pointer transition-colors",
          dragOver ? "border-accent bg-accent-bg" : "border-border hover:border-border-focus",
        ].join(" ")}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={handleDrop}
        onClick={() => inputRef.current?.click()}
      >
        {selectedFile ? (
          <div>
            <div className="text-sm text-text font-medium truncate">{selectedFile.name}</div>
            <div className="text-[10px] text-text-dimmer mt-1">
              {selectedFile.type} &middot; {formatSize(selectedFile.size)}
            </div>
          </div>
        ) : (
          <div>
            <div className="text-sm text-text-dim">Drop a file here or click to browse</div>
            <div className="text-[10px] text-text-dimmer mt-1">
              {mediaInput.accepted_types.join(", ")}
            </div>
          </div>
        )}
      </div>
      <input
        ref={inputRef}
        type="file"
        accept={accept}
        className="hidden"
        onChange={handleChange}
      />
    </div>
  );
}

function fieldLabel(field: string): string {
  const parts = field.split(".");
  const last = parts[parts.length - 1];
  return last.charAt(0).toUpperCase() + last.slice(1);
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
