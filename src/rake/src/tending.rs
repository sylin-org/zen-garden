use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

/// Errors that can occur when executing operations on stones
#[derive(Debug)]
pub enum StoneError {
    /// Connection failed (timeout, unreachable) - try another stone
    ConnectionFailed(String),
    /// Stone responded with error (404, 500, etc.) - don't retry
    ResponseError(u16, String),
    /// Data processing failed (JSON parse, etc.) - don't retry
    ProcessingError(String),
}

impl std::fmt::Display for StoneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoneError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            StoneError::ResponseError(status, msg) => write!(f, "HTTP {}: {}", status, msg),
            StoneError::ProcessingError(msg) => write!(f, "Processing failed: {}", msg),
        }
    }
}

impl std::error::Error for StoneError {}

impl StoneError {
    /// Returns true if this error indicates the stone might be offline (should try another)
    pub fn is_retryable(&self) -> bool {
        matches!(self, StoneError::ConnectionFailed(_))
    }
}

/// Tending state - persists indefinitely until explicitly changed or stone goes offline.
/// No TTL - Rake stays connected to the same stone across sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TendingState {
    pub stone_name: String,
    pub endpoint: String,
    #[serde(with = "iso8601")]
    pub last_seen: SystemTime,
}

mod iso8601 {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{SystemTime, UNIX_EPOCH};

    pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = time.duration_since(UNIX_EPOCH).map_err(serde::ser::Error::custom)?;
        let secs = duration.as_secs();
        let iso = chrono::DateTime::from_timestamp(secs as i64, 0)
            .ok_or_else(|| serde::ser::Error::custom("invalid timestamp"))?
            .to_rfc3339();
        iso.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let dt = chrono::DateTime::parse_from_rfc3339(&s)
            .map_err(serde::de::Error::custom)?;
        let secs = dt.timestamp() as u64;
        Ok(UNIX_EPOCH + std::time::Duration::from_secs(secs))
    }
}

impl TendingState {
    /// Tending is always valid once set - no TTL expiration.
    /// Validity now depends on reachability (checked at use time in dispatch).
    pub fn is_valid(&self) -> bool {
        true
    }

    /// Age in seconds since tending was last written (informational only)
    pub fn age_seconds(&self) -> u64 {
        self.last_seen.elapsed().unwrap_or_default().as_secs()
    }
}

/// Get the zen-garden data directory, using platform-appropriate paths.
///
/// Priority order:
/// 1. Linux: XDG data directory (~/.local/share/zen-garden)
/// 2. All platforms: Home directory (~/.zen-garden)
/// 3. Linux fallback: /tmp/zen-garden (for containers/services)
fn zen_garden_dir() -> Result<PathBuf> {
    // On Linux, prefer XDG data directory
    #[cfg(target_os = "linux")]
    if let Some(data_dir) = dirs::data_local_dir() {
        let zen_dir = data_dir.join("zen-garden");
        if fs::create_dir_all(&zen_dir).is_ok() {
            return Ok(zen_dir);
        }
    }

    // Try home directory (works on all platforms)
    if let Some(home) = dirs::home_dir() {
        let zen_dir = home.join(".zen-garden");
        if fs::create_dir_all(&zen_dir).is_ok() {
            return Ok(zen_dir);
        }
    }

    // Linux fallback: /tmp for containers/services without home
    #[cfg(target_os = "linux")]
    {
        let tmp_dir = PathBuf::from("/tmp/zen-garden");
        fs::create_dir_all(&tmp_dir)
            .context("Failed to create /tmp/zen-garden directory")?;
        tracing::warn!("Using /tmp/zen-garden for tending state (no home/XDG available)");
        return Ok(tmp_dir);
    }

    // Non-Linux: error if no home directory
    #[cfg(not(target_os = "linux"))]
    anyhow::bail!("Could not determine home directory for tending state")
}

fn tending_file_path() -> Result<PathBuf> {
    Ok(zen_garden_dir()?.join(".tending"))
}

pub fn read_tending() -> Result<TendingState> {
    let path = tending_file_path()?;
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read tending file: {}", path.display()))?;
    let content = garden_common::utils::strings::strip_bom(&content);
    let state: TendingState = serde_json::from_str(content)
        .context("Failed to parse tending file")?;
    Ok(state)
}

pub fn write_tending(stone_name: String, endpoint: String) -> Result<()> {
    let state = TendingState {
        stone_name,
        endpoint,
        last_seen: SystemTime::now(),
    };
    
    let path = tending_file_path()?;
    let content = serde_json::to_string_pretty(&state)
        .context("Failed to serialize tending state")?;
    fs::write(&path, content)
        .with_context(|| format!("Failed to write tending file: {}", path.display()))?;
    
    tracing::debug!(stone = %state.stone_name, endpoint = %state.endpoint, "Wrote tending state");
    Ok(())
}

pub fn clear_tending() -> Result<()> {
    let path = tending_file_path()?;
    if path.exists() {
        fs::remove_file(&path)
            .with_context(|| format!("Failed to remove tending file: {}", path.display()))?;
        tracing::debug!("Cleared tending state");
    }
    Ok(())
}

// ============================================================================
// Stone Resolution with Fallback
// ============================================================================

/// A stone candidate to try for operations.
/// Used by `execute_on_stone()` to track which stone responded.
#[derive(Debug, Clone)]
pub struct StoneCandidate {
    pub stone_name: String,
    pub endpoint: String,
    /// True if this is the currently tended stone
    pub is_tended: bool,
}

impl StoneCandidate {
    /// Create from a tending state
    pub fn from_tending(state: &TendingState) -> Self {
        Self {
            stone_name: state.stone_name.clone(),
            endpoint: state.endpoint.clone(),
            is_tended: true,
        }
    }

    /// Create from a discovery response
    pub fn from_discovery(response: &garden_common::DiscoveryResponse) -> Self {
        Self {
            stone_name: response.stone_name.clone(),
            endpoint: response.stone_endpoint.clone(),
            is_tended: false,
        }
    }
}

/// Execute an async operation against the best available stone.
///
/// This is the main entry point for stone communication. It handles:
/// 1. Trying the tended stone immediately
/// 2. After 3 seconds, start discovery in parallel (while still waiting for tended)
/// 3. Tended wins if it responds, even if slow
/// 4. If discovery responds first AND tended fails, elect new tended
///
/// # Arguments
/// * `discovery_timeout` - How long to wait for fallback discovery
/// * `on_tended_offline` - Optional callback when tended stone is offline (for UI feedback)
/// * `operation` - Async closure that performs the actual request
///
/// # Returns
/// * `Ok((result, stone))` - The operation succeeded, with the responding stone info
/// * `Err` - No stones available or all stones failed
pub async fn execute_on_stone<F, Fut, T>(
    discovery_timeout: std::time::Duration,
    on_tended_offline: Option<impl Fn(&str)>,
    operation: F,
) -> Result<(T, StoneCandidate)>
where
    F: Fn(&StoneCandidate) -> Fut + Clone,
    Fut: std::future::Future<Output = Result<T, StoneError>>,
{
    use crate::discovery;
    use tokio::time::{sleep, Duration};

    const DISCOVERY_DELAY: Duration = Duration::from_secs(3);
    const TENDED_GRACE_PERIOD: Duration = Duration::from_secs(2);

    // Check if we have a tended stone
    let tended_state = read_tending().ok();
    
    if let Some(ref state) = tended_state {
        let tended = StoneCandidate::from_tending(state);
        tracing::debug!(stone = %tended.stone_name, "Trying tended stone with parallel discovery fallback");

        // Start tended request immediately
        let operation_clone = operation.clone();
        let tended_clone = tended.clone();
        let tended_future = async move {
            operation_clone(&tended_clone).await
        };

        // Start discovery after delay
        let discovery_future = async {
            sleep(DISCOVERY_DELAY).await;
            tracing::debug!("Starting parallel discovery...");
            discovery::discover_moss_auto(discovery_timeout).await
        };

        // Race: tended vs delayed discovery
        tokio::pin!(tended_future);
        tokio::pin!(discovery_future);

        // First, try to get tended result before discovery even starts
        let tended_result = tokio::select! {
            biased; // Prefer tended
            
            result = &mut tended_future => {
                Some(result)
            }
            _ = &mut discovery_future => {
                // Discovery completed first, but give tended a grace period
                None
            }
        };

        // If tended responded, use it
        if let Some(Ok(result)) = tended_result {
            tracing::debug!(stone = %tended.stone_name, "Tended stone responded successfully");
            return Ok((result, tended));
        }

        // If tended had a non-retryable error, fail immediately
        if let Some(Err(e)) = &tended_result {
            if !e.is_retryable() {
                return Err(anyhow::anyhow!("Stone '{}' error: {}", tended.stone_name, e));
            }
            // Retryable error - tended is offline
            tracing::debug!(stone = %tended.stone_name, error = %e, "Tended stone unreachable");
            if let Some(ref callback) = on_tended_offline {
                callback(&tended.stone_name);
            }
        }

        // Tended didn't respond in time or failed - wait for discovery
        // But give tended a grace period after discovery completes
        let discovered = if tended_result.is_none() {
            // Discovery already completed, give tended one more chance
            tokio::select! {
                biased;
                result = &mut tended_future => {
                    match result {
                        Ok(r) => {
                            tracing::debug!(stone = %tended.stone_name, "Tended responded during grace period");
                            return Ok((r, tended));
                        }
                        Err(e) if !e.is_retryable() => {
                            return Err(anyhow::anyhow!("Stone '{}' error: {}", tended.stone_name, e));
                        }
                        Err(e) => {
                            tracing::debug!(stone = %tended.stone_name, error = %e, "Tended failed during grace period");
                            if let Some(ref callback) = on_tended_offline {
                                callback(&tended.stone_name);
                            }
                        }
                    }
                }
                _ = sleep(TENDED_GRACE_PERIOD) => {
                    tracing::debug!(stone = %tended.stone_name, "Tended grace period expired");
                    if let Some(ref callback) = on_tended_offline {
                        callback(&tended.stone_name);
                    }
                }
            }
            
            // Get discovery results (already completed)
            discovery::discover_moss_auto(discovery_timeout).await.unwrap_or_default()
        } else {
            // Tended failed, run discovery now
            match discovery::discover_moss_auto(discovery_timeout).await {
                Ok(stones) => stones,
                Err(e) => {
                    tracing::warn!(error = ?e, "Discovery failed");
                    anyhow::bail!("No stones available: tended offline and discovery failed");
                }
            }
        };

        // Filter out the failed tended stone and try fallbacks
        return try_fallback_stones(discovered, Some(&tended.endpoint), operation).await;
    }

    // No tended stone - just do discovery
    tracing::debug!("No tending state, running discovery...");
    let discovered = match discovery::discover_moss_auto(discovery_timeout).await {
        Ok(stones) => stones,
        Err(e) => {
            tracing::warn!(error = ?e, "Discovery failed");
            anyhow::bail!("No stones available: discovery failed");
        }
    };

    if discovered.is_empty() {
        anyhow::bail!("No stones available: none discovered");
    }

    try_fallback_stones(discovered, None, operation).await
}

/// Try fallback stones from discovery results
async fn try_fallback_stones<F, Fut, T>(
    discovered: Vec<garden_common::DiscoveryResponse>,
    exclude_endpoint: Option<&str>,
    operation: F,
) -> Result<(T, StoneCandidate)>
where
    F: Fn(&StoneCandidate) -> Fut,
    Fut: std::future::Future<Output = Result<T, StoneError>>,
{
    if discovered.is_empty() {
        anyhow::bail!("No stones available: none discovered");
    }

    // Filter out excluded endpoint (failed tended)
    let fallbacks: Vec<StoneCandidate> = discovered
        .into_iter()
        .filter(|r| {
            exclude_endpoint
                .map(|exclude| r.stone_endpoint != exclude)
                .unwrap_or(true)
        })
        .map(|r| StoneCandidate::from_discovery(&r))
        .collect();

    if fallbacks.is_empty() {
        anyhow::bail!("No stones available: tended offline, no alternatives discovered");
    }

    tracing::debug!(count = fallbacks.len(), "Trying fallback stones");

    // Try each fallback
    for candidate in fallbacks {
        match operation(&candidate).await {
            Ok(result) => {
                // Auto-tend to this stone
                tracing::info!(
                    stone = %candidate.stone_name,
                    endpoint = %candidate.endpoint,
                    "Auto-tending to new stone after fallback"
                );
                if let Err(e) = write_tending(candidate.stone_name.clone(), candidate.endpoint.clone()) {
                    tracing::warn!(error = ?e, "Failed to write tending state");
                }
                return Ok((result, candidate));
            }
            Err(e) if e.is_retryable() => {
                tracing::debug!(stone = %candidate.stone_name, error = %e, "Fallback stone unreachable");
                continue;
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Stone '{}' error: {}", candidate.stone_name, e));
            }
        }
    }

    anyhow::bail!("No stones available: all stones failed to respond")
}

/// Discover an alternative stone (for "tend another" command).
///
/// Runs UDP broadcast discovery and excludes the currently tended stone.
/// Returns the first responding alternative, or None if the current stone
/// is the only one available.
///
/// # Arguments
/// * `timeout` - How long to wait for discovery responses
///
/// # Returns
/// * `Ok(Some(candidate))` - Found an alternative stone
/// * `Ok(None)` - No alternatives found (current is the only stone, or no stones at all)
/// * `Err` - Discovery mechanism failed
pub async fn discover_alternative_stone(timeout: std::time::Duration) -> Result<Option<StoneCandidate>> {
    use crate::discovery;

    let current_endpoint = read_tending().ok().map(|t| t.endpoint);

    tracing::debug!("Discovering alternative stones...");
    let discovered = discovery::discover_moss_auto(timeout).await?;

    // Filter out current tended stone
    let alternatives: Vec<_> = discovered
        .into_iter()
        .filter(|r| {
            current_endpoint
                .as_ref()
                .map(|curr| &r.stone_endpoint != curr)
                .unwrap_or(true)
        })
        .collect();

    tracing::debug!(count = alternatives.len(), "Found alternative stones");

    // Return first alternative
    Ok(alternatives.first().map(StoneCandidate::from_discovery))
}

/// Auto-tend to a stone after successful fallback.
///
/// Call this when a non-tended stone successfully handles a request.
/// This updates the tending state so future requests use this stone directly.
///
/// # Arguments
/// * `candidate` - The stone that successfully responded
///
/// # Returns
/// Ok(()) on success, Err if tending state couldn't be written
pub fn auto_tend_to(candidate: &StoneCandidate) -> Result<()> {
    if candidate.is_tended {
        // Already tended, nothing to do
        return Ok(());
    }

    tracing::info!(
        stone = %candidate.stone_name,
        endpoint = %candidate.endpoint,
        "Auto-tending to new stone after fallback"
    );

    write_tending(candidate.stone_name.clone(), candidate.endpoint.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_tending_state_always_valid() {
        // Tending is always valid - no TTL expiration
        let state = TendingState {
            stone_name: "test-stone".to_string(),
            endpoint: "http://127.0.0.1:7185".to_string(),
            last_seen: SystemTime::now(),
        };
        assert!(state.is_valid());
    }

    #[test]
    fn test_tending_state_valid_even_when_old() {
        // Even old tending state is valid - reachability is checked at use time
        let state = TendingState {
            stone_name: "test-stone".to_string(),
            endpoint: "http://127.0.0.1:7185".to_string(),
            last_seen: SystemTime::now() - Duration::from_secs(86400), // 24 hours old
        };
        assert!(state.is_valid());
        assert!(state.age_seconds() >= 86400);
    }
}
