import { useState, useCallback, useRef, useEffect } from "react";
import type { CatalogDetail } from "../../api/types";
import { dispatch as apiDispatch } from "../../api/client";
import { useJobManager } from "../../contexts/JobManagerContext";

interface ChatTurn {
  user: string;
  assistant: string;
}

interface Props {
  detail: CatalogDetail;
  onResult: (result: unknown) => void;
  onError: (error: unknown) => void;
}

/**
 * Conversation UI for text.chat. Renders a message thread with
 * accumulated history, using text.prompt.previous for turns.
 *
 * Detection: the Workspace component renders this instead of
 * WorkspaceForm when the catalog detail includes a field with
 * path "text.prompt.previous".
 */
export default function ChatWorkspace({ detail, onResult, onError }: Props) {
  const [turns, setTurns] = useState<ChatTurn[]>(() => {
    try {
      const saved = localStorage.getItem(`chat-history:${detail.path}`);
      return saved ? JSON.parse(saved) : [];
    } catch {
      return [];
    }
  });
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settings, setSettings] = useState<Record<string, unknown>>({});
  const threadRef = useRef<HTMLDivElement>(null);
  const { track } = useJobManager();

  // Extract settings fields (non-primary, non-hidden)
  const fields = detail.fields ?? [];
  const settingsFields = fields.filter(
    (f) =>
      !f.required &&
      f.widget !== "hidden" &&
      f.field !== "text.prompt.user" &&
      f.field !== "text.prompt.previous",
  );

  // Scroll to bottom on new content
  useEffect(() => {
    threadRef.current?.scrollTo({ top: threadRef.current.scrollHeight, behavior: "smooth" });
  }, [turns, streaming]);

  // Persist turns
  useEffect(() => {
    try {
      localStorage.setItem(`chat-history:${detail.path}`, JSON.stringify(turns));
    } catch { /* ignore */ }
  }, [turns, detail.path]);

  const handleSend = useCallback(async () => {
    const text = input.trim();
    if (!text || submitting) return;
    setSubmitting(true);
    setInput("");
    setStreaming("");

    // Build payload with canonical vocabulary fields
    const payload: Record<string, unknown> = {
      text: {
        prompt: {
          user: text,
          ...(turns.length > 0 ? { previous: turns } : {}),
        },
        ...buildSettings(settings),
      },
    };

    try {
      const idempotencyKey = crypto.randomUUID();
      const url = `/v1/${detail.path.replace(/\./g, "/")}`;
      const response = await apiDispatch(url, payload, idempotencyKey);
      const contentType = response.headers.get("content-type") ?? "";

      if (contentType.includes("text/event-stream") && response.body) {
        // Streaming response — read tokens incrementally
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";
        let fullText = "";

        const read = async () => {
          while (true) {
            const { done, value } = await reader.read();
            if (done) break;
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
                    setStreaming(fullText);
                  }
                  if (data?.output?.text?.finish_reason || data?.done) {
                    const finalText =
                      data?.output?.text?.response ?? fullText;
                    setTurns((prev) => [...prev, { user: text, assistant: finalText }]);
                    setStreaming("");
                    onResult(data);
                  }
                } catch { /* ignore malformed */ }
              }
            }
          }
          // If stream ended without explicit finish, use accumulated text
          if (fullText && !streaming) {
            setTurns((prev) => [...prev, { user: text, assistant: fullText }]);
            setStreaming("");
          }
        };
        await read();
      } else if (response.ok) {
        // Sync response
        const body = await response.json();
        if (body.error) {
          onError(body);
        } else {
          const assistantText =
            body?.output?.text?.response ?? JSON.stringify(body?.output ?? {});
          setTurns((prev) => [...prev, { user: text, assistant: assistantText }]);
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
  }, [input, submitting, turns, settings, detail.path, track, onResult, onError]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend],
  );

  const clearHistory = useCallback(() => {
    setTurns([]);
    setStreaming("");
    try {
      localStorage.removeItem(`chat-history:${detail.path}`);
    } catch { /* ignore */ }
  }, [detail.path]);

  return (
    <div className="flex flex-col h-full">
      {/* Settings bar */}
      {settingsFields.length > 0 && (
        <div className="border-b border-border px-4 py-1.5 shrink-0">
          <button
            onClick={() => setSettingsOpen((p) => !p)}
            className="text-[10px] text-text-dim hover:text-text flex items-center gap-1"
          >
            <span
              className={[
                "inline-block w-[4px] h-[4px] border-r border-b border-text-dim transition-transform",
                settingsOpen ? "rotate-45" : "-rotate-45",
              ].join(" ")}
            />
            Settings
          </button>
          {settingsOpen && (
            <div className="grid grid-cols-3 gap-3 mt-2 pb-1">
              {settingsFields.map((f) => (
                <div key={f.field} className="text-[10px]">
                  <label className="text-text-dimmer">{f.label ?? f.field}</label>
                  {f.widget === "select" && f.options ? (
                    <select
                      className="w-full mt-0.5 px-1.5 py-1 bg-surface-2 border border-border rounded text-[11px] text-text"
                      value={String(settings[f.field] ?? "")}
                      onChange={(e) =>
                        setSettings((p) => ({ ...p, [f.field]: e.target.value || undefined }))
                      }
                    >
                      {f.auto && <option value="">Auto</option>}
                      {f.options.map((o) => (
                        <option key={String(o)} value={String(o)}>
                          {String(o)}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <input
                      type={f.field_type === "number" ? "number" : "text"}
                      className="w-full mt-0.5 px-1.5 py-1 bg-surface-2 border border-border rounded text-[11px] text-text"
                      value={String(settings[f.field] ?? f.default ?? "")}
                      onChange={(e) =>
                        setSettings((p) => ({ ...p, [f.field]: e.target.value }))
                      }
                    />
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Message thread */}
      <div ref={threadRef} className="flex-1 overflow-y-auto p-4 space-y-3">
        {turns.length === 0 && !streaming && (
          <div className="flex items-center justify-center h-full text-text-dimmer text-sm italic">
            Start a conversation
          </div>
        )}
        {turns.map((turn, i) => (
          <div key={i}>
            <MessageBubble role="user" text={turn.user} />
            <MessageBubble role="assistant" text={turn.assistant} />
          </div>
        ))}
        {streaming && (
          <div>
            <MessageBubble role="user" text={input || turns[turns.length - 1]?.user || ""} />
            <MessageBubble role="assistant" text={streaming} />
          </div>
        )}
      </div>

      {/* Input bar */}
      <div className="border-t border-border p-3 shrink-0">
        <div className="flex gap-2">
          <textarea
            className="flex-1 p-2.5 bg-surface-2 border border-border rounded-lg text-[13px] text-text
                       placeholder:text-text-dimmer outline-none focus:border-accent resize-none"
            placeholder="Type a message..."
            rows={2}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={submitting}
          />
          <div className="flex flex-col gap-1">
            <button
              onClick={handleSend}
              disabled={submitting || !input.trim()}
              className="px-4 py-2 bg-accent hover:bg-accent-dim text-white text-[11px] font-semibold
                         rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {submitting ? "..." : "Send"}
            </button>
            {turns.length > 0 && (
              <button
                onClick={clearHistory}
                className="px-4 py-1 text-[9px] text-text-dimmer hover:text-text-dim transition-colors"
              >
                Clear
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function MessageBubble({ role, text }: { role: "user" | "assistant"; text: string }) {
  const isUser = role === "user";
  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"} mb-1.5`}>
      <div
        className={[
          "max-w-[80%] px-3 py-2 rounded-lg text-[13px] leading-relaxed",
          isUser
            ? "bg-accent/15 text-text"
            : "bg-surface-2 text-text",
        ].join(" ")}
      >
        <div className="whitespace-pre-wrap">{text}</div>
      </div>
    </div>
  );
}

/** Build the settings portion of the payload from non-default values. */
function buildSettings(settings: Record<string, unknown>): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [dotted, value] of Object.entries(settings)) {
    if (value === undefined || value === null || value === "") continue;
    const parts = dotted.replace("text.", "").split(".");
    let current = result;
    for (let i = 0; i < parts.length - 1; i++) {
      const key = parts[i];
      if (typeof current[key] !== "object" || current[key] === null) {
        current[key] = {};
      }
      current = current[key] as Record<string, unknown>;
    }
    current[parts[parts.length - 1]] = value;
  }
  return result;
}
