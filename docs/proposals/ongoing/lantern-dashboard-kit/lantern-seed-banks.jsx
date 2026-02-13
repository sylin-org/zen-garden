import { useState } from "react";

// ═══════════════════════════════════════════════════════════════════
// DATA
// ═══════════════════════════════════════════════════════════════════
const STONES = [
  { id: "cf", name: "crystal-forest", color: "#84a59d", health: "thriving", seeds: [
    { name: "garden-primary", fs: "btrfs", size: "32GB", used: "12.4GB", mountpoint: "/mnt/seeds/primary", status: "mounted" },
  ]},
  { id: "qs", name: "quiet-stream", color: "#d4a373", health: "thriving", seeds: [
    { name: "garden-primary", fs: "ntfs", size: "128GB", used: "34.1GB", mountpoint: "D:\\Seeds\\Primary", status: "mounted" },
  ]},
  { id: "ar", name: "amber-ridge", color: "#c4b060", health: "withering", seeds: [
    { name: "garden-backup", fs: "ext4", size: "64GB", used: "29.8GB", mountpoint: "/mnt/seeds/backup", status: "mounted" },
    { name: "garden-primary", fs: "ext4", size: "16GB", used: "11.9GB", mountpoint: "/mnt/seeds/primary", status: "mounted" },
  ]},
  { id: "it", name: "ivy-terrace", color: "#a8a29e", health: "resting", seeds: [] },
];

// Build identity groups (same name = replica set)
const buildGroups = () => {
  const g = {};
  STONES.forEach(st => st.seeds.forEach(sb => {
    if (!g[sb.name]) g[sb.name] = [];
    g[sb.name].push({ stoneId: st.id, stoneName: st.name, stoneColor: st.color, health: st.health, seed: sb });
  }));
  return g;
};
const GROUPS = buildGroups();
const groupEntries = Object.entries(GROUPS).sort((a, b) => b[1].length - a[1].length);
const totalBanks = STONES.reduce((n, s) => n + s.seeds.length, 0);
const replicaGroups = groupEntries.filter(([, m]) => m.length > 1);
const stonesWithSeeds = new Set(STONES.filter(s => s.seeds.length > 0).map(s => s.id));

// Helpers
const resourceColor = (p) => (p > 85 ? "#c45050" : p > 70 ? "#d4a373" : "#84a59d");
const healthColor = (h) => (h === "thriving" ? "#84a59d" : h === "withering" ? "#d4a373" : "#78716c");

// Compute aggregate usage per group
const groupUsage = (members) => {
  let used = 0, total = 0;
  members.forEach(m => { used += parseFloat(m.seed.used); total += parseFloat(m.seed.size); });
  return { used: used.toFixed(1), total: total.toFixed(1), pct: total > 0 ? (used / total) * 100 : 0 };
};

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
.stn { display: flex; align-items: center; gap: 0.4rem; padding: 0.3rem 1rem; cursor: pointer; transition: all 0.3s var(--ease); border-left: 2px solid transparent; }
.stn:hover { background: var(--vh); }
.stn .pip { width: 7px; height: 7px; border-radius: 2px; }
.stn .nm { font-size: 0.72rem; color: var(--s6); flex: 1; }
.stn .hdot { width: 5px; height: 5px; border-radius: 50%; }
.side-foot { padding: 0.6rem 1rem; border-top: 1px solid var(--vb); font-family: var(--mono); font-size: 0.55rem; color: var(--s4); display: flex; justify-content: space-between; }

.main { overflow-y: auto; padding: 1.75rem 2.25rem; }
.pghead { margin-bottom: 1.25rem; }
.pghead h2 { font-size: 1.3rem; font-weight: 600; letter-spacing: -0.02em; }
.pgsub { font-family: var(--mono); font-size: 0.65rem; color: var(--s5); margin-top: 0.2rem; text-transform: uppercase; letter-spacing: 0.08em; }
.label { font-family: var(--mono); font-size: 0.55rem; text-transform: uppercase; letter-spacing: 0.15em; color: var(--s4); margin-bottom: 0.5rem; }
.card { background: rgba(40,40,40,0.65); border: 1px solid var(--vb); border-radius: 4px; padding: 0.9rem; transition: all 0.4s var(--ease); }
.card:hover { border-color: rgba(255,255,255,0.15); }
.btn { background: transparent; border: 1px solid var(--vb); border-radius: 2px; padding: 0.25rem 0.55rem; font-family: var(--mono); font-size: 0.55rem; cursor: pointer; color: var(--s5); text-transform: uppercase; letter-spacing: 0.03em; transition: all 0.3s var(--ease); white-space: nowrap; }
.btn:hover { background: var(--sage); color: white; border-color: var(--sage); }
.btn.warn:hover { background: var(--clay); border-color: var(--clay); }
.bg { display: flex; gap: 0.3rem; margin-left: auto; flex-shrink: 0; }

/* Summary bar */
.sumbar { display: flex; gap: 1.25rem; align-items: center; padding: 0.85rem 1.25rem; background: rgba(40,40,40,0.65); border: 1px solid var(--vb); border-radius: 4px; margin-bottom: 1.25rem; flex-wrap: wrap; }
.sumbar .sv { font-size: 1.35rem; font-weight: 600; letter-spacing: -0.03em; text-align: center; }
.sumbar .sl { font-family: var(--mono); font-size: 0.5rem; text-transform: uppercase; letter-spacing: 0.1em; color: var(--s4); margin-top: 0.1rem; text-align: center; }
.sumbar .sd { width: 1px; height: 1.75rem; background: var(--vb); }

/* Identity group cards */
.grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 0.75rem; }
.gcard { border-left: 3px solid var(--sage); }
.gcard.single { border-left-color: var(--s4); }
.grow { display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.5rem; }
.gname { font-weight: 500; font-size: 0.95rem; }
.rb { display: inline-flex; align-items: center; gap: 0.2rem; font-family: var(--mono); font-size: 0.5rem; padding: 0.04rem 0.3rem; border-radius: 2px; background: rgba(132,165,157,0.1); border: 1px solid rgba(132,165,157,0.25); color: var(--sage); }

/* Aggregate bar */
.agg { margin-bottom: 0.6rem; }
.aggbar { height: 8px; background: rgba(255,255,255,0.04); border-radius: 4px; overflow: hidden; position: relative; margin-top: 0.25rem; }
.aggfill { height: 100%; border-radius: 4px; transition: width 0.8s var(--ease); position: relative; }
.aggfill::after { content: ''; position: absolute; top: 0; right: 0; bottom: 0; width: 4px; background: rgba(255,255,255,0.2); border-radius: 0 4px 4px 0; }
.agglabels { display: flex; justify-content: space-between; font-family: var(--mono); font-size: 0.55rem; color: var(--s4); margin-top: 0.2rem; }

/* Member rows */
.memlist { display: flex; flex-direction: column; gap: 0.25rem; }
.mem { display: flex; align-items: center; gap: 0.5rem; padding: 0.35rem 0.5rem; border-radius: 2px; cursor: pointer; transition: all 0.3s; }
.mem:hover { background: rgba(255,255,255,0.03); }
.mpip { width: 3px; height: 16px; border-radius: 1px; flex-shrink: 0; }
.minfo { flex: 1; min-width: 0; }
.mname { font-family: var(--mono); font-size: 0.7rem; color: var(--s6); }
.mmeta { font-family: var(--mono); font-size: 0.55rem; color: var(--s4); }
.mbar { height: 3px; background: rgba(255,255,255,0.06); border-radius: 2px; width: 80px; flex-shrink: 0; }
.mfill { height: 100%; border-radius: 2px; }
.mpct { font-family: var(--mono); font-size: 0.5rem; color: var(--s4); width: 2.5rem; text-align: right; flex-shrink: 0; }
.mstatus { display: flex; align-items: center; gap: 0.25rem; font-family: var(--mono); font-size: 0.5rem; text-transform: uppercase; flex-shrink: 0; }
.mdot { width: 4px; height: 4px; border-radius: 50%; }

/* Identity section in cards */
.isec { margin-top: 0.5rem; padding-top: 0.45rem; border-top: 1px solid var(--vb); display: flex; align-items: center; gap: 0.4rem; flex-wrap: wrap; }
.ihint { font-family: var(--mono); font-size: 0.5rem; color: var(--s4); margin-top: 0.15rem; }

/* Stones without seeds */
.empty-row { display: flex; align-items: center; gap: 0.5rem; padding: 0.35rem 0.5rem; }
.epip { width: 3px; height: 14px; border-radius: 1px; flex-shrink: 0; }
.ename { font-family: var(--mono); font-size: 0.7rem; color: var(--s4); }
.etag { font-family: var(--mono); font-size: 0.5rem; color: var(--s4); font-style: italic; }

.fi { animation: fadeIn 0.45s var(--ease) forwards; opacity: 0; }
.fi1 { animation-delay: 0.06s; } .fi2 { animation-delay: 0.12s; } .fi3 { animation-delay: 0.18s; }
@keyframes fadeIn { to { opacity: 1; } }
`;

export default function LanternSeedBanks() {
  const unseedCount = STONES.filter(s => s.seeds.length === 0).length;

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
              <div key={s.id} className="stn">
                <div className="pip" style={{ background: s.color }} />
                <div className="nm">{s.name}</div>
                {s.seeds.length > 0 && <span style={{ fontSize: "0.6rem" }}>🌱</span>}
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
          <div className="pghead fi">
            <h2>Seed Banks</h2>
            <div className="pgsub">Storage topology across the garden · {totalBanks} bank{totalBanks !== 1 ? "s" : ""} on {stonesWithSeeds.size} stone{stonesWithSeeds.size !== 1 ? "s" : ""}</div>
          </div>

          {/* Summary */}
          <div className="sumbar fi fi1">
            <div><div className="sv">{totalBanks}</div><div className="sl">Seed Banks</div></div><div className="sd" />
            <div><div className="sv">{stonesWithSeeds.size}</div><div className="sl">Stones</div></div><div className="sd" />
            <div><div className="sv" style={{ color: replicaGroups.length > 0 ? "var(--sage)" : undefined }}>{replicaGroups.length}</div><div className="sl">Replica Groups</div></div><div className="sd" />
            <div><div className="sv">{groupEntries.length}</div><div className="sl">Identities</div></div><div className="sd" />
            <div><div className="sv">{unseedCount}</div><div className="sl">Unseeded</div></div>
          </div>

          {/* Identity groups */}
          <div className="label fi fi1">Seed Bank Identities</div>
          <div className="grid fi fi2">
            {groupEntries.map(([name, members]) => {
              const isRG = members.length > 1;
              const usage = groupUsage(members);
              return (
                <div className={`card gcard ${isRG ? "" : "single"}`} key={name}>
                  <div className="grow">
                    <span style={{ fontSize: "0.9rem" }}>🌱</span>
                    <span className="gname">{name}</span>
                    {isRG && <span className="rb">⟐ {members.length}× replicated</span>}
                    <div className="bg">
                      <button className="btn">Rename</button>
                    </div>
                  </div>

                  {/* Aggregate usage bar */}
                  <div className="agg">
                    <div className="aggbar">
                      <div className="aggfill" style={{ width: `${usage.pct}%`, background: resourceColor(usage.pct) }} />
                    </div>
                    <div className="agglabels">
                      <span>{usage.used}GB used</span>
                      <span>{usage.total}GB total</span>
                      <span>{Math.round(usage.pct)}%</span>
                    </div>
                  </div>

                  {/* Member list */}
                  <div className="memlist">
                    {members.map(m => {
                      const pct = (parseFloat(m.seed.used) / parseFloat(m.seed.size)) * 100;
                      return (
                        <div className="mem" key={m.stoneId}>
                          <div className="mpip" style={{ background: m.stoneColor }} />
                          <div className="minfo">
                            <div className="mname">{m.stoneName}</div>
                            <div className="mmeta">{m.seed.fs} · {m.seed.mountpoint}</div>
                          </div>
                          <div className="mpct">{m.seed.used}</div>
                          <div className="mbar"><div className="mfill" style={{ width: `${pct}%`, background: resourceColor(pct) }} /></div>
                          <div className="mpct">{Math.round(pct)}%</div>
                          <div className="mstatus" style={{ color: m.seed.status === "mounted" ? "var(--sage)" : "var(--s4)" }}>
                            <div className="mdot" style={{ background: m.seed.status === "mounted" ? "var(--sage)" : "var(--s4)", boxShadow: m.seed.status === "mounted" ? "0 0 4px var(--sage)" : "none" }} />
                            {m.seed.status}
                          </div>
                          <div className="bg">
                            <button className="btn" style={{ fontSize: "0.5rem", padding: "0.15rem 0.35rem" }}>Eject</button>
                            <button className="btn warn" style={{ fontSize: "0.5rem", padding: "0.15rem 0.35rem" }}>Release</button>
                          </div>
                        </div>
                      );
                    })}
                  </div>

                  {/* Identity + replication hint */}
                  <div className="isec">
                    <span className="label" style={{ margin: 0 }}>Identity</span>
                    <span style={{ fontFamily: "var(--mono)", fontSize: "0.65rem", color: "var(--s5)" }}>{name}</span>
                    {isRG && (
                      <span style={{ fontFamily: "var(--mono)", fontSize: "0.55rem", color: "var(--sage)", marginLeft: "0.3rem" }}>
                        synced across {members.length} stones
                      </span>
                    )}
                  </div>
                  <div className="ihint" style={{ paddingLeft: 0 }}>
                    {isRG
                      ? "All members replicate automatically. Renaming a member forks it into a distinct identity."
                      : "Connect a seed bank with this name on another stone to begin replication."}
                  </div>
                </div>
              );
            })}
          </div>

          {/* Stones without seed banks */}
          {unseedCount > 0 && (
            <>
              <div className="label" style={{ marginTop: "1.5rem" }}>Unseeded Stones</div>
              <div className="card fi fi3" style={{ padding: "0.6rem 0.85rem" }}>
                {STONES.filter(s => s.seeds.length === 0).map(s => (
                  <div className="empty-row" key={s.id}>
                    <div className="epip" style={{ background: s.color }} />
                    <span className="ename">{s.name}</span>
                    <span className="etag">{s.health === "resting" ? "slumbering" : "no storage attached"}</span>
                    <div className="bg">
                      <button className="btn" style={{ fontSize: "0.5rem", padding: "0.15rem 0.35rem" }}>Attach</button>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}

          {/* How it works */}
          <div className="label" style={{ marginTop: "1.5rem" }}>How Seed Bank Replication Works</div>
          <div className="card fi fi3" style={{ padding: "0.85rem" }}>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "0.75rem" }}>
              {[
                { l: "Identity", v: "Seed banks are identified by name. Same name = same identity." },
                { l: "Replication", v: "Banks sharing an identity across stones replicate automatically." },
                { l: "Renaming", v: "Renaming a bank forks it from its replica set into a new identity." },
                { l: "Filesystems", v: "Each stone can use its own filesystem (btrfs, ext4, ntfs). Replication is format-agnostic." },
                { l: "Capacity", v: "Banks in a replica set don't need identical sizes. Replication adapts to the smallest member." },
                { l: "Ejection", v: "Ejecting unmounts safely. The replica set continues with remaining members." },
              ].map(p => (
                <div key={p.l} style={{ padding: "0.3rem 0" }}>
                  <div style={{ fontFamily: "var(--mono)", fontSize: "0.5rem", color: "var(--s4)", textTransform: "uppercase", letterSpacing: "0.1em", marginBottom: "0.15rem" }}>{p.l}</div>
                  <div style={{ fontSize: "0.72rem", color: "var(--s6)", lineHeight: 1.5 }}>{p.v}</div>
                </div>
              ))}
            </div>
          </div>
        </main>
      </div>
    </>
  );
}
