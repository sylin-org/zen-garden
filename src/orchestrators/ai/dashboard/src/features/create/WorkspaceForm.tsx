import { useState, useCallback, useEffect, useRef } from "react";
import type { CatalogDetail, PersistedRequest } from "../../api/types";
import { dispatch as apiDispatch, upload } from "../../api/client";
import { useJobManager } from "../../contexts/JobManagerContext";
import FieldRenderer from "./widgets/FieldRenderer";
import FileWidget from "./widgets/FileWidget";
import ExampleCards from "./ExampleCards";
import CopyAsCurl from "./CopyAsCurl";

interface Turn {
  user: string;
  assistant: string;
}

interface Props {
  detail: CatalogDetail;
  initialValues?: Record<string, unknown> | null;
  parentId?: string;
  sourceRequest?: PersistedRequest | null;
  onResult: (result: unknown) => void;
  onError: (error: unknown) => void;
  onStreaming: (reader: ReadableStreamDefaultReader<Uint8Array>) => void;
}

export default function WorkspaceForm({
  detail,
  initialValues,
  parentId,
  sourceRequest,
  onResult,
  onError,
  onStreaming,
}: Props) {
  const fields = detail.fields ?? [];
  const mediaInputs = detail.media_inputs ?? [];

  // Detect dialogue mode from field types — the catalog drives this,
  // not field name matching.
  const dialogueField = fields.find(
    (f) => f.widget === "dialogue" || f.field_type === "dialogue",
  );
  const isDialogue = !!dialogueField;

  // Split fields into categories
  const primaryFields = fields.filter(
    (f) => f.required && f.widget !== "hidden" && f.widget !== "dialogue",
  );
  const secondaryFields = fields.filter(
    (f) => !f.required && f.widget !== "hidden" && f.widget !== "dialogue",
  );

  // Form state: dotted field path → value
  const [values, setValues] = useState<Record<string, unknown>>(() => {
    const defaults: Record<string, unknown> = {};
    for (const f of fields) {
      if (f.default !== undefined && f.default !== null) {
        defaults[f.field] = f.default;
      }
    }
    if (dialogueField) {
      defaults[dialogueField.field] = [];
    }
    if (initialValues) {
      return { ...defaults, ...initialValues };
    }
    return defaults;
  });

  const [files, setFiles] = useState<Record<string, File>>({});
  const [submitting, setSubmitting] = useState(false);
  const [userTouched, setUserTouched] = useState(!!initialValues);
  const [streamingText, setStreamingText] = useState<string | undefined>(undefined);
  const [settingsOpen, setSettingsOpen] = useState(() => {
    try {
      return localStorage.getItem(`settings-open:${detail.path}`) === "true";
    } catch {
      return false;
    }
  });

  const { track } = useJobManager();
  const threadRef = useRef<HTMLDivElement>(null);

  // Persist dialogue history to localStorage
  useEffect(() => {
    if (dialogueField) {
      try {
        const history = values[dialogueField.field];
        if (Array.isArray(history) && history.length > 0) {
          localStorage.setItem(`dialogue:${detail.path}`, JSON.stringify(history));
        }
      } catch { /* ignore */ }
    }
  }, [dialogueField, values, detail.path]);

  // Load dialogue history from localStorage on mount
  useEffect(() => {
    if (dialogueField && !initialValues) {
      try {
        const saved = localStorage.getItem(`dialogue:${detail.path}`);
        if (saved) {
          const parsed = JSON.parse(saved);
          if (Array.isArray(parsed)) {
            setValues((prev) => ({ ...prev, [dialogueField.field]: parsed }));
          }
        }
      } catch { /* ignore */ }
    }
  }, [dialogueField, detail.path, initialValues]);

  // Scroll dialogue thread on new content
  useEffect(() => {
    if (isDialogue) {
      threadRef.current?.scrollTo({ top: threadRef.current.scrollHeight, behavior: "smooth" });
    }
  }, [isDialogue, values, streamingText]);

  const setValue = useCallback((field: string, value: unknown) => {
    setValues((prev) => ({ ...prev, [field]: value }));
    setUserTouched(true);
  }, []);

  const applyExample = useCallback((flat: Record<string, unknown>) => {
    setValues((prev) => ({ ...prev, ...flat }));
    setUserTouched(true);
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
    setStreamingText(undefined);

    // For dialogue mode, capture the user's message before clearing
    const userMessageField = primaryFields.find((f) => f.field === "text.prompt.user");
    const userMessage = isDialogue && userMessageField
      ? (values[userMessageField.field] as string) ?? ""
      : "";

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

      // For dialogue: clear the user input immediately
      if (isDialogue && userMessageField) {
        setValues((prev) => ({ ...prev, [userMessageField.field]: "" }));
      }

      // Step 3: Dispatch
      const idempotencyKey = crypto.randomUUID();
      const url = `/v1/${detail.path.replace(/\./g, "/")}`;
      const response = await apiDispatch(url, payload, idempotencyKey);
      const contentType = response.headers.get("content-type") ?? "";

      if (contentType.includes("text/event-stream") && response.body) {
        // Streaming — read tokens and accumulate
        handleStreamingResponse(
          response.body.getReader(),
          userMessage,
          dialogueField?.field,
        );
      } else if (response.status === 202) {
        const body = await response.json();
        const jobId = body._meta?.request_id ?? body.job_id;
        if (jobId) track(jobId, detail.path);
        onResult(body);
      } else if (response.ok) {
        const body = await response.json();
        if (body.error) {
          onError(body);
        } else {
          // For dialogue: append the turn
          if (isDialogue && dialogueField) {
            const assistantText =
              body?.output?.text?.response ?? JSON.stringify(body?.output ?? {});
            appendTurn(dialogueField.field, userMessage, assistantText);
          }
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
  }, [submitting, values, files, detail.path, track, onResult, onError, onStreaming, isDialogue, dialogueField, primaryFields]);

  const handleStreamingResponse = useCallback(
    (reader: ReadableStreamDefaultReader<Uint8Array>, userMessage: string, historyField?: string) => {
      setStreamingText("");
      const decoder = new TextDecoder();
      let buffer = "";
      let fullText = "";

      function read() {
        reader.read().then(({ done, value }) => {
          if (done) {
            // Stream ended — finalize turn
            if (historyField && fullText) {
              appendTurn(historyField, userMessage, fullText);
              setStreamingText(undefined);
            }
            return;
          }
          buffer += decoder.decode(value, { stream: true });
          const lines = buffer.split("\n");
          buffer = lines.pop() ?? "";
          for (const line of lines) {
            if (line.startsWith("data: ")) {
              try {
                const data = JSON.parse(line.slice(6));
                const delta =
                  data?.output?.text?.delta ??
                  data?.output?.text?.response ??
                  data?.text?.delta;
                if (typeof delta === "string") {
                  fullText += delta;
                  setStreamingText(fullText);
                }
                if (data?.output?.text?.finish_reason || data?.done) {
                  const finalText = data?.output?.text?.response ?? fullText;
                  if (historyField) {
                    appendTurn(historyField, userMessage, finalText);
                    setStreamingText(undefined);
                  }
                  onResult(data);
                }
              } catch { /* ignore malformed */ }
            }
          }
          read();
        });
      }
      read();

      // Also forward to parent for result panel
      onStreaming(reader);
    },
    [onResult, onStreaming],
  );

  const appendTurn = useCallback(
    (historyField: string, user: string, assistant: string) => {
      setValues((prev) => {
        const history = (prev[historyField] as Turn[]) ?? [];
        return { ...prev, [historyField]: [...history, { user, assistant }] };
      });
    },
    [],
  );

  const clearDialogue = useCallback(() => {
    if (dialogueField) {
      setValues((prev) => ({ ...prev, [dialogueField.field]: [] }));
      setStreamingText(undefined);
      try {
        localStorage.removeItem(`dialogue:${detail.path}`);
      } catch { /* ignore */ }
    }
  }, [dialogueField, detail.path]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (isDialogue && e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [isDialogue, handleSubmit],
  );

  // Dialogue history for the thread widget
  const dialogueHistory = dialogueField
    ? ((values[dialogueField.field] as Turn[]) ?? [])
    : [];

  return (
    <div className="flex flex-col h-full" onKeyDown={handleKeyDown}>
      {/* Dialogue: thread area (scrollable, takes available space) */}
      {isDialogue && (
        <div ref={threadRef} className="flex-1 overflow-y-auto p-6 min-h-0">
          {/* Fork/view banner */}
          {sourceRequest && <ForkBanner parentId={parentId} sourceRequest={sourceRequest} />}

          {/* Example cards */}
          {detail.examples && detail.examples.length > 0 && (
            <ExampleCards
              examples={detail.examples}
              onSelect={applyExample}
              hidden={userTouched || dialogueHistory.length > 0}
            />
          )}

          {/* Dialogue thread */}
          <FieldRenderer
            field={dialogueField!}
            value={dialogueHistory}
            onChange={(v) => setValue(dialogueField!.field, v)}
            streamingText={streamingText}
          />
        </div>
      )}

      {/* Non-dialogue: scrollable form area */}
      {!isDialogue && (
        <div className="flex-1 overflow-y-auto p-6">
          {sourceRequest && <ForkBanner parentId={parentId} sourceRequest={sourceRequest} />}

          {detail.examples && detail.examples.length > 0 && (
            <ExampleCards
              examples={detail.examples}
              onSelect={applyExample}
              hidden={userTouched}
            />
          )}

          <div className="space-y-4">
            {primaryFields.map((f) => (
              <FieldRenderer
                key={f.field}
                field={f}
                value={values[f.field]}
                onChange={(v) => setValue(f.field, v)}
              />
            ))}

            {mediaInputs.map((mi) => (
              <FileWidget
                key={mi.field}
                mediaInput={mi}
                selectedFile={files[mi.field]}
                onFileSelected={(file) => setFiles((prev) => ({ ...prev, [mi.field]: file }))}
              />
            ))}
          </div>
        </div>
      )}

      {/* Input bar (always at the bottom) */}
      <div className="border-t border-border p-4 shrink-0">
        {/* For dialogue: render the primary text input inline here */}
        {isDialogue && primaryFields.length > 0 && (
          <div className="mb-3">
            {primaryFields.map((f) => (
              <FieldRenderer
                key={f.field}
                field={f}
                value={values[f.field]}
                onChange={(v) => setValue(f.field, v)}
              />
            ))}
          </div>
        )}

        <div className="flex items-center gap-3">
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
          <CopyAsCurl
            url={`/v1/${detail.path.replace(/\./g, "/")}`}
            values={values}
          />
          {isDialogue && dialogueHistory.length > 0 && (
            <button
              onClick={clearDialogue}
              className="text-[10px] text-text-dimmer hover:text-text-dim transition-colors ml-auto"
            >
              Clear history
            </button>
          )}
        </div>

        {/* Settings (secondary fields) */}
        {secondaryFields.length > 0 && (
          <details className="mt-3" open={settingsOpen}>
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

function ForkBanner({
  parentId,
  sourceRequest,
}: {
  parentId?: string;
  sourceRequest: PersistedRequest;
}) {
  return (
    <div className="mb-4 p-3 rounded-lg bg-accent-bg border border-accent/20 text-[11px]">
      <span className="text-accent font-medium">
        {parentId ? "Forked from" : "Viewing"} request
      </span>
      <span className="text-text-dim ml-1.5 font-mono">
        {sourceRequest.id.slice(0, 12)}...
      </span>
      {sourceRequest.meta.provider && (
        <span className="text-text-dimmer ml-1.5">
          via {sourceRequest.meta.provider}
          {sourceRequest.meta.latency_ms != null && ` · ${sourceRequest.meta.latency_ms}ms`}
        </span>
      )}
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
