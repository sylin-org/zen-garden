import { useState } from "react";

// ═══════════════════════════════════════════════════════════════════
// DATA
// ═══════════════════════════════════════════════════════════════════
const STONES = [
  { id: "cf", name: "crystal-forest", color: "#84a59d", services: [
    { offering: "mongodb", inst: null, status: "running", image: "mongo:7", port: 27017 },
    { offering: "redis", inst: null, status: "running", image: "redis:7-alpine", port: 6379 },
    { offering: "minio", inst: null, status: "running", image: "minio/minio:latest", port: 9000 },
  ]},
  { id: "qs", name: "quiet-stream", color: "#d4a373", services: [
    { offering: "mongodb", inst: null, status: "running", image: "mongo:7", port: 27017 },
    { offering: "postgres", inst: "snapvault", status: "running", image: "postgres:16-alpine", port: 5432 },
    { offering: "ollama", inst: null, status: "running", image: "ollama/ollama:latest", port: 11434 },
    { offering: "chromadb", inst: null, status: "running", image: "chromadb/chroma:latest", port: 8000 },
    { offering: "snapvault", inst: null, status: "running", image: "snapvault-pro:latest", port: 8080 },
  ]},
  { id: "ar", name: "amber-ridge", color: "#c4b060", services: [
    { offering: "grafana", inst: null, status: "running", image: "grafana/grafana:latest", port: 3000 },
    { offering: "mosquitto", inst: "iot-hub", status: "stopped", image: "eclipse-mosquitto:2", port: 1883 },
    { offering: "ollama", inst: null, status: "running", image: "ollama/ollama:latest", port: 11434 },
  ]},
  { id: "it", name: "ivy-terrace", color: "#a8a29e", services: [
    { offering: "mosquitto", inst: null, status: "stopped", image: "eclipse-mosquitto:2", port: 1883 },
  ]},
];

const CATALOG = [
  { name: "mongodb", cat: "database", desc: "Document database", image: "mongo:7" },
  { name: "postgres", cat: "database", desc: "Relational database", image: "postgres:16-alpine" },
  { name: "redis", cat: "cache", desc: "In-memory data store", image: "redis:7-alpine" },
  { name: "minio", cat: "storage", desc: "S3-compatible object storage", image: "minio/minio:latest" },
  { name: "ollama", cat: "ai", desc: "Local LLM inference", image: "ollama/ollama:latest" },
  { name: "chromadb", cat: "ai", desc: "Vector embedding database", image: "chromadb/chroma:latest" },
  { name: "grafana", cat: "monitoring", desc: "Observability dashboards", image: "grafana/grafana:latest" },
  { name: "mosquitto", cat: "messaging", desc: "MQTT message broker", image: "eclipse-mosquitto:2" },
  { name: "snapvault", cat: "application", desc: "AI-powered photo management", image: "snapvault-pro:latest" },
  { name: "mariadb", cat: "database", desc: "MySQL-compatible database", image: "mariadb:11" },
  { name: "influxdb", cat: "monitoring", desc: "Time-series database", image: "influxdb:2" },
];

const svcKey = (s) => (s.inst ? `${s.offering}:${s.inst}` : s.offering);

// Build deployment map
const deployMap = {};
STONES.forEach((st) =>
  st.services.forEach((sv) => {
    if (!deployMap[sv.offering]) deployMap[sv.offering] = [];
    deployMap[sv.offering].push({ stoneId: st.id, stoneName: st.name, stoneColor: st.color, inst: sv.inst, status: sv.status, port: sv.port, identityKey: svcKey(sv) });
  })
);

const groupByIdentity = (offering) => {
  const instances = deployMap[offering] || [];
  const groups = {};
  instances.forEach((i) => { if (!groups[i.identityKey]) groups[i.identityKey] = []; groups[i.identityKey].push(i); });
  return Object.entries(groups);
};

const healthColor = (h) => (h === "thriving" ? "#84a59d" : h === "withering" ? "#d4a373" : "#78716c");

// ═══════════════════════════════════════════════════════════════════
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
.stn { display: flex; align-items: center; gap: 0.4rem; padding: 0.3rem 1rem; cursor: pointer; transition: all 0.3s var(--ease); border-left: 2px solid transparent; }
.stn:hover { background: var(--vh); }
.stn .pip { width: 7px; height: 7px; border-radius: 2px; }
.stn .nm { font-size: 0.72rem; color: var(--s6); flex: 1; }
.side-foot { padding: 0.6rem 1rem; border-top: 1px solid var(--vb); font-family: var(--mono); font-size: 0.55rem; color: var(--s4); display: flex; justify-content: space-between; }

.main { overflow-y: auto; padding: 1.75rem 2.25rem; }
.pg-head { margin-bottom: 1.25rem; }
.pg-head h2 { font-size: 1.3rem; font-weight: 600; letter-spacing: -0.02em; }
.pg-sub { font-family: var(--mono); font-size: 0.65rem; color: var(--s5); margin-top: 0.2rem; text-transform: uppercase; letter-spacing: 0.08em; }

.fbar { display: flex; gap: 0.3rem; margin-bottom: 1rem; flex-wrap: wrap; }
.fbtn { background: transparent; border: 1px solid var(--vb); border-radius: 2px; padding: 0.22rem 0.55rem; font-family: var(--mono); font-size: 0.55rem; cursor: pointer; color: var(--s5); text-transform: uppercase; transition: all 0.3s var(--ease); }
.fbtn:hover { border-color: rgba(255,255,255,0.15); color: var(--s9); }
.fbtn.on { border-color: var(--sage); color: var(--sage); background: rgba(132,165,157,0.15); }

.card { background: rgba(40,40,40,0.65); border: 1px solid var(--vb); border-radius: 4px; padding: 0.9rem; transition: all 0.4s var(--ease); }
.card:hover { border-color: rgba(255,255,255,0.15); }
.cgrid { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 0.65rem; }
.cname { font-weight: 500; font-size: 0.88rem; }
.ccat { font-family: var(--mono); font-size: 0.5rem; color: var(--s4); text-transform: uppercase; letter-spacing: 0.1em; }
.cdesc { font-size: 0.72rem; color: var(--s5); margin: 0.3rem 0; }
.cimage { font-family: var(--mono); font-size: 0.55rem; color: var(--s4); margin-bottom: 0.4rem; }

.dep-section { display: flex; flex-direction: column; gap: 0.35rem; padding-top: 0.35rem; border-top: 1px solid var(--vb); }
.id-head { display: flex; align-items: center; gap: 0.35rem; margin-bottom: 0.1rem; }
.id-name { font-family: var(--mono); font-size: 0.58rem; color: var(--s5); }
.inst { color: var(--gold); }
.inst-sep { color: var(--s4); opacity: 0.5; }
.rg-badge { display: inline-flex; align-items: center; gap: 0.15rem; font-family: var(--mono); font-size: 0.48rem; padding: 0.02rem 0.25rem; border-radius: 2px; background: rgba(132,165,157,0.1); border: 1px solid rgba(132,165,157,0.25); color: var(--sage); }
.dep-row { display: flex; align-items: center; gap: 0.3rem; padding-left: 0.4rem; font-family: var(--mono); font-size: 0.58rem; color: var(--s5); cursor: pointer; transition: all 0.3s; }
.dep-row:hover { color: var(--s9); }
.dep-pip { width: 3px; height: 10px; border-radius: 1px; }
.dep-dot { width: 4px; height: 4px; border-radius: 50%; }

.dbtn { background: transparent; border: 1px solid var(--vb); border-radius: 2px; padding: 0.25rem 0.5rem; font-family: var(--mono); font-size: 0.55rem; cursor: pointer; color: var(--s5); text-transform: uppercase; width: 100%; transition: all 0.3s var(--ease); text-align: center; }
.dbtn:hover { background: var(--sage); color: white; border-color: var(--sage); }

.fi { animation: fadeIn 0.45s var(--ease) forwards; opacity: 0; }
.fi1 { animation-delay: 0.06s; } .fi2 { animation-delay: 0.12s; }
@keyframes fadeIn { to { opacity: 1; } }
`;

export default function LanternOfferings() {
  const [filter, setFilter] = useState("all");
  const cats = [...new Set(CATALOG.map((c) => c.cat))];
  const filtered = filter === "all" ? CATALOG : CATALOG.filter((c) => c.cat === filter);
  const deployed = Object.keys(deployMap).length;

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
            <div className="nav-label">Stones</div>
            {STONES.map((s) => (
              <div key={s.id} className="stn">
                <div className="pip" style={{ background: s.color }} />
                <div className="nm">{s.name}</div>
              </div>
            ))}
          </nav>
          <div className="side-foot"><span>Lantern v0.1.0</span><span>⏱ 2.3s</span></div>
        </aside>

        <main className="main">
          <div className="pg-head fi">
            <h2>Offerings</h2>
            <div className="pg-sub">Service topology across the garden · {deployed} deployed of {CATALOG.length} available</div>
          </div>

          <div className="fbar fi fi1">
            <button className={`fbtn ${filter === "all" ? "on" : ""}`} onClick={() => setFilter("all")}>All ({CATALOG.length})</button>
            {cats.map((c) => (
              <button key={c} className={`fbtn ${filter === c ? "on" : ""}`} onClick={() => setFilter(c)}>
                {c} ({CATALOG.filter((o) => o.cat === c).length})
              </button>
            ))}
          </div>

          <div className="cgrid fi fi2">
            {filtered.map((off) => {
              const identityGroups = groupByIdentity(off.name);
              const isDep = identityGroups.length > 0;

              return (
                <div className="card" key={off.name}>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "start" }}>
                    <div>
                      <div className="cname">{off.name}</div>
                      <div className="ccat">{off.cat}</div>
                    </div>
                    {isDep && (
                      <div style={{ width: 6, height: 6, borderRadius: "50%", background: "var(--sage)", boxShadow: "0 0 6px var(--sage)", marginTop: 4 }} />
                    )}
                  </div>
                  <div className="cdesc">{off.desc}</div>
                  <div className="cimage">{off.image}</div>

                  {isDep ? (
                    <div className="dep-section">
                      {identityGroups.map(([ik, instances]) => {
                        const isRG = instances.length > 1;
                        const nm = instances[0].inst;
                        return (
                          <div key={ik}>
                            <div className="id-head">
                              <span className="id-name">
                                {off.name}
                                {nm ? <><span className="inst-sep">:</span><span className="inst">{nm}</span></> : (
                                  <span style={{ fontStyle: "italic", color: "var(--s4)", marginLeft: "0.25rem" }}>(unnamed)</span>
                                )}
                              </span>
                              {isRG && <span className="rg-badge">⟐ {instances.length}×</span>}
                            </div>
                            {instances.map((d) => (
                              <div className="dep-row" key={`${d.stoneId}-${d.identityKey}`}>
                                <div className="dep-pip" style={{ background: d.stoneColor }} />
                                <span>{d.stoneName}</span>
                                <span style={{ color: "var(--s4)" }}>:{d.port}</span>
                                <div className="dep-dot" style={{
                                  background: d.status === "running" ? "var(--sage)" : "var(--s4)",
                                  boxShadow: d.status === "running" ? "0 0 4px var(--sage)" : "none",
                                }} />
                              </div>
                            ))}
                          </div>
                        );
                      })}
                    </div>
                  ) : (
                    <div style={{ paddingTop: "0.35rem", borderTop: "1px solid var(--vb)" }}>
                      <button className="dbtn">+ Deploy</button>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </main>
      </div>
    </>
  );
}
