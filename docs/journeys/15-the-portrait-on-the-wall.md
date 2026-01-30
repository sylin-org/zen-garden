# The Portrait on the Wall

*You want a screen showing what this Stone is doing.*

---

## The Story

You mount a small screen next to your Stones—an old tablet running a web browser. You want it to show what stone-amber-ridge is doing at a glance.

You open the browser and navigate to:

```
http://stone-amber-ridge.local:7185/portrait
```

The page loads. It's not a dashboard with dozens of graphs. It's simpler than that. Calmer.

---

The screen shows:

```
                              STONE

                    stone-amber-ridge

              http://192.168.1.42:7185
                 Moss 0.2.1 · Up 47d

                    ────────────────

                       FOUNDATION

                  CPU     ████░░░░░░  38%
                  Memory  ██████░░░░  62%
                  Disk    ███░░░░░░░  28%

                    ────────────────

                        OFFERINGS

                  mongodb    ● healthy
                  redis      ● healthy
                  postgres   ● healthy

                    ────────────────

                       COMPANIONS

                  cricket    ● listening
                  firefly    ● connected

                    ────────────────

                       SEED BANKS

                  seed-amber-brook  32 GB / 64 GB

                    ────────────────

                        HORIZON

               stone-coral-reef  ● online
```

That's it. One page. Everything important about this Stone, updating in real-time.

---

You watch the Portrait throughout the day. Little things change:

- The CPU bar grows when you deploy something, then settles back down
- An offering briefly shows yellow while restarting, then returns to green
- The Horizon section shows stone-coral-reef going offline for a reboot, then coming back

You don't interact with it. You don't click anything. It's just there, showing you the state of this Stone, like a window into the machine.

---

A week later, you notice something odd on the Portrait. The memory bar is nearly full:

```
Memory  █████████░  89%
```

And one offering shows yellow:

```
mongodb    ◐ degraded
```

You investigate:

```bash
garden-rake status stone-amber-ridge
```

```
Health: Degraded
  ⚠️  High memory usage (89%)
  ⚠️  mongodb: Health check slow (>5s response)
```

MongoDB is struggling because of memory pressure. You free up some space by stopping an unused offering:

```bash
garden-rake rest grafana on stone-amber-ridge
```

Within a minute, the Portrait updates:

```
Memory  ███████░░░  68%

mongodb    ● healthy
```

The tablet on the wall told you something was wrong before your applications complained.

---

You set up Portraits for both Stones. Two tablets, side by side:

```
┌─────────────────────┐    ┌─────────────────────┐
│  stone-amber-ridge  │    │  stone-coral-reef   │
│                     │    │                     │
│  CPU    ███░░░  32% │    │  CPU    █░░░░   8%  │
│  Memory ███████ 68% │    │  Memory ████░░  42% │
│                     │    │                     │
│  mongodb   ●        │    │  nginx     ●        │
│  redis     ●        │    │  grafana   ●        │
│  postgres  ●        │    │                     │
└─────────────────────┘    └─────────────────────┘
```

Two windows into your infrastructure. A glance tells you: both Stones healthy, all services running, nothing needs attention.

---

## What Just Happened

### The Portrait Page

Every Stone running Moss serves a Portrait at `/portrait`. It's a single-page application that polls for data:

```
GET http://stone-amber-ridge:7185/portrait     → HTML page
GET http://stone-amber-ridge:7185/api/v1/stone/portrait  → JSON data
```

The HTML page is self-contained—no external dependencies, no CDN, no build step. Just HTML, CSS, and a bit of JavaScript that polls for updates every few seconds.

### The Data Model

The Portrait API returns a structured snapshot of the Stone:

```json
{
  "identity": {
    "stone_name": "stone-amber-ridge",
    "stone_id": "019bf83e-ec4d-7371-98f0-abc123",
    "role": "stone",
    "version": "0.2.1",
    "uptime_system": "47d 3h 22m",
    "uptime_moss": "47d 3h 20m",
    "endpoint": "http://192.168.1.42:7185",
    "color": "#84a59d"
  },
  "foundation": {
    "cpu_cores": 4,
    "cpu_percent": 38,
    "memory_total_gb": 8.0,
    "memory_used_gb": 5.0,
    "memory_percent": 62,
    "disk_total_gb": 256,
    "disk_used_gb": 72,
    "disk_percent": 28
  },
  "offerings": [
    {
      "name": "mongodb",
      "status": "running",
      "health": "healthy",
      "image": "mongo:7.0.8",
      "port": 27017
    },
    {
      "name": "redis",
      "status": "running",
      "health": "healthy",
      "image": "redis:7.2.5",
      "port": 6379
    }
  ],
  "seed_banks": [
    {
      "name": "seed-amber-brook",
      "capacity_gb": 64,
      "used_gb": 32,
      "status": "online"
    }
  ],
  "companions": [
    {
      "name": "cricket",
      "status": "listening",
      "port": 7187
    },
    {
      "name": "firefly",
      "status": "connected",
      "port": 7188
    }
  ],
  "horizon": [
    {
      "stone_name": "stone-coral-reef",
      "status": "online"
    }
  ]
}
```

Each section tells a different story:

| Section | What It Shows |
|---------|---------------|
| **Identity** | Who is this Stone? Name, version, uptime, endpoint |
| **Foundation** | What resources does it have? CPU, memory, disk |
| **Offerings** | What services are running? Status and health |
| **Seed Banks** | What backup storage is attached? Capacity |
| **Companions** | What Companions are connected? Cricket, Firefly |
| **Horizon** | What other Stones can it see? Network neighbors |

### The Visual Design

The Portrait uses a minimal aesthetic:

- **Vellum texture**: Subtle paper-like background with grain
- **Glassmorphism**: Cards with blur and transparency
- **Monospace accents**: Technical details in fixed-width font
- **Status indicators**: Colored dots (green/yellow/red) for health
- **Progress bars**: Simple blocks for resource usage
- **Dark mode**: Automatically adapts to system preference

It's designed to be beautiful enough to hang on a wall, functional enough to be useful.

### Real-Time Updates

The page polls `/api/v1/stone/portrait` every 5 seconds:

```javascript
setInterval(async () => {
  const response = await fetch('/api/v1/stone/portrait');
  const data = await response.json();
  updateDisplay(data);
}, 5000);
```

Changes animate smoothly. The CPU bar doesn't jump—it slides. Status changes fade in. The display feels alive without being distracting.

### Why Not a Dashboard?

You might wonder: why not use Grafana or another dashboard tool?

Dashboards are great for investigation. When something's wrong, you want graphs, logs, drill-downs. But dashboards require attention. You have to look at them and interpret them.

The Portrait is for awareness. It shows the *essence* of the Stone:
- Is it healthy? (offerings all green)
- Is it stressed? (CPU/memory bars high)
- Is it connected? (horizon shows neighbors)

One second tells you everything. If you need more detail, you open a terminal. But for the ambient "is everything okay?" check, the Portrait is enough.

### The Color

Each Stone gets a unique color derived from its name:

```rust
fn stone_color(name: &str) -> String {
    let hash = blake3::hash(name.as_bytes());
    let hue = (hash.as_bytes()[0] as f32 / 255.0) * 360.0;
    format!("hsl({}, 40%, 60%)", hue)
}
```

Stone-amber-ridge might be sage green. Stone-coral-reef might be warm terracotta. The color appears as an accent throughout the Portrait—the left border, status highlights, buttons.

This isn't just decoration. When you have multiple Portraits on a wall, the colors help you instantly identify which Stone you're looking at.

---

## Setting Up a Portrait Display

**Simple setup:**
1. Find an old tablet, phone, or small monitor
2. Open a browser
3. Navigate to `http://<stone-name>.local:7185/portrait`
4. Enable full-screen mode (usually F11)

**Auto-refresh considerations:**
- The page polls automatically—no browser refresh needed
- If the Stone reboots, the page shows a connection error until it's back
- Most browsers will reconnect automatically when the Stone returns

**Multiple Stones:**
- Each Portrait only shows one Stone
- For multiple Stones, use multiple browser tabs/windows or multiple screens
- Some kiosk software can tile multiple URLs

**Low-power options:**
- Kindle Fire tablets ($50, sideload a browser)
- Raspberry Pi with attached display
- Old phones in kiosk mode

The Portrait is lightweight—any device that runs a browser can display it.

---

## Commands From This Journey

```bash
# Open Portrait in browser (from any machine)
open http://stone-amber-ridge.local:7185/portrait

# Get Portrait data as JSON (for scripting)
curl http://stone-amber-ridge:7185/api/v1/stone/portrait

# Check Stone status from CLI
garden-rake status stone-amber-ridge

# List all offerings
garden-rake observe
```

---

*Zen Garden Documentation — Journeys*
