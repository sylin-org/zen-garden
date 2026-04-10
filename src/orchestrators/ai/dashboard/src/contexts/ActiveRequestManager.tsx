import {
  createContext,
  useContext,
  useState,
  useCallback,
  useRef,
  useEffect,
  type ReactNode,
} from "react";
// Dispatch is handled via raw fetch — the manager owns the lifecycle.

// ── Types ────────────────────────────────────────────────────

interface Turn {
  user: string;
  assistant: string;
}

export type ActiveStatus = "sending" | "streaming" | "done" | "failed";

export interface ActiveRequest {
  id: string;
  action: string;
  status: ActiveStatus;
  startedAt: number;
  elapsed: number;
  payload: Record<string, unknown>;
  result?: unknown;
  error?: unknown;
  streamAccumulator: string;
  /** For dialogue: accumulated turns from streaming. */
  dialogueTurns: Turn[];
  /** The user's message that triggered this dispatch (for dialogue). */
  userMessage?: string;
  /** Field path of the dialogue history field (e.g. "text.prompt.history"). */
  dialogueField?: string;
  abortController: AbortController;
}

interface ManagerState {
  /** Dispatch a request. Returns the client-side request ID. */
  dispatch: (params: DispatchParams) => string;
  /** Get all active (in-flight) requests. */
  active: ActiveRequest[];
  /** Get a specific request by ID. */
  get: (id: string) => ActiveRequest | undefined;
  /** Cancel an in-flight request. */
  cancel: (id: string) => void;
}

export interface DispatchParams {
  url: string;
  payload: Record<string, unknown>;
  action: string;
  /** For dialogue mode: the user's current message. */
  userMessage?: string;
  /** For dialogue mode: the field path of the history field. */
  dialogueField?: string;
  /** Callback when result arrives (for form state updates). */
  onResult?: (result: unknown) => void;
  /** Callback on error. */
  onError?: (error: unknown) => void;
  /** Callback with accumulated stream text (for live rendering). */
  onStreamDelta?: (text: string) => void;
  /** Callback when a dialogue turn completes. */
  onTurnComplete?: (turn: Turn) => void;
}

// ── Context ──────────────────────────────────────────────────

const ActiveRequestContext = createContext<ManagerState>({
  dispatch: () => "",
  active: [],
  get: () => undefined,
  cancel: () => {},
});

// ── Provider ─────────────────────────────────────────────────

export function ActiveRequestProvider({ children }: { children: ReactNode }) {
  const [requests, setRequests] = useState<Map<string, ActiveRequest>>(new Map());
  const requestsRef = useRef(requests);
  requestsRef.current = requests;

  // Elapsed timer — updates every 200ms for all active requests.
  useEffect(() => {
    const interval = setInterval(() => {
      setRequests((prev) => {
        let changed = false;
        const next = new Map(prev);
        for (const [id, req] of next) {
          if (req.status === "sending" || req.status === "streaming") {
            const elapsed = (Date.now() - req.startedAt) / 1000;
            if (Math.abs(elapsed - req.elapsed) >= 0.1) {
              next.set(id, { ...req, elapsed });
              changed = true;
            }
          }
        }
        return changed ? next : prev;
      });
    }, 200);
    return () => clearInterval(interval);
  }, []);

  const updateRequest = useCallback(
    (id: string, updates: Partial<ActiveRequest>) => {
      setRequests((prev) => {
        const existing = prev.get(id);
        if (!existing) return prev;
        const next = new Map(prev);
        next.set(id, { ...existing, ...updates });
        return next;
      });
    },
    [],
  );

  const dispatchRequest = useCallback(
    (params: DispatchParams): string => {
      const id = crypto.randomUUID();
      const controller = new AbortController();

      const entry: ActiveRequest = {
        id,
        action: params.action,
        status: "sending",
        startedAt: Date.now(),
        elapsed: 0,
        payload: params.payload,
        streamAccumulator: "",
        dialogueTurns: [],
        userMessage: params.userMessage,
        dialogueField: params.dialogueField,
        abortController: controller,
      };

      setRequests((prev) => {
        const next = new Map(prev);
        next.set(id, entry);
        return next;
      });

      // Fire the HTTP request asynchronously.
      (async () => {
        try {
          const response = await fetch(params.url, {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "idempotency-key": id,
            },
            body: JSON.stringify(params.payload),
            signal: controller.signal,
          });

          const contentType = response.headers.get("content-type") ?? "";

          if (contentType.includes("text/event-stream") && response.body) {
            // Streaming response
            updateRequest(id, { status: "streaming" });
            const reader = response.body.getReader();
            const decoder = new TextDecoder();
            let buffer = "";
            let fullText = "";

            const read = async (): Promise<void> => {
              while (true) {
                const { done, value } = await reader.read();
                if (done) break;
                buffer += decoder.decode(value, { stream: true });
                const lines = buffer.split("\n");
                buffer = lines.pop() ?? "";

                for (const line of lines) {
                  if (!line.startsWith("data: ")) continue;
                  try {
                    const data = JSON.parse(line.slice(6));
                    const delta =
                      data?.output?.text?.delta ??
                      data?.output?.text?.response ??
                      data?.text?.delta;

                    if (typeof delta === "string") {
                      fullText += delta;
                      updateRequest(id, { streamAccumulator: fullText });
                      params.onStreamDelta?.(fullText);
                    }

                    if (data?.output?.text?.finish_reason || data?.done) {
                      const finalText = data?.output?.text?.response ?? fullText;
                      updateRequest(id, {
                        status: "done",
                        result: data,
                        streamAccumulator: finalText,
                      });
                      params.onResult?.(data);

                      // Dialogue: complete the turn
                      if (params.userMessage) {
                        const turn = { user: params.userMessage, assistant: finalText };
                        params.onTurnComplete?.(turn);
                      }
                      return;
                    }
                  } catch {
                    /* ignore malformed SSE */
                  }
                }
              }

              // Stream ended without explicit finish — use accumulated text
              if (fullText) {
                updateRequest(id, { status: "done", streamAccumulator: fullText });
                if (params.userMessage) {
                  const turn = { user: params.userMessage, assistant: fullText };
                  params.onTurnComplete?.(turn);
                }
              }
            };

            await read();
          } else if (response.ok) {
            // Sync or async response
            const body = await response.json();
            if (body.error) {
              updateRequest(id, { status: "failed", error: body });
              params.onError?.(body);
            } else {
              updateRequest(id, { status: "done", result: body });
              params.onResult?.(body);

              // Dialogue: complete the turn
              if (params.userMessage) {
                const assistantText =
                  body?.output?.text?.response ?? "";
                if (assistantText) {
                  const turn = { user: params.userMessage, assistant: assistantText };
                  params.onTurnComplete?.(turn);
                }
              }
            }
          } else {
            const body = await response
              .json()
              .catch(() => ({ error: { message: response.statusText } }));
            updateRequest(id, { status: "failed", error: body });
            params.onError?.(body);
          }
        } catch (e) {
          if ((e as Error).name === "AbortError") {
            updateRequest(id, { status: "failed", error: { error: { code: "cancelled", message: "Request cancelled" } } });
          } else {
            const error = { error: { code: "network", message: e instanceof Error ? e.message : "Network error" } };
            updateRequest(id, { status: "failed", error });
            params.onError?.(error);
          }
        }
      })();

      return id;
    },
    [updateRequest],
  );

  const cancelRequest = useCallback(
    (id: string) => {
      const req = requestsRef.current.get(id);
      if (req && (req.status === "sending" || req.status === "streaming")) {
        req.abortController.abort();
        updateRequest(id, { status: "failed" });
      }
    },
    [updateRequest],
  );

  const getRequest = useCallback(
    (id: string) => requestsRef.current.get(id),
    [],
  );

  const active = Array.from(requests.values()).filter(
    (r) => r.status === "sending" || r.status === "streaming",
  );

  return (
    <ActiveRequestContext.Provider
      value={{
        dispatch: dispatchRequest,
        active,
        get: getRequest,
        cancel: cancelRequest,
      }}
    >
      {children}
    </ActiveRequestContext.Provider>
  );
}

// ── Hooks ────────────────────────────────────────────────────

export function useActiveRequestManager(): ManagerState {
  return useContext(ActiveRequestContext);
}

export function useActiveRequest(id: string | undefined): ActiveRequest | undefined {
  const { get } = useActiveRequestManager();
  const [, setTick] = useState(0);
  const idRef = useRef(id);
  idRef.current = id;

  // Force re-render when the elapsed timer updates
  useEffect(() => {
    const interval = setInterval(() => {
      if (idRef.current) {
        const req = get(idRef.current);
        if (req && (req.status === "sending" || req.status === "streaming")) {
          setTick((t) => t + 1);
        }
      }
    }, 200);
    return () => clearInterval(interval);
  }, [get]);

  return id ? get(id) : undefined;
}
