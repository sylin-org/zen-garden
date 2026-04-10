import { useState, useCallback } from "react";
import type { CatalogDetail } from "../../api/types";
import { dispatch as apiDispatch, upload } from "../../api/client";
import { useJobManager } from "../../contexts/JobManagerContext";
import FieldRenderer from "./widgets/FieldRenderer";
import FileWidget from "./widgets/FileWidget";

interface Props {
  detail: CatalogDetail;
  onResult: (result: unknown) => void;
  onError: (error: unknown) => void;
  onStreaming: (reader: ReadableStreamDefaultReader<Uint8Array>) => void;
}

export default function WorkspaceForm({ detail, onResult, onError, onStreaming }: Props) {
  const fields = detail.fields ?? [];
  const mediaInputs = detail.media_inputs ?? [];

  const primaryFields = fields.filter((f) => f.required && f.widget !== "hidden");
  const secondaryFields = fields.filter((f) => !f.required && f.widget !== "hidden");

  // Form state: dotted field path → value
  const [values, setValues] = useState<Record<string, unknown>>(() => {
    const initial: Record<string, unknown> = {};
    for (const f of fields) {
      if (f.default !== undefined && f.default !== null) {
        initial[f.field] = f.default;
      }
    }
    return initial;
  });

  const [files, setFiles] = useState<Record<string, File>>({});
  const [submitting, setSubmitting] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(() => {
    try {
      return localStorage.getItem(`settings-open:${detail.path}`) === "true";
    } catch {
      return false;
    }
  });

  const { track } = useJobManager();

  const setValue = useCallback((field: string, value: unknown) => {
    setValues((prev) => ({ ...prev, [field]: value }));
  }, []);

  const toggleSettings = useCallback(() => {
    setSettingsOpen((prev) => {
      const next = !prev;
      try {
        localStorage.setItem(`settings-open:${detail.path}`, String(next));
      } catch { /* ignore */ }
      return next;
    });
  }, [detail.path]);

  const handleSubmit = useCallback(async () => {
    if (submitting) return;
    setSubmitting(true);

    try {
      // Step 1: Upload any files
      const mediaRefs: Record<string, string> = {};
      for (const [fieldPath, file] of Object.entries(files)) {
        const result = await upload("/v1/media", file) as { media_id: string };
        mediaRefs[fieldPath] = result.media_id;
      }

      // Step 2: Build nested payload from dotted paths
      const payload: Record<string, unknown> = {};
      for (const [dotted, value] of Object.entries(values)) {
        if (value === undefined || value === null) continue;
        setNested(payload, dotted, value);
      }

      // Inject media_id references
      for (const [fieldPath, mediaId] of Object.entries(mediaRefs)) {
        setNested(payload, fieldPath, { media_id: mediaId });
      }

      // Step 3: Dispatch
      const idempotencyKey = crypto.randomUUID();
      const url = `/v1/${detail.path.replace(/\./g, "/")}`;
      const response = await apiDispatch(url, payload, idempotencyKey);

      const contentType = response.headers.get("content-type") ?? "";

      if (contentType.includes("text/event-stream") && response.body) {
        // Streaming response
        onStreaming(response.body.getReader());
      } else if (response.status === 202) {
        // Async — track the job
        const body = await response.json();
        const jobId = body._meta?.request_id ?? body.job_id;
        if (jobId) track(jobId, detail.path);
        onResult(body);
      } else if (response.ok) {
        // Sync result
        const body = await response.json();
        if (body.error) {
          onError(body);
        } else {
          onResult(body);
        }
      } else {
        const body = await response.json().catch(() => ({ error: { message: response.statusText } }));
        onError(body);
      }
    } catch (e) {
      onError({ error: { code: "network", message: e instanceof Error ? e.message : "Network error" } });
    } finally {
      setSubmitting(false);
    }
  }, [submitting, values, files, detail.path, track, onResult, onError, onStreaming]);

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-y-auto p-6">
        {/* Primary fields */}
        <div className="space-y-4">
          {primaryFields.map((f) => (
            <FieldRenderer
              key={f.field}
              field={f}
              value={values[f.field]}
              onChange={(v) => setValue(f.field, v)}
            />
          ))}

          {/* Media inputs */}
          {mediaInputs.map((mi) => (
            <FileWidget
              key={mi.field}
              mediaInput={mi}
              selectedFile={files[mi.field]}
              onFileSelected={(file) => setFiles((prev) => ({ ...prev, [mi.field]: file }))}
            />
          ))}
        </div>

        {/* Send button */}
        <div className="flex items-center gap-3 mt-4">
          <button
            onClick={handleSubmit}
            disabled={submitting}
            className="px-6 py-2 bg-accent hover:bg-accent-dim text-white text-[12px] font-semibold
                       rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {submitting ? "Sending..." : "Send"}
          </button>
          <span className="text-[10px] text-text-dimmer">
            {detail.providers.join(", ")}
          </span>
        </div>

        {/* Settings (secondary fields) */}
        {secondaryFields.length > 0 && (
          <details className="mt-5" open={settingsOpen}>
            <summary
              onClick={(e) => {
                e.preventDefault();
                toggleSettings();
              }}
              className="text-[11px] text-text-dim cursor-pointer font-medium flex items-center gap-1.5 py-1.5 select-none"
            >
              <span
                className={[
                  "inline-block w-[5px] h-[5px] border-r-[1.5px] border-b-[1.5px] border-text-dim transition-transform",
                  settingsOpen ? "rotate-45" : "-rotate-45",
                ].join(" ")}
              />
              Settings
            </summary>
            <div className="grid grid-cols-2 gap-3 mt-3">
              {secondaryFields.map((f) => (
                <FieldRenderer
                  key={f.field}
                  field={f}
                  value={values[f.field]}
                  onChange={(v) => setValue(f.field, v)}
                />
              ))}
            </div>
          </details>
        )}
      </div>
    </div>
  );
}

/** Set a value at a dotted path in a nested object. */
function setNested(obj: Record<string, unknown>, dotted: string, value: unknown) {
  const parts = dotted.split(".");
  let current: Record<string, unknown> = obj;
  for (let i = 0; i < parts.length - 1; i++) {
    const key = parts[i];
    if (typeof current[key] !== "object" || current[key] === null) {
      current[key] = {};
    }
    current = current[key] as Record<string, unknown>;
  }
  current[parts[parts.length - 1]] = value;
}
