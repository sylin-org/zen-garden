import { useState, useCallback, useEffect, useRef } from "react";
import { useNavigate } from "react-router-dom";
import type { WorkspaceSpec, FieldDescriptor, PersistedRequest } from "../../api/types";
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
  spec: WorkspaceSpec;
  sourceRequest?: PersistedRequest | null;
  onResult: (result: unknown) => void;
  onError: (error: unknown) => void;
}

export default function WorkspaceForm({
  spec,
  sourceRequest,
  onResult,
  onError,
}: Props) {
  const manager = useActiveRequestManager();

  const fields = spec.fields;
  const fieldEntries = Object.entries(fields);
  const mediaInputs = spec.media_inputs ?? [];

  // Detect dialogue mode from field types
  const dialogueEntry = fieldEntries.find(([, f]) => f.widget === "dialogue");
  const dialogueKey = dialogueEntry?.[0];
  const isDialogue = !!dialogueKey;

  // Split fields by role
  const requiredFields = fieldEntries.filter(
    ([, f]) => f.required && f.widget !== "hidden" && f.widget !== "dialogue",
  );
  const optionalFields = fieldEntries.filter(
    ([, f]) => !f.required && f.widget !== "hidden" && f.widget !== "dialogue",
  );

  // Form state: the payload IS the form state.
  // If viewing a stored request, use the stored input as the payload.
  // Otherwise use the spec's pre-assembled payload template.
  const [payload, setPayload] = useState<Record<string, unknown>>(() => {
    if (sourceRequest?.input && typeof sourceRequest.input === "object") {
      return structuredClone(sourceRequest.input as Record<string, unknown>);
    }
    return structuredClone(spec.payload);
  });

  const [files, setFiles] = useState<Record<string, File>>({});
  const [userTouched, setUserTouched] = useState(!!sourceRequest);
  const [activeRequestId, setActiveRequestId] = useState<string | undefined>(undefined);
  const [settingsOpen, setSettingsOpen] = useState(() => {
    try {
      return localStorage.getItem(`settings-open:${spec.primitive}`) === "true";
    } catch { return false; }
  });

  const activeReq = useActiveRequest(activeRequestId);
  const isSending = activeReq?.status === "sending" || activeReq?.status === "streaming";
  const threadRef = useRef<HTMLDivElement>(null);

  // Persist dialogue history
  useEffect(() => {
    if (dialogueKey) {
      try {
        const history = getNestedValue(payload, dialogueKey);
        if (Array.isArray(history) && history.length > 0) {
          localStorage.setItem(`dialogue:${spec.primitive}`, JSON.stringify(history));
        }
      } catch { /* ignore */ }
    }
  }, [dialogueKey, payload, spec.primitive]);

  // Load dialogue history from localStorage on fresh mount
  useEffect(() => {
    if (dialogueKey && !sourceRequest) {
      try {
        const saved = localStorage.getItem(`dialogue:${spec.primitive}`);
        if (saved) {
          const parsed = JSON.parse(saved);
          if (Array.isArray(parsed) && parsed.length > 0) {
            setPayload((prev) => setNestedValue(prev, dialogueKey, parsed));
          }
        }
      } catch { /* ignore */ }
    }
  }, [dialogueKey, spec.primitive, sourceRequest]);

  // Scroll dialogue
  useEffect(() => {
    if (isDialogue && threadRef.current) {
      threadRef.current.scrollTo({ top: 0, behavior: "smooth" });
    }
  }, [isDialogue, payload, activeReq?.streamAccumulator]);

  /** Get a value from the payload at a dotted path. */
  const getValue = useCallback(
    (path: string): unknown => getNestedValue(payload, path),
    [payload],
  );

  /** Set a value in the payload at a dotted path. */
  const setValue = useCallback((path: string, value: unknown) => {
    setPayload((prev) => setNestedValue(prev, path, value));
    setUserTouched(true);
  }, []);

  const applyExample = useCallback((examplePayload: Record<string, unknown>) => {
    // Merge example payload into current payload (deep merge)
    setPayload((prev) => deepMerge(prev, examplePayload));
    setUserTouched(true);
  }, []);

  const toggleSettings = useCallback(() => {
    setSettingsOpen((prev) => {
      const next = !prev;
      try { localStorage.setItem(`settings-open:${spec.primitive}`, String(next)); } catch { /* */ }
      return next;
    });
  }, [spec.primitive]);

  const handleSubmit = useCallback(async () => {
    if (isSending) return;

    // For dialogue: capture user message before clearing
    const userMessage = isDialogue
      ? (getNestedValue(payload, "text.prompt.user") as string)?.trim() ?? ""
      : "";

    if (isDialogue && !userMessage) return;

    // Upload files first
    const payloadToSend = structuredClone(payload);
    for (const [fieldPath, file] of Object.entries(files)) {
      const result = await upload("/v1/media", file) as { media_id: string };
      setNestedInObject(payloadToSend, fieldPath, { media_id: result.media_id });
    }

    // Inject lineage if this is a fork
    if (sourceRequest) {
      (payloadToSend as Record<string, unknown>).lineage = { parent: sourceRequest.id };
    }

    // Clear dialogue input immediately
    if (isDialogue) {
      setPayload((prev) => setNestedValue(prev, "text.prompt.user", ""));
    }

    // Dispatch via manager
    const reqId = manager.dispatch({
      url: spec.invocation.url,
      payload: payloadToSend,
      action: spec.skill_id
        ? `${spec.primitive}.${spec.skill_id}`
        : spec.primitive,
      userMessage: isDialogue ? userMessage : undefined,
      dialogueField: dialogueKey,
      onResult,
      onError,
      onTurnComplete: dialogueKey
        ? (turn) => {
            setPayload((prev) => {
              const history = (getNestedValue(prev, dialogueKey) as Turn[]) ?? [];
              return setNestedValue(prev, dialogueKey, [...history, turn]);
            });
          }
        : undefined,
    });

    setActiveRequestId(reqId);
  }, [isSending, payload, files, spec, isDialogue, dialogueKey, sourceRequest, manager, onResult, onError]);

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
    if (dialogueKey) {
      setPayload((prev) => setNestedValue(prev, dialogueKey, []));
      try { localStorage.removeItem(`dialogue:${spec.primitive}`); } catch { /* */ }
    }
  }, [dialogueKey, spec.primitive]);

  // Dialogue history (newest first)
  const dialogueHistory = dialogueKey
    ? ((getNestedValue(payload, dialogueKey) as Turn[]) ?? [])
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
        {sourceRequest && <ForkBanner sourceRequest={sourceRequest} />}

        {spec.examples && spec.examples.length > 0 && (
          <ExampleCards
            examples={spec.examples}
            onSelect={applyExample}
            hidden={userTouched || dialogueHistory.length > 0}
          />
        )}

        {/* Required fields */}
        <div className="space-y-4">
          {requiredFields.map(([path, desc]) => (
            <FieldRenderer
              key={path}
              field={fieldDescToLegacy(path, desc)}
              value={getValue(path)}
              onChange={(v) => setValue(path, v)}
            />
          ))}

          {mediaInputs.map((mi) => {
            const miObj = mi as unknown as { field: string; accepted_types: string[]; delivery: "base64" | "by_id" | "transfer" };
            return (
              <FileWidget
                key={miObj.field}
                mediaInput={miObj}
                selectedFile={files[miObj.field]}
                onFileSelected={(file) => setFiles((prev) => ({ ...prev, [miObj.field]: file }))}
              />
            );
          })}
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
          {elapsedText && (
            <span className="text-[11px] text-orange font-mono animate-pulse">{elapsedText}</span>
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
            {spec.routing.providers.join(", ")}
          </span>
          <CopyAsCurl url={spec.invocation.url} values={payload} />
          {isDialogue && dialogueHistory.length > 0 && (
            <button
              onClick={clearDialogue}
              className="text-[10px] text-text-dimmer hover:text-text-dim transition-colors ml-auto"
            >
              Clear history
            </button>
          )}
        </div>

        {/* Settings (optional fields) */}
        {optionalFields.length > 0 && (
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
              {optionalFields.map(([path, desc]) => (
                <FieldRenderer
                  key={path}
                  field={fieldDescToLegacy(path, desc)}
                  value={getValue(path)}
                  onChange={(v) => setValue(path, v)}
                />
              ))}
            </div>
          </details>
        )}
      </div>

      {/* Dialogue history (newest first, below the form) */}
      {isDialogue && (dialogueHistory.length > 0 || isSending) && (
        <div ref={threadRef} className="flex-1 overflow-y-auto px-6 pb-6 border-t border-border min-h-0">
          <div className="pt-3 space-y-1.5">
            {isSending && activeReq && (
              <>
                <DialogueBlock
                  role="assistant"
                  content={activeReq.streamAccumulator || undefined}
                  thinking
                  elapsed={elapsedText}
                />
                <DialogueBlock role="user" content={activeReq.userMessage ?? ""} />
              </>
            )}

            {dialogueReversed.map((turn, i) => (
              <div key={dialogueHistory.length - 1 - i}>
                <DialogueBlock role="assistant" content={turn.assistant} />
                <DialogueBlock role="user" content={turn.user} />
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ── Sub-components ───────────────────────────────────────────

function DialogueBlock({
  role,
  content,
  thinking,
  elapsed,
}: {
  role: "user" | "assistant";
  content?: string;
  thinking?: boolean;
  elapsed?: string | null;
}) {
  const isUser = role === "user";

  if (thinking && !content) {
    return (
      <div className="py-2 px-3 rounded-lg bg-surface-2 border border-accent/20 text-[12px] text-text-dim flex items-center gap-2">
        <span className="animate-pulse">Thinking...</span>
        {elapsed && <span className="text-orange font-mono text-[11px]">{elapsed}</span>}
      </div>
    );
  }

  return (
    <div
      className={[
        "py-2 px-3 rounded-lg text-[13px] leading-relaxed",
        isUser ? "bg-accent/8 text-text-dim text-[12px]" : "bg-surface-2 text-text",
        thinking ? "border border-accent/20" : "",
      ].join(" ")}
    >
      {isUser && <span className="text-text-dimmer text-[10px] mr-1.5">You:</span>}
      {isUser ? (
        <span>{content}</span>
      ) : (
        <div className="flex items-start gap-2">
          <div className="flex-1">
            <Markdown content={content ?? ""} />
          </div>
          {thinking && elapsed && (
            <span className="text-orange font-mono text-[10px] shrink-0 mt-0.5 animate-pulse">{elapsed}</span>
          )}
        </div>
      )}
    </div>
  );
}

function ForkBanner({ sourceRequest }: { sourceRequest: PersistedRequest }) {
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
      <span className="text-accent font-medium">Based on request</span>
      <span className="text-text-dim ml-1.5 font-mono hover:text-accent transition-colors">
        {sourceRequest.id.slice(0, 12)}...
      </span>
      {sourceRequest.meta.provider && (
        <span className="text-text-dimmer ml-1.5">
          via {sourceRequest.meta.provider}
          {sourceRequest.meta.latency_ms != null && ` · ${formatLatency(sourceRequest.meta.latency_ms)}`}
        </span>
      )}
    </div>
  );
}

// ── Utilities ────────────────────────────────────────────────

/** Bridge from the new FieldDescriptor to the legacy CatalogField shape
 * that FieldRenderer expects. TODO: update FieldRenderer to use
 * FieldDescriptor directly in a follow-up. */
function fieldDescToLegacy(path: string, desc: FieldDescriptor): {
  field: string; label?: string; field_type?: string; widget?: string;
  required: boolean; placeholder?: string; min?: number; max?: number;
  step?: number; options?: unknown[]; auto?: { default: string; description?: string };
  description?: string; pinnable: boolean; default?: unknown;
} {
  return {
    field: path,
    label: desc.label,
    field_type: desc.type,
    widget: desc.widget,
    required: desc.required ?? false,
    placeholder: desc.placeholder,
    min: desc.min,
    max: desc.max,
    step: desc.step,
    options: desc.options,
    auto: desc.auto,
    description: desc.description,
    pinnable: false,
    default: undefined,
  };
}

function getNestedValue(obj: Record<string, unknown>, path: string): unknown {
  const parts = path.split(".");
  let current: unknown = obj;
  for (const part of parts) {
    if (current === null || current === undefined || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

function setNestedValue(obj: Record<string, unknown>, path: string, value: unknown): Record<string, unknown> {
  const parts = path.split(".");
  const result = structuredClone(obj);
  let current: Record<string, unknown> = result;
  for (let i = 0; i < parts.length - 1; i++) {
    const key = parts[i];
    if (typeof current[key] !== "object" || current[key] === null) {
      current[key] = {};
    }
    current = current[key] as Record<string, unknown>;
  }
  current[parts[parts.length - 1]] = value;
  return result;
}

function setNestedInObject(obj: Record<string, unknown>, path: string, value: unknown): void {
  const parts = path.split(".");
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

function deepMerge(target: Record<string, unknown>, source: Record<string, unknown>): Record<string, unknown> {
  const result = structuredClone(target);
  for (const [key, value] of Object.entries(source)) {
    if (value && typeof value === "object" && !Array.isArray(value) && result[key] && typeof result[key] === "object") {
      result[key] = deepMerge(result[key] as Record<string, unknown>, value as Record<string, unknown>);
    } else {
      result[key] = structuredClone(value);
    }
  }
  return result;
}

function formatLatency(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}
