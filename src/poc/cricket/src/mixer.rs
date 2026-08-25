//! 4-channel audio mixer for Cricket
//! Channels: foreground, midground, ambient, background
//!
//! The audio backend is feature-gated so cricket is target-agnostic:
//! - `audio-rodio` (default): rodio output — links libasound on Linux, needs an audio device.
//! - otherwise (`--no-default-features`): a null/headless backend with no rodio/libasound, so
//!   cricket builds and runs as a companion on aarch64-musl/Android (no libasound there, and
//!   often no exposed PCM device). Playback is a logged no-op; commands/events still work.

use anyhow::Result;

#[cfg(feature = "audio-rodio")]
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
#[cfg(feature = "audio-rodio")]
use std::sync::Arc;
#[cfg(feature = "audio-rodio")]
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// System audio setup (real only on Linux with the rodio backend)
// ---------------------------------------------------------------------------

/// Ensure audio dependencies are installed (Linux + rodio backend).
/// Returns Ok(()) if all dependencies are available (either already or after install)
#[cfg(all(target_os = "linux", feature = "audio-rodio"))]
pub fn ensure_audio_dependencies() -> Result<()> {
    use garden_companion_sdk::dependencies::{ensure_dependencies, SystemDependency};

    let deps = vec![
        SystemDependency::apt_package("alsa-utils", "aplay"),
        SystemDependency::apt_package("alsa-utils", "amixer"),
    ];

    let result = ensure_dependencies(&deps)?;

    if !result.all_ok() {
        tracing::warn!(
            "Audio dependencies incomplete. Sound may not work. Failed: {:?}",
            result.failed
        );
    }

    Ok(())
}

/// No-op without the rodio backend (or on non-Linux).
#[cfg(not(all(target_os = "linux", feature = "audio-rodio")))]
pub fn ensure_audio_dependencies() -> Result<()> {
    Ok(())
}

/// Initialize system audio on Linux (unmute and set volume) — rodio backend only.
/// This ensures the ALSA master volume is set when Cricket starts
#[cfg(all(target_os = "linux", feature = "audio-rodio"))]
pub fn init_system_audio(volume_percent: u8) -> Result<()> {
    use std::process::Command;

    let volume = volume_percent.min(100);
    tracing::info!(volume = volume, "Initializing system audio");

    // Unmute master
    match Command::new("amixer")
        .args(["set", "Master", "unmute"])
        .output()
    {
        Ok(output) => {
            if !output.status.success() {
                tracing::warn!(
                    "Failed to unmute Master: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        Err(e) => {
            tracing::warn!("amixer not available for unmute: {}", e);
        }
    }

    // Set volume percentage
    let volume_arg = format!("{}%", volume);
    match Command::new("amixer")
        .args(["set", "Master", &volume_arg])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                tracing::info!("System audio initialized: Master at {}%, unmuted", volume);
            } else {
                tracing::warn!(
                    "Failed to set Master volume: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        Err(e) => {
            tracing::warn!("amixer not available for volume: {}", e);
        }
    }

    Ok(())
}

/// No-op without the rodio backend (or on non-Linux).
#[cfg(not(all(target_os = "linux", feature = "audio-rodio")))]
pub fn init_system_audio(_volume_percent: u8) -> Result<()> {
    tracing::debug!("System audio init skipped (headless / no-rodio backend)");
    Ok(())
}

// ---------------------------------------------------------------------------
// Channel (shared across backends)
// ---------------------------------------------------------------------------

/// Channel identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Foreground, // Notifications, alerts (high priority, interrupts)
    Midground,  // UI feedback, confirmations
    Ambient,    // Crickets, nature sounds (looping)
    Background, // Pads, drones (continuous)
}

impl Channel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "foreground" | "fg" => Some(Channel::Foreground),
            "midground" | "mg" => Some(Channel::Midground),
            "ambient" | "amb" => Some(Channel::Ambient),
            "background" | "bg" => Some(Channel::Background),
            _ => None,
        }
    }
}

// ===========================================================================
// rodio backend (default)
// ===========================================================================

/// Mixer state for a single channel
#[cfg(feature = "audio-rodio")]
struct ChannelState {
    sink: Sink,
    volume: f32,
}

/// 4-channel mixer
#[cfg(feature = "audio-rodio")]
pub struct Mixer {
    stream_handle: OutputStreamHandle,
    channels: Arc<RwLock<[Option<ChannelState>; 4]>>,
    master_volume: Arc<RwLock<f32>>,
}

// SAFETY: OutputStream doesn't need to be held, only OutputStreamHandle
// which is Send+Sync safe. The Arc<RwLock<_>> fields are also Send+Sync.
#[cfg(feature = "audio-rodio")]
unsafe impl Send for Mixer {}
// SAFETY: Mixer only holds Arc<RwLock<_>> fields (which are Sync) and
// OutputStreamHandle which is used via the Send-safe stream. Access to
// shared state is guarded by the RwLock.
#[cfg(feature = "audio-rodio")]
unsafe impl Sync for Mixer {}

#[cfg(feature = "audio-rodio")]
impl Mixer {
    /// Create new mixer
    pub fn new(master_volume: f32) -> Result<Self> {
        let (_stream, stream_handle) = OutputStream::try_default()?;

        // Keep stream alive but don't store it (it will live for program duration)
        std::mem::forget(_stream);

        Ok(Self {
            stream_handle,
            channels: Arc::new(RwLock::new([None, None, None, None])),
            master_volume: Arc::new(RwLock::new(master_volume)),
        })
    }

    /// Play sample on channel from file path
    #[expect(dead_code)]
    pub async fn play(&self, channel: Channel, sample_path: &str, looping: bool) -> Result<()> {
        let file = std::fs::File::open(sample_path)?;
        let source = rodio::Decoder::new(std::io::BufReader::new(file))?;

        let source: Box<dyn Source<Item = i16> + Send> = if looping {
            Box::new(source.repeat_infinite())
        } else {
            Box::new(source)
        };

        self.play_source(channel, source).await
    }

    /// Play sample on channel from bytes (for embedded assets)
    pub async fn play_bytes(&self, channel: Channel, data: Vec<u8>, looping: bool) -> Result<()> {
        let cursor = std::io::Cursor::new(data);
        let source = rodio::Decoder::new(cursor)?;

        let source: Box<dyn Source<Item = i16> + Send> = if looping {
            Box::new(source.repeat_infinite())
        } else {
            Box::new(source)
        };

        self.play_source(channel, source).await
    }

    /// Internal: play a source on a channel
    async fn play_source(
        &self,
        channel: Channel,
        source: Box<dyn Source<Item = i16> + Send>,
    ) -> Result<()> {
        let sink = Sink::try_new(&self.stream_handle)?;
        let master_vol = *self.master_volume.read().await;
        sink.set_volume(master_vol);
        sink.append(source);

        let idx = channel as usize;
        let mut channels = self.channels.write().await;

        // Stop existing playback on this channel
        if let Some(existing) = channels[idx].take() {
            existing.sink.stop();
        }

        channels[idx] = Some(ChannelState { sink, volume: 1.0 });

        Ok(())
    }

    /// Stop channel
    pub async fn stop(&self, channel: Channel) {
        let idx = channel as usize;
        let mut channels = self.channels.write().await;

        if let Some(state) = channels[idx].take() {
            state.sink.stop();
        }
    }

    /// Set master volume
    pub async fn set_master_volume(&self, volume: f32) {
        *self.master_volume.write().await = volume.clamp(0.0, 1.0);

        // Update all active sinks
        let channels = self.channels.read().await;
        for channel_state in channels.iter().flatten() {
            channel_state.sink.set_volume(volume.clamp(0.0, 1.0));
        }
    }

    /// Set channel volume
    #[expect(dead_code)]
    pub async fn set_channel_volume(&self, channel: Channel, volume: f32) {
        let idx = channel as usize;
        let mut channels = self.channels.write().await;

        if let Some(state) = &mut channels[idx] {
            state.volume = volume.clamp(0.0, 1.0);
            let master_vol = *self.master_volume.read().await;
            state.sink.set_volume(master_vol * state.volume);
        }
    }
}

// ===========================================================================
// null / headless backend (no rodio / libasound — aarch64-musl / Android)
// ===========================================================================

/// Headless mixer: same API, playback is a logged no-op. Lets cricket run as a companion where
/// rodio/libasound is unavailable (and rodio's `OutputStream::try_default()` would otherwise fail).
#[cfg(not(feature = "audio-rodio"))]
pub struct Mixer;

#[cfg(not(feature = "audio-rodio"))]
impl Mixer {
    pub fn new(_master_volume: f32) -> Result<Self> {
        tracing::info!("cricket: headless audio backend (no rodio/libasound) — playback is a no-op");
        Ok(Mixer)
    }

    #[expect(dead_code)]
    pub async fn play(&self, channel: Channel, _sample_path: &str, looping: bool) -> Result<()> {
        tracing::debug!(?channel, looping, "headless: play (no-op)");
        Ok(())
    }

    pub async fn play_bytes(&self, channel: Channel, data: Vec<u8>, looping: bool) -> Result<()> {
        tracing::debug!(?channel, bytes = data.len(), looping, "headless: play_bytes (no-op)");
        Ok(())
    }

    pub async fn stop(&self, channel: Channel) {
        tracing::debug!(?channel, "headless: stop (no-op)");
    }

    pub async fn set_master_volume(&self, _volume: f32) {}

    #[expect(dead_code)]
    pub async fn set_channel_volume(&self, _channel: Channel, _volume: f32) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_from_str() {
        assert_eq!(Channel::from_str("foreground"), Some(Channel::Foreground));
        assert_eq!(Channel::from_str("fg"), Some(Channel::Foreground));
        assert_eq!(Channel::from_str("ambient"), Some(Channel::Ambient));
        assert_eq!(Channel::from_str("invalid"), None);
    }
}
