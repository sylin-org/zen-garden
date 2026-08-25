import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getSeeds } from "../api/client";
import type { SeedBankView } from "../types/api";
import "./SeedBanks.css";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
}

function resourceColor(p: number): string {
  if (p > 85) return "var(--red)";
  if (p > 70) return "var(--clay)";
  return "var(--sage)";
}

export function SeedBanksView() {
  const [seeds, setSeeds] = useState<SeedBankView[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const load = async () => {
      try {
        setSeeds(await getSeeds());
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

  if (loading) return <div className="view-empty">Loading seed banks...</div>;

  // Group by seed bank name (identity groups)
  const groups = new Map<string, SeedBankView[]>();
  for (const sb of seeds) {
    const list = groups.get(sb.name) ?? [];
    list.push(sb);
    groups.set(sb.name, list);
  }

  const totalCap = seeds.reduce((n, s) => n + s.capacity_bytes, 0);
  const totalUsed = seeds.reduce((n, s) => n + s.used_bytes, 0);

  return (
    <div className="seed-banks">
      <div className="sb-header">
        <span className="sb-summary">
          <strong>{seeds.length}</strong> banks / <strong>{groups.size}</strong> identity groups /
          {" "}{formatBytes(totalUsed)} of {formatBytes(totalCap)} used
        </span>
      </div>

      <div className="sb-groups">
        {Array.from(groups.entries()).map(([name, members]) => {
          const groupCap = members.reduce((n, s) => n + s.capacity_bytes, 0);
          const groupUsed = members.reduce((n, s) => n + s.used_bytes, 0);
          const pct = groupCap > 0 ? (groupUsed / groupCap) * 100 : 0;

          return (
            <div key={name} className="sb-group">
              <div className="sb-group-head">
                <span className="sb-group-name">{name}</span>
                {members.length > 1 && (
                  <span className="sb-replica-badge">{members.length}-way replica</span>
                )}
                <span className="sb-group-usage">
                  {formatBytes(groupUsed)} / {formatBytes(groupCap)}
                </span>
              </div>
              <div className="sb-group-bar">
                <div
                  className="sb-group-fill"
                  style={{ width: `${pct}%`, background: resourceColor(pct) }}
                />
              </div>
              <div className="sb-members">
                {members.map((sb) => {
                  const mbPct =
                    sb.capacity_bytes > 0
                      ? (sb.used_bytes / sb.capacity_bytes) * 100
                      : 0;
                  return (
                    <div key={`${sb.stone_id}-${sb.id}`} className="sb-member">
                      <span className={`sb-status ${sb.online ? "on" : "off"}`} />
                      <Link to={`/stones/${sb.stone_name}`} className="sb-stone">{sb.stone_name}</Link>
                      <span className="sb-vis">{sb.visibility}</span>
                      <div className="sb-mini-bar">
                        <div
                          className="sb-mini-fill"
                          style={{ width: `${mbPct}%`, background: resourceColor(mbPct) }}
                        />
                      </div>
                      <span className="sb-usage-text">
                        {formatBytes(sb.used_bytes)} / {formatBytes(sb.capacity_bytes)}
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}

        {seeds.length === 0 && (
          <div className="view-empty">No seed banks in the garden</div>
        )}
      </div>
    </div>
  );
}
