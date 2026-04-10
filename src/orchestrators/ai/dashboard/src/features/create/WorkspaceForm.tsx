import { useState, useCallback, useEffect, useRef } from "react";
import { useNavigate } from "react-router-dom";
import type { CatalogDetail, PersistedRequest } from "../../api/types";
import { upload } from "../../api/client";
import { useActiveRequestManager, useActiveRequest } from "../../contexts/ActiveRequestManager";
import FieldRenderer from "./widgets/FieldRenderer";
import FileWidget from "./widgets/FileWidget";
import ExampleCards from "./ExampleCards";
import CopyAsCurl from "./CopyAsCurl";
import Markdown from "../../components/Markdown";

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
  onStreaming: _onStreaming,
}: Props) {
  const fields = detail.fields ?? [];
  const mediaInputs = detail.media_inputs ?? [];
  const manager = useActiveRequestManager();

  // Detect dialogue mode from field types
  const dialogueField = fields.find(
    (f) => f.widget === "dialogue" || f.field_type === "dialogue",
  );
  const isDialogue = !!dialogueField;

  // Split fields
  const primaryFields = fields.filter(
    (f) => f.required && f.widget !== "hidden" && f.widget !== "dialogue",
  );
  const secondaryFields = fields.filter(
    (f) => !f.required && f.widget !== "hidden" && f.widget !== "dialogue",
  );

  // Form state
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
  const [userTouched, setUserTouched] = useState(!!initialValues);
  const [activeRequestId, setActiveRequestId] = useState<string | undefined>(undefined);
  const [settingsOpen, setSettingsOpen] = useState(() => {
    try {
      return localStorage.getItem(`settings-open:${detail.path}`) === "true";
    } catch {
      return false;
    }
  });

  const activeReq = useActiveRequest(activeRequestId);
  const isSending = activeReq?.status === "sending" || activeReq?.status === "streaming";
  const threadRef = useRef<HTMLDivElement>(null);

  // Load dialogue history from localStorage
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

  // Persist dialogue history
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

  // Scroll dialogue on new content
  useEffect(() => {
    if (isDialogue && threadRef.current) {
      threadRef.current.scrollTo({ top: 0, behavior: "smooth" });
    }
  }, [isDialogue, values, activeReq?.streamAccumulator]);

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
      try { localStorage.setItem(`settings-open:${detail.path}`, String(next)); } catch { /* */ }
      return next;
    });
  }, [detail.path]);

  const handleSubmit = useCallback(async () => {
    if (isSending) return;

    // For dialogue: capture user message before clearing
    const userMessageField = primaryFields.find((f) => f.field === "text.prompt.user");
    const userMessage = isDialogue && userMessageField
      ? (values[userMessageField.field] as string)?.trim() ?? ""
      : "";

    if (isDialogue && !userMessage) return;

    // Upload files first
    const mediaRefs: Record<string, string> = {};
    for (const [fieldPath, file] of Object.entries(files)) {
      const result = await upload("/v1/media", file) as { media_id: string };
      mediaRefs[fieldPath] = result.media_id;
    }

    // Build nested payload
    const payload: Record<string, unknown> = {};
    for (const [dotted, value] of Object.entries(values)) {
      if (value === undefined || value === null) continue;
      setNested(payload, dotted, value);
    }
    for (const [fieldPath, mediaId] of Object.entries(mediaRefs)) {
      setNested(payload, fieldPath, { media_id: mediaId });
    }

    // Clear dialogue input immediately
    if (isDialogue && userMessageField) {
      setValues((prev) => ({ ...prev, [userMessageField.field]: "" }));
    }

    // Dispatch via manager
    const url = `/v1/${detail.path.replace(/\./g, "/")}`;
    const reqId = manager.dispatch({
      url,
      payload,
      action: detail.path,
      userMessage: isDialogue ? userMessage : undefined,
      dialogueField: dialogueField?.field,
      onResult: (result) => {
        onResult(result);
      },
      onError: (error) => {
        onError(error);
      },
      onTurnComplete: dialogueField
        ? (turn) => {
            setValues((prev) => {
              const history = (prev[dialogueField.field] as Turn[]) ?? [];
              return { ...prev, [dialogueField.field]: [...history, turn] };
            });
          }
        : undefined,
    });

    setActiveRequestId(reqId);
  }, [isSending, values, files, detail.path, isDialogue, dialogueField, primaryFields, manager, onResult, onError]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (isDialogue && e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [isDialogue, handleSubmit],
  );

  const clearDialogue = useCallback(() => {
    if (dialogueField) {
      setValues((prev) => ({ ...prev, [dialogueField.field]: [] }));
      try { localStorage.removeItem(`dialogue:${detail.path}`); } catch { /* */ }
    }
  }, [dialogueField, detail.path]);

  // Dialogue history (newest first for display)
  const dialogueHistory = dialogueField
    ? ((values[dialogueField.field] as Turn[]) ?? [])
    : [];
  const dialogueReversed = [...dialogueHistory].reverse();

  // Elapsed display
  const elapsed = activeReq?.elapsed;
  const elapsedText = elapsed != null && isSending
    ? elapsed < 60 ? `${elapsed.toFixed(1)}s` : `${Math.floor(elapsed / 60)}m ${(elapsed % 60).toFixed(0)}s`
    : null;

  return (
    <div className="flex flex-col h-full" onKeyDown={handleKeyDown}>
      {/* Top section: banner + examples + form fields */}
      <div className="p-6 shrink-0">
        {sourceRequest && <ForkBanner parentId={parentId} sourceRequest={sourceRequest} />}

        {detail.examples && detail.examples.length > 0 && (
          <ExampleCards
            examples={detail.examples}
            onSelect={applyExample}
            hidden={userTouched || dialogueHistory.length > 0}
          />
        )}

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

          {mediaInputs.map((mi) => (
            <FileWidget
              key={mi.field}
              mediaInput={mi}
              selectedFile={files[mi.field]}
              onFileSelected={(file) => setFiles((prev) => ({ ...prev, [mi.field]: file }))}
            />
          ))}
        </div>

        {/* Action bar */}
        <div className="flex items-center gap-3 mt-4">
          <button
            onClick={handleSubmit}
            disabled={isSending}
            className="px-6 py-2 bg-accent hover:bg-accent-dim text-white text-[12px] font-semibold
                       rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {isSending ? "Sending" : "Send"}
          </button>
          {/* Elapsed timer */}
          {elapsedText && (
            <span className="text-[11px] text-orange font-mono animate-pulse">
              {elapsedText}
            </span>
          )}
          {isSending && (
            <button
              onClick={() => activeRequestId && manager.cancel(activeRequestId)}
              className="text-[10px] text-text-dimmer hover:text-red transition-colors"
            >
              Cancel
            </button>
          )}
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

        {/* Settings */}
        {secondaryFields.length > 0 && (
          <details className="mt-3" open={settingsOpen}>
            <summary
              onClick={(e) => { e.preventDefault(); toggleSettings(); }}
              className="text-[11px] text-text-dim cursor-pointer font-medium flex items-center gap-1.5 py-1.5 select-none"
            >
              <span className={[
                "inline-block w-[5px] h-[5px] border-r-[1.5px] border-b-[1.5px] border-text-dim transition-transform",
                settingsOpen ? "rotate-45" : "-rotate-45",
              ].join(" ")} />
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

      {/* Dialogue history (newest first, below the form) */}
      {isDialogue && (dialogueHistory.length > 0 || activeReq?.streamAccumulator) && (
        <div ref={threadRef} className="flex-1 overflow-y-auto px-6 pb-6 border-t border-border min-h-0">
          <div className="pt-3 space-y-3">
            {/* Streaming in progress — show at top (newest) */}
            {activeReq?.streamAccumulator && activeReq.status === "streaming" && (
              <div className="space-y-1">
                <div className="text-[9px] text-text-dimmer uppercase tracking-wider">Now</div>
                <DialogueTurn
                  user={activeReq.userMessage ?? ""}
                  assistant={activeReq.streamAccumulator}
                  streaming
                />
              </div>
            )}

            {/* Completed turns — newest first */}
            {dialogueReversed.map((turn, i) => (
              <DialogueTurn key={dialogueHistory.length - 1 - i} user={turn.user} assistant={turn.assistant} />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ── Sub-components ───────────────────────────────────────────

function DialogueTurn({ user, assistant, streaming }: { user: string; assistant: string; streaming?: boolean }) {
  return (
    <div className="space-y-1.5">
      <div className="flex justify-end">
        <div className="max-w-[85%] px-3 py-2 rounded-lg bg-accent/15 text-[13px] leading-relaxed">
          <Markdown content={user} />
        </div>
      </div>
      <div className="flex justify-start">
        <div className={[
          "max-w-[85%] px-3 py-2 rounded-lg bg-surface-2 text-[13px] leading-relaxed",
          streaming ? "animate-pulse" : "",
        ].join(" ")}>
          <Markdown content={assistant} />
        </div>
      </div>
    </div>
  );
}

function ForkBanner({ parentId, sourceRequest }: { parentId?: string; sourceRequest: PersistedRequest }) {
  const navigate = useNavigate();
  const handleClick = () => {
    const url = `/create/${sourceRequest.action.replace(/\./g, "/")}?r=${sourceRequest.id}`;
    navigate(url, { state: { request: sourceRequest } });
  };

  return (
    <div
      className="mb-4 p-3 rounded-lg bg-accent-bg border border-accent/20 text-[11px] cursor-pointer hover:border-accent transition-colors"
      onClick={handleClick}
      title="Click to view this request"
    >
      <span className="text-accent font-medium">
        {parentId ? "Forked from" : "Viewing"} request
      </span>
      <span className="text-text-dim ml-1.5 font-mono hover:text-accent transition-colors">
        {sourceRequest.id.slice(0, 12)}...
      </span>
      {sourceRequest.meta.provider && (
        <span className="text-text-dimmer ml-1.5">
          via {sourceRequest.meta.provider}
          {sourceRequest.meta.latency_ms != null && ` · ${formatLatency(sourceRequest.meta.latency_ms)}ms`}
        </span>
      )}
    </div>
  );
}

function formatLatency(ms: number): string {
  if (ms < 1000) return String(ms);
  return `${(ms / 1000).toFixed(1)}s`;
}

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
