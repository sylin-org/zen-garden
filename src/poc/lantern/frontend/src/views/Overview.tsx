import { useState, useEffect, useRef } from "react";
import { useNavigate } from "react-router-dom";
import { GardenSphere } from "../lib/garden-sphere";
import { useStones } from "../hooks/useStones";
import { useSSE } from "../hooks/useSSE";
import type { Stone } from "../types/api";
import "./Overview.css";

/* ── helpers ─────────────────────────────────────────────────── */

const rc = (v: number) => (v > 85 ? "#c45050" : v > 70 ? "#d4a373" : "#84a59d");
const hc = (h: string) =>
  h === "thriving" || h === "healthy"
    ? "#84a59d"
    : h === "withering" || h === "degraded"
      ? "#d4a373"
      : h === "unhealthy"
        ? "#c45050"
        : "#78716c";

/** Position a card panel beside a sphere node, keeping it on-screen */
/** Position a card panel beside a sphere node, vertically centered on the anchor */
function panelOffset(
  nodePos: { x: number; y: number } | null,
  cardH: number,
) {
  if (!nodePos) return null;
  const M = 16,
    O = 48,
    W = 254, // 220px CSS width + 2×16px padding + 2×1px border
    vw = window.innerWidth,
    vh = window.innerHeight;
  // Prefer placing to the right; flip left if it would overflow
  let px = nodePos.x + O;
  if (px + W + M > vw) px = nodePos.x - W - O;
  // Clamp to viewport
  px = Math.max(M, Math.min(vw - W - M, px));
  const py = Math.max(M, Math.min(vh - cardH - M, nodePos.y - cardH / 2));
  return { left: px, top: py };
}

/* ── StoneCard ───────────────────────────────────────────────── */

interface CardProps {
  stone: Stone;
  pos: { x: number; y: number };
  style?: React.CSSProperties;
  onClick?: () => void;
}

function StoneCard({ stone, pos, style, onClick }: CardProps) {
  const cardRef = useRef<HTMLDivElement>(null);
  const alive = stone.health !== "resting" && stone.health !== "installing";
  const cardH = cardRef.current?.offsetHeight ?? 200;
  const pOff = panelOffset(pos, cardH);
  if (!pOff) return null;

  const edgeX = pos.x < pOff.left + 127 ? pOff.left : pOff.left + 254;
  const edgeY = pOff.top + cardH / 2;
  const cpu = stone.resources?.cpu_percent ?? 0;
  const mem = stone.resources?.memory_percent ?? 0;
  const dsk = stone.resources?.disk_percent ?? 0;
  const cores = stone.resources?.cpu_cores ?? 0;
  const memGB = Math.round(
    (stone.resources?.memory_total_bytes ?? 0) / 1073741824,
  );
  const offerings = stone.offerings ?? [];

  return (
    <>
      <svg className="ov-card-line">
        <line
          x1={pos.x}
          y1={pos.y}
          x2={edgeX}
          y2={edgeY}
          stroke="#84a59d"
          strokeWidth="0.5"
          strokeDasharray="3,3"
          opacity="0.25"
        />
        <circle cx={pos.x} cy={pos.y} r="2" fill="#84a59d" opacity="0.4" />
      </svg>
      <div
        ref={cardRef}
        className="ov-panel"
        style={{ top: `${pOff.top}px`, left: `${pOff.left}px`, ...style }}
        onClick={onClick}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => e.key === "Enter" && onClick?.()}
      >
        <h3>
          <span className="ov-pip" style={{ background: stone.color }} />
          {stone.stone_name}
          <span
            className="ov-hd"
            style={{
              background: hc(stone.health),
              color: hc(stone.health),
              animation: alive
                ? `hbr ${stone.health === "withering" || stone.health === "degraded" ? "1.5s" : "3s"} ease-in-out infinite`
                : "none",
              opacity: alive ? 1 : 0.4,
            }}
          />
        </h3>
        <div className="ov-sub">
          {cores}c &middot; {memGB}GB
          {stone.tags?.includes("keystone") ? " \u00b7 keystone \u25c6" : ""}
        </div>

        {alive && (
          <>
            <div className="ov-rl">Resources</div>
            <div className="ov-res">
              {[
                { l: "CPU", v: cpu },
                { l: "MEM", v: mem },
                { l: "DSK", v: dsk },
              ].map((r) => (
                <div key={r.l}>
                  <div className="ov-res-label">{r.l}</div>
                  <div className="ov-rv" style={{ color: rc(r.v) }}>
                    {Math.round(r.v)}%
                  </div>
                  <div className="ov-ga">
                    <div
                      className="ov-gf"
                      style={{ width: `${r.v}%`, background: rc(r.v) }}
                    />
                  </div>
                </div>
              ))}
            </div>
          </>
        )}

        <div className="ov-rl">
          Offerings &middot; {offerings.length}
        </div>
        <div className="ov-svl">
          {offerings.map((sv, i) => (
            <div className="ov-sv" key={i}>
              <div
                className="ov-svd"
                style={{
                  background:
                    sv.status === "running" ? "var(--sage)" : "var(--s3)",
                  boxShadow:
                    sv.status === "running"
                      ? "0 0 3px var(--sage)"
                      : "none",
                }}
              />
              <span>{sv.name || sv.offering}</span>
            </div>
          ))}
        </div>

        {!alive && <div className="ov-slumber">slumbering</div>}
      </div>
    </>
  );
}

/* ── Tracked positions from onTrack ─────────────────────────── */

interface TrackData {
  selected: { id: string; pos: { x: number; y: number } } | null;
  departing: { id: string; pos: { x: number; y: number } } | null;
  hovered: { id: string; pos: { x: number; y: number } } | null;
  progress: number;
}

/* ── OverviewView ────────────────────────────────────────────── */

export function OverviewView() {
  const navigate = useNavigate();
  const cvRef = useRef<HTMLDivElement>(null);
  const gsRef = useRef<GardenSphere | null>(null);
  const initDone = useRef(false);

  const { stones, loading } = useStones();
  const { events } = useSSE();

  // Sphere callback state
  const [hovered, setHovered] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [departingId, setDepartingId] = useState<string | null>(null);
  const [tracked, setTracked] = useState<TrackData>({
    selected: null,
    departing: null,
    hovered: null,
    progress: 1,
  });
  const [sphereStones, setSphereStones] = useState<Stone[]>([]);

  // Mount the sphere
  useEffect(() => {
    if (!cvRef.current) return;
    const gs = new GardenSphere(cvRef.current, {
      onHover: setHovered,
      onTransition: ({
        selectedId: sid,
        departingId: did,
      }: {
        selectedId: string | null;
        departingId: string | null;
      }) => {
        setSelectedId(sid);
        setDepartingId(did);
      },
      onTrack: (data: TrackData) => setTracked(data),
      onDataChange: (s: Stone[]) => setSphereStones([...s]),
    });
    gsRef.current = gs;
    return () => {
      gs.destroy();
      gsRef.current = null;
    };
  }, []);

  // Sync API stones into the sphere (diff-based after initial load)
  useEffect(() => {
    const gs = gsRef.current;
    if (!gs || loading || stones.length === 0) return;

    try {
      // First load — full setData
      if (!initDone.current) {
        gs.setData(stones);
        initDone.current = true;
        return;
      }

      // Subsequent updates — incremental diff
      const currentIds = new Set(
        (gs.stones as Stone[]).map((s) => s.stone_id),
      );
      const newIds = new Set(stones.map((s) => s.stone_id));

      // New stones
      stones
        .filter((s) => !currentIds.has(s.stone_id))
        .forEach((s) => gs.addStone(s));

      // Removed stones
      (gs.stones as Stone[])
        .filter((s) => !newIds.has(s.stone_id))
        .forEach((s) => gs.removeStone(s.stone_id));

      // Updated stones — merge changed fields, detect online/offline
      stones
        .filter((s) => currentIds.has(s.stone_id))
        .forEach((s) => {
          const current = (gs.stones as Stone[]).find(
            (c) => c.stone_id === s.stone_id,
          );
          if (!current) return;
          const wasOffline = current.status === "offline";
          const nowOffline = s.status === "offline";
          if (!wasOffline && nowOffline) {
            gs.offlineStone(s.stone_id);
          } else if (wasOffline && !nowOffline) {
            gs.onlineStone(s.stone_id, s);
          } else {
            gs.updateStone(s.stone_id, s);
          }
        });
    } catch (err) {
      console.error("[Overview] sphere sync error:", err);
    }
  }, [stones, loading]);

  // React to SSE events for faster offline transitions
  useEffect(() => {
    if (events.length === 0) return;
    const latest = events[0];
    const gs = gsRef.current;
    if (!gs) return;

    if (latest.event_type === "stone.offline" && latest.stone_name) {
      const node = (gs.stones as Stone[]).find(
        (s) => s.stone_name === latest.stone_name,
      );
      if (node) gs.offlineStone(node.stone_id);
    }
    if (latest.event_type === "stone.registered" && latest.stone_name) {
      const node = (gs.stones as Stone[]).find(
        (s) => s.stone_name === latest.stone_name,
      );
      if (node) gs.onlineStone(node.stone_id);
    }
  }, [events]);

  // Derived card state
  const selStone = selectedId
    ? sphereStones.find((s) => s.stone_id === selectedId) ?? null
    : null;
  const depStone = departingId
    ? sphereStones.find((s) => s.stone_id === departingId) ?? null
    : null;
  const hovStone =
    !selectedId && hovered
      ? sphereStones.find((s) => s.stone_id === hovered) ?? null
      : null;

  const progress = tracked.progress;
  const arriveScale = 0.8 + 0.2 * (1 - Math.pow(1 - progress, 2));
  const arriveOpacity = 0.5 + 0.5 * progress;
  const departOpacity = Math.max(0, 1 - progress * 1.5);
  const departScale = 1 - 0.15 * progress;
  const departGray = Math.min(1, progress * 2);

  const online = sphereStones.filter((s) => s.status === "online").length;
  const svcCount = sphereStones.reduce(
    (n, s) =>
      n + (s.offerings?.filter((v) => v.status === "running").length ?? 0),
    0,
  );

  const goToStone = (name: string) => navigate(`/stones/${name}`);

  return (
    <div className="ov-wrap">
      {/* Sphere canvas — always mounted so the ref is stable for the mount effect */}
      <div ref={cvRef} className="ov-cv" />

      {/* Departing card (fading to gray as sphere rotates it away) */}
      {depStone && tracked.departing?.pos && departOpacity > 0.01 && (
        <StoneCard
          stone={depStone}
          pos={tracked.departing.pos}
          style={{
            opacity: departOpacity,
            transform: `scale(${departScale})`,
            filter: `grayscale(${departGray})`,
            transition: "none",
            pointerEvents: "none",
          }}
        />
      )}

      {/* Arriving / selected card */}
      {selStone && tracked.selected?.pos && (
        <StoneCard
          stone={selStone}
          pos={tracked.selected.pos}
          style={{
            opacity: arriveOpacity,
            transform: `scale(${arriveScale})`,
            transition: "none",
          }}
          onClick={() => goToStone(selStone.stone_name)}
        />
      )}

      {/* Hover card (only when nothing is selected) */}
      {hovStone && !selStone && tracked.hovered?.pos && (
        <StoneCard
          stone={hovStone}
          pos={tracked.hovered.pos}
          style={{ opacity: 1 }}
          onClick={() => goToStone(hovStone.stone_name)}
        />
      )}

      {/* Summary strip */}
      <div className="ov-strip ov-fi ov-fi1">
        <div className="ov-dot" />
        <div>
          <div className="ov-n">{sphereStones.length}</div>
          stones
        </div>
        <div className="ov-d" />
        <div>
          <div className="ov-n" style={{ color: "var(--sage)" }}>
            {online}
          </div>
          online
        </div>
        <div className="ov-d" />
        <div>
          <div className="ov-n">{svcCount}</div>
          services
        </div>
      </div>

      {/* Interaction hint */}
      <div className="ov-hint ov-fi ov-fi2">
        right-drag to rotate
        <br />
        scroll to zoom
        <br />
        click a stone
      </div>

      {/* Loading overlay — shown while first fetch is in flight */}
      {loading && stones.length === 0 && (
        <div className="ov-empty-overlay">
          <div className="ov-empty-sub">Loading garden topology...</div>
        </div>
      )}

      {/* Empty overlay when no stones at all */}
      {!loading && stones.length === 0 && (
        <div className="ov-empty-overlay">
          <div className="ov-empty-icon">{"\u2B50"}</div>
          <div>No stones registered yet</div>
          <div className="ov-empty-sub">
            Stones will appear here as they register with Lantern
          </div>
        </div>
      )}
    </div>
  );
}
