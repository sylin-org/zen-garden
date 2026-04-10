import { useJobManager } from "../../contexts/JobManagerContext";

export default function JobList() {
  const { recentJobs } = useJobManager();

  return (
    <div className="flex h-full">
      {/* Master: job list */}
      <div className="flex-1 overflow-y-auto">
        {recentJobs.length === 0 ? (
          <div className="p-4 text-text-dimmer text-sm italic">No jobs yet</div>
        ) : (
          recentJobs.map((job) => (
            <div
              key={job.id}
              className="flex items-center gap-3 px-4 py-2.5 border-b border-border hover:bg-surface transition-colors"
            >
              <StatusDot state={job.state} />
              <div className="flex-1 min-w-0">
                <div className="text-[12px] font-medium truncate">
                  {job.action ?? "unknown"}
                </div>
                <div className="text-[10px] text-text-dimmer">
                  {job.id.slice(0, 12)}... · {job.state}
                </div>
              </div>
              <div className="text-[10px] text-text-dimmer shrink-0">
                {formatTime(job.created_at)}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function StatusDot({ state }: { state: string }) {
  const color =
    state === "done" ? "bg-green"
    : state === "failed" ? "bg-red"
    : state === "running" ? "bg-orange"
    : "bg-text-dimmer";
  return <div className={`w-2 h-2 rounded-full shrink-0 ${color}`} />;
}

function formatTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}
