import { useEffect, useRef, useState, useCallback } from "react";

interface UseSSEOptions {
  /** Comma-separated glob patterns for /v1/events?focus=... */
  focus: string;
  /** Called for each event received. */
  onEvent: (topic: string, payload: unknown) => void;
  /** Whether to connect. Set false to pause. */
  enabled?: boolean;
}

interface UseSSEState {
  connected: boolean;
  lastSeq: number;
}

/**
 * Opens an SSE connection to /v1/events with the given focus pattern.
 * Auto-reconnects with backoff on drop. Tracks Last-Event-ID for
 * resumption. Closes on unmount or when focus changes.
 */
export function useSSE({ focus, onEvent, enabled = true }: UseSSEOptions): UseSSEState {
  const [connected, setConnected] = useState(false);
  const [lastSeq, setLastSeq] = useState(0);
  const lastSeqRef = useRef(0);
  const onEventRef = useRef(onEvent);
  onEventRef.current = onEvent;

  const connect = useCallback(() => {
    if (!enabled || !focus) return undefined;

    const params = new URLSearchParams({ focus });
    if (lastSeqRef.current > 0) {
      params.set("since", String(lastSeqRef.current));
    }
    const url = `/v1/events?${params}`;
    const es = new EventSource(url);

    es.onopen = () => setConnected(true);

    es.onmessage = (e) => {
      try {
        const data = JSON.parse(e.data);
        const seq = Number(e.lastEventId || data.seq || 0);
        if (seq > lastSeqRef.current) {
          lastSeqRef.current = seq;
          setLastSeq(seq);
        }
        onEventRef.current(data.topic, data.payload ?? data);
      } catch {
        // Ignore malformed events
      }
    };

    es.onerror = () => {
      setConnected(false);
      es.close();
    };

    return es;
  }, [focus, enabled]);

  useEffect(() => {
    let es = connect();
    let retryTimeout: ReturnType<typeof setTimeout>;
    let retryDelay = 1000;

    function handleError() {
      // Reconnect with backoff
      retryTimeout = setTimeout(() => {
        es = connect();
        if (es) {
          es.onerror = () => {
            setConnected(false);
            es?.close();
            retryDelay = Math.min(retryDelay * 2, 30000);
            handleError();
          };
        }
      }, retryDelay);
    }

    if (es) {
      es.onerror = () => {
        setConnected(false);
        es?.close();
        handleError();
      };
    }

    return () => {
      clearTimeout(retryTimeout);
      es?.close();
      setConnected(false);
    };
  }, [connect]);

  return { connected, lastSeq };
}
