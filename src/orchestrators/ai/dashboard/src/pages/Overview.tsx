import { Link } from "react-router-dom";
import type { DashboardStatus, CapabilityStatus, StoneStatus } from "../types";
import { formatUptime } from "../types";

interface OverviewProps {
  status: DashboardStatus;
}

// ── Capability Card ─────────────────────────────────────────────

function CapabilityCard({ cap }: { cap: CapabilityStatus }) {
  const border = {
    active: "border-emerald-500/60",
    needs_setup: "border-yellow-500/60",
    not_installed: "border-gray-700",
    degraded: "border-red-500/60",
  }[cap.state];

  const badge = {
    active: (
      <span className="text-[10px] font-semibold text-emerald-400">active</span>
    ),
    needs_setup: (
      <span className="text-[10px] font-semibold text-yellow-400">
        needs setup
      </span>
    ),
    not_installed: (
      <span className="text-[10px] font-semibold text-gray-500">
        not installed
      </span>
    ),
    degraded: (
      <span className="text-[10px] font-semibold text-red-400">degraded</span>
    ),
  }[cap.state];

  return (
    <Link
      to={`/capability/${cap.capability}`}
      className={`block bg-[#1a1b23] border ${border} rounded-lg p-3 hover:bg-[#22232d] transition-colors`}
    >
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm font-medium text-gray-100 capitalize">
          {cap.capability}
        </span>
        {badge}
      </div>

      {cap.state === "active" && (
        <div className="space-y-1 text-[12px]">
          {cap.recommended_model && (
            <div className="text-gray-400">
              Recommended:{" "}
              <span className="text-gray-200 font-mono">
                {cap.recommended_model}
              </span>
            </div>
          )}
          <div className="text-gray-500">
            {cap.offering_count} offering{cap.offering_count !== 1 ? "s" : ""},{" "}
            {cap.model_count} model{cap.model_count !== 1 ? "s" : ""}
          </div>
          <div className="text-gray-500">
            {cap.healthy_instance_count}/{cap.instance_count} instance
            {cap.instance_count !== 1 ? "s" : ""} healthy
          </div>
        </div>
      )}

      {cap.state === "needs_setup" && (
        <div className="space-y-1 text-[12px]">
          <div className="text-gray-400">
            {cap.offerings.join(", ")} installed but no models for this
            capability.
          </div>
          <div className="text-yellow-400/80 mt-1">Pull a model to enable</div>
        </div>
      )}

      {cap.state === "not_installed" && (
        <div className="space-y-1 text-[12px]">
          <div className="text-gray-500">
            No service detected for this capability.
          </div>
          <div className="text-gray-500 mt-1">
            Install an offering to enable.
          </div>
        </div>
      )}

      {cap.state === "degraded" && (
        <div className="space-y-1 text-[12px]">
          <div className="text-red-400/80">
            {cap.instance_count} instance{cap.instance_count !== 1 ? "s" : ""},{" "}
            {cap.healthy_instance_count} healthy
          </div>
          {cap.offerings.length > 0 && (
            <div className="text-gray-500">
              Offerings: {cap.offerings.join(", ")}
            </div>
          )}
        </div>
      )}
    </Link>
  );
}

// ── Stone VRAM Bar ──────────────────────────────────────────────

function StoneVramBar({ stone }: { stone: StoneStatus }) {
  const total = stone.vram_total_mb || 1;
  const used = stone.vram_used_mb;
  const pct = Math.min((used / total) * 100, 100);
  const free = total - used;

  return (
    <Link
      to="/stones"
      className="block bg-[#1a1b23] border border-[#2e303a] rounded-lg p-3 hover:bg-[#22232d] transition-colors"
    >
      <div className="flex items-center justify-between mb-1.5">
        <span className="text-sm text-gray-200 font-medium">{stone.name}</span>
        <span className="text-[11px] text-gray-500">
          {stone.gpu ?? "CPU only"}
        </span>
      </div>
      <div className="w-full h-2 bg-[#2e303a] rounded-full overflow-hidden mb-1">
        <div
          className="h-full bg-emerald-500/70 rounded-full transition-all"
          style={{ width: `${pct}%` }}
        />
      </div>
      <div className="flex justify-between text-[11px] text-gray-500">
        <span>
          {(used / 1024).toFixed(1)} / {(total / 1024).toFixed(1)} GB used
        </span>
        <span>{(free / 1024).toFixed(1)} GB free</span>
      </div>
    </Link>
  );
}

// ── Overview Page ───────────────────────────────────────────────

export function Overview({ status }: OverviewProps) {
  const activeCount = status.capabilities.filter(
    (c) => c.state === "active",
  ).length;

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-medium text-gray-100">Overview</h2>
          <p className="text-[12px] text-gray-500">
            {activeCount} of {status.capabilities.length} capabilities active
            &middot; uptime {formatUptime(status.uptime_secs)}
          </p>
        </div>
      </div>

      {/* Capability Grid */}
      <section>
        <h3 className="text-[11px] uppercase tracking-wider text-gray-500 font-semibold mb-3">
          Capabilities
        </h3>
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
          {status.capabilities.map((cap) => (
            <CapabilityCard key={cap.capability} cap={cap} />
          ))}
        </div>
      </section>

      {/* Stone VRAM */}
      {status.stones.length > 0 && (
        <section>
          <h3 className="text-[11px] uppercase tracking-wider text-gray-500 font-semibold mb-3">
            Stone VRAM
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {status.stones.map((stone) => (
              <StoneVramBar key={stone.id} stone={stone} />
            ))}
          </div>
        </section>
      )}

      {/* Recent Jobs */}
      {status.jobs.length > 0 && (
        <section>
          <h3 className="text-[11px] uppercase tracking-wider text-gray-500 font-semibold mb-3">
            Recent Activity
          </h3>
          <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg divide-y divide-[#2e303a]">
            {status.jobs.slice(0, 10).map((job) => (
              <div
                key={job.id}
                className="flex items-center gap-3 px-3 py-2 text-[12px]"
              >
                <span
                  className={`w-1.5 h-1.5 rounded-full shrink-0 ${
                    job.status === "Completed"
                      ? "bg-emerald-400"
                      : job.status === "Running"
                        ? "bg-blue-400"
                        : job.status === "Failed"
                          ? "bg-red-400"
                          : "bg-gray-500"
                  }`}
                />
                <span className="text-gray-400 font-mono">{job.id}</span>
                <span className="text-gray-300">
                  {Object.keys(job.kind)[0] ?? "unknown"}
                </span>
                {job.progress && (
                  <span className="text-gray-500">{job.progress}</span>
                )}
                <span className="ml-auto text-gray-500 text-[11px]">
                  {job.status}
                </span>
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
