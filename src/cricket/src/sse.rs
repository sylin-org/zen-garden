//! SSE client for garden presence events
//! Connects to Moss SSE endpoint and dispatches events to mixer

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use crate::manifest::TuneManager;
use crate::mixer::{Channel, Mixer};

/// Debounce tracker
struct DebounceState {
    last_fired: HashMap<String, Instant>,
}

impl DebounceState {
    fn new() -> Self {
        Self {
            last_fired: HashMap::new(),
        }
    }
    
    /// Check if event can fire (respects debounce)
    fn can_fire(&mut self, event_type: &str, debounce_ms: u64) -> bool {
        if debounce_ms == 0 {
            return true;
        }
        
        let now = Instant::now();
        let debounce = Duration::from_millis(debounce_ms);
        
        if let Some(last) = self.last_fired.get(event_type) {
            if now.duration_since(*last) < debounce {
                return false;
            }
        }
        
        self.last_fired.insert(event_type.to_string(), now);
        true
    }
}

/// Start SSE client task
pub async fn start_client(
    stone_endpoint: &str,
    mixer: Arc<Mixer>,
    tune_manager: Arc<TuneManager>,
) -> JoinHandle<()> {
    let endpoint = format!("{}/api/v1/stone/presence/stream", stone_endpoint);
    
    tokio::spawn(async move {
        let debounce = Arc::new(RwLock::new(DebounceState::new()));
        
        loop {
            tracing::info!(endpoint = %endpoint, "Connecting to SSE stream");
            
            match connect_and_listen(&endpoint, &mixer, &tune_manager, &debounce).await {
                Ok(()) => {
                    tracing::info!("SSE stream ended normally");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "SSE connection error");
                }
            }
            
            // Reconnect delay
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}

/// Connect to SSE and process events
async fn connect_and_listen(
    endpoint: &str,
    mixer: &Arc<Mixer>,
    tune_manager: &Arc<TuneManager>,
    debounce: &Arc<RwLock<DebounceState>>,
) -> Result<()> {
    let client = reqwest::Client::new();
    let response = client
        .get(endpoint)
        .header("Accept", "text/event-stream")
        .send()
        .await?;
    
    if !response.status().is_success() {
        anyhow::bail!("SSE endpoint returned {}", response.status());
    }
    
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    
    use futures_util::StreamExt;
    
    while let Some(result) = stream.next().await {
        let chunk = result?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        
        // Process complete events
        while let Some(pos) = buffer.find("\n\n") {
            let event_text = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();
            
            if let Some(event) = parse_sse_event(&event_text) {
                handle_event(&event, mixer, tune_manager, debounce).await;
            }
        }
    }
    
    Ok(())
}

/// Parsed SSE event
struct SseEvent {
    event_type: String,
    #[allow(dead_code)]
    data: String,
}

/// Parse SSE event from text
fn parse_sse_event(text: &str) -> Option<SseEvent> {
    let mut event_type = String::new();
    let mut data = String::new();
    
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event_type = value.trim().to_string();
        } else if let Some(value) = line.strip_prefix("data:") {
            data = value.trim().to_string();
        }
    }
    
    if event_type.is_empty() && data.is_empty() {
        return None;
    }
    
    // Default event type
    if event_type.is_empty() {
        event_type = "message".to_string();
    }
    
    Some(SseEvent { event_type, data })
}

/// Handle incoming event - the core dispatch logic
async fn handle_event(
    event: &SseEvent,
    mixer: &Arc<Mixer>,
    tune_manager: &Arc<TuneManager>,
    debounce: &Arc<RwLock<DebounceState>>,
) {
    // Get mapping from active tune
    let Some(mapping) = tune_manager.get_event_mapping(&event.event_type) else {
        tracing::trace!(event = %event.event_type, "No mapping for event");
        return;
    };
    
    // Check debounce
    {
        let mut db = debounce.write().await;
        if !db.can_fire(&event.event_type, mapping.debounce_ms) {
            tracing::trace!(event = %event.event_type, "Debounced");
            return;
        }
    }
    
    // Resolve channel
    let Some(channel) = Channel::from_str(&mapping.channel) else {
        tracing::warn!(channel = %mapping.channel, "Invalid channel in mapping");
        return;
    };
    
    // Resolve resource (works for both embedded and filesystem, with fallback)
    let active_name = tune_manager.active_name().unwrap_or_default();
    let Some(audio_data) = tune_manager.resolve_resource_bytes_with_fallback(&active_name, &mapping.resource) else {
        tracing::warn!(
            tune = %active_name,
            resource = %mapping.resource,
            "Audio resource not found (no fallback defined)"
        );
        return;
    };
    
    // Play audio
    tracing::debug!(
        event = %event.event_type,
        resource = %mapping.resource,
        channel = %mapping.channel,
        "Playing audio"
    );
    
    if let Err(e) = mixer.play_bytes(channel, audio_data, mapping.looping).await {
        tracing::error!(error = %e, "Failed to play audio");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_sse_event() {
        let text = "event: stone-online\ndata: {\"name\":\"test\"}";
        let event = parse_sse_event(text).unwrap();
        assert_eq!(event.event_type, "stone-online");
        assert_eq!(event.data, "{\"name\":\"test\"}");
    }
    
    #[test]
    fn test_parse_sse_event_data_only() {
        let text = "data: hello";
        let event = parse_sse_event(text).unwrap();
        assert_eq!(event.event_type, "message");
        assert_eq!(event.data, "hello");
    }
    
    #[tokio::test]
    async fn test_debounce() {
        let mut state = DebounceState::new();
        
        // First fire should succeed
        assert!(state.can_fire("test", 1000));
        
        // Immediate second fire should fail
        assert!(!state.can_fire("test", 1000));
        
        // Different event should succeed
        assert!(state.can_fire("other", 1000));
        
        // Zero debounce always succeeds
        assert!(state.can_fire("test", 0));
    }
}
