# The Lights on the Shelf

*A soft green glow means everything is healthy.*

---

## The Story

The package arrives on a Tuesday. Inside: a tiny circuit board, barely bigger than a postage stamp. A 5x5 grid of LEDs. A USB-C cable.

You plug it into stone-amber-ridge and run:

```bash
garden-rake hey tell firefly on
```

```
Firefly enabled. Listening to stone-amber-ridge events.
Brightness: 30%
Device: Waveshare RP2040-Matrix (/dev/ttyACM0)
```

The LEDs light up. Not all at once—they fade in softly, like fireflies appearing at dusk. Warm white dots drift across the grid, appearing and disappearing in a slow, organic rhythm.

It's beautiful. And completely useless, you think. Then you start noticing things.

---

A week later, you're walking past your desk when something catches your eye. The Firefly is pulsing amber.

You stop. You haven't looked at a dashboard. You haven't checked any logs. But that amber pulse means something.

```bash
garden-rake status stone-amber-ridge
```

```
Health: Degraded
  ⚠️  High memory usage (89%)

Offerings:
  mongodb     Running   Healthy
  wiki        Running   Healthy

Memory: 8 GB total, 7.1 GB used
```

Memory pressure. Not critical—the Stone is still working—but something to watch. The amber pulse said "pay attention" without demanding immediate action.

You add more swap space. The pulsing slows. An hour later, it's back to gentle green.

---

That night, you deploy a new service:

```bash
garden-rake offer grafana on stone-amber-ridge
```

As the container starts, you watch the Firefly from across the room. A bright green dot blooms in the center of the grid, then fades. Like a flower opening and closing.

That's the "service started" animation. You didn't have to check the terminal. The light told you.

---

Your partner walks by and asks, "What's that little glowing thing?"

"It shows how my servers are doing."

"It's pretty."

That's the point. Infrastructure doesn't have to be ugly. The Firefly sits on the shelf like a piece of ambient art. When you want to know what's happening, you look at it. When you don't, it's just a gentle glow in the corner of your vision.

---

Months later, you're watching a movie. The Firefly is in your peripheral vision, doing its slow green dance.

Then it turns red.

Not amber—red. Pulsing fast. You pause the movie.

```bash
garden-rake observe
```

```
●  stone-amber-ridge (192.168.1.42)
   Health: Critical ⚠️

   OFFERINGS:
   ├─ mongodb     FAILED   Exit code 137 (OOM killed)
   ├─ wiki        Running  Healthy
   └─ grafana     Running  Healthy
```

MongoDB got killed by the kernel. Out of memory. The fast red pulse was Firefly screaming: "Something is very wrong."

You restart MongoDB with a memory limit. The red fades to amber, then slowly back to green as health checks pass.

The movie can wait. Your infrastructure needed you.

---

## What Just Happened

### The Hardware

Firefly uses a Waveshare RP2040-Matrix—a tiny board with a 5x5 grid of RGB LEDs:

```
┌─────────────────────────────────────┐
│                                     │
│   ○ ○ ○ ○ ○   ← 5 LEDs per row     │
│   ○ ○ ○ ○ ○                        │
│   ○ ○ ○ ○ ○   25 individually      │
│   ○ ○ ○ ○ ○   addressable LEDs     │
│   ○ ○ ○ ○ ○                        │
│                                     │
│   [USB-C]     23.5mm × 18mm        │
└─────────────────────────────────────┘
```

It costs about $15. It plugs in via USB. That's the entire hardware setup.

### The Baseline Animation

When Firefly is running and the Stone is healthy, it displays a "fireflies in the garden" animation:

```
1. Select random pixel (x, y)
2. Fade that pixel from 0% → 100% → 0% brightness
3. Duration: 2-4 seconds per firefly
4. Spawn rate: Based on Stone load
   - Idle (< 10% CPU): 1-2 fireflies at a time
   - Active (10-50% CPU): 3-5 fireflies
   - Busy (> 50% CPU): 5-8 fireflies
5. Repeat forever
```

The animation is driven by real metrics. A busy Stone has more "fireflies" dancing. An idle Stone has a few gentle sparks. The display encodes load visually.

### The Color Language

Firefly uses color to indicate health:

| Color | Meaning | State |
|-------|---------|-------|
| Warm white | Normal operation | Thriving |
| Living green | Service activity | Active |
| Amber | Degraded | Withering |
| Coral red | Critical | Wilting |
| Dim blue | Connection lost | Unknown |
| Off | Stone offline | Offline |

The colors aren't arbitrary—they follow the garden metaphor. A thriving garden has warm, healthy colors. A withering garden turns amber. A wilting garden shows distress.

### The Override Layer

When specific events happen, Firefly overlays animations on top of the baseline:

| Event | Animation | Duration |
|-------|-----------|----------|
| Service started | Green bloom (center → edges) | 2 seconds |
| Service stopped | Brief dim, then return | 1 second |
| Stone tended | Sparkle cascade | 3 seconds |
| Health degraded | Pulse amber | Until resolved |
| Health critical | Fast pulse red | Until resolved |
| Seed bank detected | Green firefly joins | Persistent |

The override layer is priority-based. A critical health warning overrides a service-started bloom. The most important information always shows.

### The SSE Subscription

Like Cricket, Firefly subscribes to Moss events via Server-Sent Events:

```
GET http://localhost:7185/api/v1/stone/presence/stream
Accept: text/event-stream
```

Events flow continuously:

```
event: stone.health.changed
data: {"health": "degraded", "reason": "memory_pressure"}

event: service.started
data: {"offering": "grafana", "health": "healthy"}

event: stone.load.updated
data: {"cpu_percent": 45, "memory_percent": 89}
```

Firefly translates these events into visual changes. Health changed to degraded? Shift baseline color to amber. Service started? Play bloom animation. Load updated? Adjust firefly spawn rate.

### The Serial Protocol

Firefly sends commands to the LED matrix over USB serial:

```
Command format: <opcode> <args...> \n

Examples:
  P 2 3 255 128 0      # Set pixel (2,3) to RGB(255,128,0)
  F 64 255 64          # Fill all pixels with RGB(64,255,64)
  B 30                 # Set brightness to 30%
  C                    # Clear (all off)
```

The protocol is simple because the RP2040 is simple. Complex animations are computed on the Stone; only final pixel values are sent to the device.

### State Persistence

Firefly remembers its settings:

```json
// ~/.config/firefly/state.json
{
  "enabled": true,
  "brightness": 30,
  "last_device": "/dev/ttyACM0"
}
```

If the Stone reboots, Firefly comes back with the same brightness. You set it once and forget.

### Why 5x5?

You might wonder: why such a small display? 25 pixels isn't much.

That's the point. A small display forces simplicity. You can't show detailed graphs on 25 pixels. You can't display text. You can only show *essence*: healthy or not, busy or idle, attention needed or not.

This constraint is a feature. The Firefly answers one question: "Is everything okay?" If you need details, you check a dashboard. If you just want ambient awareness, you glance at the lights.

---

## The Glance Test

The best way to understand Firefly is the "glance test": look at it for one second, then look away. What did you learn?

- **Warm white, slow sparkles** → Everything's fine, low load
- **Green sparkles, faster movement** → Healthy but busy
- **Amber pulse** → Something needs attention soon
- **Red flash** → Something needs attention now
- **Blue flicker** → Can't reach the Stone (network issue?)
- **Off** → Stone is down

One second. No dashboards. No logins. Just look at the shelf.

This is "glanceable" design. The display rewards brief attention with useful information. You don't have to study it—a quick glance tells you what you need to know.

---

## Placement Ideas

Firefly works best when it's in your peripheral vision:

- **On a shelf** near your desk—you'll notice color changes without looking directly
- **On top of the Stone** itself—the hardware becomes its own status indicator
- **In a hallway** you walk through often—a daily health check without trying
- **On a nightstand** (at low brightness)—wake up to red if something went wrong overnight

The goal is passive awareness. You're not monitoring infrastructure; infrastructure is making itself visible to you.

---

## Commands From This Journey

```bash
# Enable Firefly
garden-rake hey tell firefly on

# Disable Firefly
garden-rake hey tell firefly off

# Set brightness (0-100)
garden-rake hey tell firefly brightness 30

# Manual status colors
garden-rake hey tell firefly status healthy
garden-rake hey tell firefly status warning
garden-rake hey tell firefly status error

# Direct pixel control
garden-rake hey tell firefly pixel 2 2 ff0000    # Red pixel at (2,2)
garden-rake hey tell firefly fill 00ff00          # All green
garden-rake hey tell firefly clear                # All off

# Built-in animations
garden-rake hey tell firefly animate rainbow
garden-rake hey tell firefly animate sparkle
garden-rake hey tell firefly stop

# Device info
garden-rake hey tell firefly info
```

---

*Zen Garden Documentation — Journeys*
