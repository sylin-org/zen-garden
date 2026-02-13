import { useState, useCallback } from "react";

// ═══════════════════════════════════════════════════════════════════
// MOCK DATA (shared across views)
// ═══════════════════════════════════════════════════════════════════
const STONES = [
  {
    id: "cf", name: "crystal-forest", color: "#84a59d", health: "thriving",
    endpoint: "http://192.168.1.42:7185", os: "Ubuntu 24.04",
    hw: { mfr: "Dell", model: "Wyse 5070", cores: 4, ram: 8, arch: "x86_64" },
    res: { cpu: 23, mem: 62, disk: 41 }, uptime: "14d 7h",
    services: [
      { offering: "mongodb", inst: null, status: "running", image: "mongo:7", port: 27017, desc: "Document database" },
      { offering: "redis", inst: null, status: "running", image: "redis:7-alpine", port: 6379, desc: "In-memory data store" },
      { offering: "minio", inst: null, status: "running", image: "minio/minio:latest", port: 9000, desc: "S3-compatible storage" },
    ],
    seeds: [{ name: "garden-primary", fs: "btrfs", size: "32GB", used: "12.4GB" }],
    companions: ["cricket", "firefly"], tags: [], pond: "keystone",
  },
  {
    id: "qs", name: "quiet-stream", color: "#d4a373", health: "thriving",
    endpoint: "http://192.168.1.108:7185", os: "Windows 11 Pro",
    hw: { mfr: "Custom", model: "GPU Workstation", cores: 16, ram: 64, arch: "x86_64" },
    res: { cpu: 67, mem: 78, disk: 55 }, uptime: "3d 19h",
    services: [
      { offering: "mongodb", inst: null, status: "running", image: "mongo:7", port: 27017, desc: "Document database" },
      { offering: "postgres", inst: "snapvault", status: "running", image: "postgres:16-alpine", port: 5432, desc: "Relational database" },
      { offering: "ollama", inst: null, status: "running", image: "ollama/ollama:latest", port: 11434, desc: "Local LLM inference" },
      { offering: "chromadb", inst: null, status: "running", image: "chromadb/chroma:latest", port: 8000, desc: "Vector embeddings" },
      { offering: "snapvault", inst: null, status: "running", image: "snapvault-pro:latest", port: 8080, desc: "AI photo management" },
    ],
    seeds: [], companions: ["cricket"], tags: ["opportunity"], pond: "member",
  },
  {
    id: "ar", name: "amber-ridge", color: "#c4b060", health: "withering",
    endpoint: "http://192.168.1.55:7185", os: "Debian 12",
    hw: { mfr: "Dell", model: "Wyse 5060", cores: 2, ram: 4, arch: "x86_64" },
    res: { cpu: 89, mem: 91, disk: 78 }, uptime: "28d 4h",
    services: [
      { offering: "grafana", inst: null, status: "running", image: "grafana/grafana:latest", port: 3000, desc: "Observability dashboards" },
      { offering: "mosquitto", inst: "iot-hub", status: "stopped", image: "eclipse-mosquitto:2", port: 1883, desc: "MQTT broker" },
      { offering: "ollama", inst: null, status: "running", image: "ollama/ollama:latest", port: 11434, desc: "Local LLM inference" },
    ],
    seeds: [{ name: "garden-backup", fs: "ext4", size: "64GB", used: "29.8GB" }],
    companions: [], tags: ["attention"], pond: "member",
  },
  {
    id: "it", name: "ivy-terrace", color: "#a8a29e", health: "resting",
    endpoint: "http://192.168.10.20:7185", os: "PostmarketOS",
    hw: { mfr: "Sony", model: "VAIO P VGN-P11Z", cores: 1, ram: 2, arch: "i686" },
    res: { cpu: 0, mem: 0, disk: 34 }, uptime: "—",
    services: [
      { offering: "mosquitto", inst: null, status: "stopped", image: "eclipse-mosquitto:2", port: 1883, desc: "MQTT broker" },
    ],
    seeds: [], companions: [], tags: [], pond: "member",
  },
];

const ACTIVITY = [
  { time: "2s ago", stone: "quiet-stream", event: "ollama.inference.completed", detail: "llama3.2 — 847 tokens, 2.1s", type: "info" },
  { time: "18s ago", stone: "crystal-forest", event: "service.health.ok", detail: "mongodb heartbeat", type: "info" },
  { time: "45s ago", stone: "amber-ridge", event: "resource.memory.high", detail: "91% memory usage", type: "warning" },
  { time: "1m ago", stone: "quiet-stream", event: "snapvault.photo.indexed", detail: "batch of 23 photos", type: "info" },
  { time: "2m ago", stone: "crystal-forest", event: "nurturing.completed", detail: "mongodb slot-A — 2.3GB", type: "success" },
  { time: "5m ago", stone: "amber-ridge", event: "resource.cpu.high", detail: "89% sustained for 5m", type: "warning" },
  { time: "8m ago", stone: "quiet-stream", event: "replica.sync.complete", detail: "mongodb ← crystal-forest", type: "success" },
];

// Helpers
const svcKey = (s) => (s.inst ? `${s.offering}:${s.inst}` : s.offering);
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
const isRepl = (sv) => (REPLICAS[svcKey(sv)]?.length || 0) > 1;
const activeRGs = () => Object.entries(REPLICAS).filter(([, m]) => m.length > 1);
const resourceColor = (p) => (p > 85 ? "#c45050" : p > 70 ? "#d4a373" : "#84a59d");
const healthColor = (h) => (h === "thriving" ? "#84a59d" : h === "withering" ? "#d4a373" : "#78716c");

// ═══════════════════════════════════════════════════════════════════
const css = `
@import url('https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@300;400;500&family=IBM+Plex+Sans:wght@300;400;500;600&display=swap');
:root {
  --bg: #1a1a1a; --bg2: #222220; --s9: #fafaf9; --s6: #a8a29e; --s5: #8a8580;
  --s4: #78716c; --s3: #57534e; --vb: rgba(255,255,255,0.08); --vh: rgba(255,255,255,0.04);
  --sage: #84a59d; --clay: #d4a373; --gold: #c4b060;
  --sans: 'IBM Plex Sans', system-ui, sans-serif; --mono: 'IBM Plex Mono', ui-monospace, monospace;
  --ease: cubic-bezier(0.22, 1, 0.36, 1);
}
* { margin: 0; padding: 0; box-sizing: border-box; }
body, #root { background: var(--bg); color: var(--s9); font-family: var(--sans); height: 100vh; overflow: hidden; }
.shell { display: grid; grid-template-columns: 200px 1fr; height: 100vh; }

/* Sidebar (compact repeat) */
.side { background: var(--bg2); border-right: 1px solid var(--vb); display: flex; flex-direction: column; }
.side-brand { padding: 1.25rem 1rem; border-bottom: 1px solid var(--vb); }
.side-brand h1 { font-family: var(--mono); font-size: 0.65rem; font-weight: 400; text-transform: uppercase; letter-spacing: 0.25em; color: var(--s5); margin-bottom: 0.3rem; }
.side-brand .gname { font-size: 1rem; font-weight: 600; }
.side-health { display: flex; align-items: center; gap: 0.35rem; margin-top: 0.4rem; font-family: var(--mono); font-size: 0.6rem; color: var(--s5); text-transform: uppercase; }
.pip-b { width: 6px; height: 6px; border-radius: 50%; background: var(--sage); animation: br 3s ease-in-out infinite; }
@keyframes br { 0%,100% { opacity:0.6; box-shadow:0 0 4px var(--sage); } 50% { opacity:1; box-shadow:0 0 10px var(--sage); } }
.side-nav { flex: 1; padding: 0.5rem 0; overflow-y: auto; }
.nav-label { font-family: var(--mono); font-size: 0.5rem; text-transform: uppercase; letter-spacing: 0.2em; color: var(--s4); padding: 0.6rem 1rem 0.25rem; }
.stn { display: flex; align-items: center; gap: 0.4rem; padding: 0.3rem 1rem; cursor: pointer; transition: all 0.3s var(--ease); border-left: 2px solid transparent; }
.stn:hover { background: var(--vh); }
.stn.active { background: var(--vh); border-left-color: var(--sage); }
.stn .pip { width: 7px; height: 7px; border-radius: 2px; }
.stn .nm { font-size: 0.72rem; color: var(--s6); flex: 1; }
.stn .hdot { width: 5px; height: 5px; border-radius: 50%; }
.side-foot { padding: 0.6rem 1rem; border-top: 1px solid var(--vb); font-family: var(--mono); font-size: 0.55rem; color: var(--s4); display: flex; justify-content: space-between; }

/* Main */
.main { overflow-y: auto; padding: 1.75rem 2.25rem; }
.pg-head { margin-bottom: 1.25rem; }
.pg-head h2 { font-size: 1.3rem; font-weight: 600; letter-spacing: -0.02em; }
.pg-sub { font-family: var(--mono); font-size: 0.65rem; color: var(--s5); margin-top: 0.2rem; text-transform: uppercase; letter-spacing: 0.08em; }

/* Summary */
.sumbar { display: flex; gap: 1.25rem; align-items: center; padding: 0.85rem 1.25rem; background: rgba(40,40,40,0.65); border: 1px solid var(--vb); border-radius: 4px; margin-bottom: 1.25rem; flex-wrap: wrap; }
.sumbar .sv { font-size: 1.35rem; font-weight: 600; letter-spacing: -0.03em; text-align: center; }
.sumbar .sl { font-family: var(--mono); font-size: 0.5rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--s4); margin-top: 0.1rem; text-align: center; }
.sumbar .sd { width: 1px; height: 1.75rem; background: var(--vb); }

/* Cards */
.card { background: rgba(40,40,40,0.65); backdrop-filter: blur(14px); border: 1px solid var(--vb); border-radius: 4px; padding: 0.9rem; }
.label { font-family: var(--mono); font-size: 0.55rem; text-transform: uppercase; letter-spacing: 0.15em; color: var(--s4); margin-bottom: 0.5rem; }

/* Stone grid */
.sgrid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 0.65rem; margin-bottom: 1.5rem; }
.scard { cursor: pointer; transition: all 0.4s var(--ease); }
.scard:hover { border-color: rgba(255,255,255,0.15); }
.scard-head { display: flex; align-items: center; gap: 0.55rem; margin-bottom: 0.6rem; }
.scard-bar { width: 4px; height: 32px; border-radius: 2px; flex-shrink: 0; }
.scard-name { font-weight: 500; font-size: 0.88rem; }
.scard-ep { font-family: var(--mono); font-size: 0.58rem; color: var(--s4); }
.hdot-i { width: 6px; height: 6px; border-radius: 50%; }
@keyframes hbr { 0%,100% { opacity:0.6; box-shadow:0 0 6px currentColor; } 50% { opacity:1; box-shadow:0 0 10px currentColor; } }

/* Resource bars */
.resgrid { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 0.4rem; margin-top: 0.4rem; }
.reslabel { font-family: var(--mono); font-size: 0.5rem; color: var(--s4); margin-bottom: 0.15rem; }
.resbar { height: 3px; background: rgba(255,255,255,0.06); border-radius: 2px; overflow: hidden; }
.resfill { height: 100%; border-radius: 2px; transition: width 0.6s var(--ease); }

/* Service chips */
.chips { display: flex; flex-wrap: wrap; gap: 0.3rem; margin-top: 0.55rem; }
.chip { display: inline-flex; align-items: center; gap: 0.25rem; font-family: var(--mono); font-size: 0.6rem; color: var(--s6); padding: 0.12rem 0.4rem; border: 1px solid var(--vb); border-radius: 2px; background: rgba(255,255,255,0.02); }
.chip.repl { border-color: rgba(132,165,157,0.3); background: rgba(132,165,157,0.06); }
.chip-dot { width: 4px; height: 4px; border-radius: 50%; }
.inst { color: var(--gold); opacity: 0.85; }
.inst-sep { color: var(--s4); opacity: 0.5; margin: 0 0.03em; }

/* Stone card footer */
.scard-foot { display: flex; justify-content: space-between; margin-top: 0.65rem; padding-top: 0.5rem; border-top: 1px solid var(--vb); }
.scard-meta { font-family: var(--mono); font-size: 0.55rem; color: var(--s4); }
.scard-tags { display: flex; gap: 0.3rem; font-size: 0.6rem; }

/* Replica groups */
.rg-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 0.65rem; margin-bottom: 1.5rem; }
.rg-card { border-left: 2px solid var(--sage); }
.rg-head { display: flex; align-items: center; gap: 0.6rem; margin-bottom: 0.4rem; }
.rg-name { font-family: var(--mono); font-weight: 500; font-size: 0.8rem; }
.rg-badge { display: inline-flex; align-items: center; gap: 0.2rem; font-family: var(--mono); font-size: 0.5rem; padding: 0.04rem 0.3rem; border-radius: 2px; background: rgba(132,165,157,0.1); border: 1px solid rgba(132,165,157,0.25); color: var(--sage); }
.rg-member { display: flex; align-items: center; gap: 0.5rem; padding: 0.2rem 0.3rem; border-radius: 2px; font-size: 0.7rem; cursor: pointer; transition: all 0.3s var(--ease); }
.rg-member:hover { background: rgba(255,255,255,0.04); }
.rg-pip { width: 3px; height: 12px; border-radius: 1px; flex-shrink: 0; }

/* Activity */
.act-list { display: flex; flex-direction: column; }
.act-item { display: grid; grid-template-columns: 55px 110px 1fr 1fr; gap: 0.6rem; padding: 0.4rem 0.85rem; border-bottom: 1px solid var(--vb); align-items: center; }
.act-time { font-family: var(--mono); font-size: 0.6rem; color: var(--s4); }
.act-stone { font-family: var(--mono); font-size: 0.6rem; color: var(--s5); }
.act-event { display: flex; align-items: center; gap: 0.4rem; font-family: var(--mono); font-size: 0.6rem; color: var(--s6); }
.act-dot { width: 4px; height: 4px; border-radius: 50%; flex-shrink: 0; }
.act-detail { font-family: var(--mono); font-size: 0.6rem; color: var(--s4); }

/* Animation */
.fi { animation: fadeIn 0.45s var(--ease) forwards; opacity: 0; }
.fi1 { animation-delay: 0.06s; } .fi2 { animation-delay: 0.12s; } .fi3 { animation-delay: 0.18s; }
@keyframes fadeIn { to { opacity: 1; } }
`;

// ═══════════════════════════════════════════════════════════════════
// COMPONENTS
// ═══════════════════════════════════════════════════════════════════
const StoneCard = ({ stone, onClick }) => {
  const running = stone.services.filter((s) => s.status === "running").length;
  return (
    <div className="card scard" onClick={() => onClick(stone.id)}>
      <div className="scard-head">
        <div className="scard-bar" style={{ background: stone.color }} />
        <div style={{ flex: 1 }}>
          <div className="scard-name">{stone.name}</div>
          <div className="scard-ep">{stone.endpoint}</div>
        </div>
        <div className="hdot-i" style={{
          background: healthColor(stone.health),
          color: healthColor(stone.health),
          boxShadow: stone.health !== "resting" ? `0 0 6px ${healthColor(stone.health)}` : "none",
          opacity: stone.health === "resting" ? 0.4 : 1,
          animation: stone.health !== "resting" ? `hbr ${stone.health === "withering" ? "1.5s" : "3s"} ease-in-out infinite` : "none",
        }} />
      </div>

      {stone.health !== "resting" && (
        <div className="resgrid">
          {[{ l: "CPU", v: stone.res.cpu }, { l: "MEM", v: stone.res.mem }, { l: "DSK", v: stone.res.disk }].map((r) => (
            <div key={r.l}>
              <div className="reslabel">{r.l} {r.v}%</div>
              <div className="resbar">
                <div className="resfill" style={{ width: `${r.v}%`, background: resourceColor(r.v) }} />
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="chips">
        {stone.services.map((svc, i) => {
          const rp = isRepl(svc);
          return (
            <div className={`chip ${rp ? "repl" : ""}`} key={`${svcKey(svc)}-${i}`}>
              <div className="chip-dot" style={{
                background: svc.status === "running" ? "var(--sage)" : "var(--s4)",
                boxShadow: svc.status === "running" ? "0 0 4px var(--sage)" : "none",
              }} />
              <span>{svc.offering}</span>
              {svc.inst && <><span className="inst-sep">:</span><span className="inst">{svc.inst}</span></>}
              {rp && <span style={{ fontSize: "0.5rem", color: "var(--sage)", marginLeft: "0.1rem" }}>⟐</span>}
            </div>
          );
        })}
      </div>

      <div className="scard-foot">
        <div className="scard-meta">{stone.hw.cores}c · {stone.hw.ram}GB · {stone.os.split(" ")[0]}</div>
        <div className="scard-tags">
          {stone.tags.includes("attention") && <span style={{ color: "var(--clay)" }}>⚠</span>}
          {stone.tags.includes("opportunity") && <span style={{ color: "var(--gold)" }}>✦</span>}
          {stone.seeds.length > 0 && <span>🌱</span>}
          {stone.companions.length > 0 && <span>🔊</span>}
        </div>
      </div>
    </div>
  );
};

// ═══════════════════════════════════════════════════════════════════
export default function LanternGarden() {
  const [selected, setSelected] = useState(null);

  const online = STONES.filter((s) => s.health !== "resting").length;
  const totalSvcs = STONES.reduce((n, s) => n + s.services.filter((v) => v.status === "running").length, 0);
  const warnings = STONES.filter((s) => s.tags.includes("attention")).length;
  const rgs = activeRGs();

  return (
    <>
      <style>{css}</style>
      <div className="shell">
        <aside className="side">
          <div className="side-brand">
            <h1>Lantern</h1>
            <div className="gname">Home Lab</div>
            <div className="side-health"><div className="pip-b" />{online}/{STONES.length} stones</div>
          </div>
          <nav className="side-nav">
            <div className="nav-label">Stones</div>
            {STONES.map((s) => (
              <div key={s.id} className={`stn ${selected === s.id ? "active" : ""}`} onClick={() => setSelected(s.id)}>
                <div className="pip" style={{ background: s.color }} />
                <div className="nm">{s.name}</div>
                {s.tags.includes("attention") && <span style={{ fontSize: "0.55rem", color: "var(--clay)" }}>⚠</span>}
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

        <main className="main">
          <div className="pg-head fi"><h2>Garden</h2><div className="pg-sub">Topology and health across all stones</div></div>

          {/* Summary */}
          <div className="sumbar fi fi1">
            <div><div className="sv">{STONES.length}</div><div className="sl">Stones</div></div><div className="sd" />
            <div><div className="sv">{online}</div><div className="sl">Online</div></div><div className="sd" />
            <div><div className="sv">{totalSvcs}</div><div className="sl">Services</div></div><div className="sd" />
            <div><div className="sv" style={{ color: rgs.length > 0 ? "var(--sage)" : undefined }}>{rgs.length}</div><div className="sl">Replica Groups</div></div><div className="sd" />
            <div><div className="sv" style={{ color: warnings > 0 ? "var(--clay)" : undefined }}>{warnings}</div><div className="sl">Attention</div></div>
          </div>

          {/* Stone cards */}
          <div className="sgrid fi fi2">
            {STONES.map((s) => <StoneCard key={s.id} stone={s} onClick={setSelected} />)}
          </div>

          {/* Replica groups */}
          {rgs.length > 0 && (
            <div style={{ marginBottom: "1.5rem" }}>
              <div className="label">Replica Groups · {rgs.length}</div>
              <div className="rg-grid">
                {rgs.map(([key, members]) => {
                  const sample = members[0].service;
                  const ok = members.every((m) => m.service.status === "running");
                  return (
                    <div className="card rg-card" key={key}>
                      <div className="rg-head">
                        <div className="rg-name">
                          {sample.offering}
                          {sample.inst && <><span className="inst-sep">:</span><span className="inst">{sample.inst}</span></>}
                        </div>
                        <span className="rg-badge">⟐ {members.length}× {ok ? "synced" : "partial"}</span>
                      </div>
                      {members.map((m) => (
                        <div className="rg-member" key={m.stoneId} onClick={() => setSelected(m.stoneId)}>
                          <div className="rg-pip" style={{ background: m.stoneColor }} />
                          <span style={{ flex: 1, fontFamily: "var(--mono)", fontSize: "0.65rem", color: "var(--s5)" }}>{m.stoneName}</span>
                          <span style={{ fontFamily: "var(--mono)", fontSize: "0.55rem", color: m.service.status === "running" ? "var(--sage)" : "var(--s4)" }}>
                            {m.service.status === "running" ? "● running" : "○ stopped"}
                          </span>
                          <span style={{ fontFamily: "var(--mono)", fontSize: "0.6rem", color: "var(--s4)" }}>:{m.service.port}</span>
                        </div>
                      ))}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Activity */}
          <div className="label">Recent Activity</div>
          <div className="card fi fi3" style={{ padding: 0, overflow: "hidden" }}>
            <div className="act-list">
              {ACTIVITY.map((ev, i) => (
                <div className="act-item" key={i}>
                  <div className="act-time">{ev.time}</div>
                  <div className="act-stone">{ev.stone}</div>
                  <div className="act-event">
                    <span className="act-dot" style={{
                      background: ev.type === "success" ? "var(--sage)" : ev.type === "warning" ? "var(--clay)" : "var(--s4)",
                    }} />
                    {ev.event}
                  </div>
                  <div className="act-detail">{ev.detail}</div>
                </div>
              ))}
            </div>
          </div>
          <div style={{ fontFamily: "var(--mono)", fontSize: "0.6rem", color: "var(--s4)", marginTop: "0.6rem", textAlign: "center" }}>
            ↓ Streaming via SSE from Lantern presence aggregator
          </div>
        </main>
      </div>
    </>
  );
}
