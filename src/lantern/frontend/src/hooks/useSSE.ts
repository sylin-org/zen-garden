import { useEffect, useRef, useState } from "react";
import type { ActivityEvent } from "../types/api";

/** Connects to the Lantern SSE presence stream */
export function useSSE() {
  const [events, setEvents] = useState<ActivityEvent[]>([]);
  const [connected, setConnected] = useState(false);
  const esRef = useRef<EventSource | null>(null);

  useEffect(() => {
    const es = new EventSource("/api/v1/garden/presence/stream");
    esRef.current = es;

    es.onopen = () => setConnected(true);
    es.onerror = () => setConnected(false);

    // Listen for all named event types
    const handler = (e: MessageEvent) => {
      try {
        const data = JSON.parse(e.data) as ActivityEvent;
        setEvents((prev) => [data, ...prev].slice(0, 100));
      } catch {
        // ignore malformed events
      }
    };

    // Register for known event types
    for (const type of [
      "snapshot",
      "stone.registered",
      "stone.heartbeat",
      "stone.offline",
      "topology.refreshed",
    ]) {
      es.addEventListener(type, handler);
    }

    return () => {
      es.close();
      esRef.current = null;
    };
  }, []);

  return { events, connected };
}
