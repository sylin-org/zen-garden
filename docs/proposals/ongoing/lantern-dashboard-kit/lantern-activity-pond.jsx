import { useState, useEffect, useRef } from "react";

// ═══════════════════════════════════════════════════════════════════
// DATA
// ═══════════════════════════════════════════════════════════════════
const STONES = [
  { id: "cf", name: "crystal-forest", color: "#84a59d", health: "thriving", endpoint: "http://192.168.1.42:7185", pond: "keystone" },
  { id: "qs", name: "quiet-stream", color: "#d4a373", health: "thriving", endpoint: "http://192.168.1.108:7185", pond: "member" },
  { id: "ar", name: "amber-ridge", color: "#c4b060", health: "withering", endpoint: "http://192.168.1.55:7185", pond: "member" },
  { id: "it", name: "ivy-terrace", color: "#a8a29e", health: "resting", endpoint: "http://192.168.10.20:7185", pond: "member" },
];

const INITIAL_EVENTS = [
  { time: "2s ago", stone: "quiet-stream", event: "ollama.inference.completed", detail: "llama3.2 — 847 tokens, 2.1s", type: "info" },
  { time: "18s ago", stone: "crystal-forest", event: "service.health.ok", detail: "mongodb heartbeat", type: "info" },
  { time: "45s ago", stone: "amber-ridge", event: "resource.memory.high", detail: "91% memory usage", type: "warning" },
  { time: "1m ago", stone: "quiet-stream", event: "snapvault.photo.indexed", detail: "batch of 23 photos processed", type: "info" },
  { time: "2m ago", stone: "crystal-forest", event: "nurturing.completed", detail: "mongodb slot-A backup — 2.3GB", type: "success" },
  { time: "5m ago", stone: "amber-ridge", event: "resource.cpu.high", detail: "89% sustained for 5m", type: "warning" },
  { time: "8m ago", stone: "quiet-stream", event: "replica.sync.complete", detail: "mongodb ← crystal-forest", type: "success" },
  { time: "12m ago", stone: "crystal-forest", event: "service.started", detail: "minio healthy", type: "info" },
  { time: "14m ago", stone: "quiet-stream", event: "ollama.model.loaded", detail: "phi3 warm in 1.8s", type: "info" },
  { time: "18m ago", stone: "amber-ridge", event: "grafana.dashboard.accessed", detail: "garden-overview panel", type: "info" },
  { time: "22m ago", stone: "crystal-forest", event: "redis.keyspace.eviction", detail: "12 keys expired (TTL)", type: "info" },
  { time: "25m ago", stone: "quiet-stream", event: "chromadb.collection.created", detail: "snapvault-embeddings", type: "success" },
  { time: "31m ago", stone: "crystal-forest", event: "minio.object.put", detail: "nurturing/slot-A/2025-02-10.bak", type: "info" },
  { time: "38m ago", stone: "amber-ridge", event: "mosquitto:iot-hub.offline", detail: "service stopped by user", type: "warning" },
  { time: "45m ago", stone: "quiet-stream", event: "postgres:snapvault.vacuum", detail: "auto-vacuum completed", type: "info" },
];

// Simulated live events that arrive over time
const LIVE_EVENTS = [
  { stone: "quiet-stream", event: "ollama.inference.completed", detail: "phi3 — 234 tokens, 0.8s", type: "info" },
  { stone: "crystal-forest", event: "mongodb.replication.heartbeat", detail: "→ quiet-stream acknowledged", type: "info" },
  { stone: "amber-ridge", event: "resource.cpu.spike", detail: "92% — ollama phi3 inference", type: "warning" },
  { stone: "quiet-stream", event: "snapvault.photo.classified", detail: "sunset, landscape — confidence 0.94", type: "success" },
  { stone: "crystal-forest", event: "redis.connection.new", detail: "client from 192.168.1.108", type: "info" },
];

const healthColor = (h) => (h === "thriving" ? "#84a59d" : h === "withering" ? "#d4a373" : "#78716c");
const typeColor = (t) => (t === "success" ? "#84a59d" : t === "warning" ? "#d4a373" : "#78716c");
const stoneColor = (name) => STONES.find(s => s.name === name)?.color || "#78716c";

const css = `
@import url('https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@300;400;500&family=IBM+Plex+Sans:wght@300;400;500;600&display=swap');
:root {
  --bg: #1a1a1a; --bg2: #222220; --s9: #fafaf9; --s6: #a8a29e; --s5: #8a8580;
  --s4: #78716c; --vb: rgba(255,255,255,0.08); --vh: rgba(255,255,255,0.04);
  --sage: #84a59d; --clay: #d4a373; --gold: #c4b060;
  --sans: 'IBM Plex Sans', system-ui, sans-serif; --mono: 'IBM Plex Mono', ui-monospace, monospace;
  --ease: cubic-bezier(0.22, 1, 0.36, 1);
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body, #root { background: var(--bg); color: var(--s9); font-family: var(--sans); height: 100vh; overflow: hidden; }
::-webkit-scrollbar { width: 5px; } ::-webkit-scrollbar-track { background: transparent; } ::-webkit-scrollbar-thumb { background: var(--s4); border-radius: 3px; }
.shell { display: grid; grid-template-columns: 200px 1fr; height: 100vh; }

.side { background: var(--bg2); border-right: 1px solid var(--vb); display: flex; flex-direction: column; }
.side-brand { padding: 1.25rem 1rem; border-bottom: 1px solid var(--vb); }
.side-brand h1 { font-family: var(--mono); font-size: 0.65rem; font-weight: 400; text-transform: uppercase; letter-spacing: 0.25em; color: var(--s5); margin-bottom: 0.3rem; }
.side-brand .gname { font-size: 1rem; font-weight: 600; }
.side-health { display: flex; align-items: center; gap: 0.35rem; margin-top: 0.4rem; font-family: var(--mono); font-size: 0.6rem; color: var(--s5); text-transform: uppercase; }
.pip-b { width: 6px; height: 6px; border-radius: 50%; background: var(--sage); animation: br 3s ease-in-out infinite; }
@keyframes br { 0%,100% { opacity:0.6; box-shadow:0 0 4px var(--sage); } 50% { opacity:1; box-shadow:0 0 10px var(--sage); } }
.side-nav { flex: 1; padding: 0.5rem 0; overflow-y: auto; }
.nav-label { font-family: var(--mono); font-size: 0.5rem; text-transform: uppercase; letter-spacing: 0.2em; color: var(--s4); padding: 0.6rem 1rem 0.25rem; }
.nav-item { display: flex; align-items: center; gap: 0.5rem; padding: 0.4rem 1rem; cursor: pointer; color: var(--s6); font-size: 0.78rem; transition: all 0.3s var(--ease); border-left: 2px solid transparent; }
.nav-item:hover { color: var(--s9); background: var(--vh); }
.nav-item.active { color: var(--s9); background: var(--vh); border-left-color: var(--sage); }
.nav-icon { font-size: 0.85rem; width: 1.1rem; text-align: center; }
.stn { display: flex; align-items: center; gap: 0.4rem; padding: 0.3rem 1rem; cursor: pointer; transition: all 0.3s var(--ease); border-left: 2px solid transparent; }
.stn:hover { background: var(--vh); }
.stn .pip { width: 7px; height: 7px; border-radius: 2px; }
.stn .nm { font-size: 0.72rem; color: var(--s6); flex: 1; }
.stn .hdot { width: 5px; height: 5px; border-radius: 50%; }
.side-foot { padding: 0.6rem 1rem; border-top: 1px solid var(--vb); font-family: var(--mono); font-size: 0.55rem; color: var(--s4); display: flex; justify-content: space-between; }

.main { overflow-y: auto; padding: 1.75rem 2.25rem; }
.card { background: rgba(40,40,40,0.65); border: 1px solid var(--vb); border-radius: 4px; }
.label { font-family: var(--mono); font-size: 0.55rem; text-transform: uppercase; letter-spacing: 0.15em; color: var(--s4); margin-bottom: 0.5rem; }
.pg-head { margin-bottom: 1.25rem; }
.pg-head h2 { font-size: 1.3rem; font-weight: 600; letter-spacing: -0.02em; }
.pg-sub { font-family: var(--mono); font-size: 0.65rem; color: var(--s5); margin-top: 0.2rem; text-transform: uppercase; letter-spacing: 0.08em; }

/* Activity */
.act-list { display: flex; flex-direction: column; }
.act-item { display: grid; grid-template-columns: 60px 24px 120px 1fr 1fr; gap: 0.5rem; padding: 0.45rem 0.85rem; border-bottom: 1px solid var(--vb); align-items: center; transition: background 0.3s; }
.act-item:hover { background: rgba(255,255,255,0.02); }
.act-item.new { animation: slideIn 0.35s var(--ease) forwards; }
@keyframes slideIn { from { opacity: 0; transform: translateY(-8px); } to { opacity: 1; transform: translateY(0); } }
.act-time { font-family: var(--mono); font-size: 0.6rem; color: var(--s4); }
.act-pip { width: 3px; height: 14px; border-radius: 1px; }
.act-stone { font-family: var(--mono); font-size: 0.6rem; color: var(--s5); }
.act-event { display: flex; align-items: center; gap: 0.4rem; font-family: var(--mono); font-size: 0.6rem; color: var(--s6); }
.act-dot { width: 5px; height: 5px; border-radius: 50%; flex-shrink: 0; }
.act-detail { font-family: var(--mono); font-size: 0.6rem; color: var(--s4); }
.stream-indicator { font-family: var(--mono); font-size: 0.6rem; color: var(--s4); margin-top: 0.7rem; text-align: center; display: flex; align-items: center; justify-content: center; gap: 0.5rem; }
.stream-dot { width: 5px; height: 5px; border-radius: 50%; background: var(--sage); animation: br 2s ease-in-out infinite; }

/* Filter bar */
.fbar { display: flex; gap: 0.3rem; margin-bottom: 0.85rem; flex-wrap: wrap; align-items: center; }
.fbtn { background: transparent; border: 1px solid var(--vb); border-radius: 2px; padding: 0.22rem 0.55rem; font-family: var(--mono); font-size: 0.55rem; cursor: pointer; color: var(--s5); text-transform: uppercase; transition: all 0.3s var(--ease); }
.fbtn:hover { border-color: rgba(255,255,255,0.15); color: var(--s9); }
.fbtn.on { border-color: var(--sage); color: var(--sage); background: rgba(132,165,157,0.15); }

/* Pond */
.pond-grid { display: flex; flex-direction: column; gap: 0.5rem; }
.pond-card { padding: 0.85rem; display: grid; grid-template-columns: auto auto 1fr auto auto; gap: 0.85rem; align-items: center; transition: all 0.3s var(--ease); }
.pond-card:hover { background: rgba(255,255,255,0.02); }
.pond-pip { width: 8px; height: 24px; border-radius: 2px; flex-shrink: 0; }
.pond-name { font-weight: 500; font-size: 0.88rem; }
.pond-ep { font-family: var(--mono); font-size: 0.58rem; color: var(--s4); }
.pond-role { font-family: var(--mono); font-size: 0.6rem; color: var(--s4); text-transform: uppercase; letter-spacing: 0.05em; padding: 0.15rem 0.5rem; border: 1px solid var(--vb); border-radius: 2px; text-align: center; }
.pond-role.keystone { color: var(--gold); border-color: rgba(196,176,96,0.3); background: rgba(196,176,96,0.08); }
.hdot-i { width: 7px; height: 7px; border-radius: 50%; }
@keyframes hbr { 0%,100% { opacity:0.6; box-shadow:0 0 6px currentColor; } 50% { opacity:1; box-shadow:0 0 10px currentColor; } }

/* Stats */
.stats-row { display: flex; gap: 1.5rem; margin-bottom: 1.25rem; flex-wrap: wrap; }
.stat-card { background: rgba(40,40,40,0.65); border: 1px solid var(--vb); border-radius: 4px; padding: 0.75rem 1rem; min-width: 120px; }
.stat-val { font-size: 1.3rem; font-weight: 600; }
.stat-label { font-family: var(--mono); font-size: 0.5rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--s4); margin-top: 0.1rem; }

.fi { animation: fadeIn 0.45s var(--ease) forwards; opacity: 0; }
.fi1 { animation-delay: 0.06s; } .fi2 { animation-delay: 0.12s; }
@keyframes fadeIn { to { opacity: 1; } }
`;

// ═══════════════════════════════════════════════════════════════════
// ACTIVITY VIEW
// ═══════════════════════════════════════════════════════════════════
const ActivityPanel = () => {
  const [events, setEvents] = useState(INITIAL_EVENTS);
  const [filter, setFilter] = useState("all");
  const [newIds, setNewIds] = useState(new Set());
  const liveIdx = useRef(0);

  // Simulate live events arriving
  useEffect(() => {
    const t = setInterval(() => {
      const ev = LIVE_EVENTS[liveIdx.current % LIVE_EVENTS.length];
      const id = Date.now();
      const newEvent = { ...ev, time: "just now", _id: id };
      setEvents(prev => [newEvent, ...prev.slice(0, 24)]);
      setNewIds(prev => new Set([...prev, id]));
      setTimeout(() => setNewIds(prev => { const n = new Set(prev); n.delete(id); return n; }), 600);
      liveIdx.current++;
    }, 5000);
    return () => clearInterval(t);
  }, []);

  const types = ["all", "info", "success", "warning"];
  const filtered = filter === "all" ? events : events.filter(e => e.type === filter);
  const counts = { all: events.length, info: events.filter(e => e.type === "info").length, success: events.filter(e => e.type === "success").length, warning: events.filter(e => e.type === "warning").length };

  return (
    <>
      <div className="pg-head fi"><h2>Activity</h2><div className="pg-sub">Real-time event stream across the garden</div></div>

      <div className="stats-row fi fi1">
        <div className="stat-card"><div className="stat-val">{events.length}</div><div className="stat-label">Events</div></div>
        <div className="stat-card"><div className="stat-val" style={{ color: "var(--sage)" }}>{counts.success}</div><div className="stat-label">Success</div></div>
        <div className="stat-card"><div className="stat-val" style={{ color: "var(--clay)" }}>{counts.warning}</div><div className="stat-label">Warnings</div></div>
        <div className="stat-card">
          <div className="stat-val">{[...new Set(events.map(e => e.stone))].length}</div>
          <div className="stat-label">Active Stones</div>
        </div>
      </div>

      <div className="fbar fi fi1">
        {types.map(t => (
          <button key={t} className={`fbtn ${filter === t ? "on" : ""}`} onClick={() => setFilter(t)}>
            {t} ({counts[t]})
          </button>
        ))}
      </div>

      <div className="card fi fi2" style={{ overflow: "hidden" }}>
        <div className="act-list">
          {filtered.map((ev, i) => (
            <div className={`act-item ${newIds.has(ev._id) ? "new" : ""}`} key={ev._id || i}>
              <div className="act-time">{ev.time}</div>
              <div className="act-pip" style={{ background: stoneColor(ev.stone) }} />
              <div className="act-stone">{ev.stone}</div>
              <div className="act-event">
                <span className="act-dot" style={{ background: typeColor(ev.type), boxShadow: `0 0 4px ${typeColor(ev.type)}` }} />
                {ev.event}
              </div>
              <div className="act-detail">{ev.detail}</div>
            </div>
          ))}
        </div>
      </div>

      <div className="stream-indicator">
        <div className="stream-dot" />
        Streaming via SSE from Lantern presence aggregator · /api/v1/garden/presence/stream
      </div>
    </>
  );
};

// ═══════════════════════════════════════════════════════════════════
// POND VIEW
// ═══════════════════════════════════════════════════════════════════
const PondPanel = () => {
  const online = STONES.filter(s => s.health !== "resting").length;

  return (
    <>
      <div className="pg-head fi"><h2>Pond</h2><div className="pg-sub">Trust circle and security mesh</div></div>

      <div className="stats-row fi fi1">
        <div className="stat-card"><div className="stat-val">{STONES.length}</div><div className="stat-label">Members</div></div>
        <div className="stat-card"><div className="stat-val" style={{ color: "var(--sage)" }}>{online}</div><div className="stat-label">Online</div></div>
        <div className="stat-card"><div className="stat-val" style={{ color: "var(--gold)" }}>1</div><div className="stat-label">Keystone</div></div>
        <div className="stat-card"><div className="stat-val">Ed25519</div><div className="stat-label">Key Type</div></div>
      </div>

      <div className="label fi fi1">Trust Circle Members</div>
      <div className="pond-grid fi fi2">
        {STONES.map(s => (
          <div className="card pond-card" key={s.id}>
            <div className="pond-pip" style={{ background: s.color }} />
            <div>
              <div className="pond-name">{s.name}</div>
              <div className="pond-ep">{s.endpoint}</div>
            </div>
            <div />
            <div className={`pond-role ${s.pond === "keystone" ? "keystone" : ""}`}>{s.pond}</div>
            <div className="hdot-i" style={{
              background: healthColor(s.health), color: healthColor(s.health),
              boxShadow: s.health !== "resting" ? `0 0 6px ${healthColor(s.health)}` : "none",
              opacity: s.health === "resting" ? 0.4 : 1,
              animation: s.health !== "resting" ? `hbr ${s.health === "withering" ? "1.5s" : "3s"} ease-in-out infinite` : "none",
            }} />
          </div>
        ))}
      </div>

      <div className="label" style={{ marginTop: "1.25rem" }}>Security Properties</div>
      <div className="card fi fi2" style={{ padding: "0.85rem" }}>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.6rem" }}>
          {[
            { l: "Authentication", v: "Mutual TLS + Ed25519 keypairs" },
            { l: "Transport", v: "TLS 1.3 between all stones" },
            { l: "Discovery", v: "mDNS with pond-token validation" },
            { l: "Keystone election", v: "Automatic on keystone loss" },
            { l: "Invitation flow", v: "QR code or CLI token exchange" },
            { l: "Revocation", v: "Immediate propagation to all peers" },
          ].map(p => (
            <div key={p.l} style={{ padding: "0.4rem 0" }}>
              <div style={{ fontFamily: "var(--mono)", fontSize: "0.5rem", color: "var(--s4)", textTransform: "uppercase", letterSpacing: "0.1em", marginBottom: "0.15rem" }}>{p.l}</div>
              <div style={{ fontSize: "0.75rem", color: "var(--s6)" }}>{p.v}</div>
            </div>
          ))}
        </div>
      </div>
    </>
  );
};

// ═══════════════════════════════════════════════════════════════════
// MAIN
// ═══════════════════════════════════════════════════════════════════
export default function LanternActivityPond() {
  const [view, setView] = useState("activity");

  return (
    <>
      <style>{css}</style>
      <div className="shell">
        <aside className="side">
          <div className="side-brand">
            <h1>Lantern</h1>
            <div className="gname">Home Lab</div>
            <div className="side-health"><div className="pip-b" />3/4 stones</div>
          </div>
          <nav className="side-nav">
            <div className="nav-label">Views</div>
            <div className={`nav-item ${view === "activity" ? "active" : ""}`} onClick={() => setView("activity")}>
              <span className="nav-icon">↯</span>Activity
            </div>
            <div className={`nav-item ${view === "pond" ? "active" : ""}`} onClick={() => setView("pond")}>
              <span className="nav-icon">🔒</span>Pond
            </div>
            <div className="nav-label" style={{ marginTop: "0.5rem" }}>Stones</div>
            {STONES.map(s => (
              <div key={s.id} className="stn">
                <div className="pip" style={{ background: s.color }} />
                <div className="nm">{s.name}</div>
                <div className="hdot" style={{
                  background: healthColor(s.health),
                  boxShadow: s.health !== "resting" ? `0 0 4px ${healthColor(s.health)}` : "none",
                  opacity: s.health === "resting" ? 0.4 : 1,
                }} />
              </div>
            ))}
          </nav>
          <div className="side-foot"><span>Lantern v0.1.0</span><span>⏱ 2.3s</span></div>
        </aside>

        <main className="main" key={view}>
          {view === "activity" && <ActivityPanel />}
          {view === "pond" && <PondPanel />}
        </main>
      </div>
    </>
  );
}
