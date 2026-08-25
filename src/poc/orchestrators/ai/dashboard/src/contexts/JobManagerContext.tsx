import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  useCallback,
  type ReactNode,
} from "react";
import { get } from "../api/client";
import type { JobView, JobListResponse } from "../api/types";
import { useSSE } from "../hooks/useSSE";

interface TrackedJob {
  view: JobView;
  result?: unknown;
  error?: unknown;
}

interface JobManagerState {
  /** Register a job for tracking. */
  track: (jobId: string, action?: string) => void;
  /** Get reactive state for a specific job. */
  jobs: Map<string, TrackedJob>;
  /** Most recent jobs (from SSE + initial fetch). */
  recentJobs: JobView[];
  /** SSE connection status. */
  connected: boolean;
}

const JobManagerContext = createContext<JobManagerState>({
  track: () => {},
  jobs: new Map(),
  recentJobs: [],
  connected: false,
});

const MAX_RECENT = 50;

export function JobManagerProvider({ children }: { children: ReactNode }) {
  const [jobs, setJobs] = useState<Map<string, TrackedJob>>(new Map());
  const [recentJobs, setRecentJobs] = useState<JobView[]>([]);
  const jobsRef = useRef(jobs);
  jobsRef.current = jobs;

  // Load initial job list
  useEffect(() => {
    get<JobListResponse>("/v1/jobs")
      .then((data) => {
        setRecentJobs(data.jobs.slice(0, MAX_RECENT));
      })
      .catch(() => {
        // Non-fatal — jobs will populate from SSE
      });
  }, []);

  const track = useCallback((jobId: string, action?: string) => {
    setJobs((prev) => {
      const next = new Map(prev);
      if (!next.has(jobId)) {
        next.set(jobId, {
          view: {
            id: jobId,
            correlation_id: "",
            category: "api",
            action,
            state: "queued",
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
          },
        });
      }
      return next;
    });
  }, []);

  const handleJobEvent = useCallback(
    (topic: string, payload: unknown) => {
      // Topics: jobs.{id}.state, jobs.{id}.progress, jobs.{id}.result
      const parts = topic.split(".");
      if (parts.length < 3 || parts[0] !== "jobs") return;
      const jobId = parts[1];
      const eventType = parts.slice(2).join(".");
      const data = payload as Record<string, unknown>;

      setJobs((prev) => {
        const next = new Map(prev);
        const existing = next.get(jobId);

        if (eventType === "state" || eventType === "created") {
          const state = (data.state as string) ?? "queued";
          const view: JobView = existing?.view
            ? { ...existing.view, state: state as JobView["state"], updated_at: new Date().toISOString() }
            : {
                id: jobId,
                correlation_id: (data.correlation_id as string) ?? "",
                category: (data.category as JobView["category"]) ?? "api",
                action: data.action as string | undefined,
                state: state as JobView["state"],
                created_at: (data.created_at as string) ?? new Date().toISOString(),
                updated_at: new Date().toISOString(),
              };
          next.set(jobId, { ...existing, view });

          // Update recent jobs list
          setRecentJobs((prev) => {
            const filtered = prev.filter((j) => j.id !== jobId);
            return [view, ...filtered].slice(0, MAX_RECENT);
          });

          // Fetch result on terminal state
          if (state === "done" || state === "failed") {
            get<unknown>(`/v1/jobs/${jobId}/result`)
              .then((result) => {
                setJobs((p) => {
                  const n = new Map(p);
                  const j = n.get(jobId);
                  if (j) n.set(jobId, { ...j, result });
                  return n;
                });
              })
              .catch(() => {});
          }
        } else if (eventType === "progress") {
          if (existing) {
            next.set(jobId, {
              ...existing,
              view: {
                ...existing.view,
                progress: data as unknown as JobView["progress"],
                updated_at: new Date().toISOString(),
              },
            });
          }
        } else if (eventType === "result") {
          if (existing) {
            next.set(jobId, { ...existing, result: data });
          }
        }
        return next;
      });
    },
    [],
  );

  const { connected } = useSSE({
    focus: "jobs.*",
    onEvent: handleJobEvent,
  });

  return (
    <JobManagerContext.Provider value={{ track, jobs, recentJobs, connected }}>
      {children}
    </JobManagerContext.Provider>
  );
}

export function useJobManager(): JobManagerState {
  return useContext(JobManagerContext);
}

export function useJob(id: string | undefined): TrackedJob | undefined {
  const { jobs } = useJobManager();
  return id ? jobs.get(id) : undefined;
}
