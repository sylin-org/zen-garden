# When the Garden Speaks

*The garden is always talking. You just need to listen.*

---

## The Story

You've noticed how Cricket plays sounds when services start or stop. How Firefly's LEDs pulse when a Stone gets busy. How the Portrait page updates in real-time without refreshing.

Where does this information come from?

You open a terminal and connect to the presence stream:

```bash
curl -N http://stone-amber-ridge.local:7185/api/v1/stone/presence/stream
```

Events start flowing:

```
event: presence.snapshot
data: {"stone":{"name":"stone-amber-ridge","health":"thriving"},"services":[...],"timestamp":"2026-01-30T14:32:00Z"}

event: service.health.changed
data: {"timestamp":"2026-01-30T14:32:15Z","service":"mongodb","health":"healthy"}

event: stone.load.updated
data: {"timestamp":"2026-01-30T14:32:45Z","cpu_percent":12.5,"memory_percent":45.2}

event: presence.heartbeat
data: {"timestamp":"2026-01-30T14:33:01Z","stone_health":"thriving","service_count":3}
```

The garden is talking. A steady stream of events describing everything that happens. Health checks. Chirps. Service state changes. Resource updates.

---

You leave the stream running. A few minutes later, you deploy a new service from another terminal:

```bash
garden-rake offer postgres
```

Back in your event stream:

```
event: moss-event
data: {"timestamp":"2026-01-30T14:35:22Z","level":"info","message":"Deployment started: postgres"}

event: moss-event
data: {"timestamp":"2026-01-30T14:35:24Z","level":"info","message":"Pulling image: postgres:16"}

event: moss-event
data: {"timestamp":"2026-01-30T14:35:45Z","level":"info","message":"Image pulled: postgres:16"}

event: moss-event
data: {"timestamp":"2026-01-30T14:35:46Z","level":"info","message":"Creating container: zen-offering-postgres"}

event: moss-event
data: {"timestamp":"2026-01-30T14:35:47Z","level":"info","message":"Container created: zen-offering-postgres"}

event: moss-event
data: {"timestamp":"2026-01-30T14:35:48Z","level":"info","message":"Starting container: zen-offering-postgres"}

event: moss-event
data: {"timestamp":"2026-01-30T14:35:49Z","level":"info","message":"Waiting for health check: postgres"}

event: moss-event
data: {"timestamp":"2026-01-30T14:35:55Z","level":"info","message":"Health check passed: postgres"}

event: moss-event
data: {"timestamp":"2026-01-30T14:35:55Z","level":"info","message":"Deployment completed: postgres"}
```

You watched the entire deployment happen in real-time. Each step. Each stage. No polling. No refreshing. Just events streaming as they occur.

---

You want to see what the Companions see. There's a different endpoint for presence events:

```bash
curl -N 'http://stone-amber-ridge.local:7185/api/v1/presence/stream?Companion=my-test'
```

First, you get a snapshot of the current state:

```
event: presence.snapshot
data: {"stone":{"name":"stone-amber-ridge","health":"thriving","cpu_percent":12,"memory_percent":45,"disk_percent":28},"offerings":[{"name":"mongodb","status":"running","health":"healthy"},{"name":"redis","status":"running","health":"healthy"},{"name":"postgres","status":"running","health":"healthy"}]}
```

Then, as things change:

```
event: stone.load.updated
data: {"stone":"stone-amber-ridge","cpu_percent":35,"memory_percent":52}

event: offering.health.changed
data: {"offering":"postgres","old_health":"healthy","new_health":"degraded"}

event: stone.load.updated
data: {"stone":"stone-amber-ridge","cpu_percent":18,"memory_percent":48}

event: offering.health.changed
data: {"offering":"postgres","old_health":"degraded","new_health":"healthy"}
```

The postgres health blipped briefly—probably during a vacuum or heavy query. The presence stream captured it.

---

You write a simple script to react to events:

```python
#!/usr/bin/env python3
import requests
import json

def watch_garden():
    url = "http://stone-amber-ridge.local:7185/api/v1/stone/presence/stream"

    with requests.get(url, stream=True) as response:
        for line in response.iter_lines():
            if line:
                line = line.decode('utf-8')
                if line.startswith('data: '):
                    event = json.loads(line[6:])
                    handle_event(event)

def handle_event(event):
    message = event.get('message', '')
    level = event.get('level', 'info')

    if 'failed' in message.lower():
        send_notification(f"⚠️ Garden alert: {message}")
    elif 'completed' in message.lower():
        send_notification(f"✅ {message}")

def send_notification(text):
    # Your notification logic here
    print(text)

watch_garden()
```

Now you get push notifications when deployments complete or something fails. No polling interval. No delay. Events flow the moment they happen.

---

## What Just Happened

### Server-Sent Events (SSE)

The garden uses SSE—a simple, one-way streaming protocol over HTTP. Unlike WebSockets, SSE is:

- **HTTP-based**: Works through proxies and firewalls
- **Text-only**: Easy to debug with curl
- **Auto-reconnecting**: Browsers handle disconnects
- **Unidirectional**: Server to client only (garden to you)

The wire format:

```
event: event-type
data: {"json":"payload"}

event: another-event
data: {"more":"data"}

```

Each message ends with a blank line. The `event:` line names the event type. The `data:` line contains the payload.

### Event Categories

The garden emits events across several categories:

**Service Lifecycle:**
```
service.started      Service container started
service.stopped      Service container stopped
service.health       Health check result
service.deployed     New service deployed
service.removed      Service removed
```

**Stone Events:**
```
stone.load.updated   CPU/memory/disk changed
stone.health.changed Stone health changed
stone.tended         Admin interacted with stone
```

**Discovery:**
```
stone.discovered     New stone found via chirp
stone.lost           Stone stopped responding
stone.goodbye        Stone announced shutdown
```

**Jobs:**
```
job.started          Background job started
job.progress         Job progress update
job.completed        Job finished successfully
job.failed           Job failed
```

### The Event Bus

Inside Moss, events flow through a broadcast channel:

```
┌─────────────────────────────────────────────────────────────────┐
│  EVENT BUS (tokio broadcast channel, capacity: 256)              │
│                                                                 │
│  Publishers:                    Subscribers:                     │
│  ├─ Health Monitor      ───────►├─ SSE Listener (HTTP clients)  │
│  ├─ Container Watcher   ───────►├─ Chirp Listener (UDP gossip)  │
│  ├─ Discovery Handler   ───────►├─ Cricket Companion              │
│  ├─ Job Executor        ───────►├─ Firefly Companion              │
│  └─ Metrics Collector   ───────►├─ Timer Listener (schedules)   │
│                                 └─ Your custom listener          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

Multiple publishers emit events. Multiple subscribers receive them. The broadcast channel ensures every subscriber gets every event (within buffer limits).

### Backpressure

What happens if a subscriber is slow? With 256 events in the buffer, fast publishers don't block. But if a subscriber falls behind:

```
event: moss-event
data: {"level":"warn","message":"Subscriber lagged, skipped 47 events"}
```

The subscriber gets a lag notification and continues with new events. This prevents slow subscribers from affecting the garden's operation.

### Unified Presence Stream

All events flow through a single endpoint:

**`/api/v1/stone/presence/stream`** — Unified event stream
- Starts with full state snapshot (`presence.snapshot`)
- All domain events: services, storage, stone health, jobs
- Regular heartbeats for connection health
- Designed for all consumers: Companions, dashboards, scripts
- Filter by category if needed (`?categories=service,job`)

This unified architecture means Cricket, Firefly, the portrait page, and your monitoring scripts all connect to the same stream. Everyone sees the same events, enabling richer integrations.

### How Companions Use This

Cricket subscribes to the presence stream and maps events to sounds:

```rust
async fn on_event(&self, event: SseEvent) {
    match event.event_type.as_str() {
        "service.started" => {
            let name = parse_service_name(&event.data);
            self.play_tune(&format!("{}-online", name)).await;
        }
        "stone.health.changed" => {
            let health = parse_health(&event.data);
            if health == "withering" {
                self.play_tune("alert-warning").await;
            }
        }
        _ => {} // Ignore other events
    }
}
```

Firefly does the same but with LED animations:

```rust
async fn on_event(&self, event: SseEvent) {
    match event.event_type.as_str() {
        "stone.load.updated" => {
            let load = parse_load(&event.data);
            self.set_brightness(load.cpu_percent).await;
        }
        "presence.snapshot" => {
            let state = parse_snapshot(&event.data);
            self.initialize_display(&state).await;
        }
        _ => {}
    }
}
```

The event stream is how the garden's physical presence—sounds, lights, displays—stays synchronized with its digital state.

### Building Your Own Listener

You can build Companions that react to garden events:

```python
import sseclient
import requests

def create_listener(stone_url, on_event):
    """Subscribe to garden events and call on_event for each."""
    url = f"{stone_url}/api/v1/presence/stream"

    while True:
        try:
            response = requests.get(url, stream=True)
            client = sseclient.SSEClient(response)

            for event in client.events():
                on_event(event.event, event.data)

        except requests.exceptions.ConnectionError:
            time.sleep(5)  # Reconnect after 5 seconds

# Example: Log all service events
def my_handler(event_type, data):
    if event_type.startswith('service.'):
        print(f"{event_type}: {data}")

create_listener("http://stone-amber-ridge.local:7185", my_handler)
```

The SSE protocol handles reconnection gracefully. Your listener just needs to process events and handle connection drops.

---

## The Garden's Voice

Events are how the garden expresses itself. When you hear a sound from Cricket or see Firefly's LEDs change, that's the garden communicating.

But you don't need physical Companions to listen. A simple `curl` command opens a window into everything happening. Scripts can react to events. Dashboards can update in real-time. Alerts can fire instantly.

The garden is always speaking. You just need to listen.

---

## Commands From This Journey

```bash
# Stream all events from a Stone (unified presence stream)
curl -N http://stone-amber-ridge.local:7185/api/v1/stone/presence/stream

# Identify yourself as a Companion
curl -N 'http://stone-amber-ridge.local:7185/api/v1/stone/presence/stream?companion=my-listener'

# Watch events with garden-rake
garden-rake presence watch

# Stream events in JSON Lines format (for parsing)
curl -N http://stone-amber-ridge.local:7185/api/v1/stone/presence/stream | grep '^data: ' | cut -c7-

# List connected presence subscribers
curl http://stone-amber-ridge.local:7185/api/v1/stone/presence/subscribers
```

---

*Zen Garden Documentation — Journeys*
