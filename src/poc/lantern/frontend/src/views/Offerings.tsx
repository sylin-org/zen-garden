import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { getOfferings } from "../api/client";
import type { OfferingGroup } from "../types/api";
import "./Offerings.css";

function healthColor(h: string): string {
  if (h === "thriving") return "var(--sage)";
  if (h === "withering") return "var(--clay)";
  return "var(--s4)";
}

export function OfferingsView() {
  const [groups, setGroups] = useState<OfferingGroup[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const load = async () => {
      try {
        setGroups(await getOfferings());
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

  if (loading) return <div className="view-empty">Loading offerings...</div>;

  const totalInstances = groups.reduce((n, g) => n + g.instances.length, 0);

  return (
    <div className="offerings">
      <div className="off-header">
        <span className="off-summary">
          <strong>{groups.length}</strong> offering types / <strong>{totalInstances}</strong> instances
        </span>
      </div>

      <div className="off-grid">
        {groups.map((group) => (
          <div key={group.offering} className="off-group">
            <div className="off-group-head">
              <span className="off-group-name">{group.offering}</span>
              <span className="off-group-cat">{group.category}</span>
              <span className="off-group-count">{group.instances.length} instance{group.instances.length !== 1 ? "s" : ""}</span>
            </div>
            <div className="off-instances">
              {group.instances.map((inst) => (
                <div key={`${inst.stone_id}-${inst.offering_id}`} className="off-inst">
                  <span
                    className="off-inst-dot"
                    style={{ background: healthColor(inst.health) }}
                  />
                  <Link to={`/stones/${inst.stone_name}`} className="off-inst-stone">{inst.stone_name}</Link>
                  <span className="off-inst-name">{inst.name}</span>
                  <span className="off-inst-port">:{inst.port}</span>
                  <span className="off-inst-status">{inst.status}</span>
                </div>
              ))}
            </div>
          </div>
        ))}

        {groups.length === 0 && (
          <div className="view-empty">No offerings deployed in the garden</div>
        )}
      </div>
    </div>
  );
}
