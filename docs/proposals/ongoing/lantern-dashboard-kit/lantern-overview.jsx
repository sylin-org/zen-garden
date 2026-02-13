import { useState, useEffect, useCallback, useRef } from "react";

// ═══════════════════════════════════════════════════════════════════
// MOCK DATA
// ═══════════════════════════════════════════════════════════════════
const STONES = [
  {
    id: "stone-crystal-forest", name: "stone-crystal-forest", color: "#84a59d", health: "thriving",
    hardware: { cpu_cores: 4, memory_gb: 8 },
    resources: { cpu: 23, memory: 62, disk: 41 },
    services: [
      { offering: "mongodb", instanceName: null, status: "running", image: "mongo:7", port: 27017, capabilities: null, description: "Document database" },
      { offering: "redis", instanceName: null, status: "running", image: "redis:7-alpine", port: 6379, capabilities: null, description: "In-memory data store" },
      { offering: "minio", instanceName: null, status: "running", image: "minio/minio:latest", port: 9000, capabilities: null, description: "S3-compatible object storage" },
    ],
    tags: [],
  },
  {
    id: "stone-quiet-stream", name: "stone-quiet-stream", color: "#d4a373", health: "thriving",
    hardware: { cpu_cores: 16, memory_gb: 64 },
    resources: { cpu: 67, memory: 78, disk: 55 },
    services: [
      { offering: "mongodb", instanceName: null, status: "running", image: "mongo:7", port: 27017, capabilities: null, description: "Document database" },
      { offering: "postgres", instanceName: "snapvault", status: "running", image: "postgres:16-alpine", port: 5432, capabilities: null, description: "Relational database" },
      { offering: "ollama", instanceName: null, status: "running", image: "ollama/ollama:latest", port: 11434, capabilities: ["llama3.2", "phi3", "gemma2"], description: "Local LLM inference" },
      { offering: "chromadb", instanceName: null, status: "running", image: "chromadb/chroma:latest", port: 8000, capabilities: null, description: "Vector embedding database" },
      { offering: "snapvault", instanceName: null, status: "running", image: "snapvault-pro:latest", port: 8080, capabilities: null, description: "AI-powered photo management" },
    ],
    tags: ["opportunity"],
  },
  {
    id: "stone-amber-ridge", name: "stone-amber-ridge", color: "#c4b060", health: "withering",
    hardware: { cpu_cores: 2, memory_gb: 4 },
    resources: { cpu: 89, memory: 91, disk: 78 },
    services: [
      { offering: "grafana", instanceName: null, status: "running", image: "grafana/grafana:latest", port: 3000, capabilities: null, description: "Observability dashboards" },
      { offering: "mosquitto", instanceName: "iot-hub", status: "stopped", image: "eclipse-mosquitto:2", port: 1883, capabilities: null, description: "MQTT message broker" },
      { offering: "ollama", instanceName: null, status: "running", image: "ollama/ollama:latest", port: 11434, capabilities: ["phi3", "gemma2"], description: "Local LLM inference" },
    ],
    tags: ["attention"],
  },
  {
    id: "stone-ivy-terrace", name: "stone-ivy-terrace", color: "#a8a29e", health: "resting",
    hardware: { cpu_cores: 1, memory_gb: 2 },
    resources: { cpu: 0, memory: 0, disk: 34 },
    services: [
      { offering: "mosquitto", instanceName: null, status: "stopped", image: "eclipse-mosquitto:2", port: 1883, capabilities: null, description: "MQTT message broker" },
    ],
    tags: [],
  },
];

// ═══════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════
const svcKey = (s) => (s.instanceName ? `${s.offering}:${s.instanceName}` : s.offering);
const svcDisplay = (s) => svcKey(s);

const buildReplicas = () => {
  const g = {};
  STONES.forEach((st) =>
    st.services.forEach((sv) => {
      const k = svcKey(sv);
      if (!g[k]) g[k] = [];
      g[k].push({ stoneId: st.id, stoneName: st.name, stoneColor: st.color, service: sv });
    })
  );
  return g;
};
const REPLICAS = buildReplicas();
const isReplica = (sv) => (REPLICAS[svcKey(sv)]?.length || 0) > 1;
const replicaPeers = (sv, excludeId) => (REPLICAS[svcKey(sv)] || []).filter((m) => m.stoneId !== excludeId);

const computeEdges = () => {
  const edges = [];
  for (let i = 0; i < STONES.length; i++)
    for (let j = i + 1; j < STONES.length; j++) {
      const shared = new Set();
      STONES[i].services.forEach((a) =>
        STONES[j].services.forEach((b) => {
          if (svcKey(a) === svcKey(b)) shared.add(svcKey(a));
        })
      );
      if (shared.size > 0) edges.push({ from: STONES[i].id, to: STONES[j].id, sets: [...shared] });
    }
  return edges;
};
const EDGES = computeEdges();

// Geometry
const DEG = Math.PI / 180;
const polar = (cx, cy, r, deg) => ({
  x: cx + r * Math.cos(deg * DEG),
  y: cy + r * Math.sin(deg * DEG),
});
const arcPath = (cx, cy, r, s, e) => {
  const a = polar(cx, cy, r, s);
  const b = polar(cx, cy, r, e);
  return `M ${a.x} ${a.y} A ${r} ${r} 0 ${e - s > 180 ? 1 : 0} 1 ${b.x} ${b.y}`;
};
const resourceColor = (p) => (p > 85 ? "#c45050" : p > 70 ? "#d4a373" : "#84a59d");
const healthColor = (h) => (h === "thriving" ? "#84a59d" : h === "withering" ? "#d4a373" : "#78716c");
const bezier = (x1, y1, x2, y2, bow = 0) => {
  const mx = (x1 + x2) / 2,
    my = (y1 + y2) / 2,
    dx = x2 - x1,
    dy = y2 - y1,
    len = Math.sqrt(dx * dx + dy * dy) || 1;
  return `M ${x1} ${y1} Q ${mx + ((-dy) / len) * bow} ${my + (dx / len) * bow} ${x2} ${y2}`;
};

// Layout
const POS = {
  "stone-crystal-forest": { x: 250, y: 195 },
  "stone-quiet-stream": { x: 650, y: 175 },
  "stone-amber-ridge": { x: 530, y: 410 },
  "stone-ivy-terrace": { x: 155, y: 430 },
};
const NR = 56;
const AW = 7;
const AR = NR - AW / 2 - 1;
const SEGS = [
  { label: "CPU", key: "cpu", base: -90 },
  { label: "MEM", key: "memory", base: 30 },
  { label: "DSK", key: "disk", base: 150 },
];
const GAP = 5;

// ═══════════════════════════════════════════════════════════════════
// STYLES
// ═══════════════════════════════════════════════════════════════════
const css = `
@import url('https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@300;400;500&family=IBM+Plex+Sans:wght@300;400;500;600&display=swap');
:root {
  --bg: #1a1a1a; --bg2: #222220; --s9: #fafaf9; --s7: #d6d3d1; --s6: #a8a29e; --s5: #8a8580;
  --s4: #78716c; --s3: #57534e; --vb: rgba(255,255,255,0.08); --vh: rgba(255,255,255,0.04);
  --sage: #84a59d; --clay: #d4a373; --gold: #c4b060;
  --sans: 'IBM Plex Sans', system-ui, sans-serif; --mono: 'IBM Plex Mono', ui-monospace, monospace;
  --ease: cubic-bezier(0.22, 1, 0.36, 1);
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body, #root { background: var(--bg); color: var(--s9); font-family: var(--sans); height: 100vh; overflow: hidden; }
.shell { display: grid; grid-template-columns: 200px 1fr; height: 100vh; }

/* Sidebar */
.side { background: var(--bg2); border-right: 1px solid var(--vb); display: flex; flex-direction: column; }
.side-brand { padding: 1.25rem 1rem; border-bottom: 1px solid var(--vb); }
.side-brand h1 { font-family: var(--mono); font-size: 0.65rem; font-weight: 400; text-transform: uppercase; letter-spacing: 0.25em; color: var(--s5); margin-bottom: 0.3rem; }
.side-brand .gname { font-size: 1rem; font-weight: 600; letter-spacing: -0.02em; }
.side-health { display: flex; align-items: center; gap: 0.35rem; margin-top: 0.4rem; font-family: var(--mono); font-size: 0.6rem; color: var(--s5); text-transform: uppercase; }
.pip-breathe { width: 6px; height: 6px; border-radius: 50%; background: var(--sage); animation: br 3s ease-in-out infinite; }
@keyframes br { 0%,100% { opacity: 0.6; box-shadow: 0 0 4px var(--sage); } 50% { opacity: 1; box-shadow: 0 0 10px var(--sage); } }
.side-nav { flex: 1; padding: 0.5rem 0; overflow-y: auto; }
.nav-label { font-family: var(--mono); font-size: 0.5rem; text-transform: uppercase; letter-spacing: 0.2em; color: var(--s4); padding: 0.6rem 1rem 0.25rem; }
.side-stones { display: flex; flex-direction: column; }
.stn { display: flex; align-items: center; gap: 0.4rem; padding: 0.3rem 1rem; cursor: pointer; transition: all 0.3s var(--ease); border-left: 2px solid transparent; }
.stn:hover { background: var(--vh); }
.stn .pip { width: 7px; height: 7px; border-radius: 2px; flex-shrink: 0; }
.stn .nm { font-size: 0.72rem; color: var(--s6); flex: 1; }
.stn .hdot { width: 5px; height: 5px; border-radius: 50%; }
.side-foot { padding: 0.6rem 1rem; border-top: 1px solid var(--vb); font-family: var(--mono); font-size: 0.55rem; color: var(--s4); display: flex; justify-content: space-between; }

/* Canvas */
.canvas { position: relative; width: 100%; height: 100vh; overflow: hidden; }
.canvas svg { width: 100%; height: 100%; }
.canvas svg text { font-family: 'IBM Plex Sans', sans-serif; }
.canvas svg .mono { font-family: 'IBM Plex Mono', monospace; }

/* Summary strip */
.sum { position: absolute; bottom: 1.25rem; left: 50%; transform: translateX(-50%); display: flex; gap: 1.2rem; align-items: center; padding: 0.5rem 1.2rem; background: rgba(26,26,26,0.88); backdrop-filter: blur(14px); border: 1px solid var(--vb); border-radius: 4px; }
.sum .sv { font-size: 0.95rem; font-weight: 600; text-align: center; }
.sum .sl { font-family: var(--mono); font-size: 0.45rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--s4); text-align: center; }
.sum .sd { width: 1px; height: 1.2rem; background: var(--vb); }

/* Bloom panel */
.bpanel {
  position: absolute; background: rgba(26,26,26,0.94); backdrop-filter: blur(18px);
  border: 1px solid var(--vb); border-radius: 4px; padding: 0.75rem 0.9rem;
  min-width: 230px; max-width: 280px; z-index: 20; pointer-events: auto;
  animation: panelIn 0.25s var(--ease) forwards;
}
@keyframes panelIn { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: translateY(0); } }
.bpanel-head { display: flex; align-items: start; justify-content: space-between; gap: 0.5rem; margin-bottom: 0.4rem; padding-bottom: 0.35rem; border-bottom: 1px solid var(--vb); }
.bpanel-name { font-weight: 500; font-size: 0.8rem; }
.bpanel-desc { font-family: var(--mono); font-size: 0.55rem; color: var(--s4); margin-top: 0.1rem; }
.bpanel-row { display: flex; align-items: center; justify-content: space-between; gap: 0.4rem; padding: 0.12rem 0; font-family: var(--mono); font-size: 0.6rem; color: var(--s5); }
.bpanel-section { margin-top: 0.35rem; padding-top: 0.35rem; border-top: 1px solid var(--vb); }
.bpanel-slabel { font-family: var(--mono); font-size: 0.45rem; color: var(--s4); text-transform: uppercase; letter-spacing: 0.1em; margin-bottom: 0.2rem; }
.bpanel-actions { display: flex; gap: 0.3rem; margin-top: 0.4rem; padding-top: 0.35rem; border-top: 1px solid var(--vb); }
.btn { background: transparent; border: 1px solid var(--vb); border-radius: 2px; padding: 0.22rem 0.5rem; font-family: var(--mono); font-size: 0.55rem; cursor: pointer; color: var(--s5); text-transform: uppercase; letter-spacing: 0.03em; transition: all 0.3s var(--ease); white-space: nowrap; }
.btn:hover { background: var(--sage); color: white; border-color: var(--sage); }
.status-dot { display: inline-flex; align-items: center; gap: 0.3rem; font-family: var(--mono); font-size: 0.55rem; text-transform: uppercase; }
.status-dot::before { content: ''; width: 4px; height: 4px; border-radius: 50%; }
.status-dot.running { color: var(--sage); }
.status-dot.running::before { background: var(--sage); box-shadow: 0 0 4px var(--sage); }
.status-dot.stopped { color: var(--s4); }
.status-dot.stopped::before { background: var(--s4); }
.cap-tag { font-family: var(--mono); font-size: 0.55rem; padding: 0.08rem 0.3rem; background: rgba(132,165,157,0.1); border: 1px solid rgba(132,165,157,0.2); border-radius: 2px; color: var(--sage); }
.inst-name { color: var(--gold); }
.inst-sep { color: var(--s4); opacity: 0.5; }
`;

// ═══════════════════════════════════════════════════════════════════
// OVERVIEW COMPONENT
// ═══════════════════════════════════════════════════════════════════
export default function LanternOverview() {
  const [bloomed, setBloomed] = useState(null);
  const [activeSat, setActiveSat] = useState(null);
  const [hovered, setHovered] = useState(null);
  const [pf, setPf] = useState(0);
  const [sparkles, setSparkles] = useState([]);
  const spkRef = useRef(0);

  // Animation loop
  useEffect(() => {
    let f = 0;
    const t = setInterval(() => { f++; setPf(f); }, 50);
    return () => clearInterval(t);
  }, []);

  // Sparkle spawner
  useEffect(() => {
    const t = setInterval(() => {
      if (!EDGES.length) return;
      const e = EDGES[Math.floor(Math.random() * EDGES.length)];
      const id = spkRef.current++;
      setSparkles((p) => [...p, { id, from: e.from, to: e.to, born: Date.now() }]);
      setTimeout(() => setSparkles((p) => p.filter((s) => s.id !== id)), 1400);
    }, 3000);
    return () => clearInterval(t);
  }, []);

  const breathe = (health, phase = 0) => {
    const rate = health === "thriving" ? 0.035 : health === "withering" ? 0.07 : 0;
    if (health === "resting") return 0.15;
    const base = Math.sin((pf + phase) * rate) * 0.5 + 0.5;
    return health === "thriving" ? base * 0.25 + 0.4 : base * 0.4 + 0.35;
  };

  const bloomStone = bloomed ? STONES.find((s) => s.id === bloomed) : null;
  const satellites = bloomStone
    ? bloomStone.services.map((svc, i) => {
        const n = bloomStone.services.length;
        const ang = (i / n) * 360 - 90;
        const r = 130;
        const p = POS[bloomStone.id];
        return {
          svc,
          key: `${bloomed}-${svcKey(svc)}-${i}`,
          x: p.x + Math.cos(ang * DEG) * r,
          y: p.y + Math.sin(ang * DEG) * r,
        };
      })
    : [];

  const onStoneClick = (id, e) => {
    e.stopPropagation();
    setBloomed(bloomed === id ? null : id);
    setActiveSat(null);
  };

  const onSatClick = (key, e) => {
    e.stopPropagation();
    setActiveSat(activeSat === key ? null : key);
  };

  const onBgClick = () => {
    setBloomed(null);
    setActiveSat(null);
  };

  const onlineCount = STONES.filter((s) => s.health !== "resting").length;
  const svcCount = STONES.reduce((n, s) => n + s.services.filter((v) => v.status === "running").length, 0);
  const rgCount = Object.values(REPLICAS).filter((m) => m.length > 1).length;

  return (
    <>
      <style>{css}</style>
      <div className="shell">
        {/* Sidebar */}
        <aside className="side">
          <div className="side-brand">
            <h1>Lantern</h1>
            <div className="gname">Home Lab</div>
            <div className="side-health">
              <div className="pip-breathe" />
              {onlineCount}/{STONES.length} stones · pond active
            </div>
          </div>
          <nav className="side-nav">
            <div className="nav-label">Stones</div>
            <div className="side-stones">
              {STONES.map((s) => (
                <div key={s.id} className="stn" onClick={(e) => onStoneClick(s.id, e)}>
                  <div className="pip" style={{ background: s.color }} />
                  <div className="nm">{s.name.replace("stone-", "")}</div>
                  {s.tags.includes("attention") && (
                    <span style={{ fontSize: "0.55rem", color: "var(--clay)" }}>⚠</span>
                  )}
                  <div
                    className="hdot"
                    style={{
                      background: healthColor(s.health),
                      boxShadow: s.health !== "resting" ? `0 0 4px ${healthColor(s.health)}` : "none",
                      opacity: s.health === "resting" ? 0.4 : 1,
                    }}
                  />
                </div>
              ))}
            </div>
          </nav>
          <div className="side-foot">
            <span>Lantern v0.1.0</span>
            <span>⏱ 2.3s</span>
          </div>
        </aside>

        {/* Canvas */}
        <div className="canvas" onClick={onBgClick}>
          <svg viewBox="0 0 900 580" preserveAspectRatio="xMidYMid meet">
            <defs>
              <filter id="gs" x="-50%" y="-50%" width="200%" height="200%">
                <feGaussianBlur stdDeviation="4" result="b" />
                <feFlood floodColor="#84a59d" floodOpacity="0.5" />
                <feComposite in2="b" operator="in" />
                <feMerge><feMergeNode /><feMergeNode in="SourceGraphic" /></feMerge>
              </filter>
              <filter id="gc" x="-50%" y="-50%" width="200%" height="200%">
                <feGaussianBlur stdDeviation="4" result="b" />
                <feFlood floodColor="#d4a373" floodOpacity="0.5" />
                <feComposite in2="b" operator="in" />
                <feMerge><feMergeNode /><feMergeNode in="SourceGraphic" /></feMerge>
              </filter>
              <pattern id="dots" width="40" height="40" patternUnits="userSpaceOnUse">
                <circle cx="20" cy="20" r="0.5" fill="rgba(255,255,255,0.03)" />
              </pattern>
            </defs>

            {/* Background */}
            <rect width="900" height="580" fill="var(--bg)" />
            <rect width="900" height="580" fill="url(#dots)" />

            {/* ── Edges ── */}
            {EDGES.map((edge, ei) => {
              const fp = POS[edge.from];
              const tp = POS[edge.to];
              if (!fp || !tp) return null;
              const isRelated = bloomed && (edge.from === bloomed || edge.to === bloomed);
              const dim = bloomed && !isRelated;
              const hRel = hovered && (edge.from === hovered || edge.to === hovered);

              return edge.sets.map((sk, si) => {
                const bow = edge.sets.length > 1 ? (si - (edge.sets.length - 1) / 2) * 30 : 0;
                const d = bezier(fp.x, fp.y, tp.x, tp.y, bow);
                const mx = (fp.x + tp.x) / 2;
                const my = (fp.y + tp.y) / 2;
                const dx = tp.x - fp.x;
                const dy = tp.y - fp.y;
                const len = Math.sqrt(dx * dx + dy * dy) || 1;
                const lx = mx + ((-dy) / len) * bow;
                const ly = my + ((dx) / len) * bow;

                return (
                  <g key={`e${ei}-${si}`} opacity={dim ? 0.08 : hRel ? 1 : bloomed ? 0.25 : 0.55}
                    style={{ transition: "opacity 0.5s ease" }}>
                    <path d={d} fill="none" stroke="transparent" strokeWidth="16" style={{ cursor: "pointer" }} />
                    <path d={d} fill="none" stroke="#84a59d" strokeWidth="1.5" strokeOpacity="0.5" />
                    <g transform={`translate(${lx},${ly})`}>
                      <rect x={-sk.length * 3.2 - 4} y={-7} width={sk.length * 6.4 + 8} height={14}
                        rx="2" fill="rgba(26,26,26,0.85)" stroke="rgba(255,255,255,0.05)" strokeWidth="0.5" />
                      <text textAnchor="middle" y="4" fill="#8a8580" fontSize="8" className="mono">{sk}</text>
                    </g>
                  </g>
                );
              });
            })}

            {/* ── Sparkles ── */}
            {sparkles.map((sp) => {
              const fp = POS[sp.from];
              const tp = POS[sp.to];
              if (!fp || !tp) return null;
              return (
                <g key={sp.id}>
                  <circle r="3" fill="#84a59d" filter="url(#gs)">
                    <animateMotion dur="1.2s" fill="freeze" path={bezier(fp.x, fp.y, tp.x, tp.y, 0)} />
                    <animate attributeName="opacity" values="0;1;1;0" keyTimes="0;0.1;0.7;1" dur="1.2s" fill="freeze" />
                  </circle>
                </g>
              );
            })}

            {/* ── Bloom spokes ── */}
            {satellites.map((sat) => {
              const sp = POS[bloomed];
              return (
                <line key={`sp-${sat.key}`}
                  x1={sp.x} y1={sp.y} x2={sat.x} y2={sat.y}
                  stroke={sat.svc.status === "running" ? "#84a59d" : "#78716c"}
                  strokeWidth="1" strokeOpacity="0.3" strokeDasharray="4 3"
                />
              );
            })}

            {/* ── Stone Nodes ── */}
            {STONES.map((stone, si) => {
              const pos = POS[stone.id];
              if (!pos) return null;
              const isB = bloomed === stone.id;
              const dim = bloomed && !isB;
              const hov = hovered === stone.id;
              const br = breathe(stone.health, si * 37);
              const sc = isB ? 1.1 : 1;

              return (
                <g key={stone.id}
                  style={{ cursor: "pointer", transition: "opacity 0.5s, transform 0.4s ease" }}
                  opacity={dim ? 0.18 : 1}
                  transform={`translate(${pos.x},${pos.y}) scale(${sc})`}
                  onClick={(e) => onStoneClick(stone.id, e)}
                  onMouseEnter={() => !bloomed && setHovered(stone.id)}
                  onMouseLeave={() => setHovered(null)}
                >
                  {/* Ambient glow ring */}
                  <circle r={NR + 12} fill="none" stroke={healthColor(stone.health)}
                    strokeWidth="1" strokeOpacity={br * 0.3} />

                  {/* Inner fill */}
                  <circle r={NR - AW - 3} fill={stone.color} fillOpacity="0.06" />
                  <circle r={NR - AW - 3} fill="none" stroke={stone.color} strokeWidth="0.5" strokeOpacity="0.15" />

                  {/* Arc segments */}
                  {SEGS.map((seg) => {
                    const s = seg.base + GAP / 2;
                    const e = seg.base + 120 - GAP / 2;
                    const v = stone.resources[seg.key] || 0;
                    const fe = s + (120 - GAP) * (v / 100);
                    const c = resourceColor(v);

                    return (
                      <g key={seg.key}>
                        <path d={arcPath(0, 0, AR, s, e)}
                          fill="none" stroke="rgba(255,255,255,0.05)" strokeWidth={AW} strokeLinecap="round" />
                        {v > 0 && (
                          <path d={arcPath(0, 0, AR, s, fe)}
                            fill="none" stroke={c} strokeWidth={AW} strokeLinecap="round"
                            strokeOpacity={0.6 + br * 0.4} />
                        )}
                        {(hov || isB) && (() => {
                          const lp = polar(0, 0, AR + 14, (s + e) / 2);
                          return (
                            <text x={lp.x} y={lp.y} textAnchor="middle" dominantBaseline="central"
                              fontSize="6" fill="#78716c" className="mono">
                              {seg.label} {v}%
                            </text>
                          );
                        })()}
                      </g>
                    );
                  })}

                  {/* Center LED */}
                  <circle r="4" fill={healthColor(stone.health)} opacity={br}
                    filter={stone.health === "withering" ? "url(#gc)" : stone.health === "thriving" ? "url(#gs)" : "none"} />

                  {/* Color pip at top */}
                  <rect x={-3} y={-(NR - AW - 3)} width={6} height={3} rx={1} fill={stone.color} opacity="0.8" />

                  {/* Name */}
                  <text y={-8} textAnchor="middle" fontSize="10" fontWeight="500"
                    fill="#fafaf9" fillOpacity={stone.health === "resting" ? 0.4 : 0.9}>
                    {stone.name.replace("stone-", "")}
                  </text>

                  {/* Service dots */}
                  <g transform="translate(0,8)">
                    {stone.services.map((sv, i) => {
                      const tw = (stone.services.length - 1) * 8;
                      const dx = i * 8 - tw / 2;
                      const run = sv.status === "running";
                      return (
                        <circle key={i} cx={dx} cy="0" r="2.5"
                          fill={run ? "#84a59d" : "transparent"}
                          stroke={run ? "none" : "#78716c"} strokeWidth="0.5"
                          opacity={stone.health === "resting" ? 0.3 : 0.8}
                        />
                      );
                    })}
                  </g>

                  {/* Hardware */}
                  <text y={22} textAnchor="middle" fontSize="6.5" fill="#78716c" className="mono"
                    opacity={stone.health === "resting" ? 0.3 : 0.5}>
                    {stone.hardware.cpu_cores}c · {stone.hardware.memory_gb}GB
                  </text>
                </g>
              );
            })}

            {/* ── Satellites ── */}
            {satellites.map((sat) => {
              const rp = isReplica(sat.svc);
              const run = sat.svc.status === "running";
              const isA = activeSat === sat.key;

              return (
                <g key={sat.key} style={{ cursor: "pointer" }} onClick={(e) => onSatClick(sat.key, e)}>
                  <circle cx={sat.x} cy={sat.y} r={28}
                    fill={isA ? "rgba(26,26,26,0.94)" : "rgba(26,26,26,0.78)"}
                    stroke={run ? (rp ? "#84a59d" : "rgba(255,255,255,0.12)") : "rgba(255,255,255,0.06)"}
                    strokeWidth={isA ? 1.5 : 1}
                  />
                  <circle cx={sat.x} cy={sat.y - 10} r="2.5"
                    fill={run ? "#84a59d" : "#78716c"} opacity={run ? 0.9 : 0.5} />
                  <text x={sat.x} y={sat.y + 1} textAnchor="middle" fontSize="8"
                    fontWeight="500" fill="#fafaf9" fillOpacity="0.85">
                    {sat.svc.offering}
                  </text>
                  {sat.svc.instanceName && (
                    <text x={sat.x} y={sat.y + 11} textAnchor="middle" fontSize="6.5"
                      className="mono" fill="#c4b060" fillOpacity="0.8">
                      :{sat.svc.instanceName}
                    </text>
                  )}
                  {!sat.svc.instanceName && rp && (
                    <text x={sat.x} y={sat.y + 11} textAnchor="middle" fontSize="6"
                      className="mono" fill="#84a59d" fillOpacity="0.7">
                      ⟐ {replicaPeers(sat.svc, bloomed).length}p
                    </text>
                  )}
                </g>
              );
            })}
          </svg>

          {/* ── Bloom Panel ── */}
          {activeSat && (() => {
            const sat = satellites.find((s) => s.key === activeSat);
            if (!sat) return null;
            const svc = sat.svc;
            const st = bloomStone;
            const rp = isReplica(svc);
            const peers = replicaPeers(svc, st.id);
            const lp = (sat.x / 900) * 100;
            const tp = (sat.y / 580) * 100;

            return (
              <div className="bpanel"
                style={{ left: `calc(${lp}% + 36px)`, top: `calc(${tp}% - 40px)` }}
                onClick={(e) => e.stopPropagation()}>
                <div className="bpanel-head">
                  <div>
                    <div className="bpanel-name">
                      {svc.offering}
                      {svc.instanceName && (
                        <><span className="inst-sep">:</span><span className="inst-name">{svc.instanceName}</span></>
                      )}
                    </div>
                    <div className="bpanel-desc">{svc.description}</div>
                  </div>
                  <span className={`status-dot ${svc.status}`}>{svc.status}</span>
                </div>

                <div className="bpanel-row"><span>Image</span><span style={{ color: "var(--s6)" }}>{svc.image}</span></div>
                <div className="bpanel-row"><span>Port</span><span style={{ color: "var(--s6)" }}>:{svc.port}</span></div>
                <div className="bpanel-row">
                  <span>Stone</span>
                  <span style={{ color: st.color }}>{st.name.replace("stone-", "")}</span>
                </div>
                <div className="bpanel-row">
                  <span>Identity</span>
                  <span style={{ color: svc.instanceName ? "var(--gold)" : "var(--s5)" }}>
                    {svcDisplay(svc)}{!svc.instanceName ? " (unnamed)" : ""}
                  </span>
                </div>

                {rp && peers.length > 0 && (
                  <div className="bpanel-section">
                    <div className="bpanel-slabel">Replica peers</div>
                    {peers.map((p) => (
                      <div key={p.stoneId} className="bpanel-row" style={{ cursor: "pointer" }}
                        onClick={() => { setBloomed(p.stoneId); setActiveSat(null); }}>
                        <span style={{ display: "flex", alignItems: "center", gap: "0.3rem" }}>
                          <span style={{ width: 3, height: 10, background: p.stoneColor, borderRadius: 1, display: "inline-block" }} />
                          {p.stoneName.replace("stone-", "")}
                        </span>
                        <span>:{p.service.port} {p.service.status === "running" ? "●" : "○"}</span>
                      </div>
                    ))}
                  </div>
                )}

                {svc.capabilities && svc.capabilities.length > 0 && (
                  <div className="bpanel-section">
                    <div className="bpanel-slabel">Capabilities</div>
                    <div style={{ display: "flex", gap: "0.2rem", flexWrap: "wrap" }}>
                      {svc.capabilities.map((c) => <span key={c} className="cap-tag">{c}</span>)}
                    </div>
                  </div>
                )}

                <div className="bpanel-actions">
                  <button className="btn">{svc.status === "running" ? "Rest" : "Wake"}</button>
                  <button className="btn">Config</button>
                  <button className="btn">Detail ↗</button>
                </div>
              </div>
            );
          })()}

          {/* ── Summary ── */}
          <div className="sum">
            <div><div className="sv">{STONES.length}</div><div className="sl">Stones</div></div>
            <div className="sd" />
            <div><div className="sv">{onlineCount}</div><div className="sl">Online</div></div>
            <div className="sd" />
            <div><div className="sv">{svcCount}</div><div className="sl">Services</div></div>
            <div className="sd" />
            <div><div className="sv" style={{ color: "var(--sage)" }}>{rgCount}</div><div className="sl">Replica Groups</div></div>
          </div>
        </div>
      </div>
    </>
  );
}
