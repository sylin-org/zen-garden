/**
 * GardenSphere — 3D rotatable sphere visualization for Zen Garden infrastructure.
 *
 * Framework-agnostic Three.js library. Renders stones as nodes on a sphere,
 * connected by great-circle arcs when they share service offerings.
 *
 * Dependency: Three.js r128+ (import * as THREE from "three")
 *
 * Usage:
 *   const gs = new GardenSphere(containerElement, options);
 *   gs.setData(stones);          // initial load
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

/** Service identity key: "offering" or "offering:instance" */
export const serviceKey = (svc) => svc.i ? `${svc.o}:${svc.i}` : svc.o;

/** Resource gauge color: sage < 70, clay 70-85, red > 85 */
const rc = (v) => v > 85 ? "#c45050" : v > 70 ? "#d4a373" : "#84a59d";

/** Health state color */
const hc = (h) => h === "thriving" ? "#84a59d" : h === "withering" ? "#d4a373" : "#78716c";

/** Fibonacci sphere distribution — even spacing on unit sphere */
function fibSphere(n) {
  if (n === 1) return [[0, 0, 1]];
  const pts = [], phi = Math.PI * (3 - Math.sqrt(5));
  for (let i = 0; i < n; i++) {
    const y = 1 - (i / (n - 1)) * 2, r = Math.sqrt(1 - y * y), t = phi * i;
    pts.push([Math.cos(t) * r, y, Math.sin(t) * r]);
  }
  return pts;
}

/** Spherical linear interpolation path between two points on sphere surface */
function greatCircle(p1, p2, R, segs = 48) {
  const v1 = p1.clone().normalize(), v2 = p2.clone().normalize();
  const om = Math.acos(THREE.MathUtils.clamp(v1.dot(v2), -1, 1));
  const sinOm = Math.sin(om), pts = [];
  for (let i = 0; i <= segs; i++) {
    const t = i / segs;
    let p;
    if (sinOm < 0.001) { p = v1.clone().lerp(v2, t).normalize(); }
    else {
      const a = Math.sin((1 - t) * om) / sinOm, b = Math.sin(t * om) / sinOm;
      p = new THREE.Vector3(v1.x*a + v2.x*b, v1.y*a + v2.y*b, v1.z*a + v2.z*b).normalize();
    }
    pts.push(p.multiplyScalar(R * 1.003));
  }
  return pts;
}

/** Find shared service offerings between all stone pairs */
function computeEdges(stones) {
  const edges = [];
  for (let i = 0; i < stones.length; i++)
    for (let j = i + 1; j < stones.length; j++) {
      const shared = new Set();
      stones[i].svcs.forEach(a => stones[j].svcs.forEach(b => {
        if (serviceKey(a) === serviceKey(b)) shared.add(serviceKey(a));
      }));
      if (shared.size > 0) edges.push({ from: i, to: j, sets: [...shared] });
    }
  return edges;
}

/**
 * Render a stone's canvas-based display sprite.
 * 512×512 transparent canvas with health arcs, center LED, name, hardware, service dots.
 */
function renderStoneCanvas(stone, offline = false) {
  const S = 512, H = S/2, CY = 195, AR = 148, LW = 7, GAP = 0.14;
  const c = document.createElement("canvas"); c.width = S; c.height = S;
  const x = c.getContext("2d");
  const SEG = (Math.PI * 2) / 3 - GAP;
  const alive = stone.health !== "resting" && !offline;

  // Resource arcs (3 segments: CPU, MEM, DSK)
  [stone.res.cpu, stone.res.mem, stone.res.dsk].forEach((val, i) => {
    const a0 = i * (Math.PI * 2) / 3 - Math.PI / 2 + GAP / 2;
    x.beginPath(); x.arc(H, CY, AR, a0, a0 + SEG);
    x.strokeStyle = "rgba(255,255,255,0.07)"; x.lineWidth = LW; x.lineCap = "round"; x.stroke();
    if (alive) {
      const fill = SEG * (val / 100);
      if (fill > 0.02) {
        x.beginPath(); x.arc(H, CY, AR, a0, a0 + fill);
        x.strokeStyle = offline ? "#555" : rc(val); x.lineWidth = LW; x.lineCap = "round"; x.stroke();
      }
    }
  });

  // Inner color ring
  x.beginPath(); x.arc(H, CY, AR - 22, 0, Math.PI * 2);
  x.strokeStyle = (offline ? "#555" : stone.color) + (alive ? "35" : "18"); x.lineWidth = 2; x.stroke();

  // Center LED
  const col = offline ? "#555" : hc(stone.health);
  x.shadowColor = col; x.shadowBlur = alive ? 25 : 8;
  x.beginPath(); x.arc(H, CY, alive ? 8 : 5, 0, Math.PI * 2); x.fillStyle = col; x.fill();
  x.shadowBlur = 0;

  // Name
  x.font = `500 ${alive ? 28 : 24}px "IBM Plex Sans",sans-serif`;
  x.fillStyle = offline ? "#555" : alive ? "#fafaf9" : "#78716c";
  x.textAlign = "center"; x.textBaseline = "top";
  x.fillText(stone.name, H, CY + AR + 12);

  // Hardware or offline label
  if (offline) {
    x.font = '500 20px "IBM Plex Mono",monospace'; x.fillStyle = "#555";
    x.fillText("OFFLINE", H, CY + AR + 46);
  } else {
    x.font = '300 17px "IBM Plex Mono",monospace'; x.fillStyle = "#78716c";
    x.fillText(`${stone.hw.cores}c · ${stone.hw.mem}GB`, H, CY + AR + 46);
  }

  // Service dots
  const svcs = stone.svcs, sp = 13, sx = H - ((svcs.length - 1) * sp) / 2;
  svcs.forEach((sv, i) => {
    x.beginPath(); x.arc(sx + i * sp, CY + AR + 72, 4, 0, Math.PI * 2);
    if (!offline && sv.s === "running") { x.fillStyle = "#84a59d"; x.fill(); }
    else { x.strokeStyle = "#57534e"; x.lineWidth = 1.5; x.stroke(); }
  });

  // Keystone badge
  if (!offline && stone.pond === "keystone") {
    x.font = '400 13px "IBM Plex Mono",monospace'; x.fillStyle = "#c4b060";
    x.fillText("◆ keystone", H, CY + AR + 93);
  }

  return c;
}

function makeGlowTex(color = "#84a59d") {
  const c = document.createElement("canvas"); c.width = 128; c.height = 128;
  const x = c.getContext("2d");
  const g = x.createRadialGradient(64, 64, 0, 64, 64, 64);
  g.addColorStop(0, color + "60"); g.addColorStop(0.4, color + "20"); g.addColorStop(1, "transparent");
  x.fillStyle = g; x.fillRect(0, 0, 128, 128);
  return new THREE.CanvasTexture(c);
}

function makeSparkTex() {
  const c = document.createElement("canvas"); c.width = 32; c.height = 32;
  const x = c.getContext("2d");
  const g = x.createRadialGradient(16, 16, 0, 16, 16, 16);
  g.addColorStop(0, "#ffffff"); g.addColorStop(0.3, "#84a59dcc"); g.addColorStop(1, "transparent");
  x.fillStyle = g; x.fillRect(0, 0, 32, 32);
  return new THREE.CanvasTexture(c);
}

// ── GardenSphere Class ──────────────────────────────────────────

/**
 * @typedef {Object} Stone
 * @property {string}  id       — Unique identifier
 * @property {string}  name     — Display name (e.g. "crystal-forest")
 * @property {string}  color    — Hex color for the node body
 * @property {string}  health   — "thriving" | "withering" | "resting"
 * @property {Object}  hw       — { cores: number, mem: number (GB) }
 * @property {Object}  res      — { cpu: 0-100, mem: 0-100, dsk: 0-100 }
 * @property {Array}   svcs     — [{ o: "offering", i: "instance"|null, s: "running"|"stopped" }]
 * @property {string}  pond     — "keystone" | "member"
 */

/**
 * @typedef {Object} GardenSphereOptions
 * @property {number}   [radius=10]      — Sphere radius in world units
 * @property {Function} [onHover]        — (id|null) => void, fires on hover change
 * @property {Function} [onTrack]        — ({ selected, departing, hovered, progress }) => void, fires every frame
 * @property {Function} [onTransition]   — ({ selectedId, departingId }) => void, fires on selection change
 * @property {Function} [onDataChange]   — (stones[]) => void, fires after any data mutation
 */

export class GardenSphere {
  constructor(container, opts = {}) {
    this.container = container;
    this.R = opts.radius || 10;
    this.onHover = opts.onHover || (() => {});
    this.onTrack = opts.onTrack || (() => {});
    this.onTransition = opts.onTransition || (() => {});
    this.onDataChange = opts.onDataChange || (() => {});
    this.nodes = []; this.conns = []; this.hitTargets = []; this.stones = [];
    this.hoveredId = null; this.selectedId = null; this.departingId = null;
    this.isDrag = false; this.prevM = { x: 0, y: 0 };
    this.vel = { x: 0, y: 0 }; this.lastInput = 0; this.t0 = performance.now();
    this.mouseInCanvas = false; this.autoRotMul = 1.0;
    this.rotTarget = null; this.rotFrom = null; this.rotProgress = 1; this.rotDuration = 0.9;
    this.layoutProgress = 1;

    const w = container.clientWidth, h = container.clientHeight;
    this.scene = new THREE.Scene();
    this.camera = new THREE.PerspectiveCamera(48, w / h, 0.1, 200);
    this.camera.position.set(0, 2, 28); this.camera.lookAt(0, 0, 0);
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(w, h); this.renderer.setClearColor(0x111110, 1);
    container.appendChild(this.renderer.domElement);
    this.scene.add(new THREE.AmbientLight(0x606060, 0.6));
    this.pLight = new THREE.PointLight(0xffffff, 0.7, 60);
    this.pLight.position.copy(this.camera.position); this.scene.add(this.pLight);
    this.sg = new THREE.Group(); this.scene.add(this.sg);

    // Equator + meridian rings
    this.ringMat = new THREE.MeshBasicMaterial({ color: 0x84a59d, transparent: true, opacity: 0.1 });
    const ring = new THREE.Mesh(new THREE.TorusGeometry(this.R, 0.02, 6, 160), this.ringMat);
    ring.rotation.x = -Math.PI / 2; this.sg.add(ring);
    const mer = new THREE.Mesh(
      new THREE.TorusGeometry(this.R, 0.012, 6, 160),
      new THREE.MeshBasicMaterial({ color: 0x84a59d, transparent: true, opacity: 0.04 })
    );
    mer.rotation.y = Math.PI / 3; this.sg.add(mer);

    // Ambient stars
    const starN = 250, starPos = new Float32Array(starN * 3);
    for (let i = 0; i < starN; i++) {
      const r = 25 + Math.random() * 30, t = Math.random() * Math.PI * 2, p = Math.acos(2 * Math.random() - 1);
      starPos[i*3] = r * Math.sin(p) * Math.cos(t);
      starPos[i*3+1] = r * Math.sin(p) * Math.sin(t);
      starPos[i*3+2] = r * Math.cos(p);
    }
    const starGeo = new THREE.BufferGeometry();
    starGeo.setAttribute("position", new THREE.BufferAttribute(starPos, 3));
    this.scene.add(new THREE.Points(starGeo, new THREE.PointsMaterial({
      color: 0x84a59d, size: 0.06, transparent: true, opacity: 0.15, sizeAttenuation: true
    })));

    this.ray = new THREE.Raycaster(); this.mouse = new THREE.Vector2(); this.sparkTex = makeSparkTex();
    this._bindEvents(this.renderer.domElement);
    this._startAnim();
  }

  // ── Public API ────────────────────────────────────────────────

  /** Load initial stone data. Clears everything and rebuilds. */
  setData(stones) {
    this._clearAll();
    this.stones = [...stones];
    const positions = fibSphere(stones.length);
    stones.forEach((st, idx) => {
      const pos = new THREE.Vector3(...positions[idx]).multiplyScalar(this.R);
      this.nodes.push(this._mkNode(st, pos));
    });
    this._rebuildEdges();
    this.onDataChange(this.stones);
  }

  /** Update a stone's data in-place. Pass any subset of Stone fields.
   *  Does NOT trigger re-layout — use for live metrics, health changes, service updates. */
  updateStone(id, patch) {
    const node = this.nodes.find(n => n.stone.id === id); if (!node) return;
    const si = this.stones.findIndex(s => s.id === id);
    if (si >= 0) this.stones[si] = { ...this.stones[si], ...patch };
    Object.assign(node.stone, patch);
    this._refreshTex(node);
    node.bodyMat.emissive = new THREE.Color(hc(node.stone.health));
    if (patch.svcs) this._rebuildEdges();
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
    const pos = new THREE.Vector3(...positions[this.stones.length - 1]).multiplyScalar(this.R);
    const newNode = this._mkNode(stone, pos);
    newNode.enterScale = 0;
    this.nodes.push(newNode);
    this.layoutProgress = 0;
    this._rebuildEdges();
    this.onDataChange(this.stones);
  }

  /** Remove a stone with fade-out animation, then re-layout remaining. */
  removeStone(id) {
    const ni = this.nodes.findIndex(n => n.stone.id === id); if (ni < 0) return;
    const node = this.nodes[ni];
    node.removing = true; node.removeProgress = 0;
    node.removeCallback = () => {
      this.sg.remove(node.group);
      this.hitTargets = this.hitTargets.filter(h => h.userData.stoneId !== id);
      this.nodes.splice(this.nodes.indexOf(node), 1);
      this.stones = this.stones.filter(s => s.id !== id);
      if (this.selectedId === id) { this.selectedId = null; this.onTransition({ selectedId: null, departingId: id }); }
      if (this.hoveredId === id) this.hoveredId = null;
      const positions = fibSphere(this.stones.length);
      this.nodes.forEach((n, idx) => {
        n.targetPos = new THREE.Vector3(...positions[idx]).multiplyScalar(this.R);
      });
      this.layoutProgress = 0;
      this._rebuildEdges();
      this.onDataChange(this.stones);
    };
  }

  /** Mark stone as offline. Stays in sphere position, goes gray, edges disconnect. */
  offlineStone(id) {
    const node = this.nodes.find(n => n.stone.id === id); if (!node) return;
    node.offline = true; this._refreshTex(node);
    node.bodyMat.color = new THREE.Color("#444");
    node.bodyMat.emissive = new THREE.Color("#333");
    this._rebuildEdges();
    this.onDataChange(this.stones);
  }

  /** Restore stone from offline. Optionally merge new data. */
  onlineStone(id, patch) {
    const node = this.nodes.find(n => n.stone.id === id); if (!node) return;
    if (patch) Object.assign(node.stone, patch);
    node.offline = false; this._refreshTex(node);
    node.bodyMat.color = new THREE.Color(node.stone.color);
    node.bodyMat.emissive = new THREE.Color(hc(node.stone.health));
    this._rebuildEdges();
    this.onDataChange(this.stones);
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
    this.scene.traverse(o => {
      if (o.geometry) o.geometry.dispose();
      if (o.material) { if (o.material.map) o.material.map.dispose(); o.material.dispose(); }
    });
    this.renderer.dispose();
    this.container.removeChild(el);
  }

  // ── Internal ──────────────────────────────────────────────────

  _bindEvents(el) {
    this._bPD = this._onPD.bind(this); this._bPM = this._onPM.bind(this); this._bPU = this._onPU.bind(this);
    this._bWh = this._onWh.bind(this); this._bCtx = e => e.preventDefault(); this._bRz = this._resize.bind(this);
    this._bEnter = () => { this.mouseInCanvas = true; }; this._bLeave = () => { this.mouseInCanvas = false; };
    el.addEventListener("contextmenu", this._bCtx); el.addEventListener("pointerdown", this._bPD);
    el.addEventListener("pointerenter", this._bEnter); el.addEventListener("pointerleave", this._bLeave);
    window.addEventListener("pointermove", this._bPM); window.addEventListener("pointerup", this._bPU);
    el.addEventListener("wheel", this._bWh, { passive: false }); window.addEventListener("resize", this._bRz);
  }

  _refreshTex(node) {
    const c = renderStoneCanvas(node.stone, node.offline);
    node.disp.material.map.dispose();
    node.disp.material.map = new THREE.CanvasTexture(c);
    node.disp.material.map.minFilter = THREE.LinearFilter;
    node.disp.material.needsUpdate = true;
  }

  _clearAll() { this.nodes.forEach(n => this.sg.remove(n.group)); this._clearEdges(); this.nodes = []; this.hitTargets = []; }

  _clearEdges() {
    this.conns.forEach(c => {
      this.sg.remove(c.tube); c.tube.geometry.dispose(); c.tubeMat.dispose();
      c.sparkles.forEach(s => { this.sg.remove(s); s.material.dispose(); });
      if (c.label) { this.sg.remove(c.label); c.labelMat.map.dispose(); c.labelMat.dispose(); }
    });
    this.conns = [];
  }

  _rebuildEdges() {
    this._clearEdges();
    const activeNodes = this.nodes.filter(n => !n.removing && !n.offline);
    const activeStones = activeNodes.map(n => n.stone);
    computeEdges(activeStones).forEach(edge => {
      const n1 = activeNodes[edge.from], n2 = activeNodes[edge.to];
      this.conns.push(this._mkConn(n1.group.position, n2.group.position, edge.sets));
    });
  }

  _computeRotTarget(node) {
    const wp = new THREE.Vector3(); node.group.getWorldPosition(wp);
    const q = new THREE.Quaternion().setFromUnitVectors(wp.clone().normalize(), this.camera.position.clone().normalize());
    return q.multiply(this.sg.quaternion.clone());
  }

  _toScreen(wp) {
    const v = wp.clone().project(this.camera), rect = this.renderer.domElement.getBoundingClientRect();
    return { x: (v.x * 0.5 + 0.5) * rect.width + rect.left, y: (-v.y * 0.5 + 0.5) * rect.height + rect.top };
  }

  _screenOf(id) {
    const n = this.nodes.find(n => n.stone.id === id); if (!n) return null;
    const wp = new THREE.Vector3(); n.group.getWorldPosition(wp); return this._toScreen(wp);
  }

  _mkNode(stone, pos) {
    const group = new THREE.Group(); group.position.copy(pos); this.sg.add(group);
    const bodyMat = new THREE.MeshStandardMaterial({
      color: new THREE.Color(stone.color), emissive: new THREE.Color(hc(stone.health)),
      emissiveIntensity: 0.4, roughness: 0.7, metalness: 0.2, transparent: true, opacity: 1
    });
    group.add(new THREE.Mesh(new THREE.SphereGeometry(0.45, 20, 20), bodyMat));
    const glowMat = new THREE.SpriteMaterial({ map: makeGlowTex(stone.color), transparent: true, opacity: 0.35, blending: THREE.AdditiveBlending, depthWrite: false });
    const glow = new THREE.Sprite(glowMat); glow.scale.set(3.5, 3.5, 1); group.add(glow);
    const tex = new THREE.CanvasTexture(renderStoneCanvas(stone)); tex.minFilter = THREE.LinearFilter;
    const dispMat = new THREE.SpriteMaterial({ map: tex, transparent: true, depthWrite: false });
    const disp = new THREE.Sprite(dispMat);
    disp.position.copy(pos.clone().normalize().multiplyScalar(0.6));
    disp.scale.set(4.2, 4.2, 1); group.add(disp);
    const hit = new THREE.Mesh(new THREE.SphereGeometry(2.2, 8, 8), new THREE.MeshBasicMaterial({ visible: false }));
    hit.userData.stoneId = stone.id; group.add(hit); this.hitTargets.push(hit);
    return { group, body: group.children[0], bodyMat, glow, glowMat, disp, dispMat, pos, stone, baseScale: 4.2, targetPos: null, enterScale: 1, offline: false, removing: false, removeProgress: 0 };
  }

  _mkConn(p1, p2, sets) {
    const pts = greatCircle(p1, p2, this.R, 48), curve = new THREE.CatmullRomCurve3(pts);
    const tubeMat = new THREE.MeshBasicMaterial({ color: 0x84a59d, transparent: true, opacity: 0.18, depthWrite: false });
    const tube = new THREE.Mesh(new THREE.TubeGeometry(curve, 48, 0.025 + sets.length * 0.008, 6, false), tubeMat);
    this.sg.add(tube);
    const lc = document.createElement("canvas"); lc.width = 256; lc.height = 48;
    const lx = lc.getContext("2d");
    lx.font = '400 16px "IBM Plex Mono",monospace'; lx.fillStyle = "#84a59d";
    lx.textAlign = "center"; lx.textBaseline = "middle"; lx.fillText(sets.join(" · "), 128, 24);
    const labelMat = new THREE.SpriteMaterial({ map: new THREE.CanvasTexture(lc), transparent: true, opacity: 0.6, depthWrite: false });
    const label = new THREE.Sprite(labelMat);
    label.position.copy(curve.getPoint(0.5).normalize().multiplyScalar(this.R * 1.06));
    label.scale.set(3.5, 0.7, 1); this.sg.add(label);
    const sparkles = [];
    for (let i = 0; i < Math.min(sets.length + 1, 3); i++) {
      const sMat = new THREE.SpriteMaterial({ map: this.sparkTex, transparent: true, opacity: 0.7, blending: THREE.AdditiveBlending, depthWrite: false });
      const s = new THREE.Sprite(sMat); s.scale.set(0.35, 0.35, 1);
      s.userData.t = i / 3; s.userData.spd = 0.08 + Math.random() * 0.06;
      s.position.copy(curve.getPoint(s.userData.t)); this.sg.add(s); sparkles.push(s);
    }
    return { tube, tubeMat, curve, sparkles, label, labelMat, sets };
  }

  _animate() {
    const t = (performance.now() - this.t0) * 0.001, dt = 1 / 60, camZ = this.camera.position.z;
    const targetMul = this.mouseInCanvas ? 0 : 1;
    this.autoRotMul += (targetMul - this.autoRotMul) * 0.03;
    const isSlerping = this.rotTarget && this.rotProgress < 1;

    if (isSlerping) {
      this.rotProgress = Math.min(this.rotProgress + dt / this.rotDuration, 1);
      const ease = 1 - Math.pow(1 - this.rotProgress, 3);
      this.sg.quaternion.copy(this.rotFrom.clone().slerp(this.rotTarget, ease));
      if (this.rotProgress >= 1) this.departingId = null;
    } else if (!this.isDrag) {
      if (Math.abs(this.vel.x) > 0.00005 || Math.abs(this.vel.y) > 0.00005) {
        this.sg.quaternion.premultiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), this.vel.x));
        this.sg.quaternion.premultiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1, 0, 0), this.vel.y));
        this.vel.x *= 0.96; this.vel.y *= 0.96;
      }
      if (performance.now() - this.lastInput > 3500) {
        const spd = 0.0008 * this.autoRotMul;
        if (spd > 0.000001) this.sg.quaternion.premultiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), spd));
      }
    }

    // Layout migration (add/remove triggers)
    if (this.layoutProgress < 1) {
      this.layoutProgress = Math.min(this.layoutProgress + dt / 0.8, 1);
      const ease = 1 - Math.pow(1 - this.layoutProgress, 3);
      let needEdgeRebuild = false;
      this.nodes.forEach(n => {
        if (n.targetPos && !n.removing) {
          n.group.position.lerp(n.targetPos, ease); n.pos = n.group.position.clone();
          n.disp.position.copy(n.pos.clone().normalize().multiplyScalar(0.6));
          if (this.layoutProgress >= 1) { n.targetPos = null; needEdgeRebuild = true; }
        }
        if (n.enterScale < 1) {
          n.enterScale = Math.min(n.enterScale + dt / 0.6, 1);
          n.group.scale.setScalar(1 - Math.pow(1 - n.enterScale, 2));
        }
      });
      if (needEdgeRebuild) this._rebuildEdges();
    }

    // Remove animation
    this.nodes.forEach(n => {
      if (n.removing) {
        n.removeProgress = Math.min(n.removeProgress + dt / 0.5, 1);
        const a = 1 - n.removeProgress;
        n.group.scale.setScalar(a); n.bodyMat.opacity = a; n.dispMat.opacity = a; n.glowMat.opacity = a * 0.35;
        if (n.removeProgress >= 1 && n.removeCallback) { n.removeCallback(); n.removeCallback = null; }
      }
    });

    // Depth-based opacity/scale
    this.ringMat.opacity = 0.09 + 0.025 * Math.sin(t * 0.7);
    const wp = new THREE.Vector3();
    this.nodes.forEach(n => {
      if (n.removing) return;
      n.group.getWorldPosition(wp);
      const dist = this.camera.position.distanceTo(wp);
      const near = camZ - this.R, far = camZ + this.R;
      const depth = THREE.MathUtils.clamp((dist - near) / (far - near), 0, 1);
      const opacity = THREE.MathUtils.lerp(1.0, 0.08, depth);
      const scale = THREE.MathUtils.lerp(1.0, 0.55, depth);
      n.dispMat.opacity = opacity; n.disp.scale.setScalar(n.baseScale * scale);
      n.glowMat.opacity = opacity * 0.35; n.bodyMat.opacity = opacity;
      const alive = n.stone.health !== "resting" && !n.offline;
      const rate = n.stone.health === "thriving" ? 0.5 : n.stone.health === "withering" ? 1.3 : 0;
      const breath = alive ? 0.25 + 0.25 * Math.sin(t * rate * Math.PI * 2) : 0.08;
      n.bodyMat.emissiveIntensity = n.offline ? 0.05 : breath * (1 - depth * 0.5);
      if (n.stone.id === this.hoveredId || n.stone.id === this.selectedId) {
        n.glowMat.opacity = Math.min(opacity * 0.7, 0.7);
        n.disp.scale.setScalar(n.baseScale * scale * 1.08);
      }
    });

    // Emit tracking data for card positioning
    this.onTrack({
      selected: this.selectedId ? { id: this.selectedId, pos: this._screenOf(this.selectedId) } : null,
      departing: this.departingId ? { id: this.departingId, pos: this._screenOf(this.departingId) } : null,
      hovered: this.hoveredId ? { id: this.hoveredId, pos: this._screenOf(this.hoveredId) } : null,
      progress: isSlerping ? this.rotProgress : 1,
    });

    // Animate sparkles and connection labels
    this.conns.forEach(c => {
      c.sparkles.forEach(s => {
        s.userData.t = (s.userData.t + s.userData.spd * 0.016) % 1;
        s.position.copy(c.curve.getPoint(s.userData.t));
        s.material.opacity = 0.4 + 0.3 * Math.sin(t * 2.5 + s.userData.t * 8);
      });
      if (c.label) {
        const lp = new THREE.Vector3(); c.label.getWorldPosition(lp);
        const ld = this.camera.position.distanceTo(lp);
        const dd = THREE.MathUtils.clamp((ld - (camZ - this.R)) / ((camZ + this.R) - (camZ - this.R)), 0, 1);
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
    const hits = this.ray.intersectObjects(this.hitTargets);
    return hits.length > 0 ? hits[0].object.userData.stoneId : null;
  }

  _onPD(e) {
    if (e.button === 2 || e.button === 1) {
      this.isDrag = true; this.prevM = { x: e.clientX, y: e.clientY }; this.vel = { x: 0, y: 0 };
      this.rotProgress = 1; // cancel slerp on drag
    } else if (e.button === 0) {
      const id = this._rayTest(e);
      const newId = id === this.selectedId ? null : id;
      const prevId = this.selectedId;
      this.selectedId = newId;
      if (newId) {
        this.departingId = prevId;
        const node = this.nodes.find(n => n.stone.id === newId);
        if (node) {
          this.rotFrom = this.sg.quaternion.clone();
          this.rotTarget = this._computeRotTarget(node);
          this.rotProgress = 0; this.vel = { x: 0, y: 0 };
        }
        this.onTransition({ selectedId: newId, departingId: prevId });
      } else {
        this.departingId = prevId;
        this.onTransition({ selectedId: null, departingId: prevId });
      }
    }
    this.lastInput = performance.now();
  }

  _onPM(e) {
    if (!this.isDrag) {
      const id = this._rayTest(e);
      if (id !== this.hoveredId) { this.hoveredId = id; this.onHover(id); }
    }
    if (this.isDrag) {
      const dx = e.clientX - this.prevM.x, dy = e.clientY - this.prevM.y;
      this.prevM = { x: e.clientX, y: e.clientY };
      const spd = 0.004;
      this.sg.quaternion.premultiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), dx * spd));
      this.sg.quaternion.premultiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(1, 0, 0), dy * spd));
      this.vel = { x: dx * spd, y: dy * spd };
      this.lastInput = performance.now();
    }
  }

  _onPU(e) { if (e.button === 2 || e.button === 1) this.isDrag = false; }

  _onWh(e) {
    e.preventDefault();
    this.camera.position.z = THREE.MathUtils.clamp(this.camera.position.z + e.deltaY * 0.025, 16, 48);
    this.camera.lookAt(0, 0, 0); this.lastInput = performance.now();
  }

  _resize() {
    const w = this.container.clientWidth, h = this.container.clientHeight;
    this.camera.aspect = w / h; this.camera.updateProjectionMatrix(); this.renderer.setSize(w, h);
  }

  _startAnim() { const loop = () => { this._animId = requestAnimationFrame(loop); this._animate(); }; loop(); }
}

export default GardenSphere;
