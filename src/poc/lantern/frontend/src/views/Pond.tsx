import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getPond } from "../api/client";
import type { PondMember } from "../types/api";
import "./Pond.css";

function healthColor(h: string): string {
  if (h === "thriving") return "var(--sage)";
  if (h === "withering") return "var(--clay)";
  return "var(--s4)";
}

export function PondView() {
  const [members, setMembers] = useState<PondMember[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const load = async () => {
      try {
        setMembers(await getPond());
      } catch {
        // ignore
      } finally {
        setLoading(false);
      }
    };
    load();
    const id = setInterval(load, 10000);
    return () => clearInterval(id);
  }, []);

  if (loading) return <div className="view-empty">Loading pond...</div>;

  return (
    <div className="pond">
      <div className="pond-header">
        <span className="pond-summary">
          <strong>{members.length}</strong> members in the trust circle
        </span>
      </div>

      <div className="pond-grid">
        {members.map((m) => (
          <Link key={m.stone_id} to={`/stones/${m.stone_name}`} className="pond-card">
            <div className="pond-card-head">
              <span
                className="pond-hdot"
                style={{ background: healthColor(m.health) }}
              />
              <span className="pond-name">{m.stone_name}</span>
              <span className="pond-status">{m.status}</span>
            </div>
            <div className="pond-card-meta">
              <span>{m.endpoint}</span>
              <span>{m.services_count} services</span>
              {m.os && <span>{m.os}</span>}
              {m.cpu_cores && <span>{m.cpu_cores} cores</span>}
              {m.memory_mb && <span>{Math.round(m.memory_mb / 1024)} GB</span>}
            </div>
            {m.tags.length > 0 && (
              <div className="pond-tags">
                {m.tags.map((t) => (
                  <span key={t} className="pond-tag">{t}</span>
                ))}
              </div>
            )}
            {m.mac && <div className="pond-mac">{m.mac}</div>}
          </Link>
        ))}

        {members.length === 0 && (
          <div className="view-empty">No members in the trust circle</div>
        )}
      </div>
    </div>
  );
}
