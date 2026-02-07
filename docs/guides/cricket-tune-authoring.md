# How to Create a Tune (Audio Bank)

**Version**: 0.1.0  
**Last Updated**: 2026-01-26

---

## Overview

A **tune** in Cricket is a curated audio bank that maps garden events to specific sound samples. Cricket ships with three default tunes:

- **zen-garden**: Calm, natural sounds (chimes, crickets, warm pads)
- **mr-robot**: Glitchy, digital sounds (error tones, industrial beats, dark drones)
- **lo-fi-ops**: Vinyl warmth, chill beats (crackle, lofi loops, soft pads)

This guide shows you how to create your own custom tune.

---

## Quick Start

**Prerequisites:**
- CC0 or properly licensed audio samples (MP3 format recommended)
- Samples organized by category

**Steps:**
1. Create sample directory structure
2. Add your samples to categories
3. Update Cricket's tune configuration
4. Test your tune

---

## Sample Directory Structure

Cricket expects samples in this structure:

```
samples/
└── your-tune-name/
    ├── notifications/       # Short alerts, chimes, beeps
    ├── ambient_nature/      # Looping background ambience
    ├── synth_pads/          # Continuous pads/drones
    ├── glitch_digital/      # Error sounds, glitches
    └── vinyl_lofi/          # Texture layers (crackle, etc.)
```

### Category Guidelines

| Category | Purpose | Examples | Loop? |
|----------|---------|----------|-------|
| `notifications/` | Event alerts | Chimes, beeps, bells | No |
| `ambient_nature/` | Background ambience | Crickets, rain, forest | Yes |
| `synth_pads/` | Continuous tones | Pads, drones, atmospheres | Yes |
| `glitch_digital/` | Digital feedback | Errors, glitches, blips | No |
| `vinyl_lofi/` | Texture layers | Vinyl crackle, tape hiss | Yes |

---

## File Requirements

### Format
- **Preferred**: MP3 (128kbps or higher)
- **Supported**: WAV, FLAC, OGG (via rodio)
- **Duration**: 0.3-10 seconds for events, any length for loops

### Naming Convention
Use descriptive, kebab-case names:
```
✓ stone-online-chime.mp3
✓ offering-planted-soft.mp3
✓ cricket-loop-stereo.mp3

✗ sound1.mp3
✗ MyChime.mp3
```

### Licensing
- **CC0 (Public Domain)**: Preferred
- **CC-BY**: Acceptable (provide attribution)
- **Commercial**: Verify license allows use

**Source Recommendations:**
- [Freesound.org](https://freesound.org) - Search with `license:\"Creative Commons 0\"`
- [BBC Sound Effects](https://sound-effects.bbcrewind.co.uk/) - All CC
- [Sonniss GDC Bundles](https://sonniss.com/gameaudiogdc) - Free commercial use

---

## Tune Configuration

Update `src/cricket/src/tune.rs` to add your tune:

### 1. Add Enum Variant

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tune {
    ZenGarden,
    MrRobot,
    LoFiOps,
    YourTuneName,  // Add here
}
```

### 2. Add String Mapping

```rust
impl Tune {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "zen-garden" => Some(Tune::ZenGarden),
            "mr-robot" => Some(Tune::MrRobot),
            "lo-fi-ops" => Some(Tune::LoFiOps),
            "your-tune-name" => Some(Tune::YourTuneName),  // Add here
            _ => None,
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            Tune::ZenGarden => "zen-garden",
            Tune::MrRobot => "mr-robot",
            Tune::LoFiOps => "lo-fi-ops",
            Tune::YourTuneName => "your-tune-name",  // Add here
        }
    }
}
```

### 3. Add Event Mappings

```rust
impl TuneManager {
    pub fn new(active_tune: Tune, sample_base_path: String) -> Self {
        let mut configs = HashMap::new();
        
        // ... existing tunes ...
        
        // Your custom tune
        let mut your_events = HashMap::new();
        your_events.insert("stone_online".to_string(), 
            "notifications/stone-online-chime.mp3".to_string());
        your_events.insert("stone_offline".to_string(), 
            "notifications/stone-offline-bell.mp3".to_string());
        your_events.insert("offering_planted".to_string(), 
            "notifications/offering-planted-soft.mp3".to_string());
        your_events.insert("offering_removed".to_string(), 
            "notifications/offering-removed-beep.mp3".to_string());
        your_events.insert("nourishment_available".to_string(), 
            "notifications/nourishment-alert.mp3".to_string());
        
        configs.insert(Tune::YourTuneName, TuneConfig {
            name: "Your Tune Display Name".to_string(),
            ambient_loop: "ambient_nature/your-ambient-loop.mp3".to_string(),
            background_pad: "synth_pads/your-pad-c.mp3".to_string(),
            event_sounds: your_events,
        });
        
        // ... rest of function ...
    }
}
```

---

## Event Reference

Cricket responds to these garden events:

| Event | When Triggered | Channel | Notes |
|-------|----------------|---------|-------|
| `stone_online` | Stone joins garden | Foreground | Welcome sound |
| `stone_offline` | Stone leaves garden | Foreground | Goodbye sound |
| `offering_planted` | New offering deployed | Midground | Confirmation |
| `offering_removed` | Offering removed | Midground | Removal notice |
| `nourishment_available` | Updates available | Foreground | Alert user |
| `election_started` | Leader election begins | Midground | Optional |
| `election_won` | This stone won election | Foreground | Optional |

**Custom Events**: You can add custom events using the `Custom(String)` variant in `tune.rs`.

---

## Testing Your Tune

### 1. Build Cricket
```bash
cargo build --package garden-cricket
```

### 2. Start Cricket with Your Tune
```bash
./target/debug/garden-cricket \
  --stone http://localhost:7185 \
  --samples ./samples/your-tune-name \
  --tune your-tune-name
```

### 3. Test Event Playback

Use Rake's hey-tell command:

```bash
# Play ambient loop
garden-rake hey-tell cricket, play zen-garden

# Manually trigger event sample
garden-rake hey-tell cricket, play stone-online-chime on foreground

# Adjust volume
garden-rake hey-tell cricket, volume 0.5

# Stop channel
garden-rake hey-tell cricket, stop ambient
```

### 4. Monitor Logs
```bash
RUST_LOG=info ./target/debug/garden-cricket ...
```

Look for:
```
[INFO] Playing zen-garden on Channel::Foreground
[INFO] Loaded sample: samples/your-tune-name/notifications/stone-online-chime.mp3
```

---

## Audio Design Tips

### 1. Consistent Aesthetic
Pick a cohesive theme:
- **Nature**: Organic, earthy sounds
- **Cyber**: Digital, synthetic sounds
- **Industrial**: Mechanical, metallic sounds
- **Retro**: 8-bit, chip-tune sounds

### 2. Volume Levels
- **Foreground**: Loudest (0.8-1.0) - demands attention
- **Midground**: Medium (0.5-0.7) - noticeable but not jarring
- **Ambient**: Low (0.3-0.5) - barely noticeable, constant
- **Background**: Very low (0.2-0.4) - subliminal texture

### 3. Frequency Balance
- **High frequencies** (chimes, bells): Attention-grabbing
- **Mid frequencies** (beeps, synths): Clear communication
- **Low frequencies** (drones, bass): Emotional atmosphere

### 4. Loop Design
For ambient/background loops:
- **Duration**: 10-60 seconds
- **Seamless**: Ensure loop point doesn't click
- **Subtle variation**: Avoid listener fatigue
- **No melodic content**: Use textures, not tunes

### 5. Sample Density
Don't overdo it:
- **Essential events**: stone_online, nourishment_available (always map)
- **Optional events**: election_started, election_won (can skip)
- **Leave silence**: Not every event needs a sound

---

## Example: Creating "Cyberpunk Noir"

### 1. Theme & Aesthetic
- Dark, dystopian cyberpunk
- Industrial machinery hums
- Digital glitches and errors
- Neon-lit ambience

### 2. Sample Selection
```
samples/cyberpunk-noir/
├── notifications/
│   ├── stone-online-hack.mp3        # Digital breach sound
│   ├── stone-offline-disconnect.mp3 # Power down glitch
│   ├── offering-planted-install.mp3 # Software install beep
│   └── nourishment-alert-scan.mp3   # Scanner alert
├── ambient_nature/
│   └── city-hum-loop.mp3            # Distant traffic, machinery
├── synth_pads/
│   └── dark-drone-am.mp3            # Ominous synthetic drone
└── glitch_digital/
    └── error-layer.mp3               # Occasional glitch texture
```

### 3. Configuration
```rust
let mut cyberpunk_events = HashMap::new();
cyberpunk_events.insert("stone_online".to_string(), 
    "notifications/stone-online-hack.mp3".to_string());
cyberpunk_events.insert("stone_offline".to_string(), 
    "notifications/stone-offline-disconnect.mp3".to_string());
cyberpunk_events.insert("offering_planted".to_string(), 
    "notifications/offering-planted-install.mp3".to_string());
cyberpunk_events.insert("nourishment_available".to_string(), 
    "notifications/nourishment-alert-scan.mp3".to_string());

configs.insert(Tune::CyberpunkNoir, TuneConfig {
    name: "Cyberpunk Noir".to_string(),
    ambient_loop: "ambient_nature/city-hum-loop.mp3".to_string(),
    background_pad: "synth_pads/dark-drone-am.mp3".to_string(),
    event_sounds: cyberpunk_events,
});
```

---

## Attribution

If using CC-BY licensed samples, create `attribution.json`:

```json
{
  "license": "CC-BY-4.0",
  "source": "Freesound.org",
  "samples": {
    "notifications": [
      {
        "filename": "stone-online-hack.mp3",
        "title": "Digital Breach",
        "author": "cybersound_artist",
        "source_url": "https://freesound.org/people/cybersound_artist/sounds/123456/",
        "attribution": "\"Digital Breach\" by cybersound_artist (CC-BY-4.0)"
      }
    ]
  }
}
```

---

## Sharing Your Tune

### 1. Package Structure
```
your-tune-name/
├── README.md              # Description, theme, credits
├── attribution.json       # Full credits (if CC-BY)
├── samples/
│   ├── notifications/
│   ├── ambient_nature/
│   └── synth_pads/
└── tune_config.rs.snippet # Code snippet to add to tune.rs
```

### 2. Submit to Community
- Open PR to `zen-garden` repo
- Tag as `enhancement: audio`
- Include demo video/audio preview

---

## Troubleshooting

### Sample Not Playing
```bash
# Check file exists
ls samples/your-tune-name/notifications/stone-online-chime.mp3

# Check Cricket logs
RUST_LOG=debug ./target/debug/garden-cricket ...
```

### Wrong Channel
Verify channel mapping in `tune.rs`:
- Foreground: Urgent alerts
- Midground: Confirmations
- Ambient: Loops (crickets)
- Background: Continuous texture (pads)

### Volume Too Loud/Quiet
Adjust in `mixer.rs` or use `set_channel_volume()`:
```rust
mixer.set_channel_volume(Channel::Ambient, 0.3).await;
```

---

## Reference Files

- [tune.rs](../../src/cricket/src/tune.rs) - Tune manager implementation
- [mixer.rs](../../src/cricket/src/mixer.rs) - 4-channel mixer
- [CRICKET-SPEC.md](../specs/cricket-spec.md) - Full Cricket specification
- [audio-sample-library.json](../specs/audio-sample-library.json) - Default sample catalog

---

## License Note

Cricket itself is licensed under your project's license. However:
- **Your samples** can use any license (CC0, CC-BY, commercial)
- **Attribution required** for CC-BY samples (include attribution.json)
- **Respect licensing** when sharing tunes publicly
