import { Link } from "react-router-dom";
import { useStones } from "../hooks/useStones";
import "./Garden.css";

function healthColor(h: string): string {
  if (h === "thriving") return "var(--sage)";
  if (h === "withering") return "var(--clay)";
  return "var(--s4)";
}

function resourceColor(p: number): string {
  if (p > 85) return "var(--red)";
  if (p > 70) return "var(--clay)";
  return "var(--sage)";
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

export function GardenView() {
  const { stones, loading } = useStones();

  if (loading) {
    return <div className="view-empty">Loading...</div>;
  }

  const online = stones.filter((s) => s.status === "online").length;
  const totalServices = stones.reduce((n, s) => n + s.services.length, 0);

  return (
    <div className="garden">
      <div className="garden-header">
        <div className="garden-summary">
          <span className="gs-item">
            <strong>{stones.length}</strong> stones
          </span>
          <span className="gs-sep">/</span>
          <span className="gs-item">
            <strong>{online}</strong> online
          </span>
          <span className="gs-sep">/</span>
          <span className="gs-item">
            <strong>{totalServices}</strong> services
          </span>
        </div>
      </div>

      <div className="garden-grid">
        {stones.map((stone) => {
          const cpu = stone.resources?.cpu_percent ?? 0;
          const mem = stone.resources?.memory_percent ?? 0;
          const dsk = stone.resources?.disk_percent ?? 0;

          return (
            <Link
              key={stone.stone_id}
              to={`/stones/${stone.stone_name}`}
              className="stone-card"
            >
              <div
                className="stone-card-bar"
                style={{ background: healthColor(stone.health) }}
              />
              <div className="stone-card-body">
                <div className="stone-card-head">
                  <span className="stone-card-name">{stone.stone_name}</span>
                  <span
                    className="stone-card-hdot"
                    style={{ background: healthColor(stone.health) }}
                  />
                </div>

                <div className="stone-card-resources">
                  <ResourceBar label="CPU" value={cpu} />
                  <ResourceBar label="MEM" value={mem} />
                  <ResourceBar label="DSK" value={dsk} />
                </div>

                <div className="stone-card-services">
                  {stone.services.map((svc) => (
                    <span key={svc.offering_id} className="svc-chip">
                      {svc.name || svc.offering}
                    </span>
                  ))}
                  {stone.services.length === 0 && (
                    <span className="svc-chip empty">no services</span>
                  )}
                </div>

                {stone.seed_banks.length > 0 && (
                  <div className="stone-card-seeds">
                    {stone.seed_banks.map((sb) => (
                      <span key={sb.id} className="seed-chip">
                        {sb.name} ({formatBytes(sb.used_bytes)} / {formatBytes(sb.capacity_bytes)})
                      </span>
                    ))}
                  </div>
                )}
              </div>
            </Link>
          );
        })}
      </div>
    </div>
  );
}

function ResourceBar({ label, value }: { label: string; value: number }) {
  return (
    <div className="res-bar">
      <span className="res-label">{label}</span>
      <div className="res-track">
        <div
          className="res-fill"
          style={{
            width: `${Math.max(1, value)}%`,
            background: resourceColor(value),
          }}
        />
      </div>
      <span className="res-val">{Math.round(value)}%</span>
    </div>
  );
}
