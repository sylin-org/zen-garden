# Cricket Implementation Specification

**Status:** Draft  
**Date:** 2026-01-26  
**Scope:** Cricket audio presence Companion implementation details

---

## Overview

Cricket is an audio presence Companion that transforms Zen Garden events into ambient soundscapes. It subscribes to the PRESENCE-0001 SSE stream and plays contextual audio based on infrastructure activity.

**Binary:** `garden-cricket`  
**Package:** `garden-cricket`  
**Systemd Unit:** `garden-cricket.service`

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│  garden-cricket                                                    │
│                                                                    │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────────┐ │
│  │   SSE        │    │   Command    │    │   Audio Engine       │ │
│  │   Client     │───▶│   Handler    │───▶│   (rodio)            │ │
│  └──────────────┘    └──────────────┘    └──────────────────────┘ │
│         │                   │                      │              │
│         │                   │                      ▼              │
│         │                   │            ┌──────────────────────┐ │
│         │                   │            │   4-Channel Mixer    │ │
│         │                   │            │                      │ │
│         │                   │            │  ┌─────────────────┐ │ │
│         ▼                   ▼            │  │   foreground    │ │ │
│  ┌──────────────┐    ┌──────────────┐    │  │   midground     │ │ │
│  │   Event      │    │   Tune       │    │  │   ambient       │ │ │
│  │   Router     │    │   Manager    │    │  │   background    │ │ │
│  └──────────────┘    └──────────────┘    │  └─────────────────┘ │ │
│                                          └──────────────────────┘ │
└────────────────────────────────────────────────────────────────────┘
```

---

## Component Details

### SSE Client

Connects to Moss presence stream and receives events.

```rust
pub struct SseClient {
    endpoint: String,
    reconnect_delay: Duration,
}

impl SseClient {
    pub async fn connect(&self) -> Result<EventStream> {
        let url = format!("{}/api/v1/stone/presence/stream", self.endpoint);
        
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("Accept", "text/event-stream")
            .send()
            .await?;
        
        Ok(EventStream::from_response(response))
    }
    
    pub async fn run(&self, tx: mpsc::Sender<PresenceEvent>) -> Result<()> {
        loop {
            match self.connect().await {
                Ok(mut stream) => {
                    while let Some(event) = stream.next().await {
                        match event {
                            Ok(sse) => {
                                if let Ok(presence) = parse_presence_event(&sse) {
                                    let _ = tx.send(presence).await;
                                }
                            }
                            Err(e) => {
                                tracing::warn!("SSE error: {}", e);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to connect: {}", e);
                }
            }
            
            tokio::time::sleep(self.reconnect_delay).await;
        }
    }
}
```

### Event Router

Maps presence events to audio actions.

```rust
pub struct EventRouter {
    tune: Arc<RwLock<Tune>>,
    mixer: Arc<Mixer>,
}

impl EventRouter {
    pub async fn handle_event(&self, event: PresenceEvent) {
        let tune = self.tune.read().await;
        
        match event {
            PresenceEvent::StoneHeartbeat { stone, .. } => {
                // Update ambient layer based on fleet health
                if let Some(mapping) = tune.get_mapping("heartbeat") {
                    self.mixer.play_on_channel(Channel::Ambient, &mapping).await;
                }
            }
            
            PresenceEvent::OfferingStateChange { offering, old, new, .. } => {
                // Play foreground sound for state changes
                let key = format!("offering.{}.{}", old, new);
                if let Some(mapping) = tune.get_mapping(&key) {
                    self.mixer.play_on_channel(Channel::Foreground, &mapping).await;
                }
            }
            
            PresenceEvent::OfferingResourceSpike { offering, metric, .. } => {
                // Modulate midground based on resource usage
                let key = format!("spike.{}", metric);
                if let Some(mapping) = tune.get_mapping(&key) {
                    self.mixer.play_on_channel(Channel::Midground, &mapping).await;
                }
            }
            
            PresenceEvent::DeploymentStarted { offering, .. } => {
                if let Some(mapping) = tune.get_mapping("deployment.started") {
                    self.mixer.play_on_channel(Channel::Foreground, &mapping).await;
                }
            }
            
            PresenceEvent::DeploymentCompleted { offering, success, .. } => {
                let key = if success { 
                    "deployment.success" 
                } else { 
                    "deployment.failed" 
                };
                if let Some(mapping) = tune.get_mapping(key) {
                    self.mixer.play_on_channel(Channel::Foreground, &mapping).await;
                }
            }
            
            // ... more event handlers
        }
    }
}
```

### Audio Engine (rodio)

```rust
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::collections::HashMap;

pub struct Mixer {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    channels: HashMap<Channel, Sink>,
    samples: HashMap<String, Arc<Vec<u8>>>,
    master_volume: f32,
}

#[derive(Clone, Copy, Hash, Eq, PartialEq)]
pub enum Channel {
    Foreground,  // Discrete events (notifications, alerts)
    Midground,   // Activity indicators (CPU, network)
    Ambient,     // Continuous texture (fleet health)
    Background,  // Base layer (always playing)
}

impl Mixer {
    pub fn new() -> Result<Self> {
        let (stream, handle) = OutputStream::try_default()?;
        
        let mut channels = HashMap::new();
        for ch in [Channel::Foreground, Channel::Midground, 
                   Channel::Ambient, Channel::Background] {
            let sink = Sink::try_new(&handle)?;
            channels.insert(ch, sink);
        }
        
        Ok(Self {
            _stream: stream,
            handle,
            channels,
            samples: HashMap::new(),
            master_volume: 0.5,
        })
    }
    
    pub fn load_sample(&mut self, name: &str, path: &Path) -> Result<()> {
        let data = std::fs::read(path)?;
        self.samples.insert(name.to_string(), Arc::new(data));
        Ok(())
    }
    
    pub async fn play_on_channel(&self, channel: Channel, mapping: &SampleMapping) {
        let Some(data) = self.samples.get(&mapping.sample) else {
            return;
        };
        
        let cursor = Cursor::new(data.clone());
        let source = rodio::Decoder::new(cursor)
            .expect("Failed to decode sample")
            .amplify(mapping.volume * self.master_volume);
        
        if let Some(sink) = self.channels.get(&channel) {
            if mapping.loop_sample {
                sink.append(source.repeat_infinite());
            } else {
                sink.append(source);
            }
        }
    }
    
    pub fn set_channel_volume(&self, channel: Channel, volume: f32) {
        if let Some(sink) = self.channels.get(&channel) {
            sink.set_volume(volume * self.master_volume);
        }
    }
    
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
        for sink in self.channels.values() {
            sink.set_volume(self.master_volume);
        }
    }
    
    pub fn stop_channel(&self, channel: Channel) {
        if let Some(sink) = self.channels.get(&channel) {
            sink.stop();
        }
    }
    
    pub fn stop_all(&self) {
        for sink in self.channels.values() {
            sink.stop();
        }
    }
}
```

### Tune Manager

```rust
pub struct TuneManager {
    official_dir: PathBuf,   // /usr/share/garden-cricket/tunes/
    community_dir: PathBuf,  // /etc/zen-garden/cricket/tunes/
    current: Arc<RwLock<Option<Tune>>>,
}

impl TuneManager {
    pub fn list_tunes(&self) -> Vec<TuneInfo> {
        let mut tunes = Vec::new();
        
        // Official tunes
        if let Ok(entries) = std::fs::read_dir(&self.official_dir) {
            for entry in entries.flatten() {
                if let Some(tune) = self.load_tune_info(&entry.path(), true) {
                    tunes.push(tune);
                }
            }
        }
        
        // Community tunes
        if let Ok(entries) = std::fs::read_dir(&self.community_dir) {
            for entry in entries.flatten() {
                if let Some(tune) = self.load_tune_info(&entry.path(), false) {
                    tunes.push(tune);
                }
            }
        }
        
        tunes
    }
    
    pub async fn select(&self, name: &str, mixer: &Mixer) -> Result<()> {
        // Find tune
        let tune = self.find_tune(name)?;
        
        // Stop current
        mixer.stop_all();
        
        // Load samples
        for (alias, path) in &tune.resources {
            mixer.load_sample(alias, &tune.base_path.join(path))?;
        }
        
        // Start background layer
        if let Some(bg) = tune.channels.get(&Channel::Background) {
            for sample in &bg.samples {
                mixer.play_on_channel(Channel::Background, sample).await;
            }
        }
        
        // Start ambient layer
        if let Some(ambient) = tune.channels.get(&Channel::Ambient) {
            for sample in &ambient.samples {
                mixer.play_on_channel(Channel::Ambient, sample).await;
            }
        }
        
        *self.current.write().await = Some(tune);
        
        Ok(())
    }
    
    pub async fn pull(&self, url: &str) -> Result<TuneInfo> {
        // Download
        let response = reqwest::get(url).await?;
        let bytes = response.bytes().await?;
        
        // Extract to temp dir
        let temp_dir = tempfile::tempdir()?;
        let archive_path = temp_dir.path().join("tune.tar.gz");
        std::fs::write(&archive_path, &bytes)?;
        
        // Validate before moving
        let tune = validate_tune_archive(&archive_path)?;
        
        // Move to community dir
        let dest = self.community_dir.join(&tune.name);
        extract_tar_gz(&archive_path, &dest)?;
        
        Ok(tune.into())
    }
    
    pub fn remove(&self, name: &str) -> Result<()> {
        let path = self.community_dir.join(name);
        
        // Cannot remove official tunes
        if self.official_dir.join(name).exists() {
            anyhow::bail!("Cannot remove official tune: {}", name);
        }
        
        if path.exists() {
            std::fs::remove_dir_all(&path)?;
            Ok(())
        } else {
            anyhow::bail!("Tune not found: {}", name);
        }
    }
}
```

---

## tune.yaml Schema

```yaml
# Tune metadata
name: "mr-robot"
version: "1.0.0"
description: "Industrial/tech atmosphere inspired by fsociety"
author: "cricket-community"
license: "CC0-1.0"

# Resource aliases (for reuse)
resources:
  n1: "samples/nature01-crickets-calm.ogg"
  n2: "samples/nature02-crickets-active.ogg"
  s1: "samples/synth01-pad-low.ogg"
  s2: "samples/synth02-pulse.ogg"
  b1: "samples/beep-success.ogg"
  b2: "samples/beep-fail.ogg"
  b3: "samples/beep-alert.ogg"

# Channel configuration
channels:
  background:
    samples:
      - resource: "s1"
        volume: 0.3
        loop: true
    
  ambient:
    samples:
      - resource: "n1"
        volume: 0.4
        loop: true

  midground:
    # Configured per-event, not continuous

  foreground:
    # Configured per-event, not continuous

# Event mappings
events:
  # Heartbeat modulates ambient
  heartbeat:
    channel: ambient
    behavior: modulate
    healthy:
      resource: "n1"
      volume: 0.3
    degraded:
      resource: "n2"
      volume: 0.5
  
  # State changes play discrete sounds
  offering.stopped.running:
    channel: foreground
    resource: "b1"
    volume: 0.6
  
  offering.running.stopped:
    channel: foreground
    resource: "b2"
    volume: 0.7
  
  offering.running.error:
    channel: foreground
    resource: "b3"
    volume: 0.8
  
  # Deployment sounds
  deployment.started:
    channel: foreground
    resource: "s2"
    volume: 0.5
    loop: true
    tag: "deployment"  # Allows stopping by tag
  
  deployment.success:
    channel: foreground
    stop_tag: "deployment"  # Stop the looping sound
    resource: "b1"
    volume: 0.7
  
  deployment.failed:
    channel: foreground
    stop_tag: "deployment"
    resource: "b2"
    volume: 0.8
  
  # Resource spikes
  spike.cpu:
    channel: midground
    resource: "s2"
    volume_from_metric: true  # 0-100% maps to 0.0-1.0
    min_volume: 0.2
    max_volume: 0.8
  
  spike.memory:
    channel: midground
    resource: "s2"
    volume: 0.5
```

---

## Command Handler

```rust
pub struct CommandHandler {
    tune_manager: Arc<TuneManager>,
    mixer: Arc<Mixer>,
}

impl CommandHandler {
    pub async fn handle(&self, raw_args: Vec<String>) -> CompanionCommandResponse {
        if raw_args.is_empty() {
            return CompanionCommandResponse::error("No command specified")
                .with_suggestion("Try: select, list, volume, status, pull, remove");
        }
        
        let command = &raw_args[0];
        let args = &raw_args[1..];
        
        match command.as_str() {
            "select" => self.cmd_select(args).await,
            "list" => self.cmd_list().await,
            "volume" => self.cmd_volume(args).await,
            "status" => self.cmd_status().await,
            "pull" => self.cmd_pull(args).await,
            "remove" => self.cmd_remove(args).await,
            _ => {
                CompanionCommandResponse::error(format!("Unknown command: {}", command))
                    .with_suggestions(self.suggest_commands(command))
            }
        }
    }
    
    async fn cmd_select(&self, args: &[String]) -> CompanionCommandResponse {
        let Some(tune_name) = args.first() else {
            return CompanionCommandResponse::error("Usage: select <tune>")
                .with_suggestion("Use 'list' to see available tunes");
        };
        
        match self.tune_manager.select(tune_name, &self.mixer).await {
            Ok(()) => CompanionCommandResponse::success(
                format!("Switched to tune '{}'", tune_name)
            ),
            Err(e) => CompanionCommandResponse::error(format!("Failed: {}", e))
                .with_suggestions(self.suggest_tunes(tune_name)),
        }
    }
    
    async fn cmd_list(&self) -> CompanionCommandResponse {
        let tunes = self.tune_manager.list_tunes();
        let current = self.tune_manager.current.read().await;
        let current_name = current.as_ref().map(|t| t.name.as_str());
        
        let mut output = String::new();
        output.push_str("Installed Tunes:\n\n");
        
        // Official tunes
        output.push_str("  Official:\n");
        for tune in tunes.iter().filter(|t| t.official) {
            let marker = if Some(tune.name.as_str()) == current_name { "●" } else { "○" };
            let active = if Some(tune.name.as_str()) == current_name { " (active)" } else { "" };
            output.push_str(&format!("    {} {}{}\n", marker, tune.name, active));
            output.push_str(&format!("      {}\n", tune.description));
        }
        
        // Community tunes
        let community: Vec<_> = tunes.iter().filter(|t| !t.official).collect();
        if !community.is_empty() {
            output.push_str("\n  Community:\n");
            for tune in community {
                let marker = if Some(tune.name.as_str()) == current_name { "●" } else { "○" };
                output.push_str(&format!("    {} {}\n", marker, tune.name));
                output.push_str(&format!("      {}\n", tune.description));
            }
        }
        
        CompanionCommandResponse::success("Listed tunes")
            .with_output(output)
    }
    
    async fn cmd_volume(&self, args: &[String]) -> CompanionCommandResponse {
        let Some(level_str) = args.first() else {
            return CompanionCommandResponse::error("Usage: volume <0-100>");
        };
        
        let Ok(level) = level_str.parse::<u32>() else {
            return CompanionCommandResponse::error("Volume must be a number 0-100");
        };
        
        if level > 100 {
            return CompanionCommandResponse::error("Volume must be 0-100");
        }
        
        self.mixer.set_master_volume(level as f32 / 100.0);
        
        CompanionCommandResponse::success(format!("Volume set to {}%", level))
    }
    
    async fn cmd_status(&self) -> CompanionCommandResponse {
        let current = self.tune_manager.current.read().await;
        
        let tune_name = current.as_ref()
            .map(|t| t.name.as_str())
            .unwrap_or("none");
        
        let volume = (self.mixer.master_volume * 100.0) as u32;
        
        let mut output = String::new();
        output.push_str("Cricket Status:\n");
        output.push_str(&format!("  Tune:     {}\n", tune_name));
        output.push_str(&format!("  Volume:   {}%\n", volume));
        output.push_str("  Channels:\n");
        output.push_str(&format!("    foreground:  {}\n", self.channel_meter(Channel::Foreground)));
        output.push_str(&format!("    midground:   {}\n", self.channel_meter(Channel::Midground)));
        output.push_str(&format!("    ambient:     {}\n", self.channel_meter(Channel::Ambient)));
        output.push_str(&format!("    background:  {}\n", self.channel_meter(Channel::Background)));
        
        CompanionCommandResponse::success("Status retrieved")
            .with_output(output)
    }
    
    async fn cmd_pull(&self, args: &[String]) -> CompanionCommandResponse {
        let Some(url) = args.first() else {
            return CompanionCommandResponse::error("Usage: pull <url>");
        };
        
        match self.tune_manager.pull(url).await {
            Ok(info) => CompanionCommandResponse::success(
                format!("Tune '{}' installed ({} samples, {})", 
                        info.name, info.sample_count, info.size_human)
            ),
            Err(e) => CompanionCommandResponse::error(format!("Failed: {}", e)),
        }
    }
    
    async fn cmd_remove(&self, args: &[String]) -> CompanionCommandResponse {
        let Some(name) = args.first() else {
            return CompanionCommandResponse::error("Usage: remove <tune>");
        };
        
        match self.tune_manager.remove(name) {
            Ok(()) => CompanionCommandResponse::success(
                format!("Tune '{}' removed", name)
            ),
            Err(e) => CompanionCommandResponse::error(format!("{}", e)),
        }
    }
    
    fn suggest_commands(&self, input: &str) -> Vec<String> {
        let commands = ["select", "list", "volume", "status", "pull", "remove"];
        commands.iter()
            .filter(|c| levenshtein(input, c) <= 2)
            .map(|c| c.to_string())
            .collect()
    }
    
    fn suggest_tunes(&self, input: &str) -> Vec<String> {
        self.tune_manager.list_tunes()
            .iter()
            .filter(|t| levenshtein(input, &t.name) <= 3)
            .map(|t| t.name.clone())
            .collect()
    }
    
    fn channel_meter(&self, _channel: Channel) -> String {
        // Placeholder - would read actual audio levels
        "▃▃▃▄▄▅▅▆".to_string()
    }
}
```

---

## Configuration

### Config File

**Location:** `/etc/zen-garden/cricket/config.toml`

```toml
# Stone to connect to
stone_endpoint = "http://localhost:7185"

# Default tune on startup
default_tune = "zen-garden"

# Master volume (0-100)
volume = 50

# Reconnect delay when SSE disconnects
reconnect_delay_secs = 5

# Logging level (-v, -vv, -vvv)
# Overridden by command line
log_level = "warn"
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ZG_STONE` | `http://localhost:7185` | Stone endpoint |
| `CRICKET_VOLUME` | `50` | Master volume |
| `CRICKET_TUNE` | `zen-garden` | Default tune |
| `RUST_LOG` | `warn` | Log level |

---

## Systemd Unit

**File:** `/etc/systemd/system/garden-cricket.service`

```ini
[Unit]
Description=Zen Garden Cricket Audio Companion
After=network.target garden-moss.service
Wants=garden-moss.service

[Service]
Type=simple
ExecStart=/usr/local/bin/garden-cricket
Restart=on-failure
RestartSec=5
User=garden
Group=audio
Environment=RUST_LOG=warn

# Allow audio access
SupplementaryGroups=audio pulse-access

[Install]
WantedBy=multi-user.target
```

---

## Official Tunes

Bundled with package (immutable):

| Tune | Description |
|------|-------------|
| `zen-garden` | Calm nature sounds, soft chimes |
| `mr-robot` | Industrial, digital, technical |
| `lo-fi-ops` | Chill beats, vinyl crackle |
| `silence` | No audio (debugging mode) |

---

## Dependencies

```toml
[dependencies]
rodio = "0.19"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["stream"] }
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
tracing = "0.1"
tracing-subscriber = "0.3"
anyhow = "1"
clap = { version = "4", features = ["derive"] }
```

---

## Related Documents

- [Companion-COMMAND-PROTOCOL.md](Companion-COMMAND-PROTOCOL.md) - Command flow
- [Companion-SERVICE-REGISTRY.md](Companion-SERVICE-REGISTRY.md) - Service registration  
- [HEY-TELL-SYNTAX.md](HEY-TELL-SYNTAX.md) - Rake syntax
- [CRICKET-0001-audio-Companion-spec.md](../decisions/CRICKET-0001-audio-Companion-spec.md) - Design decision

---

**Document Status:** Draft  
**Last Updated:** 2026-01-26
