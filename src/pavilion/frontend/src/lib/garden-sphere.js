/**
 * GardenSphere — 3D rotatable sphere visualization for Zen Garden infrastructure.
 *
 * Adapted from the sphere-kit reference to consume Lantern API field names directly.
 * Stones use the shape returned by GET /api/v1/garden/stones.
 *
 * Dependency: Three.js (loaded via CDN importmap)
 *
 * Usage:
 *   const gs = new GardenSphere(containerElement, options);
 *   gs.setData(stones);          // initial load (API stone objects)
 *   gs.updateStone(id, patch);   // live metrics/health/services
 *   gs.addStone(stone);          // new node, animated re-layout
 *   gs.removeStone(id);          // fade out + re-layout
 *   gs.offlineStone(id);         // gray out, stays in place
 *   gs.onlineStone(id, patch?);  // restore from offline
 *   gs.resetView();              // reset camera
 *   gs.destroy();                // cleanup
 */

import * as THREE from "three";

// ── Helpers ──────────────────────────────────────────────────────

/** Service identity key from an offering entry */
export const serviceKey = (svc) => {
  const id = svc.offering || svc.name || "";
  return svc.instance_name ? `${id}:${svc.instance_name}` : id;
};

/** Resource gauge color: sage < 70, clay 70-85, red > 85 */
const rc = (v) => (v > 85 ? "#c45050" : v > 70 ? "#d4a373" : "#84a59d");

/** Health state color — accepts both spec vocabulary and real API values */
const hc = (h) =>
  h === "thriving" || h === "healthy"
    ? "#84a59d"
    : h === "withering" || h === "degraded"
      ? "#d4a373"
      : h === "unhealthy"
        ? "#c45050"
        : "#78716c";

/** Is the stone considered alive (not sleeping/offline)? */
const isAlive = (h) => h !== "resting" && h !== "installing";

/** Apply alpha to any CSS color string (hex, hsl, rgb, named) */
const _alphaCtx = document.createElement("canvas").getContext("2d");
function withAlpha(cssColor, alpha) {
  _alphaCtx.clearRect(0, 0, 1, 1);
  _alphaCtx.fillStyle = cssColor;
  _alphaCtx.fillRect(0, 0, 1, 1);
  const [r, g, b] = _alphaCtx.getImageData(0, 0, 1, 1).data;
  return `rgba(${r},${g},${b},${alpha})`;
}

/** Fallback color when API doesn't provide one */
const FALLBACK_COLOR = "#84a59d";

/** Extract display name from stone_name (strip "stone-" prefix) */
function displayName(stone) {
  const n = stone.stone_name || "";
  return n.startsWith("stone-") ? n.slice(6) : n;
}

/** Extract CPU cores from resources */
function getCores(stone) {
  return stone.resources?.cpu_cores ?? 0;
}

/** Extract memory in GB from resources */
function getMemGB(stone) {
  const bytes = stone.resources?.memory_total_bytes ?? 0;
  return Math.round(bytes / 1073741824);
}

/** Extract resource percentages */
function getRes(stone) {
  return {
    cpu: stone.resources?.cpu_percent ?? 0,
    mem: stone.resources?.memory_percent ?? 0,
    dsk: stone.resources?.disk_percent ?? 0,
  };
}

/** Fibonacci sphere distribution — even spacing on unit sphere */
function fibSphere(n) {
  if (n === 0) return [];
  if (n === 1) return [[0, 0, 1]];
  const pts = [],
    phi = Math.PI * (3 - Math.sqrt(5));
  for (let i = 0; i < n; i++) {
    const y = 1 - (i / (n - 1)) * 2,
      r = Math.sqrt(1 - y * y),
      t = phi * i;
    pts.push([Math.cos(t) * r, y, Math.sin(t) * r]);
  }
  return pts;
}

/** Spherical linear interpolation path between two points on sphere surface */
function greatCircle(p1, p2, R, segs = 48) {
  const v1 = p1.clone().normalize(),
    v2 = p2.clone().normalize();
  const om = Math.acos(THREE.MathUtils.clamp(v1.dot(v2), -1, 1));
  const sinOm = Math.sin(om),
    pts = [];
  for (let i = 0; i <= segs; i++) {
    const t = i / segs;
    let p;
    if (sinOm < 0.001) {
      p = v1.clone().lerp(v2, t).normalize();
    } else {
      const a = Math.sin((1 - t) * om) / sinOm,
        b = Math.sin(t * om) / sinOm;
      p = new THREE.Vector3(
        v1.x * a + v2.x * b,
        v1.y * a + v2.y * b,
        v1.z * a + v2.z * b,
      ).normalize();
    }
    pts.push(p.multiplyScalar(R * 1.003));
  }
  return pts;
}

/**
 * Compute set-membership edges between stone pairs.
 *
 * For each pair (i, j), collect the offerings they share by serviceKey.
 * For each shared offering, look up each stone's role (primary | replica
 * | joining | degraded | null) and classify the edge:
 *
 * - **directed**: one stone is primary AND the other is replica (or
 *   joining catching up). Renders as a dashed gold line with the head
 *   pointing from the primary to the replica.
 * - **peer**: neither side claims primary, OR roles are unknown.
 *   Renders as a solid sage line, symmetric.
 *
 * `direction` on each edge:
 *   - 0 = peer (no primary↔replica relation)
 *   - 1 = i → j (i is primary)
 *   - 2 = j → i (j is primary)
 *
 * Mixed sets where some are primary↔replica and others are peer-only
 * fall under whichever the FIRST shared offering classifies as. The
 * rendering picks one shape per edge — over-rendering one tube per
 * shared offering would cluster too densely on the sphere. Future
 * work: tooltips / labels listing each set's role pair.
 */
function computeEdges(stones) {
  const edges = [];
  for (let i = 0; i < stones.length; i++)
    for (let j = i + 1; j < stones.length; j++) {
      const shared = new Set();
      let direction = 0;
      const svcsA = stones[i].offerings || [];
      const svcsB = stones[j].offerings || [];
      svcsA.forEach((a) =>
        svcsB.forEach((b) => {
          if (serviceKey(a) === serviceKey(b)) {
            shared.add(serviceKey(a));
            // First shared offering with a primary↔(replica|joining)
            // pair pins the edge direction.
            if (direction === 0) {
              const aRole = (a.role || "").toLowerCase();
              const bRole = (b.role || "").toLowerCase();
              const aIsPrimary = aRole === "primary";
              const bIsPrimary = bRole === "primary";
              const aIsFollower = aRole === "replica" || aRole === "joining";
              const bIsFollower = bRole === "replica" || bRole === "joining";
              if (aIsPrimary && bIsFollower) direction = 1;
              else if (bIsPrimary && aIsFollower) direction = 2;
            }
          }
        }),
      );
      if (shared.size > 0)
        edges.push({ from: i, to: j, sets: [...shared], direction });
    }
  return edges;
}

/**
 * Render a stone's canvas-based display sprite.
 * 512×512 transparent canvas with health arcs, center LED, name, hardware, service dots.
 *
 * Consumes Lantern API stone shape directly.
 */
function renderStoneCanvas(stone, offline = false) {
  const S = 512,
    H = S / 2,
    CY = 195,
    AR = 148,
    LW = 7,
    GAP = 0.14;
  const c = document.createElement("canvas");
  c.width = S;
  c.height = S;
  const x = c.getContext("2d");
  const SEG = (Math.PI * 2) / 3 - GAP;
  const alive = isAlive(stone.health) && !offline;
  const res = getRes(stone);
  const color = stone.color || FALLBACK_COLOR;

  // Resource arcs (3 segments: CPU, MEM, DSK)
  [res.cpu, res.mem, res.dsk].forEach((val, i) => {
    const a0 = (i * (Math.PI * 2)) / 3 - Math.PI / 2 + GAP / 2;
    x.beginPath();
    x.arc(H, CY, AR, a0, a0 + SEG);
    x.strokeStyle = "rgba(255,255,255,0.07)";
    x.lineWidth = LW;
    x.lineCap = "round";
    x.stroke();
    if (alive) {
      const fill = SEG * (val / 100);
      if (fill > 0.02) {
        x.beginPath();
        x.arc(H, CY, AR, a0, a0 + fill);
        x.strokeStyle = offline ? "#555" : rc(val);
        x.lineWidth = LW;
        x.lineCap = "round";
        x.stroke();
      }
    }
  });

  // Inner color ring
  x.beginPath();
  x.arc(H, CY, AR - 22, 0, Math.PI * 2);
  x.strokeStyle = withAlpha(offline ? "#555" : color, alive ? 0.21 : 0.09);
  x.lineWidth = 2;
  x.stroke();

  // Center LED
  const col = offline ? "#555" : hc(stone.health);
  x.shadowColor = col;
  x.shadowBlur = alive ? 25 : 8;
  x.beginPath();
  x.arc(H, CY, alive ? 8 : 5, 0, Math.PI * 2);
  x.fillStyle = col;
  x.fill();
  x.shadowBlur = 0;

  // Name
  const name = displayName(stone);
  x.font = `500 ${alive ? 28 : 24}px "IBM Plex Sans",sans-serif`;
  x.fillStyle = offline ? "#555" : alive ? "#fafaf9" : "#78716c";
  x.textAlign = "center";
  x.textBaseline = "top";
  x.fillText(name, H, CY + AR + 12);

  // Hardware or offline label
  const cores = getCores(stone);
  const memGB = getMemGB(stone);
  if (offline) {
    x.font = '500 20px "IBM Plex Mono",monospace';
    x.fillStyle = "#555";
    x.fillText("OFFLINE", H, CY + AR + 46);
  } else {
    x.font = '300 17px "IBM Plex Mono",monospace';
    x.fillStyle = "#78716c";
    x.fillText(`${cores}c · ${memGB}GB`, H, CY + AR + 46);
  }

  // Service dots (from offerings)
  const offerings = stone.offerings || [];
  const sp = 13,
    sx = H - ((offerings.length - 1) * sp) / 2;
  offerings.forEach((sv, i) => {
    x.beginPath();
    x.arc(sx + i * sp, CY + AR + 72, 4, 0, Math.PI * 2);
    if (!offline && sv.status === "running") {
      x.fillStyle = "#84a59d";
      x.fill();
    } else {
      x.strokeStyle = "#57534e";
      x.lineWidth = 1.5;
      x.stroke();
    }
  });

  // Keystone badge — check tags for "keystone" since API doesn't expose pond role
  const isKeystone =
    stone.tags?.includes("keystone") || stone._pond === "keystone";
  if (!offline && isKeystone) {
    x.font = '400 13px "IBM Plex Mono",monospace';
    x.fillStyle = "#c4b060";
    x.fillText("◆ keystone", H, CY + AR + 93);
  }

  return c;
}

/// Identity helper for banks. The Pavilion bank shape may use
/// either a dedicated `id` field or a `replica_set_name` /
/// `name` fallback depending on which Tauri command produced
/// it; the canvas accepts either form and treats whatever is
/// present as the unique key.
export function bankIdOf(bank) {
  return bank.id || bank.replica_set_id || bank.replica_set_name || bank.name || "";
}

/// Render a bank's canvas-based display sprite. 384×384 transparent
/// canvas with a usage gauge ring, name, capacity, and seed-count
/// chips. ORCH-0039 §"Canvas" calls for banks alongside stones;
/// the visual language differs (smaller, ring-only gauge, mono
/// capacity readout) so the two kinds are distinguishable at a
/// glance even on a busy sphere.
function renderBankCanvas(bank) {
  const S = 384,
    H = S / 2,
    CY = 160,
    AR = 110,
    LW = 6;
  const c = document.createElement("canvas");
  c.width = S;
  c.height = S;
  const x = c.getContext("2d");

  const usedPct = bankUsedPercent(bank);
  const isReplicated = (bank.local_volume_count || 0) > 0
    && (bank.replica_count || bank.replicas?.length || 0) > 1;
  const accent = "#c4b060"; // gold — banks read as treasure

  // Outer usage ring.
  const ringStart = -Math.PI / 2;
  x.beginPath();
  x.arc(H, CY, AR, ringStart, ringStart + Math.PI * 2);
  x.strokeStyle = "rgba(255,255,255,0.07)";
  x.lineWidth = LW;
  x.stroke();
  if (usedPct > 0.5) {
    x.beginPath();
    x.arc(H, CY, AR, ringStart, ringStart + Math.PI * 2 * (usedPct / 100));
    x.strokeStyle = usedPct > 85 ? "#c45050" : usedPct > 70 ? "#d4a373" : accent;
    x.lineWidth = LW;
    x.lineCap = "round";
    x.stroke();
  }

  // Inner accent ring (replicated banks get a brighter halo).
  x.beginPath();
  x.arc(H, CY, AR - 16, 0, Math.PI * 2);
  x.strokeStyle = withAlpha(accent, isReplicated ? 0.45 : 0.22);
  x.lineWidth = isReplicated ? 3 : 2;
  x.stroke();

  // Center diamond (signals "bank" — stones use a circle LED).
  x.shadowColor = accent;
  x.shadowBlur = 14;
  x.fillStyle = accent;
  x.translate(H, CY);
  x.rotate(Math.PI / 4);
  x.fillRect(-7, -7, 14, 14);
  x.rotate(-Math.PI / 4);
  x.translate(-H, -CY);
  x.shadowBlur = 0;

  // Name.
  x.font = '500 24px "IBM Plex Sans",sans-serif';
  x.fillStyle = "#fafaf9";
  x.textAlign = "center";
  x.textBaseline = "top";
  x.fillText(bank.name || bank.replica_set_name || "bank", H, CY + AR + 12);

  // Capacity readout.
  x.font = '300 14px "IBM Plex Mono",monospace';
  x.fillStyle = "#78716c";
  const cap = formatBytesShort(bank.capacity_bytes || 0);
  const used = formatBytesShort(bank.used_bytes || 0);
  x.fillText(`${used} / ${cap}`, H, CY + AR + 42);

  // Seed-count chip when present (filled in by setSeedCount).
  if (bank._seedCount && bank._seedCount > 0) {
    x.font = '500 12px "IBM Plex Mono",monospace';
    x.fillStyle = accent;
    x.fillText(`${bank._seedCount} seed${bank._seedCount === 1 ? "" : "s"}`, H, CY + AR + 68);
  }

  return c;
}

/// Compute used-percent given a bank's capacity / used fields,
/// safely handling missing data.
function bankUsedPercent(bank) {
  const cap = bank.capacity_bytes || 0;
  if (cap <= 0) return 0;
  const used = bank.used_bytes || 0;
  return Math.min(100, (used / cap) * 100);
}

/// Format a byte count for the canvas in short form (e.g. "1.2T").
function formatBytesShort(bytes) {
  if (!bytes) return "—";
  const units = ["B", "K", "M", "G", "T", "P"];
  let i = 0;
  let n = bytes;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i += 1;
  }
  return `${n.toFixed(n < 10 && i > 0 ? 1 : 0)}${units[i]}`;
}

function makeGlowTex(color = "#84a59d") {
  const c = document.createElement("canvas");
  c.width = 128;
  c.height = 128;
  const x = c.getContext("2d");
  const g = x.createRadialGradient(64, 64, 0, 64, 64, 64);
  g.addColorStop(0, withAlpha(color, 0.38));
  g.addColorStop(0.4, withAlpha(color, 0.13));
  g.addColorStop(1, "transparent");
  x.fillStyle = g;
  x.fillRect(0, 0, 128, 128);
  return new THREE.CanvasTexture(c);
}

function makeSparkTex() {
  const c = document.createElement("canvas");
  c.width = 32;
  c.height = 32;
  const x = c.getContext("2d");
  const g = x.createRadialGradient(16, 16, 0, 16, 16, 16);
  g.addColorStop(0, "#ffffff");
  g.addColorStop(0.3, "#84a59dcc");
  g.addColorStop(1, "transparent");
  x.fillStyle = g;
  x.fillRect(0, 0, 32, 32);
  return new THREE.CanvasTexture(c);
}

// ── GardenSphere Class ──────────────────────────────────────────

export class GardenSphere {
  constructor(container, opts = {}) {
    this.container = container;
    this.R = opts.radius || 10;
    this.onHover = opts.onHover || (() => {});
    this.onTrack = opts.onTrack || (() => {});
    this.onTransition = opts.onTransition || (() => {});
    this.onDataChange = opts.onDataChange || (() => {});
    this.nodes = [];
    this.conns = [];
    this.hitTargets = [];
    this.stones = [];

    // Banks (ORCH-0039 Frame 2) — rendered as smaller nodes on
    // an inner sphere at radius `R * BANK_RADIUS_RATIO`. Each
    // bank's hit targets land in `bankHitTargets` so click
    // dispatching can distinguish a bank pick from a stone pick
    // without traversing the whole hit list.
    this.banks = [];
    this.bankNodes = [];
    this.bankHitTargets = [];
    this.bankRadius = (opts.radius || 10) * 0.55;
    this.hoveredKind = null; // 'stone' | 'bank' | null

    this.hoveredId = null;
    this.selectedId = null;
    this.departingId = null;
    this.isDrag = false;
    this.prevM = { x: 0, y: 0 };
    this.vel = { x: 0, y: 0 };
    this.lastInput = 0;
    this.t0 = performance.now();
    this.mouseInCanvas = false;
    this.autoRotMul = 1.0;
    this.rotTarget = null;
    this.rotFrom = null;
    this.rotProgress = 1;
    this.rotDuration = 0.9;
    this.layoutProgress = 1;

    const w = container.clientWidth,
      h = container.clientHeight;
    this.scene = new THREE.Scene();
    this.camera = new THREE.PerspectiveCamera(48, w / h, 0.1, 200);
    this.camera.position.set(0, 2, 28);
    this.camera.lookAt(0, 0, 0);
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(w, h);
    this.renderer.setClearColor(0x111110, 1);
    container.appendChild(this.renderer.domElement);
    this.scene.add(new THREE.AmbientLight(0x606060, 0.6));
    this.pLight = new THREE.PointLight(0xffffff, 0.7, 60);
    this.pLight.position.copy(this.camera.position);
    this.scene.add(this.pLight);
    this.sg = new THREE.Group();
    this.scene.add(this.sg);

    // Equator + meridian rings
    this.ringMat = new THREE.MeshBasicMaterial({
      color: 0x84a59d,
      transparent: true,
      opacity: 0.1,
    });
    const ring = new THREE.Mesh(
      new THREE.TorusGeometry(this.R, 0.02, 6, 160),
      this.ringMat,
    );
    ring.rotation.x = -Math.PI / 2;
    this.sg.add(ring);
    const mer = new THREE.Mesh(
      new THREE.TorusGeometry(this.R, 0.012, 6, 160),
      new THREE.MeshBasicMaterial({
        color: 0x84a59d,
        transparent: true,
        opacity: 0.04,
      }),
    );
    mer.rotation.y = Math.PI / 3;
    this.sg.add(mer);

    // Ambient stars
    const starN = 250,
      starPos = new Float32Array(starN * 3);
    for (let i = 0; i < starN; i++) {
      const r = 25 + Math.random() * 30,
        t = Math.random() * Math.PI * 2,
        p = Math.acos(2 * Math.random() - 1);
      starPos[i * 3] = r * Math.sin(p) * Math.cos(t);
      starPos[i * 3 + 1] = r * Math.sin(p) * Math.sin(t);
      starPos[i * 3 + 2] = r * Math.cos(p);
    }
    const starGeo = new THREE.BufferGeometry();
    starGeo.setAttribute(
      "position",
      new THREE.BufferAttribute(starPos, 3),
    );
    this.scene.add(
      new THREE.Points(
        starGeo,
        new THREE.PointsMaterial({
          color: 0x84a59d,
          size: 0.06,
          transparent: true,
          opacity: 0.15,
          sizeAttenuation: true,
        }),
      ),
    );

    this.ray = new THREE.Raycaster();
    this.mouse = new THREE.Vector2();
    this.sparkTex = makeSparkTex();
    this._bindEvents(this.renderer.domElement);
    this._startAnim();
  }

  // ── Public API ────────────────────────────────────────────────

  /** Load initial stone data. Clears everything and rebuilds. */
  setData(stones) {
    this._clearAll();
    this.stones = [...stones];
    const positions = fibSphere(this.stones.length);
    this.stones.forEach((st, idx) => {
      const pos = new THREE.Vector3(...positions[idx]).multiplyScalar(this.R);
      this.nodes.push(this._mkNode(st, pos));
    });
    this._rebuildEdges();
    this.onDataChange(this.stones);
  }

  /** Update a stone's data in-place. Pass any subset of stone fields.
   *  Does NOT trigger re-layout — use for live metrics, health changes, service updates. */
  updateStone(id, patch) {
    const node = this.nodes.find((n) => n.stone.stone_id === id);
    if (!node) return;
    const si = this.stones.findIndex((s) => s.stone_id === id);
    if (si >= 0) this.stones[si] = { ...this.stones[si], ...patch };
    Object.assign(node.stone, patch);
    this._refreshTex(node);
    node.bodyMat.emissive = new THREE.Color(hc(node.stone.health));
    if (patch.offerings) this._rebuildEdges();
    this.onDataChange(this.stones);
  }

  /** Add a new stone. Triggers animated Fibonacci re-layout. */
  addStone(stone) {
    this.stones.push(stone);
    const positions = fibSphere(this.stones.length);
    this.nodes.forEach((n, idx) => {
      const [px, py, pz] = positions[idx];
      n.targetPos = new THREE.Vector3(px, py, pz).multiplyScalar(this.R);
    });
    const pos = new THREE.Vector3(
      ...positions[this.stones.length - 1],
    ).multiplyScalar(this.R);
    const newNode = this._mkNode(stone, pos);
    newNode.enterScale = 0;
    this.nodes.push(newNode);
    this.layoutProgress = 0;
    this._rebuildEdges();
    this.onDataChange(this.stones);
  }

  /** Remove a stone with fade-out animation, then re-layout remaining. */
  removeStone(id) {
    const ni = this.nodes.findIndex((n) => n.stone.stone_id === id);
    if (ni < 0) return;
    const node = this.nodes[ni];
    node.removing = true;
    node.removeProgress = 0;
    node.removeCallback = () => {
      this.sg.remove(node.group);
      this.hitTargets = this.hitTargets.filter(
        (h) => h.userData.stoneId !== id,
      );
      this.nodes.splice(this.nodes.indexOf(node), 1);
      this.stones = this.stones.filter((s) => s.stone_id !== id);
      if (this.selectedId === id) {
        this.selectedId = null;
        this.onTransition({ selectedId: null, departingId: id });
      }
      if (this.hoveredId === id) this.hoveredId = null;
      const positions = fibSphere(this.stones.length);
      this.nodes.forEach((n, idx) => {
        n.targetPos = new THREE.Vector3(...positions[idx]).multiplyScalar(
          this.R,
        );
      });
      this.layoutProgress = 0;
      this._rebuildEdges();
      this.onDataChange(this.stones);
    };
  }

  /** Mark stone as offline. Stays in sphere position, goes gray, edges disconnect. */
  offlineStone(id) {
    const node = this.nodes.find((n) => n.stone.stone_id === id);
    if (!node) return;
    node.offline = true;
    this._refreshTex(node);
    node.bodyMat.color = new THREE.Color("#444");
    node.bodyMat.emissive = new THREE.Color("#333");
    this._rebuildEdges();
    this.onDataChange(this.stones);
  }

  /** Restore stone from offline. Optionally merge new data. */
  onlineStone(id, patch) {
    const node = this.nodes.find((n) => n.stone.stone_id === id);
    if (!node) return;
    if (patch) Object.assign(node.stone, patch);
    node.offline = false;
    this._refreshTex(node);
    const color = node.stone.color || FALLBACK_COLOR;
    node.bodyMat.color = new THREE.Color(color);
    node.bodyMat.emissive = new THREE.Color(hc(node.stone.health));
    this._rebuildEdges();
    this.onDataChange(this.stones);
  }

  // ── Bank API (ORCH-0039 Frame 2) ──────────────────────────────

  /** Replace the bank pool with a fresh list. */
  setBanks(banks) {
    this._clearBanks();
    this.banks = [...banks];
    const positions = fibSphere(this.banks.length);
    this.banks.forEach((b, idx) => {
      const pos = new THREE.Vector3(...positions[idx]).multiplyScalar(this.bankRadius);
      this.bankNodes.push(this._mkBankNode(b, pos));
    });
    this.onDataChange(this.stones);
  }

  /** Add a single bank. Triggers animated re-layout of bank ring. */
  addBank(bank) {
    if (this.banks.find((b) => bankIdOf(b) === bankIdOf(bank))) return;
    this.banks.push(bank);
    const positions = fibSphere(this.banks.length);
    this.bankNodes.forEach((n, idx) => {
      const [px, py, pz] = positions[idx];
      n.targetPos = new THREE.Vector3(px, py, pz).multiplyScalar(this.bankRadius);
    });
    const last = positions[this.banks.length - 1];
    const pos = new THREE.Vector3(...last).multiplyScalar(this.bankRadius);
    const newNode = this._mkBankNode(bank, pos);
    newNode.enterScale = 0;
    this.bankNodes.push(newNode);
    this.layoutProgress = 0;
  }

  /** Remove a bank by id with fade-out then re-layout. */
  removeBank(id) {
    const ni = this.bankNodes.findIndex((n) => bankIdOf(n.bank) === id);
    if (ni < 0) return;
    const node = this.bankNodes[ni];
    node.removing = true;
    node.removeProgress = 0;
    node.removeCallback = () => {
      this.sg.remove(node.group);
      this.bankHitTargets = this.bankHitTargets.filter(
        (h) => h.userData.bankId !== id,
      );
      this.bankNodes.splice(this.bankNodes.indexOf(node), 1);
      this.banks = this.banks.filter((b) => bankIdOf(b) !== id);
      const positions = fibSphere(this.banks.length);
      this.bankNodes.forEach((n, idx) => {
        n.targetPos = new THREE.Vector3(...positions[idx]).multiplyScalar(
          this.bankRadius,
        );
      });
      this.layoutProgress = 0;
    };
  }

  /** Live-update a bank's data (capacity, used bytes, seed count). */
  updateBank(id, patch) {
    const node = this.bankNodes.find((n) => bankIdOf(n.bank) === id);
    if (!node) return;
    Object.assign(node.bank, patch);
    const bi = this.banks.findIndex((b) => bankIdOf(b) === id);
    if (bi >= 0) this.banks[bi] = { ...this.banks[bi], ...patch };
    this._refreshBankTex(node);
  }

  /** Update a bank's seed count chip without disturbing other
   *  fields. Used by the canvas when seed catalogs change. */
  setSeedCount(id, count) {
    this.updateBank(id, { _seedCount: count });
  }

  /** Reset sphere rotation and camera to default position. */
  resetView() {
    this.sg.quaternion.identity();
    this.camera.position.set(0, 2, 28);
    this.camera.lookAt(0, 0, 0);
  }

  /** Full cleanup — remove renderer, dispose GPU resources, unbind events. */
  destroy() {
    cancelAnimationFrame(this._animId);
    const el = this.renderer.domElement;
    el.removeEventListener("contextmenu", this._bCtx);
    el.removeEventListener("pointerdown", this._bPD);
    el.removeEventListener("pointerenter", this._bEnter);
    el.removeEventListener("pointerleave", this._bLeave);
    window.removeEventListener("pointermove", this._bPM);
    window.removeEventListener("pointerup", this._bPU);
    el.removeEventListener("wheel", this._bWh);
    window.removeEventListener("resize", this._bRz);
    this.scene.traverse((o) => {
      if (o.geometry) o.geometry.dispose();
      if (o.material) {
        if (o.material.map) o.material.map.dispose();
        o.material.dispose();
      }
    });
    this.renderer.dispose();
    this.container.removeChild(el);
  }

  // ── Internal ──────────────────────────────────────────────────

  _bindEvents(el) {
    this._bPD = this._onPD.bind(this);
    this._bPM = this._onPM.bind(this);
    this._bPU = this._onPU.bind(this);
    this._bWh = this._onWh.bind(this);
    this._bCtx = (e) => e.preventDefault();
    this._bRz = this._resize.bind(this);
    this._bEnter = () => {
      this.mouseInCanvas = true;
    };
    this._bLeave = () => {
      this.mouseInCanvas = false;
    };
    el.addEventListener("contextmenu", this._bCtx);
    el.addEventListener("pointerdown", this._bPD);
    el.addEventListener("pointerenter", this._bEnter);
    el.addEventListener("pointerleave", this._bLeave);
    window.addEventListener("pointermove", this._bPM);
    window.addEventListener("pointerup", this._bPU);
    el.addEventListener("wheel", this._bWh, { passive: false });
    window.addEventListener("resize", this._bRz);
  }

  _refreshTex(node) {
    const c = renderStoneCanvas(node.stone, node.offline);
    node.disp.material.map.dispose();
    node.disp.material.map = new THREE.CanvasTexture(c);
    node.disp.material.map.minFilter = THREE.LinearFilter;
    node.disp.material.needsUpdate = true;
  }

  _clearAll() {
    this.nodes.forEach((n) => this.sg.remove(n.group));
    this._clearEdges();
    this.nodes = [];
    this.hitTargets = [];
  }

  _clearEdges() {
    this.conns.forEach((c) => {
      this.sg.remove(c.tube);
      c.tube.geometry.dispose();
      c.tubeMat.dispose();
      c.sparkles.forEach((s) => {
        this.sg.remove(s);
        s.material.dispose();
      });
      if (c.label) {
        this.sg.remove(c.label);
        c.labelMat.map.dispose();
        c.labelMat.dispose();
      }
    });
    this.conns = [];
  }

  _rebuildEdges() {
    this._clearEdges();
    const activeNodes = this.nodes.filter((n) => !n.removing && !n.offline);
    const activeStones = activeNodes.map((n) => n.stone);
    computeEdges(activeStones).forEach((edge) => {
      const n1 = activeNodes[edge.from],
        n2 = activeNodes[edge.to];
      // Direction picks which endpoint is the "primary" — the
      // sparkles flow from primary toward replica. Direction 0 =
      // peer (sparkles symmetric).
      const fromPos = edge.direction === 2 ? n2.group.position : n1.group.position;
      const toPos = edge.direction === 2 ? n1.group.position : n2.group.position;
      this.conns.push(
        this._mkConn(fromPos, toPos, edge.sets, edge.direction),
      );
    });
  }

  _computeRotTarget(node) {
    const wp = new THREE.Vector3();
    node.group.getWorldPosition(wp);
    const q = new THREE.Quaternion().setFromUnitVectors(
      wp.clone().normalize(),
      this.camera.position.clone().normalize(),
    );
    return q.multiply(this.sg.quaternion.clone());
  }

  _toScreen(wp) {
    const v = wp.clone().project(this.camera),
      rect = this.renderer.domElement.getBoundingClientRect();
    // Container-relative coordinates (not page-relative)
    // Cards are positioned inside .ov-wrap which is position:relative
    return {
      x: (v.x * 0.5 + 0.5) * rect.width,
      y: (-v.y * 0.5 + 0.5) * rect.height,
    };
  }

  _screenOf(id) {
    // Stones first, then banks — same dispatch as raycasting.
    const stoneNode = this.nodes.find((n) => n.stone.stone_id === id);
    if (stoneNode) {
      const wp = new THREE.Vector3();
      stoneNode.disp.getWorldPosition(wp);
      const screen = this._toScreen(wp);
      const dist = this.camera.position.distanceTo(wp);
      const vFov = this.camera.fov * (Math.PI / 180);
      const rect = this.renderer.domElement.getBoundingClientRect();
      const spriteScreenH =
        (stoneNode.disp.scale.y / (2 * dist * Math.tan(vFov / 2))) * rect.height;
      // Stone sprite: arc center at CY=195 in a 512px canvas
      // → 11.9% above center.
      screen.y -= spriteScreenH * ((256 - 195) / 512);
      return screen;
    }
    const bankNode = this.bankNodes.find((n) => bankIdOf(n.bank) === id);
    if (bankNode) {
      const wp = new THREE.Vector3();
      bankNode.disp.getWorldPosition(wp);
      const screen = this._toScreen(wp);
      const dist = this.camera.position.distanceTo(wp);
      const vFov = this.camera.fov * (Math.PI / 180);
      const rect = this.renderer.domElement.getBoundingClientRect();
      const spriteScreenH =
        (bankNode.disp.scale.y / (2 * dist * Math.tan(vFov / 2))) * rect.height;
      // Bank sprite: ring center at CY=160 in a 384px canvas
      // → 8.3% above center.
      screen.y -= spriteScreenH * ((192 - 160) / 384);
      return screen;
    }
    return null;
  }

  _mkNode(stone, pos) {
    const color = stone.color || FALLBACK_COLOR;
    const group = new THREE.Group();
    group.position.copy(pos);
    this.sg.add(group);
    const bodyMat = new THREE.MeshStandardMaterial({
      color: new THREE.Color(color),
      emissive: new THREE.Color(hc(stone.health)),
      emissiveIntensity: 0.4,
      roughness: 0.7,
      metalness: 0.2,
      transparent: true,
      opacity: 1,
    });
    group.add(new THREE.Mesh(new THREE.SphereGeometry(0.45, 20, 20), bodyMat));
    const glowMat = new THREE.SpriteMaterial({
      map: makeGlowTex(color),
      transparent: true,
      opacity: 0.35,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const glow = new THREE.Sprite(glowMat);
    glow.scale.set(3.5, 3.5, 1);
    group.add(glow);
    const tex = new THREE.CanvasTexture(renderStoneCanvas(stone));
    tex.minFilter = THREE.LinearFilter;
    const dispMat = new THREE.SpriteMaterial({
      map: tex,
      transparent: true,
      depthWrite: false,
    });
    const disp = new THREE.Sprite(dispMat);
    disp.position.copy(pos.clone().normalize().multiplyScalar(0.6));
    disp.scale.set(4.2, 4.2, 1);
    group.add(disp);
    const hit = new THREE.Mesh(
      new THREE.SphereGeometry(2.2, 8, 8),
      new THREE.MeshBasicMaterial({ visible: false }),
    );
    hit.userData.stoneId = stone.stone_id;
    group.add(hit);
    this.hitTargets.push(hit);
    return {
      group,
      body: group.children[0],
      bodyMat,
      glow,
      glowMat,
      disp,
      dispMat,
      pos,
      stone,
      baseScale: 4.2,
      targetPos: null,
      enterScale: 1,
      offline: false,
      removing: false,
      removeProgress: 0,
    };
  }

  _mkBankNode(bank, pos) {
    const accent = "#c4b060"; // gold for banks
    const group = new THREE.Group();
    group.position.copy(pos);
    this.sg.add(group);
    const bodyMat = new THREE.MeshStandardMaterial({
      color: new THREE.Color(accent),
      emissive: new THREE.Color(accent),
      emissiveIntensity: 0.35,
      roughness: 0.6,
      metalness: 0.3,
      transparent: true,
      opacity: 1,
    });
    // Banks are visually smaller than stones — half the body radius.
    group.add(new THREE.Mesh(new THREE.SphereGeometry(0.28, 18, 18), bodyMat));
    const glowMat = new THREE.SpriteMaterial({
      map: makeGlowTex(accent),
      transparent: true,
      opacity: 0.3,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const glow = new THREE.Sprite(glowMat);
    glow.scale.set(2.6, 2.6, 1);
    group.add(glow);
    const tex = new THREE.CanvasTexture(renderBankCanvas(bank));
    tex.minFilter = THREE.LinearFilter;
    const dispMat = new THREE.SpriteMaterial({
      map: tex,
      transparent: true,
      depthWrite: false,
    });
    const disp = new THREE.Sprite(dispMat);
    disp.position.copy(pos.clone().normalize().multiplyScalar(0.5));
    disp.scale.set(2.8, 2.8, 1);
    group.add(disp);
    const id = bankIdOf(bank);
    const hit = new THREE.Mesh(
      new THREE.SphereGeometry(1.4, 8, 8),
      new THREE.MeshBasicMaterial({ visible: false }),
    );
    hit.userData.bankId = id;
    group.add(hit);
    this.bankHitTargets.push(hit);
    return {
      kind: "bank",
      group,
      body: group.children[0],
      bodyMat,
      glow,
      glowMat,
      disp,
      dispMat,
      pos,
      bank,
      baseScale: 2.8,
      targetPos: null,
      enterScale: 1,
      removing: false,
      removeProgress: 0,
    };
  }

  _refreshBankTex(node) {
    const c = renderBankCanvas(node.bank);
    node.disp.material.map.dispose();
    node.disp.material.map = new THREE.CanvasTexture(c);
    node.disp.material.map.minFilter = THREE.LinearFilter;
    node.disp.material.needsUpdate = true;
  }

  _clearBanks() {
    this.bankNodes.forEach((n) => this.sg.remove(n.group));
    this.bankNodes = [];
    this.bankHitTargets = [];
    this.banks = [];
  }

  /**
   * Build an edge between two stones for shared offering(s).
   *
   * `direction` (from `computeEdges`):
   *   - 0: peer↔peer — solid sage tube, sparkles drift symmetrically
   *   - 1 or 2: primary↔replica — gold tube, sparkles flow from
   *     primary (p1) to replica (p2) and the line is rendered
   *     thinner with a gold accent.
   *
   * The geometric direction is encoded in the curve order: when
   * direction != 0 the caller has already swapped p1/p2 such that
   * p1 is always the primary. So sparkle motion is always p1 → p2.
   */
  _mkConn(p1, p2, sets, direction = 0) {
    const directed = direction !== 0;
    const pts = greatCircle(p1, p2, this.R, 48),
      curve = new THREE.CatmullRomCurve3(pts);

    // Peer edges: sage. Directed (primary→replica): gold accent.
    const tubeColor = directed ? 0xc4b060 : 0x84a59d;
    const labelColor = directed ? "#c4b060" : "#84a59d";

    const tubeMat = new THREE.MeshBasicMaterial({
      color: tubeColor,
      transparent: true,
      opacity: directed ? 0.28 : 0.18,
      depthWrite: false,
    });
    const tube = new THREE.Mesh(
      new THREE.TubeGeometry(
        curve,
        48,
        0.025 + sets.length * 0.008,
        6,
        false,
      ),
      tubeMat,
    );
    this.sg.add(tube);
    const lc = document.createElement("canvas");
    lc.width = 256;
    lc.height = 48;
    const lx = lc.getContext("2d");
    lx.font = '400 16px "IBM Plex Mono",monospace';
    lx.fillStyle = labelColor;
    lx.textAlign = "center";
    lx.textBaseline = "middle";
    // Directed edges add a "▶" cue so a still-frame screenshot still
    // communicates the relationship.
    const labelText = directed ? `${sets.join(" · ")} ▶` : sets.join(" · ");
    lx.fillText(labelText, 128, 24);
    const labelMat = new THREE.SpriteMaterial({
      map: new THREE.CanvasTexture(lc),
      transparent: true,
      opacity: 0.6,
      depthWrite: false,
    });
    const label = new THREE.Sprite(labelMat);
    label.position.copy(
      curve.getPoint(0.5).normalize().multiplyScalar(this.R * 1.06),
    );
    label.scale.set(3.5, 0.7, 1);
    this.sg.add(label);
    const sparkles = [];
    // Directed edges: more sparkles, all moving in the +t direction
    // (the animation loop already advances t each frame). Peer
    // edges: existing 3-sparkle symmetric pattern.
    const sparkCount = directed
      ? Math.min(sets.length + 2, 4)
      : Math.min(sets.length + 1, 3);
    for (let i = 0; i < sparkCount; i++) {
      const sMat = new THREE.SpriteMaterial({
        map: this.sparkTex,
        transparent: true,
        opacity: directed ? 0.85 : 0.7,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      });
      const s = new THREE.Sprite(sMat);
      s.scale.set(directed ? 0.4 : 0.35, directed ? 0.4 : 0.35, 1);
      s.userData.t = i / sparkCount;
      // Directed edges sparkle slightly faster — visual cue for "this
      // is an active replication relationship, not just a cohabitation".
      s.userData.spd =
        (directed ? 0.11 : 0.08) + Math.random() * 0.06;
      s.position.copy(curve.getPoint(s.userData.t));
      this.sg.add(s);
      sparkles.push(s);
    }
    return { tube, tubeMat, curve, sparkles, label, labelMat, sets, directed };
  }

  _animate() {
    const t = (performance.now() - this.t0) * 0.001,
      dt = 1 / 60,
      camZ = this.camera.position.z;
    const targetMul = this.mouseInCanvas ? 0 : 1;
    this.autoRotMul += (targetMul - this.autoRotMul) * 0.03;
    const isSlerping = this.rotTarget && this.rotProgress < 1;

    if (isSlerping) {
      this.rotProgress = Math.min(this.rotProgress + dt / this.rotDuration, 1);
      const ease = 1 - Math.pow(1 - this.rotProgress, 3);
      this.sg.quaternion.copy(
        this.rotFrom.clone().slerp(this.rotTarget, ease),
      );
      if (this.rotProgress >= 1) this.departingId = null;
    } else if (!this.isDrag) {
      if (
        Math.abs(this.vel.x) > 0.00005 ||
        Math.abs(this.vel.y) > 0.00005
      ) {
        this.sg.quaternion.premultiply(
          new THREE.Quaternion().setFromAxisAngle(
            new THREE.Vector3(0, 1, 0),
            this.vel.x,
          ),
        );
        this.sg.quaternion.premultiply(
          new THREE.Quaternion().setFromAxisAngle(
            new THREE.Vector3(1, 0, 0),
            this.vel.y,
          ),
        );
        this.vel.x *= 0.96;
        this.vel.y *= 0.96;
      }
      if (performance.now() - this.lastInput > 3500) {
        const spd = 0.0008 * this.autoRotMul;
        if (spd > 0.000001)
          this.sg.quaternion.premultiply(
            new THREE.Quaternion().setFromAxisAngle(
              new THREE.Vector3(0, 1, 0),
              spd,
            ),
          );
      }
    }

    // Layout migration (add/remove triggers)
    if (this.layoutProgress < 1) {
      this.layoutProgress = Math.min(this.layoutProgress + dt / 0.8, 1);
      const ease = 1 - Math.pow(1 - this.layoutProgress, 3);
      let needEdgeRebuild = false;
      this.nodes.forEach((n) => {
        if (n.targetPos && !n.removing) {
          n.group.position.lerp(n.targetPos, ease);
          n.pos = n.group.position.clone();
          n.disp.position.copy(
            n.pos.clone().normalize().multiplyScalar(0.6),
          );
          if (this.layoutProgress >= 1) {
            n.targetPos = null;
            needEdgeRebuild = true;
          }
        }
        if (n.enterScale < 1) {
          n.enterScale = Math.min(n.enterScale + dt / 0.6, 1);
          n.group.scale.setScalar(1 - Math.pow(1 - n.enterScale, 2));
        }
      });
      // Bank nodes share the same layout-migration tick.
      this.bankNodes.forEach((n) => {
        if (n.targetPos && !n.removing) {
          n.group.position.lerp(n.targetPos, ease);
          n.pos = n.group.position.clone();
          n.disp.position.copy(n.pos.clone().normalize().multiplyScalar(0.5));
          if (this.layoutProgress >= 1) {
            n.targetPos = null;
          }
        }
        if (n.enterScale < 1) {
          n.enterScale = Math.min(n.enterScale + dt / 0.6, 1);
          n.group.scale.setScalar(1 - Math.pow(1 - n.enterScale, 2));
        }
      });
      if (needEdgeRebuild) this._rebuildEdges();
    }

    // Remove animation (stones)
    this.nodes.forEach((n) => {
      if (n.removing) {
        n.removeProgress = Math.min(n.removeProgress + dt / 0.5, 1);
        const a = 1 - n.removeProgress;
        n.group.scale.setScalar(a);
        n.bodyMat.opacity = a;
        n.dispMat.opacity = a;
        n.glowMat.opacity = a * 0.35;
        if (n.removeProgress >= 1 && n.removeCallback) {
          n.removeCallback();
          n.removeCallback = null;
        }
      }
    });

    // Remove animation (banks) — same fade pattern.
    this.bankNodes.forEach((n) => {
      if (n.removing) {
        n.removeProgress = Math.min(n.removeProgress + dt / 0.5, 1);
        const a = 1 - n.removeProgress;
        n.group.scale.setScalar(a);
        n.bodyMat.opacity = a;
        n.dispMat.opacity = a;
        n.glowMat.opacity = a * 0.3;
        if (n.removeProgress >= 1 && n.removeCallback) {
          n.removeCallback();
          n.removeCallback = null;
        }
      }
    });

    // Depth-based opacity/scale
    this.ringMat.opacity = 0.09 + 0.025 * Math.sin(t * 0.7);
    const wp = new THREE.Vector3();
    this.nodes.forEach((n) => {
      if (n.removing) return;
      n.group.getWorldPosition(wp);
      const dist = this.camera.position.distanceTo(wp);
      const near = camZ - this.R,
        far = camZ + this.R;
      const depth = THREE.MathUtils.clamp(
        (dist - near) / (far - near),
        0,
        1,
      );
      const opacity = THREE.MathUtils.lerp(1.0, 0.08, depth);
      const scale = THREE.MathUtils.lerp(1.0, 0.55, depth);
      n.dispMat.opacity = opacity;
      n.disp.scale.setScalar(n.baseScale * scale);
      n.glowMat.opacity = opacity * 0.35;
      n.bodyMat.opacity = opacity;
      const alive = isAlive(n.stone.health) && !n.offline;
      const rate =
        n.stone.health === "thriving" || n.stone.health === "healthy"
          ? 0.5
          : n.stone.health === "withering" || n.stone.health === "degraded"
            ? 1.3
            : 0;
      const breath = alive
        ? 0.25 + 0.25 * Math.sin(t * rate * Math.PI * 2)
        : 0.08;
      n.bodyMat.emissiveIntensity = n.offline
        ? 0.05
        : breath * (1 - depth * 0.5);
      if (
        n.stone.stone_id === this.hoveredId ||
        n.stone.stone_id === this.selectedId
      ) {
        n.glowMat.opacity = Math.min(opacity * 0.7, 0.7);
        n.disp.scale.setScalar(n.baseScale * scale * 1.08);
      }
    });

    // Bank nodes — depth-based opacity / scale + steady gold breath.
    // Banks read as treasure, not living organisms, so the
    // breath rate is slower and never goes dim.
    this.bankNodes.forEach((n) => {
      if (n.removing) return;
      n.group.getWorldPosition(wp);
      const dist = this.camera.position.distanceTo(wp);
      const near = camZ - this.bankRadius,
        far = camZ + this.bankRadius;
      const depth = THREE.MathUtils.clamp(
        (dist - near) / (far - near),
        0,
        1,
      );
      const opacity = THREE.MathUtils.lerp(1.0, 0.18, depth);
      const scale = THREE.MathUtils.lerp(1.0, 0.65, depth);
      n.dispMat.opacity = opacity;
      n.disp.scale.setScalar(n.baseScale * scale);
      n.glowMat.opacity = opacity * 0.3;
      n.bodyMat.opacity = opacity;
      n.bodyMat.emissiveIntensity =
        0.25 + 0.15 * Math.sin(t * 0.4 * Math.PI * 2);
      const id = bankIdOf(n.bank);
      if (id === this.hoveredId || id === this.selectedId) {
        n.glowMat.opacity = Math.min(opacity * 0.7, 0.7);
        n.disp.scale.setScalar(n.baseScale * scale * 1.1);
      }
    });

    // Emit tracking data for card positioning
    this.onTrack({
      selected: this.selectedId
        ? { id: this.selectedId, pos: this._screenOf(this.selectedId) }
        : null,
      departing: this.departingId
        ? { id: this.departingId, pos: this._screenOf(this.departingId) }
        : null,
      hovered: this.hoveredId
        ? { id: this.hoveredId, pos: this._screenOf(this.hoveredId) }
        : null,
      progress: isSlerping ? this.rotProgress : 1,
    });

    // Animate sparkles and connection labels
    this.conns.forEach((c) => {
      c.sparkles.forEach((s) => {
        s.userData.t = (s.userData.t + s.userData.spd * 0.016) % 1;
        s.position.copy(c.curve.getPoint(s.userData.t));
        s.material.opacity =
          0.4 + 0.3 * Math.sin(t * 2.5 + s.userData.t * 8);
      });
      if (c.label) {
        const lp = new THREE.Vector3();
        c.label.getWorldPosition(lp);
        const ld = this.camera.position.distanceTo(lp);
        const dd = THREE.MathUtils.clamp(
          (ld - (camZ - this.R)) / (camZ + this.R - (camZ - this.R)),
          0,
          1,
        );
        c.labelMat.opacity = THREE.MathUtils.lerp(0.55, 0.05, dd);
      }
    });

    this.pLight.position.copy(this.camera.position);
    this.renderer.render(this.scene, this.camera);
  }

  _rayTest(e) {
    const rect = this.renderer.domElement.getBoundingClientRect();
    this.mouse.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
    this.mouse.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
    this.ray.setFromCamera(this.mouse, this.camera);
    // Stones pick first (outer sphere); banks fall through (inner)
    // when no stone is hit. The two pools are kept separate so a
    // ray that grazes both prefers the stone, matching the spatial
    // hierarchy.
    const stoneHits = this.ray.intersectObjects(this.hitTargets);
    if (stoneHits.length > 0) {
      return { kind: "stone", id: stoneHits[0].object.userData.stoneId };
    }
    const bankHits = this.ray.intersectObjects(this.bankHitTargets);
    if (bankHits.length > 0) {
      return { kind: "bank", id: bankHits[0].object.userData.bankId };
    }
    return null;
  }

  _onPD(e) {
    if (e.button === 2 || e.button === 1) {
      this.isDrag = true;
      this.prevM = { x: e.clientX, y: e.clientY };
      this.vel = { x: 0, y: 0 };
      this.rotProgress = 1; // cancel slerp on drag
    } else if (e.button === 0) {
      const hit = this._rayTest(e);
      const hitId = hit ? hit.id : null;
      const hitKind = hit ? hit.kind : null;
      const newId = hitId === this.selectedId ? null : hitId;
      const newKind = newId ? hitKind : null;
      const prevId = this.selectedId;
      this.selectedId = newId;
      this.selectedKind = newKind;
      if (newId) {
        this.departingId = prevId;
        const node =
          newKind === "bank"
            ? this.bankNodes.find((n) => bankIdOf(n.bank) === newId)
            : this.nodes.find((n) => n.stone.stone_id === newId);
        if (node) {
          this.rotFrom = this.sg.quaternion.clone();
          this.rotTarget = this._computeRotTarget(node);
          this.rotProgress = 0;
          this.vel = { x: 0, y: 0 };
        }
        this.onTransition({
          selectedId: newId,
          departingId: prevId,
          kind: newKind,
        });
      } else {
        this.departingId = prevId;
        this.onTransition({
          selectedId: null,
          departingId: prevId,
          kind: null,
        });
      }
    }
    this.lastInput = performance.now();
  }

  _onPM(e) {
    if (!this.isDrag) {
      const hit = this._rayTest(e);
      const hitId = hit ? hit.id : null;
      const hitKind = hit ? hit.kind : null;
      if (hitId !== this.hoveredId) {
        this.hoveredId = hitId;
        this.hoveredKind = hitKind;
        this.onHover(hitId, hitKind);
      }
    }
    if (this.isDrag) {
      const dx = e.clientX - this.prevM.x,
        dy = e.clientY - this.prevM.y;
      this.prevM = { x: e.clientX, y: e.clientY };
      const spd = 0.004;
      this.sg.quaternion.premultiply(
        new THREE.Quaternion().setFromAxisAngle(
          new THREE.Vector3(0, 1, 0),
          dx * spd,
        ),
      );
      this.sg.quaternion.premultiply(
        new THREE.Quaternion().setFromAxisAngle(
          new THREE.Vector3(1, 0, 0),
          dy * spd,
        ),
      );
      this.vel = { x: dx * spd, y: dy * spd };
      this.lastInput = performance.now();
    }
  }

  _onPU(e) {
    if (e.button === 2 || e.button === 1) this.isDrag = false;
  }

  _onWh(e) {
    e.preventDefault();
    this.camera.position.z = THREE.MathUtils.clamp(
      this.camera.position.z + e.deltaY * 0.025,
      16,
      48,
    );
    this.camera.lookAt(0, 0, 0);
    this.lastInput = performance.now();
  }

  _resize() {
    const w = this.container.clientWidth,
      h = this.container.clientHeight;
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(w, h);
  }

  _startAnim() {
    const loop = () => {
      this._animId = requestAnimationFrame(loop);
      this._animate();
    };
    loop();
  }
}

export default GardenSphere;
