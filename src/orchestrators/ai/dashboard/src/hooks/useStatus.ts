import { useEffect, useRef, useState, useCallback } from "react";
import type { DashboardStatus } from "../types";

interface UseStatusResult {
  status: DashboardStatus | null;
  loading: boolean;
  error: string | null;
}

export function useStatus(): UseStatusResult {
  const [status, setStatus] = useState<DashboardStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const eventSourceRef = useRef<EventSource | null>(null);

  const fetchStatus = useCallback(async () => {
    try {
      const res = await fetch("/api/status");
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}: ${res.statusText}`);
      }
      const data: DashboardStatus = await res.json();
      setStatus(data);
      setError(null);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : "Failed to fetch status";
      setError(message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchStatus();

    const es = new EventSource("/api/events");
    eventSourceRef.current = es;

    es.onmessage = () => {
      fetchStatus();
    };

    // Named events from the SSE stream trigger a refetch
    const eventTypes = [
      "registry.updated",
      "config.updated",
      "job.created",
      "job.done",
      "benchmark.sample",
      "tending.changed",
    ];

    for (const type of eventTypes) {
      es.addEventListener(type, () => {
        fetchStatus();
      });
    }

    es.onerror = () => {
      // EventSource auto-reconnects; we just refetch on reconnect
      setError("SSE connection lost, reconnecting...");
    };

    return () => {
      es.close();
      eventSourceRef.current = null;
    };
  }, [fetchStatus]);

  return { status, loading, error };
}
