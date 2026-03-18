//! Pulse Event Infrastructure
//!
//! Unified event channel for real-time observability. Replaces the former
//! SseListener by consolidating domain events and transport (UDP) events
//! into a single broadcast channel (`pulse`).
//!
//! Two event sources feed `pulse`:
//! - **PulseDomainBridge** (EventListener): converts DomainEvent → PulseEvent::Domain
//! - **TransportTap** (spawned task): converts raw UDP announcements → PulseEvent::Transport
//!
//! Consumers:
//! - `/api/v1/stone/pulse/stream` — full firehose (both transport + domain)
//! - `/api/v1/stone/presence/stream` — filtered domain-only, translated to
//!   Companion vocabulary (backward-compatible with Firefly/Cricket)

use crate::domain::events::{
    DomainEvent, JobEvent, OfferingEvent, PondEvent, StoneEvent, StorageEvent,
};
use crate::infra::event_bus::EventListener;
use chrono::Utc;
use garden_common::constants::{
    EVENT_DEPLOYED, EVENT_DESTROYED, EVENT_HEALTH_CHANGED, EVENT_REMOVED, EVENT_RENAMED,
    EVENT_STARTED, EVENT_STOPPED, EVENT_UPDATED, SSE_LEVEL_INFO,
};
use garden_common::infra::communications::announcement_types;
use garden_common::presence::{event_types, StoneHealthChangedPayload, StoneLoadUpdatedPayload};
use serde::Serialize;
use std::net::SocketAddr;
use tokio::sync::broadcast;

// ============================================================================
// PulseEvent — unified event envelope
// ============================================================================

/// Unified pulse event — everything a stone experiences, in one channel.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum PulseEvent {
    /// Domain events (offerings, storage, stone health, jobs, pond)
    Domain(DomainPulse),
    /// Transport events (raw UDP announcements: chirps, elections, beacons)
    Transport(TransportPulse),
}

/// A domain event translated for pulse consumers.
#[derive(Debug, Clone, Serialize)]
pub struct DomainPulse {
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// Event level (info, warn, error)
    pub level: String,
    /// Presence-vocabulary event type (e.g. "service.started", "stone.tended")
    pub event_type: String,
    /// Human-readable message
    pub message: String,
    /// Presence category (service, stone, storage, job, pond)
    pub category: String,
    /// Optional entity name (offering name, bank name, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    /// Optional job ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    /// Additional structured data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// A raw transport (UDP) event surfaced for observability.
#[derive(Debug, Clone, Serialize)]
pub struct TransportPulse {
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// Announcement type (e.g. "stone_chirp", "election_request")
    pub announcement_type: String,
    /// Source address (IP:port)
    pub from: String,
    /// Human-readable summary
    pub summary: String,
    /// Approximate payload size in bytes
    pub payload_bytes: usize,
    /// Truncated payload preview (compact JSON, max ~512 chars)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_preview: Option<serde_json::Value>,
}

// ============================================================================
// DomainPulse construction (migrated from SseEvent)
// ============================================================================

impl DomainPulse {
    /// Convert a DomainEvent into a DomainPulse.
    pub fn from_domain_event(event: &DomainEvent) -> Self {
        match event {
            DomainEvent::Offering(e) => Self::from_offering(e),
            DomainEvent::Storage(e) => Self::from_storage(e),
            DomainEvent::Stone(e) => Self::from_stone(e),
            DomainEvent::Job(e) => Self::from_job(e),
            DomainEvent::Pond(e) => Self::from_pond(e),
        }
    }

    fn from_offering(event: &OfferingEvent) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            level: SSE_LEVEL_INFO.to_string(),
            event_type: translate_offering_type(event.event_type()).to_string(),
            message: event.to_message(),
            category: event_types::CATEGORY_SERVICE.to_string(),
            entity: Some(event.name().to_string()),
            job_id: None,
            data: None,
        }
    }

    fn from_storage(event: &StorageEvent) -> Self {
        let data = match event {
            StorageEvent::StorageConnected {
                name,
                device,
                mount_path,
                capacity_gb,
                roles,
                ..
            } => Some(serde_json::json!({
                "name": name,
                "device": device,
                "mount_path": mount_path,
                "capacity_gb": capacity_gb,
                "roles": roles,
            })),
            StorageEvent::StorageDetected {
                device,
                state,
                capacity_gb,
                used_gb,
                ..
            } => Some(serde_json::json!({
                "device": device,
                "state": state,
                "capacity_gb": capacity_gb,
                "used_gb": used_gb,
            })),
            StorageEvent::StorageRemoved { name, device, .. } => Some(serde_json::json!({
                "name": name,
                "device": device,
            })),
            StorageEvent::StorageReleased { name, .. } => Some(serde_json::json!({
                "name": name,
            })),
            StorageEvent::StorageSensed { name, roles, .. } => Some(serde_json::json!({
                "name": name,
                "roles": roles,
            })),
            StorageEvent::StorageRenamed {
                replica_set_id,
                new_name,
                ..
            } => Some(serde_json::json!({
                "replica_set_id": replica_set_id,
                "new_name": new_name,
            })),
            StorageEvent::StorageRoleChanged {
                device_id,
                replica_set_id,
                new_role,
                ..
            } => Some(serde_json::json!({
                "device_id": device_id,
                "replica_set_id": replica_set_id,
                "new_role": new_role,
            })),
            StorageEvent::StoragePinChanged {
                device_id,
                replica_set_id,
                ..
            } => Some(serde_json::json!({
                "device_id": device_id,
                "replica_set_id": replica_set_id,
            })),
            StorageEvent::StorageReclassified { .. } => None,
            StorageEvent::SyncStarted { name, .. } => Some(serde_json::json!({ "name": name })),
            StorageEvent::SyncCompleted { name, success, .. } => {
                Some(serde_json::json!({ "name": name, "success": success }))
            }
        };

        Self {
            timestamp: Utc::now().to_rfc3339(),
            level: SSE_LEVEL_INFO.to_string(),
            event_type: event.event_type().to_string(),
            message: event.to_message(),
            category: event_types::CATEGORY_STORAGE.to_string(),
            entity: None,
            job_id: None,
            data,
        }
    }

    fn from_stone(event: &StoneEvent) -> Self {
        let data = match event {
            StoneEvent::Tended {
                by, from, message, ..
            } => Some(serde_json::json!({
                "by": by,
                "from": from,
                "message": message,
            })),
            StoneEvent::HealthChanged {
                health,
                cpu_percent,
                memory_percent,
                ..
            } => {
                let payload = StoneHealthChangedPayload {
                    health: health.clone(),
                    cpu_percent: *cpu_percent,
                    memory_percent: *memory_percent,
                };
                serde_json::to_value(payload).ok()
            }
            StoneEvent::LoadUpdated {
                cpu_percent,
                memory_percent,
                disk_percent,
                io_percent,
                gpu_percent,
                gpu_active,
                net_rx_bytes_per_sec,
                net_tx_bytes_per_sec,
                ..
            } => {
                let payload = StoneLoadUpdatedPayload {
                    cpu_percent: *cpu_percent,
                    memory_percent: *memory_percent,
                    disk_percent: *disk_percent,
                    io_percent: *io_percent,
                    gpu_percent: *gpu_percent,
                    gpu_active: *gpu_active,
                    net_rx_bytes_per_sec: *net_rx_bytes_per_sec,
                    net_tx_bytes_per_sec: *net_tx_bytes_per_sec,
                };
                serde_json::to_value(payload).ok()
            }
            StoneEvent::NetworkReady { ip, interface, .. } => Some(serde_json::json!({
                "ip": ip,
                "interface": interface,
            })),
        };

        Self {
            timestamp: Utc::now().to_rfc3339(),
            level: SSE_LEVEL_INFO.to_string(),
            event_type: event.event_type().to_string(),
            message: event.to_message(),
            category: event_types::CATEGORY_STONE.to_string(),
            entity: None,
            job_id: None,
            data,
        }
    }

    fn from_job(event: &JobEvent) -> Self {
        let data = match event {
            JobEvent::Started { operation, .. } => Some(serde_json::json!({
                "operation": operation,
            })),
            JobEvent::Progress { .. } => None,
            JobEvent::Completed { .. } => None,
            JobEvent::Failed { error, .. } => Some(serde_json::json!({
                "error": error,
            })),
        };

        Self {
            timestamp: Utc::now().to_rfc3339(),
            level: event.level().to_string(),
            event_type: event.event_type().to_string(),
            message: event.to_message(),
            category: event_types::CATEGORY_JOB.to_string(),
            entity: Some(event.offering().to_string()),
            job_id: Some(event.job_id().to_string()),
            data,
        }
    }

    fn from_pond(event: &PondEvent) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            level: SSE_LEVEL_INFO.to_string(),
            event_type: event.event_type().to_string(),
            message: event.to_message(),
            category: "pond".to_string(),
            entity: None,
            job_id: None,
            data: None,
        }
    }
}

/// Translate internal offering event types to presence protocol vocabulary.
fn translate_offering_type(event_type: &str) -> &'static str {
    match event_type {
        EVENT_DEPLOYED => event_types::SERVICE_STARTED,
        EVENT_STARTED => event_types::SERVICE_STARTED,
        EVENT_STOPPED => event_types::SERVICE_STOPPED,
        EVENT_REMOVED => event_types::SERVICE_STOPPED,
        EVENT_DESTROYED => event_types::SERVICE_STOPPED,
        EVENT_UPDATED => event_types::SERVICE_UPDATED,
        EVENT_RENAMED => event_types::SERVICE_RENAMED,
        EVENT_HEALTH_CHANGED => event_types::SERVICE_HEALTH_CHANGED,
        _ => event_types::SERVICE_STARTED, // Fallback for unknown types
    }
}

// ============================================================================
// DomainPulse → presence SSE event (for backward-compatible presence stream)
// ============================================================================

impl DomainPulse {
    /// Convert to an axum SSE Event in the presence vocabulary.
    ///
    /// Used by the `/presence/stream` endpoint to maintain backward
    /// compatibility with Firefly, Cricket, and other Companions.
    pub fn to_presence_event(&self) -> axum::response::sse::Event {
        let mut data = serde_json::json!({
            "timestamp": self.timestamp,
            "message": self.message,
        });

        if let Some(ref entity) = self.entity {
            data["service"] = serde_json::Value::String(entity.clone());
        }
        if let Some(serde_json::Value::Object(map)) = self.data.as_ref() {
            for (k, v) in map {
                data[k] = v.clone();
            }
        }

        axum::response::sse::Event::default()
            .event(&self.event_type)
            .data(data.to_string())
    }
}

// ============================================================================
// TransportPulse construction
// ============================================================================

impl TransportPulse {
    /// Build a transport pulse from a raw UDP announcement.
    pub fn from_announcement(
        announcement_type: &str,
        payload: &serde_json::Value,
        from: SocketAddr,
    ) -> Self {
        let payload_str = payload.to_string();
        let payload_bytes = payload_str.len();
        let summary = summarize_transport(announcement_type, payload);
        let preview = truncate_payload_preview(payload);

        Self {
            timestamp: Utc::now().to_rfc3339(),
            announcement_type: announcement_type.to_string(),
            from: from.to_string(),
            summary,
            payload_bytes,
            payload_preview: Some(preview),
        }
    }
}

/// Generate a human-readable summary for each transport announcement type.
fn summarize_transport(announcement_type: &str, payload: &serde_json::Value) -> String {
    match announcement_type {
        announcement_types::STONE_CHIRP => {
            let name = payload["name"].as_str().unwrap_or("?");
            let svc_count = payload["services"].as_array().map(|a| a.len()).unwrap_or(0);
            format!("Chirp from {} ({} services)", name, svc_count)
        }
        announcement_types::STONE_GOODBYE => {
            let name = payload["name"].as_str().unwrap_or("?");
            format!("Goodbye from {}", name)
        }
        announcement_types::DISCOVERY_REQUEST => {
            let from = payload["stone_name"].as_str().unwrap_or("?");
            format!("Discovery request from {}", from)
        }
        announcement_types::DISCOVERY_RESPONSE => {
            let name = payload["name"].as_str().unwrap_or("?");
            format!("Discovery response from {}", name)
        }
        announcement_types::ELECTION_REQUEST => {
            let from = payload["requester_name"]
                .as_str()
                .or_else(|| payload["stone_name"].as_str())
                .unwrap_or("?");
            format!("Election request from {}", from)
        }
        announcement_types::ELECTION_CANDIDATE => {
            let name = payload["stone_name"]
                .as_str()
                .or_else(|| payload["name"].as_str())
                .unwrap_or("?");
            format!("Election candidate: {}", name)
        }
        announcement_types::ELECTION_RESULT => {
            let winner = payload["winner_name"]
                .as_str()
                .or_else(|| payload["name"].as_str())
                .unwrap_or("?");
            format!("Election result: winner={}", winner)
        }
        announcement_types::STORAGE_BEACON => {
            let name = payload["stone_name"]
                .as_str()
                .or_else(|| payload["name"].as_str())
                .unwrap_or("?");
            let banks = payload["banks"].as_array().map(|a| a.len()).unwrap_or(0);
            format!("Storage beacon from {} ({} banks)", name, banks)
        }
        announcement_types::TOOLS_BEACON => {
            let name = payload["stone_name"]
                .as_str()
                .or_else(|| payload["name"].as_str())
                .unwrap_or("?");
            format!("Tools beacon from {}", name)
        }
        other => format!("Unknown: {}", other),
    }
}

/// Create a compact payload preview by stripping large nested structures.
///
/// For arrays of objects with `name`, `offering`, or `fqid` fields,
/// preserves lightweight `{"name": "..."}` objects so consumers can display
/// item names. Arrays of primitives or objects without name-like fields
/// get the count-based `"[...N items]"` fallback. Capped at 20 items.
fn truncate_payload_preview(payload: &serde_json::Value) -> serde_json::Value {
    match payload {
        serde_json::Value::Object(map) => {
            let mut preview = serde_json::Map::new();
            for (k, v) in map {
                match v {
                    serde_json::Value::Array(arr) if arr.len() > 3 => {
                        // Try to extract name-like fields for a lightweight summary
                        let name_key = arr.first().and_then(|item| {
                            if item.get("offering").and_then(|n| n.as_str()).is_some() {
                                Some("offering")
                            } else if item.get("name").and_then(|n| n.as_str()).is_some() {
                                Some("name")
                            } else if item.get("fqid").and_then(|n| n.as_str()).is_some() {
                                Some("fqid")
                            } else {
                                None
                            }
                        });

                        if let Some(field) = name_key {
                            // Preserve as lightweight name-only array (capped at 20)
                            let name_array: Vec<serde_json::Value> = arr
                                .iter()
                                .take(20)
                                .filter_map(|item| {
                                    let name = item.get(field).and_then(|n| n.as_str())?;
                                    Some(serde_json::json!({ field: name }))
                                })
                                .collect();
                            preview.insert(k.clone(), serde_json::Value::Array(name_array));
                        } else {
                            // No name-like field — fall back to count
                            preview.insert(
                                k.clone(),
                                serde_json::json!(format!("[...{} items]", arr.len())),
                            );
                        }
                    }
                    serde_json::Value::Object(inner) if inner.len() > 10 => {
                        preview.insert(
                            k.clone(),
                            serde_json::json!(format!("{{...{} fields}}", inner.len())),
                        );
                    }
                    _ => {
                        preview.insert(k.clone(), v.clone());
                    }
                }
            }
            serde_json::Value::Object(preview)
        }
        other => other.clone(),
    }
}

// ============================================================================
// PulseDomainBridge — EventListener that feeds pulse
// ============================================================================

/// Bridges domain events into the pulse channel.
///
/// Replaces the former `SseListener`. Implements `EventListener` so it can
/// be spawned via `event_bus::spawn_listener()`.
pub struct PulseDomainBridge {
    tx: broadcast::Sender<PulseEvent>,
}

impl PulseDomainBridge {
    /// Create a new bridge with the given pulse channel sender.
    pub fn new(tx: broadcast::Sender<PulseEvent>) -> Self {
        Self { tx }
    }
}

#[async_trait::async_trait]
impl EventListener for PulseDomainBridge {
    async fn on_event(&self, event: &DomainEvent) {
        let pulse = DomainPulse::from_domain_event(event);
        let pulse_event = PulseEvent::Domain(pulse);

        match self.tx.send(pulse_event) {
            Ok(count) => {
                tracing::trace!(
                    event_type = event.event_type(),
                    receivers = count,
                    "Pulse domain event broadcast"
                );
            }
            Err(_) => {
                tracing::trace!(
                    event_type = event.event_type(),
                    "Pulse domain event dropped (no receivers)"
                );
            }
        }
    }

    fn name(&self) -> &'static str {
        super::names::PULSE
    }
}

// ============================================================================
// Transport tap — spawns a task that bridges UDP → pulse
// ============================================================================

/// Spawn the transport tap task.
///
/// Subscribes to ALL raw UDP announcements via the p2p singleton and forwards
/// them as `PulseEvent::Transport` on `pulse`. Uses `receiver_count() == 0`
/// guard for zero overhead when nobody is listening.
///
/// Respects the shutdown token (MOSS-0004) for cooperative shutdown.
pub fn spawn_transport_tap(
    tx: broadcast::Sender<PulseEvent>,
    shutdown_token: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use garden_common::infra::communications::p2p;

        let mut udp_rx = match p2p::subscribe_to_all().await {
            Ok(rx) => rx,
            Err(e) => {
                tracing::error!(error = ?e, "Transport tap: failed to subscribe to p2p events");
                return;
            }
        };

        tracing::info!("Transport tap started (pulse channel)");

        loop {
            tokio::select! {
                msg = udp_rx.recv() => {
                    match msg {
                        Some((announcement_type, payload, from_addr)) => {
                            // Skip if nobody is listening
                            if tx.receiver_count() == 0 {
                                continue;
                            }

                            let transport = TransportPulse::from_announcement(
                                &announcement_type,
                                &payload,
                                from_addr,
                            );
                            let _ = tx.send(PulseEvent::Transport(transport));
                        }
                        None => {
                            tracing::info!("Transport tap: p2p channel closed");
                            break;
                        }
                    }
                }
                _ = shutdown_token.cancelled() => {
                    tracing::debug!("Transport tap: shutdown token cancelled");
                    break;
                }
            }
        }
    })
}

// ============================================================================
// Convenience: build DomainPulse from ad-hoc storage events (storage.rs)
// ============================================================================

impl DomainPulse {
    /// Build a DomainPulse for ad-hoc storage progress/lifecycle events
    /// emitted directly by API handlers (not through EventBus).
    pub fn storage_event(
        event_type: &str,
        message: impl Into<String>,
        level: &str,
        job_id: Option<String>,
        data: Option<serde_json::Value>,
    ) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            level: level.to_string(),
            event_type: event_type.to_string(),
            message: message.into(),
            category: event_types::CATEGORY_STORAGE.to_string(),
            entity: None,
            job_id,
            data,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_pulse_from_offering() {
        let event = DomainEvent::Offering(OfferingEvent::deployed(
            "id-1", "mongodb", "stone-01", "mongo:7",
        ));
        let pulse = DomainPulse::from_domain_event(&event);

        assert_eq!(pulse.event_type, event_types::SERVICE_STARTED);
        assert_eq!(pulse.message, "Service mongodb deployed");
        assert_eq!(pulse.category, event_types::CATEGORY_SERVICE);
        assert_eq!(pulse.entity, Some("mongodb".to_string()));
    }

    #[test]
    fn test_domain_pulse_from_storage_connected() {
        let event = DomainEvent::Storage(StorageEvent::storage_connected(
            "backup",
            "/dev/sdb1",
            "/mnt/backup",
            500,
            vec!["seed-bank".to_string()],
        ));
        let pulse = DomainPulse::from_domain_event(&event);

        assert_eq!(pulse.event_type, event_types::STORAGE_CONNECTED);
        assert!(pulse.message.contains("backup"));
        assert_eq!(pulse.category, event_types::CATEGORY_STORAGE);
        assert!(pulse.data.is_some());
    }

    #[test]
    fn test_domain_pulse_from_storage_detected() {
        let event = DomainEvent::Storage(StorageEvent::storage_detected(
            "/dev/sdc1",
            "has_data",
            500,
            200,
        ));
        let pulse = DomainPulse::from_domain_event(&event);

        assert_eq!(pulse.event_type, event_types::STORAGE_DETECTED);
        assert!(pulse.message.contains("/dev/sdc1"));
        assert_eq!(pulse.category, event_types::CATEGORY_STORAGE);
    }

    #[test]
    fn test_domain_pulse_from_stone() {
        let event = DomainEvent::Stone(StoneEvent::tended(
            "rake",
            "leo-laptop",
            Some("Hello".to_string()),
        ));
        let pulse = DomainPulse::from_domain_event(&event);

        assert_eq!(pulse.event_type, event_types::STONE_TENDED);
        assert!(pulse.message.contains("rake"));
        assert_eq!(pulse.category, event_types::CATEGORY_STONE);
        assert!(pulse.data.is_some());
    }

    #[tokio::test]
    async fn test_pulse_domain_bridge_broadcast() {
        let (tx, mut rx) = broadcast::channel::<PulseEvent>(16);
        let bridge = PulseDomainBridge::new(tx);

        let event = DomainEvent::Offering(OfferingEvent::started("id-1", "mongodb", "stone-01"));
        bridge.on_event(&event).await;

        let received = rx.recv().await.unwrap();
        match received {
            PulseEvent::Domain(pulse) => {
                assert_eq!(pulse.event_type, event_types::SERVICE_STARTED);
                assert_eq!(pulse.message, "Service mongodb started");
            }
            PulseEvent::Transport(_) => panic!("Expected Domain, got Transport"),
        }
    }

    #[test]
    fn test_transport_summary_chirp() {
        let payload = serde_json::json!({
            "name": "stone-01",
            "services": [{"name": "mongodb"}, {"name": "ollama"}]
        });
        let summary = summarize_transport(announcement_types::STONE_CHIRP, &payload);
        assert_eq!(summary, "Chirp from stone-01 (2 services)");
    }

    #[test]
    fn test_transport_summary_election() {
        let payload = serde_json::json!({
            "requester_name": "stone-02"
        });
        let summary = summarize_transport(announcement_types::ELECTION_REQUEST, &payload);
        assert_eq!(summary, "Election request from stone-02");
    }

    #[test]
    fn test_truncate_payload_preview() {
        let payload = serde_json::json!({
            "name": "stone-01",
            "services": [1, 2, 3, 4, 5],
            "simple": "value"
        });
        let preview = truncate_payload_preview(&payload);
        let obj = preview.as_object().unwrap();
        assert_eq!(obj["name"], "stone-01");
        assert_eq!(obj["simple"], "value");
        // services should be truncated
        assert!(obj["services"].as_str().unwrap().contains("5 items"));
    }

    #[test]
    fn test_truncate_payload_preview_named_objects() {
        let payload = serde_json::json!({
            "name": "stone-01",
            "services": [
                {"name": "mongodb", "status": "running"},
                {"name": "ollama", "status": "running"},
                {"name": "redis", "status": "running"},
                {"name": "weaviate", "status": "running"},
                {"name": "postgres", "status": "stopped"},
            ],
            "simple": "value"
        });
        let preview = truncate_payload_preview(&payload);
        let obj = preview.as_object().unwrap();
        assert_eq!(obj["name"], "stone-01");
        assert_eq!(obj["simple"], "value");
        // services should be a lightweight name-only array (not truncated to string)
        let svc_arr = obj["services"].as_array().unwrap();
        assert_eq!(svc_arr.len(), 5);
        assert_eq!(svc_arr[0]["name"], "mongodb");
        assert_eq!(svc_arr[4]["name"], "postgres");
        // Full objects should be stripped (no "status" field)
        assert!(svc_arr[0].get("status").is_none());
    }

    #[test]
    fn test_truncate_payload_preview_offering_field() {
        // Services with "offering" field (TopologyServiceEntry format)
        let payload = serde_json::json!({
            "services": [
                {"offering": "mongodb", "name": "my-mongo", "status": "running"},
                {"offering": "ollama", "name": "my-ollama", "status": "running"},
                {"offering": "redis", "name": "my-redis", "status": "running"},
                {"offering": "weaviate", "name": "my-weaviate", "status": "running"},
            ]
        });
        let preview = truncate_payload_preview(&payload);
        let obj = preview.as_object().unwrap();
        let svc_arr = obj["services"].as_array().unwrap();
        assert_eq!(svc_arr.len(), 4);
        // Should prefer "offering" field
        assert_eq!(svc_arr[0]["offering"], "mongodb");
        assert!(svc_arr[0].get("name").is_none());
    }

    #[test]
    fn test_presence_event_conversion() {
        let pulse = DomainPulse {
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            level: "info".to_string(),
            event_type: event_types::SERVICE_STARTED.to_string(),
            message: "Service mongodb started".to_string(),
            category: event_types::CATEGORY_SERVICE.to_string(),
            entity: Some("mongodb".to_string()),
            job_id: None,
            data: None,
        };
        // Should not panic — validates the builder works
        let _event = pulse.to_presence_event();
    }
}
