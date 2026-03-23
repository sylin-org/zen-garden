//! Hardware capabilities API endpoint
//!
//! Returns static hardware inventory detected at startup.
//! Capabilities are cached and updated in the background.

use crate::api::responses::ApiResponse;
use crate::domain::Current;
use axum::{extract::State, Json};
use garden_common::HardwareCapabilities;
use std::sync::Arc;

/// GET /api/capabilities - Hardware inventory and capabilities
///
/// Returns the detected hardware capabilities including:
/// - CPU (model, cores, features, architecture)
/// - Memory (total, available)
/// - GPUs (AI runtimes: CUDA, ROCm, DirectML, OpenVINO)
/// - Disk (capacity, type)
/// - Runtime (Docker version, OS, kernel)
///
/// The capabilities are detected progressively at startup:
/// 1. Fast detection: CPU, memory, disk (< 100ms)
/// 2. Slow detection: GPUs (may take seconds)
///
/// Results are cached to disk for instant startup on subsequent runs.
pub async fn get_capabilities(
    State(current): State<Arc<Current>>,
) -> Json<ApiResponse<HardwareCapabilities>> {
    // Read from cache - capabilities are detected in background at startup
    let caps_guard = current.capabilities.read().await;

    if let Some(caps) = caps_guard.as_ref() {
        Json(ApiResponse::new(caps.clone()))
    } else {
        // Should never happen - skeleton is created immediately at startup
        // But handle gracefully with skeleton data
        let skeleton =
            crate::infra::hardware::create_skeleton(current.stone.name.to_string());
        Json(ApiResponse::new(skeleton))
    }
}
