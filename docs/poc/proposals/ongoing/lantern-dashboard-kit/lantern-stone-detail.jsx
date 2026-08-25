import { useState } from "react";

// ═══════════════════════════════════════════════════════════════════
// DATA
// ═══════════════════════════════════════════════════════════════════
const STONES = [
  {
    id: "cf", name: "crystal-forest", color: "#84a59d", health: "thriving",
    endpoint: "http://192.168.1.42:7185", os: "Ubuntu 24.04",
    hw: { mfr: "Dell", model: "Wyse 5070", cores: 4, ram: 8, arch: "x86_64" },
    res: { cpu: 23, mem: 62, disk: 41 }, net: { rx: 142.3, tx: 38.7 }, uptime: "14d 7h",
    services: [
      { offering: "mongodb", inst: null, status: "running", image: "mongo:7", port: 27017, desc: "Document database", caps: null },
      { offering: "redis", inst: null, status: "running", image: "redis:7-alpine", port: 6379, desc: "In-memory data store", caps: null },
      { offering: "minio", inst: null, status: "running", image: "minio/minio:latest", port: 9000, desc: "S3-compatible storage", caps: null },
    ],
    seeds: [{ name: "garden-primary", fs: "btrfs", size: "32GB", used: "12.4GB", mountpoint: "/mnt/seeds/primary", status: "mounted" }],
    companions: [{ name: "cricket", status: "running", port: 7187, detail: "tune: mr-robot · vol: 65" }, { name: "firefly", status: "running", port: 7188, detail: "mode: presence" }],
    pond: "keystone",
  },
  {
    id: "qs", name: "quiet-stream", color: "#d4a373", health: "thriving",
    endpoint: "http://192.168.1.108:7185", os: "Windows 11 Pro",
    hw: { mfr: "Custom", model: "GPU Workstation", cores: 16, ram: 64, arch: "x86_64" },
    res: { cpu: 67, mem: 78, disk: 55 }, net: { rx: 892.1, tx: 234.5 }, uptime: "3d 19h",
    services: [
      { offering: "mongodb", inst: null, status: "running", image: "mongo:7", port: 27017, desc: "Document database", caps: null },
      { offering: "postgres", inst: "snapvault", status: "running", image: "postgres:16-alpine", port: 5432, desc: "Relational database", caps: null },
      { offering: "ollama", inst: null, status: "running", image: "ollama/ollama:latest", port: 11434, desc: "Local LLM inference", caps: ["llama3.2", "phi3", "gemma2"] },
      { offering: "chromadb", inst: null, status: "running", image: "chromadb/chroma:latest", port: 8000, desc: "Vector embeddings", caps: null },
      { offering: "snapvault", inst: null, status: "running", image: "snapvault-pro:latest", port: 8080, desc: "AI photo management", caps: null },
    ],
    seeds: [{ name: "garden-primary", fs: "ntfs", size: "128GB", used: "34.1GB", mountpoint: "D:\\Seeds\\Primary", status: "mounted" }],
    companions: [{ name: "cricket", status: "running", port: 7187, detail: "tune: zen-garden · vol: 40" }],
    pond: "member",
  },
  {
    id: "ar", name: "amber-ridge", color: "#c4b060", health: "withering",
    endpoint: "http://192.168.1.55:7185", os: "Debian 12",
    hw: { mfr: "Dell", model: "Wyse 5060", cores: 2, ram: 4, arch: "x86_64" },
    res: { cpu: 89, mem: 91, disk: 78 }, net: { rx: 45.2, tx: 12.1 }, uptime: "28d 4h",
    services: [
      { offering: "grafana", inst: null, status: "running", image: "grafana/grafana:latest", port: 3000, desc: "Observability dashboards", caps: null },
      { offering: "mosquitto", inst: "iot-hub", status: "stopped", image: "eclipse-mosquitto:2", port: 1883, desc: "MQTT broker", caps: null },
      { offering: "ollama", inst: null, status: "running", image: "ollama/ollama:latest", port: 11434, desc: "Local LLM inference", caps: ["phi3", "gemma2"] },
    ],
    seeds: [
      { name: "garden-backup", fs: "ext4", size: "64GB", used: "29.8GB", mountpoint: "/mnt/seeds/backup", status: "mounted" },
      { name: "garden-primary", fs: "ext4", size: "16GB", used: "11.9GB", mountpoint: "/mnt/seeds/primary", status: "mounted" },
    ],
    companions: [], pond: "member",
  },
  {
    id: "it", name: "ivy-terrace", color: "#a8a29e", health: "resting",
    endpoint: "http://192.168.10.20:7185", os: "PostmarketOS",
    hw: { mfr: "Sony", model: "VAIO P VGN-P11Z", cores: 1, ram: 2, arch: "i686" },
    res: { cpu: 0, mem: 0, disk: 34 }, net: { rx: 0, tx: 0 }, uptime: "—",
    services: [{ offering: "mosquitto", inst: null, status: "stopped", image: "eclipse-mosquitto:2", port: 1883, desc: "MQTT broker", caps: null }],
    seeds: [], companions: [], pond: "member",
  },
];

// Service replicas
const svcKey = (s) => (s.inst ? `${s.offering}:${s.inst}` : s.offering);
const buildReplicas = () => { const g = {}; STONES.forEach(st => st.services.forEach(sv => { const k = svcKey(sv); if (!g[k]) g[k] = []; g[k].push({ stoneId: st.id, stoneName: st.name, stoneColor: st.color, service: sv }); })); return g; };
const REPLICAS = buildReplicas();
const isRepl = (sv) => (REPLICAS[svcKey(sv)]?.length || 0) > 1;
const svcPeers = (sv, exId) => (REPLICAS[svcKey(sv)] || []).filter(m => m.stoneId !== exId);

// Seed bank replicas (same name = replica set)
const buildSeedReplicas = () => { const g = {}; STONES.forEach(st => st.seeds.forEach(sb => { if (!g[sb.name]) g[sb.name] = []; g[sb.name].push({ stoneId: st.id, stoneName: st.name, stoneColor: st.color, seed: sb }); })); return g; };
const SEED_REPLICAS = buildSeedReplicas();
const isSeedRepl = (sb) => (SEED_REPLICAS[sb.name]?.length || 0) > 1;
const seedPeers = (sb, exId) => (SEED_REPLICAS[sb.name] || []).filter(m => m.stoneId !== exId);

const resourceColor = (p) => (p > 85 ? "#c45050" : p > 70 ? "#d4a373" : "#84a59d");
const healthColor = (h) => (h === "thriving" ? "#84a59d" : h === "withering" ? "#d4a373" : "#78716c");

const css = `
@import url('https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@300;400;500&family=IBM+Plex+Sans:wght@300;400;500;600&display=swap');
:root {
  --bg: #1a1a1a; --bg2: #222220; --s9: #fafaf9; --s7: #d6d3d1; --s6: #a8a29e; --s5: #8a8580;
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
.stn { display: flex; align-items: center; gap: 0.4rem; padding: 0.3rem 1rem; cursor: pointer; transition: all 0.3s var(--ease); border-left: 2px solid transparent; }
.stn:hover { background: var(--vh); }
.stn.active { background: var(--vh); border-left-color: var(--sage); }
.stn .pip { width: 7px; height: 7px; border-radius: 2px; }
.stn .nm { font-size: 0.72rem; color: var(--s6); flex: 1; }
.stn .hdot { width: 5px; height: 5px; border-radius: 50%; }
.side-foot { padding: 0.6rem 1rem; border-top: 1px solid var(--vb); font-family: var(--mono); font-size: 0.55rem; color: var(--s4); display: flex; justify-content: space-between; }
.main { overflow-y: auto; padding: 1.75rem 2.25rem; }
.card { background: rgba(40,40,40,0.65); border: 1px solid var(--vb); border-radius: 4px; padding: 0.9rem; }
.label { font-family: var(--mono); font-size: 0.55rem; text-transform: uppercase; letter-spacing: 0.15em; color: var(--s4); margin-bottom: 0.5rem; }
.btn { background: transparent; border: 1px solid var(--vb); border-radius: 2px; padding: 0.25rem 0.55rem; font-family: var(--mono); font-size: 0.55rem; cursor: pointer; color: var(--s5); text-transform: uppercase; letter-spacing: 0.03em; transition: all 0.3s var(--ease); white-space: nowrap; }
.btn:hover { background: var(--sage); color: white; border-color: var(--sage); }
.btn.warn:hover { background: var(--clay); border-color: var(--clay); }
.btn.sm { padding: 0.18rem 0.4rem; font-size: 0.5rem; }
.bg { display: flex; gap: 0.3rem; margin-left: auto; flex-shrink: 0; }

/* Header */
.dh { display: flex; align-items: center; gap: 0.65rem; margin-bottom: 0.4rem; flex-wrap: wrap; }
.dbar { width: 4px; height: 28px; border-radius: 2px; flex-shrink: 0; }
.dname { font-size: 1.3rem; font-weight: 600; letter-spacing: -0.02em; }
.dsub { font-family: var(--mono); font-size: 0.62rem; color: var(--s5); margin-bottom: 1.25rem; }
.hdi { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
@keyframes hbr { 0%,100% { opacity:0.6; box-shadow:0 0 6px currentColor; } 50% { opacity:1; box-shadow:0 0 10px currentColor; } }

/* Resources */
.rg { display: grid; grid-template-columns: 1fr 1fr 1fr 1fr; gap: 1.25rem; }
.rv { font-size: 1.4rem; font-weight: 600; }
.ga { height: 3px; background: rgba(255,255,255,0.06); border-radius: 2px; margin-top: 0.3rem; }
.gf { height: 100%; border-radius: 2px; transition: width 0.8s var(--ease); }
.rm { font-size: 1rem; font-weight: 500; font-family: var(--mono); }

/* Row layout: left content, right actions */
.row { display: flex; align-items: center; gap: 0.85rem; }
.rstat { flex-shrink: 0; }
.rcont { flex: 1; min-width: 0; }
.rmeta { display: flex; align-items: center; gap: 0.6rem; flex-shrink: 0; }
.sd { display: flex; align-items: center; gap: 0.3rem; font-family: var(--mono); font-size: 0.55rem; text-transform: uppercase; }
.sdot { width: 4px; height: 4px; border-radius: 50%; }
.sn { font-weight: 500; font-size: 0.82rem; }
.sdesc { font-family: var(--mono); font-size: 0.6rem; color: var(--s4); }
.simg { font-family: var(--mono); font-size: 0.6rem; color: var(--s4); }
.sp { font-family: var(--mono); font-size: 0.68rem; color: var(--s5); font-weight: 500; }
.inst { color: var(--gold); font-weight: 400; opacity: 0.85; }
.isep { color: var(--s4); opacity: 0.5; margin: 0 0.03em; }
.rb { display: inline-flex; align-items: center; gap: 0.2rem; font-family: var(--mono); font-size: 0.5rem; padding: 0.04rem 0.3rem; border-radius: 2px; background: rgba(132,165,157,0.1); border: 1px solid rgba(132,165,157,0.25); color: var(--sage); }

/* Subsections */
.sub { margin-top: 0.4rem; padding-top: 0.35rem; border-top: 1px solid var(--vb); padding-left: 1.8rem; }
.subl { font-family: var(--mono); font-size: 0.45rem; color: var(--s4); text-transform: uppercase; letter-spacing: 0.1em; margin-bottom: 0.15rem; }
.irow { display: flex; align-items: center; gap: 0.4rem; flex-wrap: wrap; }
.ihint { font-family: var(--mono); font-size: 0.5rem; color: var(--s4); margin-top: 0.15rem; line-height: 1.4; }
.pc { display: inline-flex; align-items: center; gap: 0.25rem; padding: 0.12rem 0.4rem; background: rgba(255,255,255,0.02); border: 1px solid var(--vb); border-radius: 2px; font-family: var(--mono); font-size: 0.6rem; color: var(--s5); cursor: pointer; transition: all 0.3s; }
.pc:hover { background: rgba(255,255,255,0.05); }
.pp { width: 3px; height: 10px; border-radius: 1px; display: inline-block; }
.pd { width: 4px; height: 4px; border-radius: 50%; display: inline-block; }
.ct { font-family: var(--mono); font-size: 0.55rem; padding: 0.06rem 0.28rem; background: rgba(132,165,157,0.1); border: 1px solid rgba(132,165,157,0.2); border-radius: 2px; color: var(--sage); }

/* Seed usage bar */
.su { display: flex; align-items: center; gap: 0.5rem; margin-top: 0.3rem; }
.sbar { height: 3px; background: rgba(255,255,255,0.06); border-radius: 2px; flex: 1; max-width: 120px; }
.sfill { height: 100%; border-radius: 2px; transition: width 0.6s var(--ease); }

.slist { display: flex; flex-direction: column; gap: 0.5rem; }
.fi { animation: fadeIn 0.45s var(--ease) forwards; opacity: 0; }
.fi1 { animation-delay: 0.06s; } .fi2 { animation-delay: 0.12s; } .fi3 { animation-delay: 0.18s; } .fi4 { animation-delay: 0.24s; }
@keyframes fadeIn { to { opacity: 1; } }
`;

export default function LanternStoneDetail() {
  const [selected, setSelected] = useState("qs");
  const stone = STONES.find(s => s.id === selected);
  const alive = stone.health !== "resting";

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
            {STONES.map(s => (
              <div key={s.id} className={`stn ${selected === s.id ? "active" : ""}`} onClick={() => setSelected(s.id)}>
                <div className="pip" style={{ background: s.color }} />
                <div className="nm">{s.name}</div>
                {s.health === "withering" && <span style={{ fontSize: "0.55rem", color: "var(--clay)" }}>⚠</span>}
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

        <main className="main" key={selected}>
          {/* Header */}
          <div className="fi">
            <div className="dh">
              <button className="btn" onClick={() => {}}>← Garden</button>
              <div className="dbar" style={{ background: stone.color }} />
              <div className="dname">{stone.name}</div>
              <div className="hdi" style={{
                background: healthColor(stone.health), color: healthColor(stone.health),
                boxShadow: alive ? `0 0 6px ${healthColor(stone.health)}` : "none",
                opacity: stone.health === "resting" ? 0.4 : 1,
                animation: alive ? `hbr ${stone.health === "withering" ? "1.5s" : "3s"} ease-in-out infinite` : "none",
              }} />
            </div>
            <div className="dsub">{stone.endpoint} · {stone.hw.mfr} {stone.hw.model} · {stone.os} · {stone.hw.arch}</div>
          </div>

          {alive ? (
            <>
              {/* Resources */}
              <div className="card fi fi1" style={{ marginBottom: "1rem" }}>
                <div className="rg">
                  {[
                    { l: "CPU", v: stone.res.cpu, u: "%" },
                    { l: "Memory", v: stone.res.mem, u: "%" },
                    { l: "Disk", v: stone.res.disk, u: "%" },
                    { l: "Uptime", v: stone.uptime },
                  ].map(r => (
                    <div key={r.l}>
                      <div className="label">{r.l}</div>
                      {typeof r.v === "number" ? (
                        <><div className="rv" style={{ color: resourceColor(r.v) }}>{r.v}{r.u}</div><div className="ga"><div className="gf" style={{ width: `${r.v}%`, background: resourceColor(r.v) }} /></div></>
                      ) : <div className="rm">{r.v}</div>}
                    </div>
                  ))}
                </div>
              </div>

              {/* Network + Pond */}
              <div className="card fi fi1" style={{ marginBottom: "1rem" }}>
                <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: "1.25rem" }}>
                  <div><div className="label">Network RX</div><div className="rm">{stone.net.rx} MB</div></div>
                  <div><div className="label">Network TX</div><div className="rm">{stone.net.tx} MB</div></div>
                  <div><div className="label">Pond Role</div><div className="rm" style={{ color: stone.pond === "keystone" ? "var(--gold)" : "var(--s5)", textTransform: "uppercase", fontSize: "0.75rem" }}>{stone.pond}</div></div>
                </div>
              </div>

              {/* ── Offerings ── */}
              <div className="label">Offerings · {stone.services.length}</div>
              <div className="slist fi fi2">
                {stone.services.map((svc, si) => {
                  const rp = isRepl(svc);
                  const prs = svcPeers(svc, stone.id);
                  const setSize = REPLICAS[svcKey(svc)]?.length || 1;
                  return (
                    <div className="card" key={`${svcKey(svc)}-${si}`}>
                      <div className="row">
                        <div className="rstat">
                          <div className="sd" style={{ color: svc.status === "running" ? "var(--sage)" : "var(--s4)" }}>
                            <div className="sdot" style={{ background: svc.status === "running" ? "var(--sage)" : "var(--s4)", boxShadow: svc.status === "running" ? "0 0 4px var(--sage)" : "none" }} />
                            {svc.status}
                          </div>
                        </div>
                        <div className="rcont">
                          <div style={{ display: "flex", alignItems: "center", gap: "0.4rem", flexWrap: "wrap" }}>
                            <span className="sn">{svc.offering}{svc.inst && <><span className="isep">:</span><span className="inst">{svc.inst}</span></>}</span>
                            {rp && <span className="rb">⟐ {prs.length} peer{prs.length !== 1 ? "s" : ""}</span>}
                          </div>
                          <div className="sdesc">{svc.desc}</div>
                        </div>
                        <div className="rmeta">
                          <div className="simg">{svc.image}</div>
                          <div className="sp">:{svc.port}</div>
                          <div className="bg">
                            <button className="btn">{svc.status === "running" ? "Rest" : "Wake"}</button>
                            <button className="btn">Config</button>
                          </div>
                        </div>
                      </div>
                      {svc.caps && svc.caps.length > 0 && (
                        <div className="sub"><div className="subl">Capabilities</div><div style={{ display: "flex", gap: "0.2rem", flexWrap: "wrap" }}>{svc.caps.map(c => <span key={c} className="ct">{c}</span>)}</div></div>
                      )}
                      {rp && prs.length > 0 && (
                        <div className="sub"><div className="subl">Replica peers</div><div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap" }}>
                          {prs.map(p => (<span key={p.stoneId} className="pc" onClick={() => setSelected(p.stoneId)}><span className="pp" style={{ background: p.stoneColor }} />{p.stoneName}<span style={{ color: "var(--s4)", fontSize: "0.55rem" }}>:{p.service.port}</span><span className="pd" style={{ background: p.service.status === "running" ? "var(--sage)" : "var(--s4)" }} /></span>))}
                        </div></div>
                      )}
                      <div className="sub">
                        <div className="irow">
                          <span className="subl" style={{ margin: 0 }}>Instance</span>
                          {svc.inst
                            ? <span style={{ fontFamily: "var(--mono)", fontSize: "0.65rem", color: "var(--gold)" }}>{svc.inst}</span>
                            : <span style={{ fontFamily: "var(--mono)", fontSize: "0.6rem", color: "var(--s4)", fontStyle: "italic" }}>unnamed</span>}
                          <div className="bg">
                            {svc.inst ? <><button className="btn sm">Rename</button><button className="btn sm warn">Clear</button></> : <button className="btn sm">Name</button>}
                          </div>
                        </div>
                        <div className="ihint">
                          {!svc.inst && rp ? `⚠ Naming will wipe data and fork from this ${setSize}-member replica set.` : !svc.inst ? "Naming creates a distinct identity." : "Renaming or clearing will wipe data and migrate identity."}
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>

              {/* ── Seed Banks ── */}
              {stone.seeds.length > 0 && (
                <>
                  <div className="label" style={{ marginTop: "1.25rem" }}>Seed Banks · {stone.seeds.length}</div>
                  <div className="slist fi fi3">
                    {stone.seeds.map((sb, si) => {
                      const rp = isSeedRepl(sb);
                      const sps = seedPeers(sb, stone.id);
                      const pct = (parseFloat(sb.used) / parseFloat(sb.size)) * 100;
                      return (
                        <div className="card" key={`${sb.name}-${si}`}>
                          <div className="row">
                            <div style={{ fontSize: "0.9rem", flexShrink: 0, width: "1.4rem", textAlign: "center" }}>🌱</div>
                            <div className="rcont">
                              <div style={{ display: "flex", alignItems: "center", gap: "0.4rem", flexWrap: "wrap" }}>
                                <span className="sn">{sb.name}</span>
                                {rp && <span className="rb">⟐ {sps.length} peer{sps.length !== 1 ? "s" : ""}</span>}
                              </div>
                              <div className="sdesc">{sb.fs} · {sb.mountpoint}</div>
                              <div className="su">
                                <span style={{ fontFamily: "var(--mono)", fontSize: "0.55rem", color: "var(--s5)" }}>{sb.used} / {sb.size}</span>
                                <div className="sbar"><div className="sfill" style={{ width: `${pct}%`, background: resourceColor(pct) }} /></div>
                                <span style={{ fontFamily: "var(--mono)", fontSize: "0.5rem", color: "var(--s4)" }}>{Math.round(pct)}%</span>
                              </div>
                            </div>
                            <div className="rmeta">
                              <span className="sd" style={{ color: sb.status === "mounted" ? "var(--sage)" : "var(--s4)" }}>
                                <span className="sdot" style={{ background: sb.status === "mounted" ? "var(--sage)" : "var(--s4)", boxShadow: sb.status === "mounted" ? "0 0 4px var(--sage)" : "none" }} />
                                {sb.status}
                              </span>
                              <div className="bg">
                                <button className="btn">Eject</button>
                                <button className="btn warn">Release</button>
                              </div>
                            </div>
                          </div>
                          {rp && sps.length > 0 && (
                            <div className="sub"><div className="subl">Replica peers</div><div style={{ display: "flex", gap: "0.35rem", flexWrap: "wrap" }}>
                              {sps.map(p => (<span key={p.stoneId} className="pc" onClick={() => setSelected(p.stoneId)}><span className="pp" style={{ background: p.stoneColor }} />{p.stoneName}<span style={{ color: "var(--s4)", fontSize: "0.55rem" }}>{p.seed.fs} · {p.seed.used}/{p.seed.size}</span><span className="pd" style={{ background: "var(--sage)" }} /></span>))}
                            </div></div>
                          )}
                          <div className="sub">
                            <div className="irow">
                              <span className="subl" style={{ margin: 0 }}>Identity</span>
                              <span style={{ fontFamily: "var(--mono)", fontSize: "0.65rem", color: "var(--s5)" }}>{sb.name}</span>
                              <div className="bg"><button className="btn sm">Rename</button></div>
                            </div>
                            <div className="ihint">
                              {rp ? `Same-named seed banks replicate automatically. Renaming forks from this ${SEED_REPLICAS[sb.name].length}-member set.` : "Seed banks sharing the same name across stones enter replica mode."}
                            </div>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </>
              )}

              {/* ── Companions ── */}
              {stone.companions.length > 0 && (
                <>
                  <div className="label" style={{ marginTop: "1.25rem" }}>Companions · {stone.companions.length}</div>
                  <div className="slist fi fi3">
                    {stone.companions.map(c => (
                      <div className="card" key={c.name}>
                        <div className="row">
                          <div className="rstat">
                            <div className="sd" style={{ color: c.status === "running" ? "var(--sage)" : "var(--s4)" }}>
                              <div className="sdot" style={{ background: c.status === "running" ? "var(--sage)" : "var(--s4)", boxShadow: c.status === "running" ? "0 0 4px var(--sage)" : "none" }} />
                              {c.status}
                            </div>
                          </div>
                          <div className="rcont">
                            <div className="sn">{c.name}</div>
                            <div className="sdesc">{c.detail}</div>
                          </div>
                          <div className="rmeta">
                            <div className="sp">:{c.port}</div>
                            <div className="bg"><button className="btn">Commands</button></div>
                          </div>
                        </div>
                      </div>
                    ))}
                  </div>
                </>
              )}
            </>
          ) : (
            <div className="card fi fi1" style={{ textAlign: "center", padding: "2.5rem" }}>
              <div style={{ fontSize: "0.85rem", color: "var(--s5)", marginBottom: "1rem" }}>This stone is slumbering</div>
              <div style={{ fontFamily: "var(--mono)", fontSize: "0.6rem", color: "var(--s4)", marginBottom: "1.25rem" }}>
                {stone.services.length} offering{stone.services.length !== 1 ? "s" : ""} configured · will resume on wake
              </div>
              <button className="btn">☀ Rouse</button>
            </div>
          )}

          {/* ── Administration — right-aligned ── */}
          <div className="label" style={{ marginTop: "1.25rem" }}>Stone Administration</div>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: "0.4rem", flexWrap: "wrap" }} className="fi fi4">
            <button className="btn">Portrait ↗</button>
            <button className="btn">Nourish</button>
            <button className="btn">Reconcile</button>
            {alive && <button className="btn">Stir (Reboot)</button>}
            {alive && <button className="btn warn">Slumber</button>}
            {!alive && <button className="btn">Rouse</button>}
          </div>
        </main>
      </div>
    </>
  );
}
