import { useEffect, useState, useCallback } from "react";
import type { SkillInfo } from "../types";

interface UseSkillsResult {
  skills: SkillInfo[];
  loading: boolean;
}

export function useSkills(): UseSkillsResult {
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [loading, setLoading] = useState(true);

  const fetchSkills = useCallback(async () => {
    try {
      const res = await fetch("/v1/skills");
      if (res.ok) {
        const data: SkillInfo[] = await res.json();
        setSkills(data);
      }
    } catch {
      // Non-fatal — skills are optional
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchSkills();

    // Refetch on SSE events
    const es = new EventSource("/api/events");
    const handler = () => fetchSkills();
    es.addEventListener("registry.updated", handler);

    // Also poll every 10s (skills may change during provisioning)
    const interval = setInterval(fetchSkills, 10_000);

    return () => {
      es.close();
      clearInterval(interval);
    };
  }, [fetchSkills]);

  return { skills, loading };
}
