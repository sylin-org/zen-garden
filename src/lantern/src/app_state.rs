use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};

use crate::domain::topology::GardenTopology;
use crate::infra::event_bus::{EventBus, SseEvent};
use crate::infra::moss_client::MossClient;

const ACTIVITY_BUFFER_SIZE: usize = 200;

/// Unified dependency injection container for all Lantern handlers.
///
/// All mutable state is behind Arc<RwLock<T>> for concurrent access.
/// API endpoints read from caches only — no I/O in handlers.
#[derive(Clone)]
pub struct AppState {
    /// Lantern instance identifier
    pub lantern_name: String,

    /// Process start time (for uptime calculation)
    pub start_time: Instant,

    /// HTTP port Lantern is listening on
    pub api_port: u16,

    /// Aggregated garden topology from all registered stones
    pub topology: Arc<RwLock<GardenTopology>>,

    /// SSE broadcast channel for real-time presence events
    pub sse_tx: broadcast::Sender<SseEvent>,

    /// Domain event bus
    pub event_bus: EventBus,

    /// Recent activity events (ring buffer for the activity endpoint)
    pub activity: Arc<RwLock<VecDeque<SseEvent>>>,

    /// HTTP client for proxying actions to Moss instances
    pub http_client: MossClient,

    /// Koi embedded handle — provides mDNS discovery (and future capabilities)
    pub koi_handle: Arc<koi_embedded::KoiHandle>,
}

impl AppState {
    pub fn new(
        lantern_name: String,
        api_port: u16,
        koi_handle: Arc<koi_embedded::KoiHandle>,
    ) -> Self {
        let (sse_tx, _) = broadcast::channel(256);

        Self {
            lantern_name,
            start_time: Instant::now(),
            api_port,
            topology: Arc::new(RwLock::new(GardenTopology::new())),
            sse_tx,
            event_bus: EventBus::new(),
            activity: Arc::new(RwLock::new(VecDeque::with_capacity(ACTIVITY_BUFFER_SIZE))),
            http_client: MossClient::new(
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                    .expect("Failed to create HTTP client"),
            ),
            koi_handle,
        }
    }

    /// Maximum number of events retained in the activity buffer
    pub fn activity_buffer_size() -> usize {
        ACTIVITY_BUFFER_SIZE
    }
}
