use anyhow::Result;
use std::time::Duration;
use std::sync::{Arc, Mutex};
use garden_common::{DiscoveryRequest, DiscoveryResponse};
use garden_common::infra::communications::p2p;

/// Cached Lantern discovery result
static LANTERN_CACHE: once_cell::sync::Lazy<Arc<Mutex<Option<Option<String>>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

/// Start background Lantern discovery (non-blocking)
/// Returns immediately, result will be cached for future use
pub fn discover_lantern_background() {
    std::thread::spawn(|| {
        let result = discover_lantern_sync();
        if let Ok(mut cache) = LANTERN_CACHE.lock() {
            *cache = Some(result);
        }
    });
}

/// Get cached Lantern endpoint (non-blocking)
/// Returns None if discovery is still in progress or no Lantern found
pub fn get_cached_lantern() -> Option<String> {
    LANTERN_CACHE.lock().ok()?.as_ref()?.clone()
}

/// Async Lantern discovery using p2p transport
async fn discover_lantern_async() -> Option<String> {
    // Subscribe to discovery responses (Lantern uses same discovery protocol)
    let mut response_rx = p2p::subscribe_to_announcement(
        garden_common::infra::communications::announcement_types::DISCOVERY_RESPONSE
    ).await.ok()?;

    let request_id = uuid::Uuid::now_v7().to_string();
    let request = DiscoveryRequest {
        discover: "moss".into(),
        request_id: request_id.clone(),
        requester: "rake-cli".into(),
    };

    // Send discovery request via p2p transport
    p2p::send_announcement(
        garden_common::infra::communications::announcement_types::DISCOVERY_REQUEST,
        &request
    ).await.ok()?;

    tracing::debug!(request_id = %request_id, "Sent Lantern discovery broadcast (via p2p)");

    // Wait for Lantern response (2 second timeout)
    let response = tokio::time::timeout(
        Duration::from_secs(2),
        async {
            while let Some((payload, addr)) = response_rx.recv().await {
                if let Ok(response) = serde_json::from_value::<DiscoveryResponse>(payload) {
                    // Lantern responses have "lantern" in the discover field or specific port
                    tracing::info!(?addr, endpoint = %response.stone_endpoint, "Discovered Lantern registry");
                    return Some(response.stone_endpoint);
                }
            }
            None
        }
    ).await.ok()??;

    Some(response)
}

/// Synchronous wrapper for async Lantern discovery
fn discover_lantern_sync() -> Option<String> {
    // Use tokio runtime handle if available, otherwise create blocking task
    tokio::runtime::Handle::try_current()
        .ok()
        .and_then(|handle| {
            handle.block_on(discover_lantern_async())
        })
}

pub async fn discover_moss() -> Result<String> {
    // Subscribe to discovery responses before sending request
    let mut response_rx = p2p::subscribe_to_announcement(
        garden_common::infra::communications::announcement_types::DISCOVERY_RESPONSE
    ).await?;

    let request_id = uuid::Uuid::now_v7().to_string();
    let request = DiscoveryRequest {
        discover: "moss".into(),
        request_id: request_id.clone(),
        requester: "rake-cli".into(),
    };

    // Send discovery request via p2p transport
    p2p::send_announcement(
        garden_common::infra::communications::announcement_types::DISCOVERY_REQUEST,
        &request
    ).await?;

    tracing::debug!(request_id = %request_id, "Sent UDP discovery broadcast (via p2p)");

    // Wait for first response (3 second timeout)
    let response = tokio::time::timeout(
        Duration::from_secs(3),
        async {
            if let Some((payload, addr)) = response_rx.recv().await {
                let response: DiscoveryResponse = serde_json::from_value(payload)?;
                tracing::info!(?addr, stone = %response.stone_name, endpoint = %response.stone_endpoint, %request_id, "Discovered Moss");
                Ok::<String, anyhow::Error>(response.stone_endpoint)
            } else {
                anyhow::bail!("P2P channel closed")
            }
        }
    ).await??;

    Ok(response)
}

/// Discover all Moss instances on the network
/// Discover all Moss instances on the network with progressive disclosure
/// 
/// Streams discovered stones via callback as they respond, rather than batching.
/// This exposes network physics and provides immediate feedback to users.
/// 
/// # Arguments
/// * `timeout` - Maximum duration to wait for responses
/// * `on_discovered` - Callback invoked for each unique stone discovered
///   - Receives: (DiscoveryResponse, discovery_instant)
///   - Called immediately when stone responds
/// 
/// # Returns
/// Async version using p2p transport for streaming discovery
pub async fn discover_all_moss_stream_async<F>(
    timeout: Duration,
    mut on_discovered: F,
) -> Result<usize>
where
    F: FnMut(DiscoveryResponse, std::time::Instant) + Send,
{
    use std::collections::HashSet;
    use std::time::Instant;

    // Subscribe to discovery responses before sending request
    let mut response_rx = p2p::subscribe_to_announcement(
        garden_common::infra::communications::announcement_types::DISCOVERY_RESPONSE
    ).await?;

    let request_id = uuid::Uuid::now_v7().to_string();
    let request = DiscoveryRequest {
        discover: "moss".into(),
        request_id: request_id.clone(),
        requester: "rake-cli".into(),
    };

    // Send discovery request via p2p transport
    p2p::send_announcement(
        garden_common::infra::communications::announcement_types::DISCOVERY_REQUEST,
        &request
    ).await?;

    tracing::debug!(request_id = %request_id, "Sent UDP discovery broadcast (streaming mode, via p2p)");

    let start = Instant::now();
    let mut discovered_endpoints = HashSet::new();

    // Stream responses until timeout
    let response_future = async {
        while let Some((payload, addr)) = response_rx.recv().await {
            if let Ok(response) = serde_json::from_value::<DiscoveryResponse>(payload) {
                // Only process unique endpoints
                if !discovered_endpoints.contains(&response.stone_endpoint) {
                    discovered_endpoints.insert(response.stone_endpoint.clone());
                    let discovery_instant = Instant::now();
                    
                    tracing::info!(
                        ?addr, 
                        stone = %response.stone_name,
                        elapsed_ms = discovery_instant.duration_since(start).as_millis(),
                        "Discovered Moss (streaming)"
                    );
                    
                    // ✅ IMMEDIATE CALLBACK - Progressive disclosure
                    on_discovered(response, discovery_instant);
                }
            }
        }
        discovered_endpoints.len()
    };

    // Apply timeout
    match tokio::time::timeout(timeout, response_future).await {
        Ok(count) => Ok(count),
        Err(_) => {
            tracing::debug!(count = discovered_endpoints.len(), "Discovery timeout reached");
            Ok(discovered_endpoints.len())
        }
    }
}

/// Synchronous wrapper for async streaming discovery
/// Total count of unique stones discovered
/// 
/// DEPRECATED: Use discover_all_moss_stream_async directly from async contexts
pub fn discover_all_moss_stream<F>(
    timeout: Duration,
    on_discovered: F,
) -> Result<usize>
where
    F: FnMut(DiscoveryResponse, std::time::Instant) + Send,
{
    // Create a new runtime for truly synchronous contexts only
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(discover_all_moss_stream_async(timeout, on_discovered))
}

// ============================================================================
// mDNS Discovery (Linux only)
// ============================================================================

/// Discover Moss instances via mDNS service browse (Linux only)
///
/// Browses for `_moss._tcp.local.` services announced by Moss instances.
/// This is the preferred discovery method on Linux as it's more reliable
/// than UDP broadcast and works better with firewalls.
///
/// # Arguments
/// * `timeout` - Maximum duration to wait for mDNS responses
///
/// # Returns
/// Vector of discovered stone responses
#[cfg(not(target_os = "windows"))]
pub fn discover_moss_mdns(timeout: Duration) -> Result<Vec<DiscoveryResponse>> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};
    use std::time::Instant;

    let mdns = ServiceDaemon::new()
        .map_err(|e| anyhow::anyhow!("Failed to create mDNS daemon: {}", e))?;

    let service_type = "_moss._tcp.local.";
    let receiver = mdns.browse(service_type)
        .map_err(|e| anyhow::anyhow!("Failed to browse mDNS services: {}", e))?;

    tracing::debug!(service_type = %service_type, "Starting mDNS service browse");

    let mut stones = Vec::new();
    let start = Instant::now();

    while start.elapsed() < timeout {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                // Extract stone_id from TXT record properties (if present)
                let stone_id: Option<String> = info.get_properties()
                    .iter()
                    .find(|p| p.key() == "stone_id")
                    .map(|p| p.val_str().to_string());

                // Extract stone name from TXT record, or fall back to instance name
                let stone_name: String = info.get_properties()
                    .iter()
                    .find(|p| p.key() == "stone_name")
                    .map(|p| p.val_str().to_string())
                    .unwrap_or_else(|| {
                        info.get_fullname()
                            .split('.')
                            .next()
                            .unwrap_or("unknown")
                            .to_string()
                    });

                // Get the first address (prefer any available)
                if let Some(ip) = info.get_addresses().iter().next() {
                    let endpoint = format!("http://{}:{}", ip, info.get_port());

                    tracing::info!(
                        stone = %stone_name,
                        stone_id = ?stone_id,
                        endpoint = %endpoint,
                        "Discovered Moss via mDNS"
                    );

                    stones.push(DiscoveryResponse {
                        stone_id,
                        stone_name,
                        stone_endpoint: endpoint,
                        moss_version: String::new(),
                        lantern_endpoint: None,
                    });
                }
            }
            Ok(ServiceEvent::SearchStarted(_)) => {
                tracing::debug!("mDNS search started");
            }
            Ok(_) => {
                // Other events (ServiceFound, ServiceRemoved, etc.)
            }
            Err(flume::RecvTimeoutError::Timeout) => {
                // Continue polling
            }
            Err(e) => {
                tracing::debug!(error = ?e, "mDNS browse error");
                break;
            }
        }
    }

    // Stop the browse
    let _ = mdns.stop_browse(service_type);

    tracing::debug!(count = stones.len(), "mDNS discovery complete");
    Ok(stones)
}

/// Discover Moss instances via mDNS with streaming callback (Linux only)
///
/// Like `discover_moss_mdns` but invokes callback immediately for each discovery.
#[cfg(not(target_os = "windows"))]
pub fn discover_moss_mdns_stream<F>(timeout: Duration, mut on_discovered: F) -> Result<usize>
where
    F: FnMut(DiscoveryResponse, std::time::Instant),
{
    use mdns_sd::{ServiceDaemon, ServiceEvent};
    use std::time::Instant;
    use std::collections::HashSet;

    let mdns = ServiceDaemon::new()
        .map_err(|e| anyhow::anyhow!("Failed to create mDNS daemon: {}", e))?;

    let service_type = "_moss._tcp.local.";
    let receiver = mdns.browse(service_type)
        .map_err(|e| anyhow::anyhow!("Failed to browse mDNS services: {}", e))?;

    tracing::debug!(service_type = %service_type, "Starting mDNS service browse (streaming)");

    let mut discovered_endpoints = HashSet::new();
    let start = Instant::now();

    while start.elapsed() < timeout {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                // Extract stone_id from TXT record properties (if present)
                let stone_id: Option<String> = info.get_properties()
                    .iter()
                    .find(|p| p.key() == "stone_id")
                    .map(|p| p.val_str().to_string());

                // Extract stone name from TXT record, or fall back to instance name
                let stone_name: String = info.get_properties()
                    .iter()
                    .find(|p| p.key() == "stone_name")
                    .map(|p| p.val_str().to_string())
                    .unwrap_or_else(|| {
                        info.get_fullname()
                            .split('.')
                            .next()
                            .unwrap_or("unknown")
                            .to_string()
                    });

                // Get the first address (prefer any available)
                if let Some(ip) = info.get_addresses().iter().next() {
                    let endpoint = format!("http://{}:{}", ip, info.get_port());

                    // Only process unique endpoints
                    if !discovered_endpoints.contains(&endpoint) {
                        discovered_endpoints.insert(endpoint.clone());
                        let discovery_instant = Instant::now();

                        tracing::info!(
                            stone = %stone_name,
                            stone_id = ?stone_id,
                            endpoint = %endpoint,
                            elapsed_ms = discovery_instant.duration_since(start).as_millis(),
                            "Discovered Moss via mDNS (streaming)"
                        );

                        on_discovered(
                            DiscoveryResponse {
                                stone_id,
                                stone_name,
                                stone_endpoint: endpoint.clone(),
                                moss_version: String::new(),
                                lantern_endpoint: None,
                            },
                            discovery_instant,
                        );
                    }
                }
            }
            Ok(_) => {}
            Err(flume::RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
    }

    let _ = mdns.stop_browse(service_type);
    Ok(discovered_endpoints.len())
}

/// Stub for Windows - mDNS discovery not available
#[cfg(target_os = "windows")]
pub fn discover_moss_mdns(_timeout: Duration) -> Result<Vec<DiscoveryResponse>> {
    tracing::debug!("mDNS discovery not available on Windows");
    Ok(Vec::new())
}

/// Stub for Windows - mDNS discovery not available
#[cfg(target_os = "windows")]
pub fn discover_moss_mdns_stream<F>(_timeout: Duration, _on_discovered: F) -> Result<usize>
where
    F: FnMut(DiscoveryResponse, std::time::Instant),
{
    tracing::debug!("mDNS discovery not available on Windows");
    Ok(0)
}

/// Platform-aware discovery that uses the best method for the current OS
///
/// - Linux: Runs mDNS AND UDP broadcast in parallel, merges results
/// - Windows: Uses UDP broadcast only
///
/// Note: Windows Moss services don't announce via mDNS, so we must always do UDP
/// broadcast to discover them, even on Linux.
pub async fn discover_moss_auto(timeout: Duration) -> Result<Vec<DiscoveryResponse>> {
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    let results = Arc::new(Mutex::new(Vec::new()));
    let seen_endpoints = Arc::new(Mutex::new(HashSet::new()));

    // On Linux, run mDNS and UDP in parallel
    #[cfg(not(target_os = "windows"))]
    {
        let mdns_results = results.clone();
        let mdns_seen = seen_endpoints.clone();
        let mdns_timeout = timeout;

        // Spawn mDNS discovery in background thread
        let mdns_handle = std::thread::spawn(move || {
            if let Ok(stones) = discover_moss_mdns(mdns_timeout) {
                let mut results = mdns_results.lock().unwrap();
                let mut seen = mdns_seen.lock().unwrap();
                for response in stones {
                    if !seen.contains(&response.stone_endpoint) {
                        seen.insert(response.stone_endpoint.clone());
                        results.push(response);
                    }
                }
            }
        });

        // Run UDP discovery in main thread
        let udp_results = results.clone();
        let udp_seen = seen_endpoints.clone();
        let _ = discover_all_moss_stream_async(timeout, |response, _instant| {
            let mut results = udp_results.lock().unwrap();
            let mut seen = udp_seen.lock().unwrap();
            if !seen.contains(&response.stone_endpoint) {
                seen.insert(response.stone_endpoint.clone());
                results.push(response);
            }
        }).await;

        // Wait for mDNS to complete
        let _ = mdns_handle.join();
    }

    // Windows: UDP only
    #[cfg(target_os = "windows")]
    {
        let _ = discover_all_moss_stream_async(timeout, |response, _instant| {
            let mut results = results.lock().unwrap();
            let mut seen = seen_endpoints.lock().unwrap();
            if !seen.contains(&response.stone_endpoint) {
                seen.insert(response.stone_endpoint.clone());
                results.push(response);
            }
        }).await;
    }

    let final_results = match Arc::try_unwrap(results) {
        Ok(mutex) => mutex.into_inner().unwrap_or_else(|e| e.into_inner()),
        Err(arc) => arc.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    };

    tracing::debug!(total = final_results.len(), "Auto-discovery complete");
    Ok(final_results)
}

/// Platform-aware streaming discovery (parallel mDNS + UDP)
///
/// - Linux: Runs mDNS AND UDP broadcast in parallel, streams results as they arrive
/// - Windows: Uses UDP broadcast only
///
/// Results are deduplicated by endpoint and passed to callback immediately.
/// This provides the fastest possible progressive disclosure.
pub fn discover_moss_auto_stream<F>(timeout: Duration, on_discovered: F) -> Result<usize>
where
    F: FnMut(DiscoveryResponse, std::time::Instant) + Send + 'static,
{
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    let seen_endpoints = Arc::new(Mutex::new(HashSet::new()));
    let callback = Arc::new(Mutex::new(on_discovered));
    let total_count = Arc::new(Mutex::new(0usize));

    // On Linux, run mDNS and UDP in parallel
    #[cfg(not(target_os = "windows"))]
    {
        let mdns_seen = seen_endpoints.clone();
        let mdns_callback = callback.clone();
        let mdns_count = total_count.clone();
        let mdns_timeout = timeout;

        // Spawn mDNS discovery in background thread
        let mdns_handle = std::thread::spawn(move || {
            let _ = discover_moss_mdns_stream(mdns_timeout, |response, instant| {
                let mut seen = mdns_seen.lock().unwrap();
                if !seen.contains(&response.stone_endpoint) {
                    seen.insert(response.stone_endpoint.clone());
                    drop(seen); // Release lock before callback

                    let mut cb = mdns_callback.lock().unwrap();
                    cb(response, instant);

                    let mut count = mdns_count.lock().unwrap();
                    *count += 1;
                }
            });
        });

        // Run UDP discovery in main thread
        let udp_seen = seen_endpoints.clone();
        let udp_callback = callback.clone();
        let udp_count = total_count.clone();
        let _ = discover_all_moss_stream(timeout, |response, instant| {
            let mut seen = udp_seen.lock().unwrap();
            if !seen.contains(&response.stone_endpoint) {
                seen.insert(response.stone_endpoint.clone());
                drop(seen); // Release lock before callback

                let mut cb = udp_callback.lock().unwrap();
                cb(response, instant);

                let mut count = udp_count.lock().unwrap();
                *count += 1;
            }
        });

        // Wait for mDNS to complete
        let _ = mdns_handle.join();
    }

    // Windows: UDP only
    #[cfg(target_os = "windows")]
    {
        let _ = discover_all_moss_stream(timeout, |response, instant| {
            let mut seen = seen_endpoints.lock().unwrap();
            if !seen.contains(&response.stone_endpoint) {
                seen.insert(response.stone_endpoint.clone());
                drop(seen);

                let mut cb = callback.lock().unwrap();
                cb(response, instant);

                let mut count = total_count.lock().unwrap();
                *count += 1;
            }
        });
    }

    let final_count = *total_count.lock().unwrap();
    tracing::debug!(total = final_count, "Auto-discovery complete (parallel)");
    Ok(final_count)
}
