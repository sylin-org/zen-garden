//! Timing breakdowns — present on every primitive's output.

use crate::domain::field_path::FieldPath;

/// Time spent in the contextualizer + dispatcher resolving routing.
pub const ROUTING_MS: FieldPath = FieldPath::new("timing.routing_ms");
/// Time the request waited in the provider's queue before execution.
pub const QUEUE_MS: FieldPath = FieldPath::new("timing.queue_ms");
/// Time the provider spent performing the actual inference.
pub const INFERENCE_MS: FieldPath = FieldPath::new("timing.inference_ms");
/// End-to-end time from request receipt to response serialization.
pub const TOTAL_MS: FieldPath = FieldPath::new("timing.total_ms");
