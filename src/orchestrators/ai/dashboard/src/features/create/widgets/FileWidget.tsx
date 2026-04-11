import { useCallback, useEffect, useRef, useState } from "react";
import type { MediaInput } from "../../../api/types";

interface Props {
  mediaInput: MediaInput;
  /**
   * The current media id stored in the payload at this field (if
   * any). When set, the widget renders the uploaded-state pill with
   * a thumbnail. Cleared by passing `undefined`.
   */
  currentMediaId: string | undefined;
  /**
   * Called when the widget wants to update the payload: either to
   * write a freshly-uploaded `media_id` or to clear the field.
   */
  onMediaIdChange: (mediaId: string | undefined) => void;
  /**
   * Called whenever this widget's upload state changes. The parent
   * form disables Send while any widget reports `true` so the user
   * cannot dispatch a half-ready payload.
   */
  onUploadStateChange: (inFlight: boolean) => void;
}

type UploadState =
  | { kind: "empty" }
  | { kind: "uploading"; filename: string; size: number }
  | { kind: "uploaded"; filename: string; size: number; mediaId: string }
  | { kind: "failed"; filename: string; error: string };

/**
 * File attachment widget backed by the orchestrator's media store
 * (ORCH-0038 + media pipeline). The widget uploads the picked file
 * as soon as it lands, stores the returned `media_id` into the form
 * payload, and renders a thumbnail sourced from the representation
 * endpoint.
 *
 * Four-state machine:
 *
 *   empty → uploading → uploaded   (success path)
 *                  ↘ failed         (retry from here)
 *
 * Uploads use an `AbortController` so a mid-flight navigation away
 * aborts the request; the partial media never lands and the TTL
 * sweeper reaps anything that did.
 */
export default function FileWidget({
  mediaInput,
  currentMediaId,
  onMediaIdChange,
  onUploadStateChange,
}: Props) {
  const [dragOver, setDragOver] = useState(false);
  const [state, setState] = useState<UploadState>(() =>
    currentMediaId
      ? { kind: "uploaded", filename: "", size: 0, mediaId: currentMediaId }
      : { kind: "empty" },
  );
  const inputRef = useRef<HTMLInputElement>(null);
  const abortRef = useRef<AbortController | null>(null);

  // Keep local state in sync if the parent clears the field
  // externally (example injection, payload reset, etc.).
  useEffect(() => {
    if (!currentMediaId && state.kind === "uploaded") {
      setState({ kind: "empty" });
    }
  }, [currentMediaId, state.kind]);

  // Tell the parent whether an upload is in flight so Send can
  // gate on all widgets being idle.
  useEffect(() => {
    onUploadStateChange(state.kind === "uploading");
  }, [state.kind, onUploadStateChange]);

  // Abort any in-flight upload on unmount. The partial upload is
  // either (a) a zero-length orphan the TTL sweeper will reap, or
  // (b) never lands at all because the request was cancelled
  // before the response body was processed.
  useEffect(() => {
    return () => {
      abortRef.current?.abort();
    };
  }, []);

  const uploadFile = useCallback(
    async (file: File) => {
      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;

      setState({ kind: "uploading", filename: file.name, size: file.size });

      try {
        const response = await fetch("/v1/media", {
          method: "POST",
          headers: {
            "Content-Type": file.type || "application/octet-stream",
          },
          body: file,
          signal: controller.signal,
        });

        if (!response.ok) {
          const text = await response.text().catch(() => response.statusText);
          throw new Error(`${response.status}: ${text}`);
        }

        const json = (await response.json()) as { media_id: string };
        if (controller.signal.aborted) return;

        setState({
          kind: "uploaded",
          filename: file.name,
          size: file.size,
          mediaId: json.media_id,
        });
        onMediaIdChange(json.media_id);
      } catch (err) {
        if (controller.signal.aborted) return;
        setState({
          kind: "failed",
          filename: file.name,
          error: err instanceof Error ? err.message : String(err),
        });
      }
    },
    [onMediaIdChange],
  );

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setDragOver(false);
      const file = e.dataTransfer.files[0];
      if (file) void uploadFile(file);
    },
    [uploadFile],
  );

  const handleChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) void uploadFile(file);
      // Reset the input so picking the same file again re-triggers change
      e.target.value = "";
    },
    [uploadFile],
  );

  const clear = useCallback(() => {
    abortRef.current?.abort();
    setState({ kind: "empty" });
    onMediaIdChange(undefined);
  }, [onMediaIdChange]);

  const pick = useCallback(() => inputRef.current?.click(), []);

  return (
    <div>
      <label className="block text-[11px] text-text-dim font-medium mb-1">
        {fieldLabel(mediaInput.field)}
        <span className="text-red ml-1">*</span>
      </label>

      {state.kind === "empty" && (
        <div
          className={[
            "border-2 border-dashed rounded-lg p-6 text-center cursor-pointer transition-colors",
            dragOver
              ? "border-accent bg-accent-bg"
              : "border-border hover:border-border-focus",
          ].join(" ")}
          onDragOver={(e) => {
            e.preventDefault();
            setDragOver(true);
          }}
          onDragLeave={() => setDragOver(false)}
          onDrop={handleDrop}
          onClick={pick}
        >
          <div className="text-sm text-text-dim">
            Drop a file here or click to browse
          </div>
          <div className="text-[10px] text-text-dimmer mt-1">
            {mediaInput.accepted_types.join(", ")}
          </div>
        </div>
      )}

      {state.kind === "uploading" && (
        <div className="border border-border rounded-lg p-3 flex items-center gap-3 bg-surface-2">
          <div className="w-12 h-12 rounded bg-surface flex items-center justify-center shrink-0">
            <Spinner />
          </div>
          <div className="flex-1 min-w-0">
            <div className="text-sm text-text truncate">{state.filename}</div>
            <div className="text-[10px] text-text-dimmer mt-0.5">
              Uploading… {formatSize(state.size)}
            </div>
          </div>
          <button
            type="button"
            onClick={clear}
            className="text-[11px] text-text-dimmer hover:text-red transition-colors px-2"
            aria-label="Cancel upload"
          >
            Cancel
          </button>
        </div>
      )}

      {state.kind === "uploaded" && (
        <div className="border border-border rounded-lg p-3 flex items-center gap-3 bg-surface-2">
          <img
            src={`/v1/media/${state.mediaId}?format=thumbnail`}
            alt={state.filename || "attachment"}
            className="w-12 h-12 rounded object-cover bg-surface shrink-0"
          />
          <div className="flex-1 min-w-0">
            <div className="text-sm text-text truncate">
              {state.filename || "Attachment"}
            </div>
            <div className="text-[10px] text-text-dimmer mt-0.5 font-mono truncate">
              {state.mediaId}
            </div>
          </div>
          <button
            type="button"
            onClick={clear}
            className="text-[11px] text-text-dimmer hover:text-red transition-colors px-2"
            aria-label="Remove attachment"
          >
            Remove
          </button>
        </div>
      )}

      {state.kind === "failed" && (
        <div className="border border-red/40 rounded-lg p-3 flex items-center gap-3 bg-red/5">
          <div className="w-12 h-12 rounded bg-surface flex items-center justify-center shrink-0 text-red text-xl">
            !
          </div>
          <div className="flex-1 min-w-0">
            <div className="text-sm text-text truncate">{state.filename}</div>
            <div className="text-[10px] text-red mt-0.5 truncate">
              {state.error}
            </div>
          </div>
          <button
            type="button"
            onClick={pick}
            className="text-[11px] text-accent hover:text-accent-bright transition-colors px-2"
          >
            Retry
          </button>
          <button
            type="button"
            onClick={clear}
            className="text-[11px] text-text-dimmer hover:text-red transition-colors px-2"
          >
            Remove
          </button>
        </div>
      )}

      <input
        ref={inputRef}
        type="file"
        accept={mediaInput.accepted_types.join(",")}
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

function Spinner() {
  return (
    <svg
      className="animate-spin text-accent"
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
    >
      <circle
        cx="12"
        cy="12"
        r="10"
        stroke="currentColor"
        strokeWidth="3"
        opacity="0.25"
      />
      <path
        d="M22 12a10 10 0 0 1-10 10"
        stroke="currentColor"
        strokeWidth="3"
        strokeLinecap="round"
      />
    </svg>
  );
}
