# The Sound of the Garden

*A chime plays. Something just happened.*

---

## The Story

You're in the kitchen making coffee when you hear it—a soft, digital chime from the other room. Not your phone. Not your computer. Something else.

You walk back to your desk. The two Stones sit on the shelf, LEDs blinking quietly. Nothing looks different. But something happened. You heard it.

---

A week ago, you plugged a small speaker into stone-amber-ridge and ran a command:

```bash
garden-rake hey tell cricket on
```

```
Cricket enabled. Listening to stone-amber-ridge events.
Active tune: zen-tech
Volume: 75%
```

That was it. You didn't configure anything else. Cricket—the audio Companion—started listening.

---

Now, making coffee, you heard the garden speak.

You check what happened:

```bash
garden-rake observe
```

```
●  stone-amber-ridge (192.168.1.42)
   OFFERINGS:
   └─ mongodb     Running   Healthy   27017

●  stone-coral-reef (192.168.1.58)
   OFFERINGS:
   └─ redis       Running   Healthy   6379
   └─ postgres    Running   Healthy   5432    ← New
```

Ah. Someone deployed Postgres to stone-coral-reef. The chime was Cricket announcing: "A service started."

---

You decide to test it. From your laptop:

```bash
garden-rake offer nginx on stone-coral-reef
```

A moment later, from the speaker on the shelf:

*ding*

A different sound than the first one—a short, bright synth note. You check the tune configuration:

```bash
garden-rake hey tell cricket show zen-tech
```

```
Tune: zen-tech
Description: Clean digital cues for garden stone presence

Event Mappings:
  presence.snapshot      → beep-positive.mp3          (foreground)
  service.started        → success-synth.mp3          (midground)
  service.stopped        → telephone-dock-beep.mp3    (midground)
  stone.health.changed   → beep-sound-onoff.mp3       (background)
  stone.tended           → wind-chime-single-04.mp3   (foreground)
  storage.detected       → computer-chimes.mp3        (foreground)
  storage.prepared       → success-synth.mp3          (foreground)
```

So `service.started` plays `success-synth.mp3`. That's the *ding* you heard.

---

You stop nginx:

```bash
garden-rake rest nginx on stone-coral-reef
```

From the speaker: a lower tone, like a phone docking.

You check the logs:

```bash
garden-rake watch stone-coral-reef
```

```
[10:42:15] service.stopped: nginx
[10:42:03] service.started: nginx
[10:41:58] service.started: postgres
```

Every event has a sound. The garden has a voice now.

---

That night, you're watching TV when the sound changes.

Not a chime. A different tone—lower, more insistent. You mute the TV and listen.

*beep... beep... beep...*

Something's wrong.

You grab your laptop:

```bash
garden-rake observe
```

```
●  stone-amber-ridge (192.168.1.42)
   OFFERINGS:
   └─ mongodb     Unhealthy   ⚠️

○  stone-coral-reef (offline)
```

Stone-coral-reef is gone. And MongoDB on stone-amber-ridge is unhealthy. The sound was Cricket saying: "Health changed. Pay attention."

You check the Stone:

```bash
garden-rake status stone-amber-ridge
```

```
Health: Degraded
  ⚠️  mongodb: Health check failing
      Last error: Connection refused

Disk: 223 GB (2% free)  ← Problem
```

The disk filled up. MongoDB can't write. You clear some old Docker images:

```bash
ssh stone@stone-amber-ridge "docker system prune -af"
```

A moment later:

*ding*

The happy sound. You check:

```bash
garden-rake observe
```

```
●  stone-amber-ridge (192.168.1.42)
   OFFERINGS:
   └─ mongodb     Running   Healthy   27017
```

MongoDB recovered. The garden told you there was a problem, you fixed it, and the garden told you it was better.

---

A month later, you buy a second tune:

```bash
garden-rake hey tell cricket list
```

```
Available Tunes:
  zen-tech     Clean digital cues (active)
  mr-robot     Cyberpunk aesthetic
```

You switch:

```bash
garden-rake hey tell cricket select mr-robot
```

```
Switched to tune: mr-robot
```

Now the sounds are different. Grittier. More electronic. The service-started sound has a slight distortion. The health warning sounds like something from a hacker movie.

Same events. Different aesthetic. The garden speaks in a different voice.

---

## What Just Happened

### The SSE Subscription

When you ran `cricket on`, Cricket connected to Moss via Server-Sent Events (SSE):

```
GET http://stone-amber-ridge:7185/api/v1/stone/presence/stream
Accept: text/event-stream
```

This opened a persistent connection. Moss sends events down this stream whenever something happens:

```
event: service.started
data: {"offering": "nginx", "stone": "stone-coral-reef", "timestamp": "..."}

event: stone.health.changed
data: {"stone": "stone-amber-ridge", "health": "degraded", "reason": "..."}
```

Cricket receives these events and decides what to play based on the active tune's configuration.

### The Tune System

A tune is a YAML file that maps events to sounds:

```yaml
# zen-tech/tune.yaml
name: zen-tech
version: "1.0.0"
description: Clean digital cues for garden stone presence

events:
  service.started:
    resource: samples/success-synth.mp3
    channel: midground
    debounce_ms: 2000

  service.stopped:
    resource: samples/telephone-dock-beep.mp3
    channel: midground
    debounce_ms: 2000

  stone.health.changed:
    resource: samples/beep-sound-onoff.mp3
    channel: background
    debounce_ms: 5000
```

Each event maps to:
- **resource**: The audio file to play
- **channel**: Which mixer channel (foreground, midground, ambient, background)
- **debounce_ms**: Minimum time between plays (prevents sound spam)

The channels have different purposes:
- **Foreground**: Important alerts, played at full volume
- **Midground**: Notifications, slightly quieter
- **Ambient**: Background atmosphere
- **Background**: Persistent soundscapes

### The Event Handler

When Cricket receives an event, it follows a simple flow:

```rust
async fn on_event(&self, event: SseEvent) {
    // 1. Look up event in active tune
    let Some(mapping) = self.tune.get_event_mapping(&event.event_type) else {
        return; // No mapping for this event
    };

    // 2. Check debounce
    if self.recently_played(&event.event_type, mapping.debounce_ms) {
        return; // Too soon since last play
    }

    // 3. Load and play the sound
    let sound = self.load_resource(&mapping.resource)?;
    self.mixer.play_on_channel(mapping.channel, sound);

    // 4. Record play time for debounce
    self.record_play(&event.event_type);
}
```

The handler is generic—it doesn't know what `service.started` means. It just looks up the event type in the tune's mapping and plays the corresponding sound. This means new events can be supported just by adding them to the tune file.

### The 4-Channel Mixer

Cricket runs a real-time audio mixer with four channels. Each channel can play independently:

```
Foreground ─────┐
                │
Midground  ─────┼──► Mixer ──► Output (speakers)
                │
Ambient    ─────┤
                │
Background ─────┘
```

When a service starts (midground) while an ambient loop is playing (ambient), both sounds play together. The mixer handles volume levels so nothing clips.

### The Debounce

The `debounce_ms` setting prevents sound spam. If you deploy 5 services at once, you don't want 5 overlapping *dings*. With a 2000ms debounce on `service.started`:

```
10:00:00.000  service.started (nginx)     → plays
10:00:00.500  service.started (redis)     → skipped (within 2000ms)
10:00:01.000  service.started (postgres)  → skipped (within 2000ms)
10:00:02.100  service.started (mongodb)   → plays (debounce expired)
```

Different events can have different debounce times. Health warnings might have a longer debounce (5 seconds) so they don't constantly beep during a flapping failure.

### State Persistence

Cricket remembers its state across restarts:

```json
// ~/.config/cricket/state.json
{
  "enabled": true,
  "active_tune": "zen-tech",
  "volume": 75
}
```

If the Stone reboots, Cricket comes back up with the same tune and volume. You don't have to reconfigure it.

### Why Audio?

You might wonder: why audio? Why not just check a dashboard?

The answer is **ambient awareness**. A dashboard requires you to look at it. Audio reaches you wherever you are—making coffee, watching TV, sleeping in the next room.

The garden speaks quietly when things are normal (occasional chimes as services start and stop). It speaks urgently when things are wrong (repeated warning tones). You learn to hear the difference without thinking about it.

This is physical infrastructure making itself present. Not through screens, but through the air.

---

## The Sound You Learn

After a few weeks with Cricket, something changes in how you relate to your infrastructure.

You're in the shower and hear a chime. You know, without checking, that the backup just finished—that's the `storage.prepared` sound.

You're falling asleep and hear nothing. That's good. Silence means health.

You're cooking dinner and hear that warning tone. You know something needs attention, but it's not urgent—it's the background channel, not foreground.

The sounds become a language. The garden speaks, and you understand.

---

## Commands From This Journey

```bash
# Enable Cricket
garden-rake hey tell cricket on

# Disable Cricket
garden-rake hey tell cricket off

# Set volume (0-100)
garden-rake hey tell cricket volume 75

# List available tunes
garden-rake hey tell cricket list

# Switch tune
garden-rake hey tell cricket select mr-robot

# Show tune event mappings
garden-rake hey tell cricket show zen-tech

# Test play a specific event
garden-rake hey tell cricket play service.started

# Stop all sounds
garden-rake hey tell cricket stop
```

---

*Zen Garden Documentation — Journeys*
