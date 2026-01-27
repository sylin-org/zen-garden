//! 4-channel audio mixer for Cricket
//! Channels: foreground, midground, ambient, background

use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;

/// Channel identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Foreground,  // Notifications, alerts (high priority, interrupts)
    Midground,   // UI feedback, confirmations
    Ambient,     // Crickets, nature sounds (looping)
    Background,  // Pads, drones (continuous)
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

/// Mixer state for a single channel
struct ChannelState {
    sink: Sink,
    volume: f32,
}

/// 4-channel mixer
pub struct Mixer {
    stream_handle: OutputStreamHandle,
    channels: Arc<RwLock<[Option<ChannelState>; 4]>>,
    master_volume: Arc<RwLock<f32>>,
}

// SAFETY: OutputStream doesn't need to be held, only OutputStreamHandle
// which is Send+Sync safe
unsafe impl Send for Mixer {}
unsafe impl Sync for Mixer {}

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
    async fn play_source(&self, channel: Channel, source: Box<dyn Source<Item = i16> + Send>) -> Result<()> {
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
        
        channels[idx] = Some(ChannelState {
            sink,
            volume: 1.0,
        });
        
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
