import { useEffect, useState, useCallback } from "react";
import { getStones } from "../api/client";
import type { Stone } from "../types/api";

/** Polls GET /api/v1/garden/stones on an interval */
export function useStones(intervalMs = 5000) {
  const [stones, setStones] = useState<Stone[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const data = await getStones();
      setStones(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, intervalMs);
    return () => clearInterval(id);
  }, [refresh, intervalMs]);

  return { stones, error, loading, refresh };
}
