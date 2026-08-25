import { useEffect, useState } from "react";
import { useParams, Link } from "react-router-dom";
import { getStone, restService, wakeService } from "../api/client";
import type { Stone } from "../types/api";
import "./StoneDetail.css";

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

export function StoneDetailView() {
  const { stoneId } = useParams<{ stoneId: string }>();
  const [stone, setStone] = useState<Stone | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!stoneId) return;
    const load = async () => {
      try {
        const data = await getStone(stoneId);
        setStone(data);
      } catch (e) {
        setError(e instanceof Error ? e.message : "Failed to load stone");
      }
    };
    load();
    const id = setInterval(load, 5000);
    return () => clearInterval(id);
  }, [stoneId]);

  if (error) {
    return (
      <div className="view-empty">
        <div>Stone not found: {stoneId}</div>
        <Link to="/garden" className="back-link">Back to Garden</Link>
      </div>
    );
  }

  if (!stone) {
    return <div className="view-empty">Loading stone...</div>;
  }

  const cpu = stone.resources?.cpu_percent ?? 0;
  const mem = stone.resources?.memory_percent ?? 0;
  const dsk = stone.resources?.disk_percent ?? 0;
  const os = stone.capabilities?.runtime?.os ?? "—";
  const cores = stone.capabilities?.hardware?.cpu.cores ?? "—";
  const ramMb = stone.capabilities?.hardware?.memory.total_mb;

  return (
    <div className="stone-detail">
      <div className="sd-header">
        <Link to="/garden" className="sd-back">Garden</Link>
        <span className="sd-sep">/</span>
        <span
          className="sd-bar"
          style={{ background: healthColor(stone.health) }}
        />
        <h2 className="sd-name">{stone.stone_name}</h2>
        <span
          className="sd-hdot"
          style={{ background: healthColor(stone.health) }}
        />
        <span className="sd-health">{stone.health}</span>
      </div>

      <div className="sd-meta">
        <span>{os}</span>
        <span>{cores} cores</span>
        {ramMb && <span>{Math.round(ramMb / 1024)} GB RAM</span>}
        <span>Moss {stone.moss_version}</span>
      </div>

      <div className="sd-resources">
        <ResourceGauge label="CPU" value={cpu} />
        <ResourceGauge label="Memory" value={mem} />
        <ResourceGauge label="Disk" value={dsk} />
      </div>

      <section className="sd-section">
        <h3 className="sd-section-title">Offerings ({stone.offerings.length})</h3>
        <div className="sd-offerings-grid">
          {stone.offerings.map((o) => (
            <div key={o.offering_id} className="offering-card">
              <div className="offering-head">
                <Link to="/offerings" className="offering-name">
                  {o.name || o.offering}
                  {o.instance_name && (
                    <span className="inst-name">:{o.instance_name}</span>
                  )}
                </Link>
                <span
                  className="offering-hdot"
                  style={{ background: healthColor(o.health) }}
                />
              </div>
              <div className="offering-meta">
                <span>{o.category}</span>
                <span>:{o.port}</span>
                <span className="offering-status">{o.status}</span>
              </div>
              <div className="offering-actions">
                {o.status === "running" ? (
                  <button
                    className="act-btn rest"
                    onClick={() => restService(stone.stone_id, o.name || o.offering)}
                  >
                    Rest
                  </button>
                ) : (
                  <button
                    className="act-btn wake"
                    onClick={() => wakeService(stone.stone_id, o.name || o.offering)}
                  >
                    Wake
                  </button>
                )}
              </div>
            </div>
          ))}
          {stone.offerings.length === 0 && (
            <div className="sd-empty">No offerings deployed</div>
          )}
        </div>
      </section>

      {stone.seed_banks.length > 0 && (
        <section className="sd-section">
          <h3 className="sd-section-title">Seed Banks ({stone.seed_banks.length})</h3>
          <div className="sd-seeds-grid">
            {stone.seed_banks.map((sb) => {
              const pct =
                sb.capacity_bytes > 0
                  ? (sb.used_bytes / sb.capacity_bytes) * 100
                  : 0;
              return (
                <div key={sb.id} className="seed-card">
                  <div className="seed-head">
                    <span className="seed-name">{sb.name}</span>
                    <span className={`seed-status ${sb.online ? "on" : "off"}`}>
                      {sb.online ? "online" : "offline"}
                    </span>
                  </div>
                  <div className="seed-bar-wrap">
                    <div
                      className="seed-bar-fill"
                      style={{
                        width: `${pct}%`,
                        background: resourceColor(pct),
                      }}
                    />
                  </div>
                  <div className="seed-meta">
                    <span>
                      {formatBytes(sb.used_bytes)} / {formatBytes(sb.capacity_bytes)}
                    </span>
                    <span>{sb.visibility}</span>
                  </div>
                </div>
              );
            })}
          </div>
        </section>
      )}

      {stone.companions.length > 0 && (
        <section className="sd-section">
          <h3 className="sd-section-title">Companions ({stone.companions.length})</h3>
          <div className="sd-companions">
            {stone.companions.map((c) => (
              <div key={c.id} className="companion-card">
                <span className="companion-name">{c.name}</span>
                <span className="companion-status">{c.status}</span>
                <span className="companion-port">:{c.port}</span>
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

function ResourceGauge({ label, value }: { label: string; value: number }) {
  return (
    <div className="res-gauge">
      <span
        className="res-gauge-val"
        style={{ color: resourceColor(value) }}
      >
        {Math.round(value)}%
      </span>
      <div className="res-gauge-track">
        <div
          className="res-gauge-fill"
          style={{
            width: `${Math.max(1, value)}%`,
            background: resourceColor(value),
          }}
        />
      </div>
      <span className="res-gauge-label">{label}</span>
    </div>
  );
}
