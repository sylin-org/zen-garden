import { NavLink } from "react-router-dom";
import { useStones } from "../hooks/useStones";
import { useSSE } from "../hooks/useSSE";
import "./Sidebar.css";

const NAV_ITEMS = [
  { to: "/", label: "Overview" },
  { to: "/garden", label: "Garden" },
  { to: "/offerings", label: "Offerings" },
  { to: "/seeds", label: "Seed Banks" },
  { to: "/activity", label: "Activity" },
  { to: "/pond", label: "Pond" },
];

function healthColor(h: string): string {
  if (h === "thriving") return "var(--sage)";
  if (h === "withering") return "var(--clay)";
  return "var(--s4)";
}

export function Sidebar() {
  const { stones } = useStones();
  const { connected } = useSSE();

  const online = stones.filter((s) => s.status === "online").length;

  return (
    <aside className="side">
      <div className="side-brand">
        <h1>Lantern</h1>
        <div className="gname">Zen Garden</div>
        <div className="side-health">
          <span
            className="pip-b"
            style={{
              background: connected ? "var(--sage)" : "var(--s4)",
              animationName: connected ? "breathe" : "none",
            }}
          />
          {online} / {stones.length} stones
        </div>
      </div>

      <nav className="side-nav">
        <div className="nav-label">Views</div>
        {NAV_ITEMS.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.to === "/"}
            className={({ isActive }) => `nav-item${isActive ? " active" : ""}`}
          >
            {item.label}
          </NavLink>
        ))}

        {stones.length > 0 && (
          <>
            <div className="nav-label">Stones</div>
            {stones.map((s) => (
              <NavLink
                key={s.stone_id}
                to={`/stones/${s.stone_name}`}
                className={({ isActive }) => `stn${isActive ? " active" : ""}`}
              >
                <span
                  className="pip"
                  style={{ background: healthColor(s.health) }}
                />
                <span className="nm">{s.stone_name}</span>
                <span
                  className="hdot"
                  style={{ background: healthColor(s.health) }}
                />
              </NavLink>
            ))}
          </>
        )}
      </nav>

      <div className="side-foot">
        <span>Lantern v0.1</span>
        <span className={`sse-dot ${connected ? "on" : "off"}`} />
      </div>
    </aside>
  );
}
