import { useState, useEffect, useRef, useMemo } from "react";

// ═══════════════════════════════════════════════════════
// ASTRONOMY
// ═══════════════════════════════════════════════════════
function getMoonPhase(date = new Date()) {
  const year = date.getFullYear(), month = date.getMonth() + 1, day = date.getDate();
  let r = year % 100; r %= 19;
  if (r > 9) r -= 19;
  r = ((r * 11) % 30) + month + day;
  if (month < 3) r += 2;
  r -= ((year < 2000) ? 4 : 8.3);
  r = Math.floor(r + 0.5) % 30;
  return (r < 0) ? r + 30 : r;
}
function getSunPos(hour) {
  return Math.max(0, Math.sin(((hour - 6) / 12) * Math.PI));
}

// ═══════════════════════════════════════════════════════
// IDENTITY
// ═══════════════════════════════════════════════════════
function stoneHue(name) {
  let h = 0;
  for (let i = 0; i < name.length; i++) h = name.charCodeAt(i) + ((h << 5) - h);
  return Math.abs(h) % 360;
}
function stoneColor(name) { return `hsl(${stoneHue(name)}, 40%, 60%)`; }

const SAGE = "#84a59d";
const HONEY = "#c4b060";
const CLAY = "#d4a373";

// ═══════════════════════════════════════════════════════
// PIXEL DRAWING
// ═══════════════════════════════════════════════════════
function px(ctx, x, y, color, a = 1) {
  if (a <= 0) return;
  ctx.globalAlpha = Math.min(1, a);
  ctx.fillStyle = color;
  ctx.fillRect(Math.floor(x), Math.floor(y), 1, 1);
  ctx.globalAlpha = 1;
}

function drawSprite(ctx, ox, oy, rows, pal) {
  for (let r = 0; r < rows.length; r++) {
    for (let c = 0; c < rows[r].length; c++) {
      const ch = rows[r][c];
      if (ch !== " " && pal[ch]) px(ctx, ox + c, oy + r, pal[ch]);
    }
  }
}

// ═══════════════════════════════════════════════════════
// SPRITES
// ═══════════════════════════════════════════════════════

// Cricket — traced from user's pixel art reference, colorized
// 21×16 side profile with dark 1px border, bent hind legs, wing detail
const CRICKET_REST = [
  "            #        ",
  "       #####E#       ",
  "      #HHHBH#        ",
  "     #H#####         ",
  "    #H#              ",
  "    ## ## #########  ",
  "   #BW#WW#WwWWwWWW# ",
  "  #BWW#WW#WWW#WwWW# ",
  "  #B#W#WW#Ww#W#WWW# ",
  "  #BWW#WW#W#WwW#W#  ",
  "   #BW#WW##WW#W##   ",
  "    ######WW##W#    ",
  "    #L#L# ## #L#    ",
  "   #L##L#    #L#    ",
  "  #L##L#      #L#   ",
  "   #  #        #    ",
];
const CRICKET_CHIRP = [
  "            #     ~  ",
  "       #####E#   ~   ",
  "      #HHHBH#        ",
  "     #H#####         ",
  "    #H#              ",
  "    ## ## #########  ",
  "   #BW#WW#WwWWwWWW# ",
  "  #BWW#WW#WWW#WwWW# ",
  "  #B#W#WW#Ww#W#WWW# ",
  "  #BWW#WW#W#WwW#W#  ",
  "   #BW#WW##WW#W##   ",
  "    ######WW##W#    ",
  "    #L#L# ## #L#    ",
  "   #L##L#    #L#    ",
  "  #L##L#      #L#   ",
  "   #  #        #    ",
];
const CRICKET_PAL = {
  "#": "#1a1a1a", "H": "#3d6838", "E": "#8ab840", "B": "#4a7a42",
  "W": "#5a9a4e", "w": "#3a6a36", "L": "#2d5a2a", "~": HONEY,
};

// Scene lantern (tōrō) — 17×30, Japanese stone lantern with detail
// Parts: hōju finial, kasa roof, hibukuro fire box, chūdai platform, sao pillar, kiso base
const LANTERN_SCENE = [
  "        .        ",
  "       .R.       ",
  "       .R.       ",
  "      .RRR.      ",
  "     ..RRR..     ",
  "    .RRRRRRR.    ",
  "   .RRRRRRRRR.   ",
  "  .RRRRRRRRRRR.  ",
  " .rrRRRRRRRRRrr. ",
  "  .............  ",
  "   .BBBBBBBBB.   ",
  "   .B..FFF..B.   ",
  "   .B.FFFFF.B.   ",
  "   .B.FFFFF.B.   ",
  "   .B.FFFFF.B.   ",
  "   .B..FFF..B.   ",
  "   .BBBBBBBBB.   ",
  "    .........    ",
  "    .PPPPPPP.    ",
  "     .PPPPP.     ",
  "      .PPP.      ",
  "      .PPP.      ",
  "      .PPP.      ",
  "      .PPP.      ",
  "     .PPPPP.     ",
  "    .PPPPPPP.    ",
  "   ...........   ",
  "  .BBBBBBBBBBB.  ",
  "  .BBBBBBBBBBB.  ",
  "  .............  ",
];
const LANTERN_PAL_DAY = {
  ".": "#6a6560", "B": "#8a8580", "P": "#7a7570",
  "R": "#908880", "r": "#7a756e", "F": "#c4a040",
};
const LANTERN_PAL_NIGHT = {
  ".": "#2a2825", "B": "#3a3835", "P": "#32302d",
  "R": "#3e3c38", "r": "#2e2c28", "F": "#c4a040",
};
const LANTERN_PAL_DUSK = {
  ".": "#4a4540", "B": "#5a5550", "P": "#504b46",
  "R": "#5e5a54", "r": "#484440", "F": "#c4a040",
};

// Stone sprites — ~30w × 16h, FLAT BOTTOM, distinct top silhouettes
var STONE_SPRITES = [
  [ // Rounded dome — smooth arc top, flat base
    "           ......           ",
    "        ..SSSSSSSS..        ",
    "      ..SSSSSSSSSSSS..      ",
    "    ..SSSSSSHHHSSSSSSS..    ",
    "   .SSSSSSSSHHHHSSSSSSSS.   ",
    "  .SSSSSSSSSSSSSSSSSSSSSS.  ",
    " .SSSSSSSSSSSSSSSSSSSSSSSS. ",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    ".SSDSSSSSSSSSSSSSSSSSDSSSS.",
    ".SSDDSSSSSSSSSSSSSSSSDDSS. ",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    "............................",
  ],
  [ // Craggy peak — jagged asymmetric top, flat base
    "                 ....       ",
    "               .SSSS..      ",
    "             .SSSSSSS..     ",
    "      ...  .SSSSSHHHSS.    ",
    "    ..SSS..SSSSSSHHHSSS.   ",
    "   .SSSSSSSSSSSSSSSSSSSSS.  ",
    "  .SSSSSSSSSSSSSSSSSSSSSSS. ",
    " .SSSSSSSSSSSSSSSSSSSSSSSSS.",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    ".SSDSSSSSSSSSSSSSSSSSSSSSS.",
    ".SSDDSSSSSSSSSSSSSSSDDSSS. ",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    "............................",
  ],
  [ // Wide shelf — very flat, low, wide, subtle ridge
    "                            ",
    "     ......................  ",
    "   ..SSSSSSSSSSSSSSSSSSSS.. ",
    "  .SSSSSSSSHHHSSSSSSSSSSSSS.",
    " .SSSSSSSSHHHHSSSSSSSSSSSSSS",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    ".SSDSSSSSSSSSSSSSSSSSDSSSS.",
    ".SSDDSSSSSSSSSSSSSSSSDDSS. ",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    ".SSSSSSSSSSSSSSSSSSSSSSSSSS.",
    "............................",
  ],
];

function getStonePal(hue, nightDim = 1) {
  const s = 25, bl = 38;
  return {
    ".": `hsl(${hue}, ${s}%, ${Math.round((bl - 14) * nightDim)}%)`,
    "S": `hsl(${hue}, ${s}%, ${Math.round(bl * nightDim)}%)`,
    "H": `hsl(${hue}, ${s - 5}%, ${Math.round((bl + 8) * nightDim)}%)`,
    "D": `hsl(${hue}, ${s + 3}%, ${Math.round((bl - 8) * nightDim)}%)`,
  };
}

// Capability icons (8×8)
const ICON_SEED = [
  "   ..   ",
  "  .SS.  ",
  " .S..S. ",
  "  .SS.  ",
  "   ||   ",
  "   ||   ",
  "  .||.  ",
  " ..||.. ",
];
const SEED_PAL = { ".": "#3d6838", "S": "#6aaa4e", "|": "#5a4a32" };

const ICON_AI = [
  " ...... ",
  ".AAAAAA.",
  ".A.AA.A.",
  ".AAAAAA.",
  " .AAAA. ",
  "  .AA.  ",
  " .A..A. ",
  ".A.  .A.",
];
const AI_PAL_ON = { ".": "#4a5a8a", "A": "#7a9aca" };
const AI_PAL_OFF = { ".": "#2a2a2a", "A": "#3a3a3a" };
const AI_PAL_ACTIVE = { ".": "#6a5a2a", "A": HONEY };

const ICON_LANTERN = [
  "   ..   ",
  "  .LL.  ",
  " .LLLL. ",
  " .LLLL. ",
  " .LLLL. ",
  "  .LL.  ",
  "  ....  ",
  "   ..   ",
];
const LANTERN_PAL_ON = { ".": "#6a5020", "L": "#c4a040" };

function drawMoon(ctx, x, y, phase, radius = 5) {
  const col = "#e8e4d8";
  const np = phase / 29.5;
  for (let py = -radius; py <= radius; py++) {
    for (let ppx = -radius; ppx <= radius; ppx++) {
      const dist = Math.sqrt(ppx * ppx + py * py);
      if (dist > radius) continue;
      if (np < 0.03 || np > 0.97) {
        if (dist > radius - 1.2) px(ctx, x + ppx, y + py, col, 0.1);
        continue;
      }
      let lit;
      if (np <= 0.5) lit = ppx >= radius * Math.cos(np * Math.PI * 2);
      else lit = ppx <= -radius * Math.cos(np * Math.PI * 2);
      if (lit) {
        const tex = ((ppx * 7 + py * 13) % 5 === 0) ? 0.82 : 1;
        px(ctx, x + ppx, y + py, col, tex);
      }
    }
  }
}

function makeStars(count, w, h) {
  let rng = 42;
  const next = () => { rng = (rng * 1103515245 + 12345) & 0x7fffffff; return rng / 0x7fffffff; };
  return Array.from({ length: count }, () => ({
    x: Math.floor(next() * w), y: Math.floor(next() * h),
    b: next() * 0.4 + 0.25, ts: next() * 0.03 + 0.008, to: next() * Math.PI * 2,
  }));
}

function makeServiceFFs(offerings, hue, w, h) {
  let rng = 777 + hue;
  const next = () => { rng = (rng * 1103515245 + 12345) & 0x7fffffff; return rng / 0x7fffffff; };
  return offerings.map((o) => ({
    name: o.name, health: o.h,
    bx: 8 + next() * (w - 24), by: 4 + next() * (h * 0.55),
    dr: 3 + next() * 5, sp: 0.007 + next() * 0.012,
    ppx: next() * Math.PI * 2, ppy: next() * Math.PI * 2,
    depth: next(),
  }));
}

function hexRgb(hex) {
  const r = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  return r ? [parseInt(r[1],16), parseInt(r[2],16), parseInt(r[3],16)] : [0,0,0];
}
function lerpC(a, b, t) {
  const [ar,ag,ab] = hexRgb(a), [br,bg,bb] = hexRgb(b);
  return `rgb(${Math.round(ar+(br-ar)*t)},${Math.round(ag+(bg-ag)*t)},${Math.round(ab+(bb-ab)*t)})`;
}

// Gauge color: mid-gray(0) → teal → green → yellow → orange → hot orange(100)
function gaugeColor(val) {
  if (val < 15) return `rgb(80,80,80)`;
  if (val < 30) {
    const t = (val - 15) / 15;
    return `rgb(${Math.round(80-10*t)},${Math.round(80+50*t)},${Math.round(80+50*t)})`;
  }
  if (val < 50) {
    const t = (val - 30) / 20;
    return `rgb(${Math.round(70+70*t)},${Math.round(130+50*t)},${Math.round(130-80*t)})`;
  }
  if (val < 70) {
    const t = (val - 50) / 20;
    return `rgb(${Math.round(140+70*t)},${Math.round(180-20*t)},${Math.round(50-30*t)})`;
  }
  if (val < 85) {
    const t = (val - 70) / 15;
    return `rgb(${Math.round(210+30*t)},${Math.round(160-70*t)},${Math.round(20)})`;
  }
  const t = Math.min(1, (val - 85) / 15);
  return `rgb(${Math.round(240+15*t)},${Math.round(90-50*t)},${Math.round(20-15*t)})`;
}

// ═══════════════════════════════════════════════════════
// SCENE RENDERER
// ═══════════════════════════════════════════════════════

function TDisplay({ stone, hour, tick, showPond, hasCricket, hasGpu, gpuActive, isLantern, scale = 3.2 }) {
  const canvasRef = useRef(null);
  const starsRef = useRef(null);
  const W = 135, H = 240;
  const BAR = 5;

  // ── LAYOUT ──
  // Header: identity block + gap + gauges + gap = ~72px
  // Scenery: ~100px (42%)
  // Footer: offerings + icons = ~68px
  const HEAD_H = 96;
  const FOOT_H = 72;
  const SCENE_T = HEAD_H;
  const SCENE_H = H - HEAD_H - FOOT_H; // 100px
  const SCENE_B = SCENE_T + SCENE_H;

  const hue = stoneHue(stone.name);
  const sColor = stoneColor(stone.name);
  const moonPhase = getMoonPhase();
  const sunP = getSunPos(hour);
  const isNight = sunP < 0.15;
  const isDusk = sunP >= 0.15 && sunP < 0.35;
  const nightDim = isNight ? 0.45 : isDusk ? 0.7 : 1;

  if (!starsRef.current) starsRef.current = makeStars(26, W - BAR, SCENE_H * 0.7);

  const svcFFs = useMemo(() =>
    makeServiceFFs(stone.offerings, hue, W - BAR, SCENE_H),
    [stone.name, stone.offerings.length]
  );

  useEffect(() => {
    const c = canvasRef.current;
    if (!c) return;
    const ctx = c.getContext("2d");
    ctx.imageSmoothingEnabled = false;

    const healthColor = stone.health === "thriving" ? SAGE : stone.health === "withering" ? CLAY : HONEY;
    const panelBg = `hsl(${hue}, 6%, 8%)`;

    ctx.fillStyle = "#08090d";
    ctx.fillRect(0, 0, W, H);

    // ═══ COLOR BAR ═══
    ctx.fillStyle = sColor;
    ctx.fillRect(0, 0, BAR, H);
    const bg = ctx.createLinearGradient(0, 0, 0, H);
    bg.addColorStop(0, "rgba(255,255,255,0.18)");
    bg.addColorStop(0.5, "rgba(0,0,0,0)");
    bg.addColorStop(1, "rgba(0,0,0,0.22)");
    ctx.fillStyle = bg;
    ctx.fillRect(0, 0, BAR, H);

    // ═══════════════════════════════════════
    // TOP PANEL
    // ═══════════════════════════════════════
    ctx.fillStyle = panelBg;
    ctx.fillRect(BAR, 0, W - BAR, HEAD_H);
    ctx.fillStyle = `hsla(${hue}, 20%, 30%, 0.3)`;
    ctx.fillRect(BAR, HEAD_H - 1, W - BAR, 1);

    const cx = BAR + 7;
    const cw = W - cx - 6;

    // ── Identity block ──
    ctx.font = "bold 7px monospace";
    ctx.textBaseline = "top";
    ctx.fillStyle = `hsla(${hue}, 25%, 50%, 0.45)`;
    ctx.fillText("STONE", cx, 5);

    // Bigger name
    ctx.font = "bold 13px monospace";
    ctx.fillStyle = "#ece8e0";
    const displayName = stone.name.replace("stone-", "");
    ctx.fillText(displayName, cx, 15);

    // Health + uptime
    const statusY = 34;
    const bAlpha = 0.55 + Math.sin(tick * 0.05) * 0.35;
    ctx.beginPath();
    ctx.arc(cx + 3, statusY + 4, 2.5, 0, Math.PI * 2);
    ctx.fillStyle = healthColor;
    ctx.globalAlpha = bAlpha;
    ctx.fill();
    ctx.globalAlpha = 1;

    const dg = ctx.createRadialGradient(cx + 3, statusY + 4, 0, cx + 3, statusY + 4, 7);
    dg.addColorStop(0, healthColor + "22");
    dg.addColorStop(1, "transparent");
    ctx.fillStyle = dg;
    ctx.fillRect(cx - 4, statusY - 3, 14, 14);

    ctx.font = "bold 8px monospace";
    ctx.fillStyle = healthColor;
    ctx.fillText(stone.health.toUpperCase(), cx + 10, statusY);

    ctx.font = "7px monospace";
    ctx.fillStyle = "#4a4a4a";
    const upW = ctx.measureText(stone.uptime).width;
    ctx.fillText(stone.uptime, W - upW - 6, statusY + 1);

    // ── Separator ──
    const sepY = statusY + 16;
    ctx.fillStyle = `hsla(${hue}, 15%, 25%, 0.2)`;
    ctx.fillRect(cx, sepY, cw, 1);

    // ── Gauge bars — 4 rows with value-based color ──
    const gY = sepY + 7;
    const gW = cw - 24;
    const gH = 3;

    ["CPU", "MEM", "DSK", "I/O"].forEach((label, i) => {
      const y = gY + i * 9;
      const val = [stone.cpu, stone.mem, stone.disk, stone.io][i];

      ctx.font = "bold 6px monospace";
      ctx.fillStyle = "#4a4a4a";
      ctx.fillText(label, cx, y);

      ctx.fillStyle = "rgba(255,255,255,0.035)";
      ctx.fillRect(cx + 22, y + 1, gW, gH);

      const fw = Math.round(gW * val / 100);
      if (fw > 0) {
        ctx.fillStyle = gaugeColor(val);
        ctx.globalAlpha = 0.8;
        ctx.fillRect(cx + 22, y + 1, fw, gH);
        ctx.globalAlpha = 1;
      }

      ctx.font = "6px monospace";
      ctx.fillStyle = val > 80 ? CLAY : "#555";
      ctx.fillText(`${val}`, cx + cw - 10, y);
    });

    // ═══════════════════════════════════════
    // SCENERY
    // ═══════════════════════════════════════

    // Sky gradient — ends at ground line
    let skyT, skyM, skyB;
    if (isNight) { skyT = "#05061a"; skyM = "#0a0d22"; skyB = "#10132a"; }
    else if (isDusk) {
      const t = (sunP - 0.15) / 0.2;
      skyT = lerpC("#05061a", "#281838", t);
      skyM = lerpC("#0a0d22", "#4a2a40", t);
      skyB = lerpC("#10132a", "#b06030", t);
    } else {
      const t = Math.min(1, (sunP - 0.35) / 0.4);
      skyT = lerpC("#281838", "#3a5878", t);
      skyM = lerpC("#4a2a40", "#6a8aa8", t);
      skyB = lerpC("#b06030", "#98b0c0", t);
    }

    // Ground at 85% of scene — 15% for sand/water
    const groundY = SCENE_T + Math.round(SCENE_H * 0.85);
    const groundH = SCENE_B - groundY;
    const isWithering = stone.health === "withering";

    if (isWithering) {
      // ══════════════════════════════════════════
      // WITHERING — fire beneath the camera
      // ══════════════════════════════════════════
      // Sky fills whole scene, tinted red
      const skyG = ctx.createLinearGradient(0, SCENE_T, 0, SCENE_B);
      skyG.addColorStop(0, skyT);
      skyG.addColorStop(0.3, skyM);
      skyG.addColorStop(0.6, "#3a1a10");
      skyG.addColorStop(1, "#1a0800");
      ctx.fillStyle = skyG;
      ctx.fillRect(BAR, SCENE_T, W - BAR, SCENE_H);

      // Dim, reddened stars
      if (sunP < 0.5) {
        const sa = Math.max(0, 1 - sunP * 2.5) * 0.3;
        starsRef.current.forEach(st => {
          const tw = Math.sin(tick * st.ts + st.to) * 0.3 + 0.7;
          if (SCENE_T + st.y < SCENE_T + SCENE_H * 0.35) {
            px(ctx, BAR + st.x % (W - BAR), SCENE_T + st.y, "#e8b8a0", st.b * tw * sa);
          }
        });
      }

      // Moon — obscured, reddened
      if (sunP < 0.45) {
        ctx.globalAlpha = Math.max(0, 1 - sunP * 2.5) * 0.2;
        drawMoon(ctx, W - 18, SCENE_T + 10 + sunP * 10, moonPhase, 5);
        ctx.globalAlpha = 1;
      }

      // Fireflies flee upward — huddled at top, jittery
      const drawFF = (sf, i) => {
        const elapsed = tick * sf.sp;
        let fx = BAR + sf.bx + Math.sin(elapsed + sf.ppx) * sf.dr;
        let fy = SCENE_T + sf.by * 0.3 + Math.cos(elapsed * 0.7 + sf.ppy) * 2;
        fy += Math.sin(tick * 0.04 + i * 3.7) * 3; // jitter
        fy = Math.max(SCENE_T + 3, Math.min(fy, SCENE_T + SCENE_H * 0.35));

        let fH = 42, fS = 75, fL = 75;
        if (sf.health === "warning") { fH = 30; fS = 80; fL = 65; }
        else if (sf.health !== "healthy") { fH = 0; fS = 60; fL = 55; }

        const pulse = Math.sin(tick * 0.055 + i * 2.1) * 0.3 + 0.7;
        const nightBoost = isNight ? 1.5 : isDusk ? 1.1 : 0.65;
        const a = pulse * nightBoost;
        const gR = isNight ? 3.5 : 2;
        for (let gy = -gR; gy <= gR; gy++) {
          for (let gx = -gR; gx <= gR; gx++) {
            const d = Math.sqrt(gx * gx + gy * gy);
            if (d <= gR) px(ctx, fx + gx, fy + gy, `hsl(${fH},${fS}%,${fL}%)`, (1 - d / gR) * a * 0.3);
          }
        }
        px(ctx, Math.floor(fx), Math.floor(fy), `hsl(${fH},${fS - 15}%,${fL + 15}%)`, Math.min(1, a * 0.9));
      };
      svcFFs.forEach(drawFF);

      // ── Fire haze — fills bottom 75% of scene ──
      const heatH = Math.round(SCENE_H * 0.75);
      const heatTop = SCENE_B - heatH;
      for (let hy = heatTop; hy < SCENE_B; hy++) {
        const t = (hy - heatTop) / heatH; // 0→1 top to bottom
        // Transparent amber → dense orange → deep red at base
        const hr = Math.round(160 + 80 * t);
        const hg = Math.round(100 * (1 - t * 0.8));
        const hb = Math.round(15 * (1 - t));
        const ha = t * t * 0.45 + 0.02; // much denser at bottom
        const shimmer = Math.sin(tick * 0.1 + hy * 0.4) * 0.04;
        const shimmer2 = Math.sin(tick * 0.07 + hy * 0.25 + 1.5) * 0.02;
        ctx.fillStyle = `rgba(${hr},${hg},${hb},${Math.min(0.95, ha + shimmer + shimmer2)})`;
        ctx.fillRect(BAR, hy, W - BAR, 1);
      }

      // Ember particles rising from the fire
      for (let ei = 0; ei < 6; ei++) {
        const ePhase = (tick * 0.02 + ei * 1.1) % 1;
        const ex = BAR + 10 + ((ei * 37 + Math.floor(tick * 0.3)) % (W - BAR - 20));
        const ey = SCENE_B - ePhase * SCENE_H * 0.7;
        const ea = (1 - ePhase) * 0.7;
        const eCol = ePhase < 0.3 ? "#ff8020" : ePhase < 0.6 ? "#ff5010" : "#aa3010";
        px(ctx, ex, ey, eCol, ea);
        if (ePhase < 0.5) px(ctx, ex, ey - 1, eCol, ea * 0.4);
      }

    } else {
      // ══════════════════════════════════════════
      // HEALTHY / NORMAL — peaceful scene
      // ══════════════════════════════════════════
      const skyG = ctx.createLinearGradient(0, SCENE_T, 0, groundY);
      skyG.addColorStop(0, skyT);
      skyG.addColorStop(0.45, skyM);
      skyG.addColorStop(1, skyB);
      ctx.fillStyle = skyG;
      ctx.fillRect(BAR, SCENE_T, W - BAR, groundY - SCENE_T);

      // Stars
      if (sunP < 0.5) {
        const sa = Math.max(0, 1 - sunP * 2.5);
        starsRef.current.forEach(st => {
          const tw = Math.sin(tick * st.ts + st.to) * 0.3 + 0.7;
          px(ctx, BAR + st.x % (W - BAR), SCENE_T + st.y, "#e8e4d8", st.b * tw * sa);
        });
      }

      // Moon
      if (sunP < 0.45) {
        ctx.globalAlpha = Math.max(0, 1 - sunP * 2.5);
        drawMoon(ctx, W - 18, SCENE_T + 10 + sunP * 10, moonPhase, 5);
        ctx.globalAlpha = 1;
      }

      // Stone
      const stoneVariant = Math.floor(hue / 120) % 3;
      const stoneSprite = STONE_SPRITES[stoneVariant];
      const stonePal = getStonePal(hue, nightDim);
      const stoneW = stoneSprite[0].length;
      const stoneHt = stoneSprite.length;
      const stoneCx = BAR + Math.round((W - BAR) * 0.42);
      const stoneX = stoneCx - Math.round(stoneW / 2);
      const stoneY = groundY - stoneHt + 2;

      // Ground surface
      if (showPond) {
        const wt = isNight ? "#0a1422" : isDusk ? "#182232" : "#2a4858";
        const wb = isNight ? "#060c16" : isDusk ? "#0c1520" : "#1a3848";
        const wg = ctx.createLinearGradient(0, groundY, 0, SCENE_B);
        wg.addColorStop(0, wt); wg.addColorStop(1, wb);
        ctx.fillStyle = wg;
        ctx.fillRect(BAR, groundY, W - BAR, groundH);
        ctx.fillStyle = `rgba(160,190,210,${isNight ? 0.05 : 0.10})`;
        ctx.fillRect(BAR, groundY, W - BAR, 1);
        for (let ri = 0; ri < 2; ri++) {
          const rr = 5 + ri * 4 + Math.sin(tick * 0.035 + ri * 1.2) * 1.5;
          const ra = (0.08 - ri * 0.025) * nightDim;
          const ry = groundY + 3 + ri * 3;
          if (ry < SCENE_B) {
            for (let rpx = -rr; rpx <= rr; rpx++) {
              if (Math.abs(Math.abs(rpx) - rr) < 1.2) px(ctx, stoneCx + rpx, ry, "#8ab0c8", ra);
            }
          }
        }
      } else {
        // Zen garden raked sand
        let sandBase, sandLight, sandDark, rakeCol;
        if (isNight) { sandBase = "#1c1b18"; sandLight = "#201f1b"; sandDark = "#141311"; rakeCol = "#242320"; }
        else if (isDusk) { sandBase = "#3a362e"; sandLight = "#403c32"; sandDark = "#2a2620"; rakeCol = "#484438"; }
        else { sandBase = "#d0cbbe"; sandLight = "#ddd8cc"; sandDark = "#c0baa8"; rakeCol = "#e8e4d8"; }
        ctx.fillStyle = sandBase;
        ctx.fillRect(BAR, groundY, W - BAR, groundH);
        for (let sy = groundY; sy < SCENE_B; sy++) {
          for (let sx = BAR; sx < W; sx++) {
            const n = ((sx * 17 + sy * 31) % 7);
            if (n === 0) px(ctx, sx, sy, sandLight, 0.45);
            else if (n === 3) px(ctx, sx, sy, sandDark, 0.3);
          }
        }
        for (let ring = 0; ring < 10; ring++) {
          const baseR = 12 + ring * 4;
          const rAlpha = (ring % 2 === 0) ? 0.5 : 0.3;
          const rCol = (ring % 2 === 0) ? rakeCol : sandDark;
          for (let angle = 0; angle < 360; angle += 1) {
            const rad = (angle * Math.PI) / 180;
            const erx = baseR * 1.8, ery = baseR * 0.55;
            const rpx = stoneCx + Math.cos(rad) * erx;
            const rpy = groundY + 1 + Math.sin(rad) * ery;
            if (rpx >= BAR && rpx < W && rpy >= groundY && rpy < SCENE_B) {
              const dx = rpx - stoneCx, dy = rpy - (groundY + 1);
              if (Math.abs(dx) < 11 && dy > -6 && dy < 4) continue;
              px(ctx, rpx, rpy, rCol, rAlpha);
            }
          }
        }
        for (let shx = -7; shx <= 7; shx++) {
          const d = Math.abs(shx) / 7;
          px(ctx, stoneCx + shx, groundY + 1, "#888070", (isNight ? 0.06 : 0.12) * (1 - d * 0.6));
        }
      }

      // Fireflies
      const drawFF = (sf, i) => {
        const elapsed = tick * sf.sp;
        let fx = BAR + sf.bx + Math.sin(elapsed + sf.ppx) * sf.dr;
        let fy = SCENE_T + sf.by + Math.cos(elapsed * 0.7 + sf.ppy) * (sf.dr * 0.5);
        if (fy > groundY - 2) return;
        let fH = 42, fS = 75, fL = 75;
        if (sf.health === "warning") { fH = 30; fS = 80; fL = 65; }
        else if (sf.health !== "healthy") { fH = 0; fS = 60; fL = 55; }
        const pulse = Math.sin(tick * 0.055 + i * 2.1) * 0.3 + 0.7;
        const nightBoost = isNight ? 1.5 : isDusk ? 1.1 : 0.65;
        const a = pulse * nightBoost;
        const gR = isNight ? 3.5 : 2;
        for (let gy = -gR; gy <= gR; gy++) {
          for (let gx = -gR; gx <= gR; gx++) {
            const d = Math.sqrt(gx * gx + gy * gy);
            if (d <= gR) px(ctx, fx + gx, fy + gy, `hsl(${fH},${fS}%,${fL}%)`, (1 - d / gR) * a * 0.3);
          }
        }
        px(ctx, Math.floor(fx), Math.floor(fy), `hsl(${fH},${fS - 15}%,${fL + 15}%)`, Math.min(1, a * 0.9));
        if (showPond && fy < groundY) {
          const rfy = groundY + (groundY - fy) * 0.08;
          if (rfy < SCENE_B) px(ctx, Math.floor(fx), Math.floor(rfy), `hsl(${fH},${fS}%,${fL}%)`, a * 0.05);
        }
      };

      svcFFs.filter(f => f.depth < 0.5).forEach(drawFF);
      drawSprite(ctx, stoneX, stoneY, stoneSprite, stonePal);
      if (showPond) {
        const reflSprite = [...stoneSprite].reverse().slice(0, Math.min(3, groundH - 1));
        ctx.globalAlpha = 0.08;
        drawSprite(ctx, stoneX, groundY + 1, reflSprite, stonePal);
        ctx.globalAlpha = 1;
      }
      svcFFs.filter(f => f.depth >= 0.5).forEach(drawFF);

      // Cricket
      if (hasCricket) {
        const chirping = (tick % 90) < 14;
        const sprite = chirping ? CRICKET_CHIRP : CRICKET_REST;
        let cPal = { ...CRICKET_PAL };
        if (isNight) {
          for (const k of Object.keys(cPal)) {
            if (k !== "~") {
              const rgb = hexRgb(cPal[k]);
              cPal[k] = `rgb(${Math.round(rgb[0]*0.5)},${Math.round(rgb[1]*0.5)},${Math.round(rgb[2]*0.5)})`;
            }
          }
        }
        const cricketX = stoneX + Math.round(stoneW * 0.25);
        const cricketY = stoneY - sprite.length + 6;
        drawSprite(ctx, cricketX, cricketY, sprite, cPal);
      }

      // Lantern (tōrō)
      if (isLantern) {
        const lPal = isNight ? LANTERN_PAL_NIGHT : isDusk ? LANTERN_PAL_DUSK : LANTERN_PAL_DAY;
        const flickerPal = { ...lPal };
        const flicker1 = 0.7 + Math.sin(tick * 0.18) * 0.15 + Math.sin(tick * 0.31) * 0.1;
        const flicker2 = 0.8 + Math.sin(tick * 0.23 + 1) * 0.2;
        flickerPal["F"] = `rgb(${Math.round(196 * flicker1)},${Math.round(160 * flicker1)},${Math.round(64 * flicker2)})`;
        const lanternH = LANTERN_SCENE.length, lanternW = LANTERN_SCENE[0].length;
        const lx = stoneX + stoneW + 4, ly = groundY - lanternH + 2;
        drawSprite(ctx, lx, ly, LANTERN_SCENE, flickerPal);
        const glowCx = lx + Math.floor(lanternW / 2), glowCy = ly + 13;
        const glowR = isNight ? 16 : isDusk ? 12 : 8;
        const glowI = isNight ? 0.20 : isDusk ? 0.12 : 0.07;
        const glowP = glowI + Math.sin(tick * 0.12) * glowI * 0.3;
        for (let gy = -glowR; gy <= glowR; gy++) {
          for (let gx = -glowR; gx <= glowR; gx++) {
            const d = Math.sqrt(gx * gx + gy * gy);
            if (d <= glowR && d > 0) px(ctx, glowCx + gx, glowCy + gy, "#c4a040", Math.pow(1 - d / glowR, 2) * glowP);
          }
        }
        drawSprite(ctx, lx, ly, LANTERN_SCENE, flickerPal);
        if (groundY + 1 < SCENE_B) {
          const pA = isNight ? 0.15 : isDusk ? 0.08 : 0.04;
          for (let px2 = -6; px2 <= 6; px2++) {
            const d = Math.abs(px2) / 6;
            px(ctx, glowCx + px2, groundY + 1, "#c4a040", pA * (1 - d * 0.7));
            if (groundY + 2 < SCENE_B) px(ctx, glowCx + px2, groundY + 2, "#c4a040", pA * 0.5 * (1 - d));
            if (groundY + 3 < SCENE_B) px(ctx, glowCx + px2, groundY + 3, "#c4a040", pA * 0.2 * (1 - d));
          }
        }
      }
    } // end healthy/withering branch

    // Scene frame lines
    ctx.fillStyle = `hsla(${hue}, 20%, 25%, 0.35)`;
    ctx.fillRect(BAR, SCENE_T, W - BAR, 1);
    ctx.fillRect(BAR, SCENE_B - 1, W - BAR, 1);

    // ═══════════════════════════════════════
    // BOTTOM PANEL
    // ═══════════════════════════════════════
    ctx.fillStyle = panelBg;
    ctx.fillRect(BAR, SCENE_B, W - BAR, FOOT_H);
    ctx.fillStyle = `hsla(${hue}, 20%, 30%, 0.3)`;
    ctx.fillRect(BAR, SCENE_B, W - BAR, 1);

    const fx2 = BAR + 7;
    const fw2 = W - fx2 - 6;

    // Offerings list
    ctx.font = "bold 6px monospace";
    ctx.fillStyle = `hsla(${hue}, 25%, 50%, 0.4)`;
    ctx.textBaseline = "top";
    ctx.fillText("OFFERINGS", fx2, SCENE_B + 5);

    const maxShow = 4;
    const shown = stone.offerings.slice(0, maxShow);
    shown.forEach((o, i) => {
      const oy = SCENE_B + 16 + i * 9;
      const oCol = o.h === "healthy" ? SAGE : o.h === "warning" ? HONEY : CLAY;
      ctx.beginPath();
      ctx.arc(fx2 + 2, oy + 3, 1.5, 0, Math.PI * 2);
      ctx.fillStyle = oCol;
      ctx.fill();
      ctx.font = "7px monospace";
      ctx.fillStyle = "#9a9690";
      ctx.fillText(o.name, fx2 + 7, oy);
    });
    if (stone.offerings.length > maxShow) {
      const oy = SCENE_B + 16 + maxShow * 9;
      ctx.font = "6px monospace";
      ctx.fillStyle = "#3a3a3a";
      ctx.fillText(`+${stone.offerings.length - maxShow} more`, fx2 + 7, oy);
    }

    // ── Capability icons — RIGHT ALIGNED, with animation ──
    const iconsY = H - 16;
    ctx.fillStyle = `hsla(${hue}, 15%, 25%, 0.2)`;
    ctx.fillRect(fx2, iconsY - 5, fw2, 1);

    const icons = [];
    if (stone.seedBank) icons.push({ s: ICON_SEED, p: SEED_PAL, col: "#6aaa4e", speed: 0.025, active: true, busy: false });
    if (hasGpu) icons.push({
      s: ICON_AI, p: gpuActive ? AI_PAL_ACTIVE : AI_PAL_ON,
      col: gpuActive ? HONEY : "#7a9aca", speed: 0.06,
      active: true, busy: gpuActive, glow: gpuActive,
    });
    if (!hasGpu && stone.offerings.some(o => o.name === "ollama")) {
      icons.push({ s: ICON_AI, p: AI_PAL_OFF, col: "#3a3a3a", speed: 0, active: false, busy: false });
    }
    if (isLantern) icons.push({ s: ICON_LANTERN, p: LANTERN_PAL_ON, col: "#c4a040", speed: 0.035, active: true, busy: false });

    if (icons.length > 0) {
      const iconW = 8, gap = 10;
      const totalW = icons.length * iconW + (icons.length - 1) * gap;
      let ix = W - 6 - totalW;

      icons.forEach((ic, idx) => {
        // Draw sprite
        drawSprite(ctx, ix, iconsY, ic.s, ic.p);

        if (ic.active) {
          // ── Scanning underline — only when busy ──
          if (ic.busy) {
            const scanY = iconsY + 9;
            const scanPhase = (tick * ic.speed + idx * 1.7) % 1;
            const pp = scanPhase < 0.5 ? scanPhase * 2 : 2 - scanPhase * 2;
            const scanX = ix + pp * (iconW - 1);

            const dir = scanPhase < 0.5 ? 1 : -1;
            for (let t = 3; t >= 0; t--) {
              const tx = scanX - dir * t;
              if (tx >= ix && tx < ix + iconW) {
                const ta = (1 - t / 4) * 0.75;
                px(ctx, tx, scanY, ic.col, ta);
                if (t < 2) px(ctx, tx, scanY - 1, ic.col, ta * 0.15);
              }
            }
          }

          // ── Breathing corner brackets ──
          const ba = 0.2 + Math.sin(tick * 0.04 + idx * 2.3) * 0.15;

          // Top-left corner
          px(ctx, ix - 1, iconsY - 1, ic.col, ba);
          px(ctx, ix, iconsY - 1, ic.col, ba * 0.6);
          px(ctx, ix - 1, iconsY, ic.col, ba * 0.6);

          // Top-right corner
          px(ctx, ix + iconW, iconsY - 1, ic.col, ba);
          px(ctx, ix + iconW - 1, iconsY - 1, ic.col, ba * 0.6);
          px(ctx, ix + iconW, iconsY, ic.col, ba * 0.6);

          // Bottom-left corner
          px(ctx, ix - 1, iconsY + 8, ic.col, ba);
          px(ctx, ix, iconsY + 8, ic.col, ba * 0.6);
          px(ctx, ix - 1, iconsY + 7, ic.col, ba * 0.6);

          // Bottom-right corner
          px(ctx, ix + iconW, iconsY + 8, ic.col, ba);
          px(ctx, ix + iconW - 1, iconsY + 8, ic.col, ba * 0.6);
          px(ctx, ix + iconW, iconsY + 7, ic.col, ba * 0.6);
        }

        // GPU inferencing extra: soft radial glow
        if (ic.glow) {
          const ga = 0.10 + Math.sin(tick * 0.08) * 0.06;
          for (let gy = -3; gy <= 3; gy++) {
            for (let gx = -3; gx <= 3; gx++) {
              const d = Math.sqrt(gx * gx + gy * gy);
              if (d <= 3 && d > 0) px(ctx, ix + 4 + gx, iconsY + 4 + gy, HONEY, (1 - d / 3) * ga);
            }
          }
        }

        ix += iconW + gap;
      });
    }

  }, [stone, hour, tick, showPond, hasCricket, hasGpu, gpuActive, isLantern, svcFFs]);

  return (
    <canvas ref={canvasRef} width={W} height={H}
      style={{ width: W * scale, height: H * scale, imageRendering: "pixelated", borderRadius: 4 }}
    />
  );
}

// ═══════════════════════════════════════════════════════
// CONTROLS
// ═══════════════════════════════════════════════════════

function Toggle({ label, value, onChange }) {
  return (
    <div
      onClick={() => onChange(!value)}
      style={{
        display: "flex", alignItems: "center", gap: 8, cursor: "pointer",
        color: value ? "#a8a4a0" : "#4a4a4a", fontSize: 10,
        fontFamily: "'JetBrains Mono','Fira Code',monospace",
        padding: "3px 0", userSelect: "none",
      }}
    >
      <div style={{
        width: 16, height: 9, borderRadius: 5, position: "relative",
        background: value ? SAGE + "60" : "rgba(255,255,255,0.1)",
        transition: "background 0.2s", flexShrink: 0,
      }}>
        <div style={{
          position: "absolute", width: 7, height: 7, borderRadius: "50%",
          background: value ? SAGE : "#555", top: 1,
          left: value ? 8 : 1, transition: "left 0.15s, background 0.15s",
        }} />
      </div>
      <span>{label}</span>
    </div>
  );
}

function Panel({ title, children }) {
  return (
    <div style={{
      padding: "10px 14px", background: "rgba(255,255,255,0.02)",
      borderRadius: 8, border: "1px solid rgba(255,255,255,0.05)",
    }}>
      {title && (
        <div style={{
          fontSize: 9, color: "#4a4a4a", letterSpacing: 1,
          textTransform: "uppercase", marginBottom: 8,
          fontFamily: "'JetBrains Mono','Fira Code',monospace",
        }}>
          {title}
        </div>
      )}
      {children}
    </div>
  );
}

// ═══════════════════════════════════════════════════════
// APP
// ═══════════════════════════════════════════════════════

const STONES = [
  {
    name: "stone-quartz-fen", cpu: 38, mem: 62, disk: 28, io: 12, uptime: "7h 28m", health: "thriving",
    offerings: [{ name: "mongodb", h: "healthy" }, { name: "redis", h: "healthy" }],
    seedBank: { name: "seed-quartz", used: 32, total: 64 },
  },
  {
    name: "stone-amber-ridge", cpu: 71, mem: 45, disk: 55, io: 34, uptime: "47d 3h", health: "thriving",
    offerings: [
      { name: "ollama", h: "healthy" }, { name: "chromadb", h: "healthy" },
      { name: "mongodb", h: "warning" }, { name: "redis", h: "healthy" },
    ],
    seedBank: null,
  },
  {
    name: "stone-coral-reef", cpu: 92, mem: 88, disk: 72, io: 85, uptime: "12d 8h", health: "withering",
    offerings: [{ name: "postgresql", h: "degraded" }, { name: "redis", h: "healthy" }],
    seedBank: { name: "seed-coral", used: 52, total: 64 },
  },
];

export default function App() {
  const [si, setSi] = useState(0);
  const [hour, setHour] = useState(() => {
    const n = new Date(); return n.getHours() + n.getMinutes() / 60;
  });
  const [realTime, setRealTime] = useState(true);
  const [tick, setTick] = useState(0);
  const [pond, setPond] = useState(false);
  const [cricket, setCricket] = useState(true);
  const [gpu, setGpu] = useState(true);
  const [gpuAct, setGpuAct] = useState(false);
  const [lantern, setLantern] = useState(false);

  const stone = STONES[si];
  const sColor = stoneColor(stone.name);

  useEffect(() => {
    const iv = setInterval(() => setTick(t => t + 1), 100);
    return () => clearInterval(iv);
  }, []);

  useEffect(() => {
    if (!realTime) return;
    const iv = setInterval(() => {
      const n = new Date(); setHour(n.getHours() + n.getMinutes() / 60);
    }, 30000);
    return () => clearInterval(iv);
  }, [realTime]);

  const timeLabel = useMemo(() => {
    const h = Math.floor(hour), m = Math.floor((hour - h) * 60);
    return `${(h % 12) || 12}:${String(m).padStart(2, "0")} ${h >= 12 ? "PM" : "AM"}`;
  }, [hour]);

  const moonPhase = getMoonPhase();
  const moonNames = [
    "New","Wax Crescent","Wax Crescent","Wax Crescent","Wax Crescent","Wax Crescent","Wax Crescent",
    "First Qtr","First Qtr","Wax Gibbous","Wax Gibbous","Wax Gibbous","Wax Gibbous","Wax Gibbous",
    "Wax Gibbous","Full","Full","Wan Gibbous","Wan Gibbous","Wan Gibbous","Wan Gibbous","Wan Gibbous",
    "Last Qtr","Last Qtr","Wan Crescent","Wan Crescent","Wan Crescent","Wan Crescent","Wan Crescent","Wan Crescent",
  ];

  return (
    <div style={{
      minHeight: "100vh", background: "#0a0a0c", color: "#c8c4bc",
      fontFamily: "'JetBrains Mono','Fira Code','SF Mono',monospace",
      display: "flex", flexDirection: "column", alignItems: "center", padding: "20px 16px",
    }}>
      <div style={{ textAlign: "center", marginBottom: 20 }}>
        <h1 style={{
          fontSize: 14, fontWeight: 600, color: "#e8e4de",
          letterSpacing: 4, margin: 0, textTransform: "uppercase",
        }}>
          Firefly Living Diorama
        </h1>
        <div style={{ fontSize: 10, color: "#4a4a4a", marginTop: 4, letterSpacing: 1 }}>
          135×240 T-Display · v6
        </div>
      </div>

      <div style={{
        display: "flex", gap: 32, alignItems: "flex-start",
        flexWrap: "wrap", justifyContent: "center",
      }}>
        <div style={{
          padding: 7, background: "#111", borderRadius: 9,
          boxShadow: "0 4px 28px rgba(0,0,0,0.7)", border: "1px solid #1a1a1a",
        }}>
          <TDisplay
            stone={stone} hour={hour} tick={tick}
            showPond={pond} hasCricket={cricket}
            hasGpu={gpu} gpuActive={gpuAct} isLantern={lantern}
            scale={3.2}
          />
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: 12, minWidth: 240, maxWidth: 280 }}>
          <Panel title="Scene">
            <div style={{ fontSize: 10, color: "#7a7a7a", lineHeight: 1.9 }}>
              <span style={{ color: "#555" }}>Time:</span> {timeLabel}
              {" · "}<span style={{ color: "#555" }}>Moon:</span> {moonNames[moonPhase]}
              <br />
              <span style={{ color: "#555" }}>Services:</span> {stone.offerings.length} fireflies
              {" · "}<span style={{ color: "#555" }}>Health:</span>{" "}
              <span style={{ color: stone.health === "thriving" ? SAGE : CLAY }}>{stone.health}</span>
              <br />
              <span style={{ color: "#555" }}>Ground:</span> {pond ? "pond water" : "raked sand"}
            </div>
          </Panel>

          <Panel title="Time of Day">
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 6 }}>
              <div
                onClick={() => {
                  if (realTime) setRealTime(false);
                  else { const n = new Date(); setHour(n.getHours() + n.getMinutes() / 60); setRealTime(true); }
                }}
                style={{
                  background: realTime ? SAGE + "20" : "rgba(255,255,255,0.04)",
                  border: `1px solid ${realTime ? SAGE + "40" : "rgba(255,255,255,0.08)"}`,
                  borderRadius: 3, padding: "2px 8px", fontSize: 9,
                  color: realTime ? SAGE : "#5a5a5a", cursor: "pointer", userSelect: "none",
                }}
              >
                {realTime ? "● LIVE" : "manual"}
              </div>
              <span style={{ fontSize: 10, color: "#666" }}>{timeLabel}</span>
            </div>
            <input type="range" min={0} max={24} step={0.25} value={hour}
              onChange={e => { setHour(parseFloat(e.target.value)); setRealTime(false); }}
              style={{ width: "100%", accentColor: sColor, height: 3, opacity: 0.7 }}
            />
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: 7, color: "#333", marginTop: 2 }}>
              <span>12am</span><span>6am</span><span>12pm</span><span>6pm</span><span>12am</span>
            </div>
          </Panel>

          <Panel title="Stone">
            <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
              {STONES.map((s, i) => (
                <div
                  key={s.name}
                  onClick={() => setSi(i)}
                  style={{
                    background: i === si ? stoneColor(s.name) + "18" : "transparent",
                    border: `1px solid ${i === si ? stoneColor(s.name) + "40" : "rgba(255,255,255,0.04)"}`,
                    borderRadius: 4, padding: "4px 8px", fontSize: 10,
                    color: i === si ? stoneColor(s.name) : "#4a4a4a", cursor: "pointer", textAlign: "left",
                    display: "flex", alignItems: "center", gap: 7, transition: "all 0.15s",
                    userSelect: "none",
                  }}
                >
                  <span style={{
                    width: 7, height: 7, borderRadius: 2, background: stoneColor(s.name),
                    opacity: i === si ? 1 : 0.3, flexShrink: 0,
                  }} />
                  <span>{s.name.replace("stone-", "")}</span>
                  <span style={{ marginLeft: "auto", fontSize: 8, opacity: 0.4 }}>{s.offerings.length}</span>
                </div>
              ))}
            </div>
          </Panel>

          <Panel title="Scene Elements">
            <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
              <Toggle label="Pond (water + ripples)" value={pond} onChange={setPond} />
              <Toggle label="Cricket companion" value={cricket} onChange={setCricket} />
              <Toggle label="GPU present" value={gpu} onChange={setGpu} />
              <Toggle label="GPU active (inferencing)" value={gpuAct} onChange={setGpuAct} />
              <Toggle label="Lantern role" value={lantern} onChange={setLantern} />
            </div>
          </Panel>

          <Panel title="Visual Language">
            <div style={{ fontSize: 9, color: "#555", lineHeight: 2 }}>
              {[
                [`hsl(42,75%,75%)`, "Firefly = running service"],
                ["#5a9a4e", "Cricket = audio companion"],
                [sColor, "Stone tinted by identity"],
                ["#ddd8cc", "Raked sand = zen garden"],
                ["#8ab0c8", "Ripples = pond membership"],
                ["#e8e4d8", "Moon = real lunar phase"],
                ["#7a9aca", "AI brain = GPU capability"],
                ["#c4a040", "Lantern = registry role"],
                ["#6aaa4e", "Sprout = seed bank"],
              ].map(([c, desc]) => (
                <div key={desc} style={{ display: "flex", alignItems: "center", gap: 6 }}>
                  <span style={{ width: 8, height: 8, borderRadius: 2, background: c, flexShrink: 0, opacity: 0.9 }} />
                  {desc}
                </div>
              ))}
            </div>
          </Panel>
        </div>
      </div>
    </div>
  );
}
