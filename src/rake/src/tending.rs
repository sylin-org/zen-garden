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
    let state: TendingState = serde_json::from_str(&content)
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
/// 1. Trying the tended stone first (instant, no network discovery)
/// 2. If tended fails → UDP broadcast discovery (works on Windows + Linux)
/// 3. Auto-tend to first responder, or fail gracefully ("no stones detected")
///
/// # Arguments
/// * `discovery_timeout` - How long to wait for fallback discovery (only used if needed)
/// * `on_tended_offline` - Optional callback when tended stone is offline (for UI feedback)
/// * `operation` - Async closure that performs the actual request. Returns `Some(T)` on success.
///
/// # Returns
/// * `Ok((result, stone))` - The operation succeeded, with the responding stone info
/// * `Err` - No stones available or all stones failed
///
/// # Example
/// ```ignore
/// let (topology, stone) = tending::execute_on_stone(
///     Duration::from_secs(3),
///     Some(|name| println!("Stone '{}' is offline, trying fallback...", name)),
///     |candidate| async move {
///         let url = format!("{}/api/v1/garden/topology", candidate.endpoint);
///         client.get(&url).send().await.ok()?.json().await.ok()
///     },
/// ).await?;
/// ```
pub async fn execute_on_stone<F, Fut, T>(
    discovery_timeout: std::time::Duration,
    on_tended_offline: Option<impl Fn(&str)>,
    operation: F,
) -> Result<(T, StoneCandidate)>
where
    F: Fn(&StoneCandidate) -> Fut,
    Fut: std::future::Future<Output = Result<T, StoneError>>,
{
    use crate::discovery;

    // 1. Try tended stone first (instant - no discovery)
    if let Ok(tended_state) = read_tending() {
        let tended = StoneCandidate::from_tending(&tended_state);
        tracing::debug!(stone = %tended.stone_name, "Trying tended stone");

        match operation(&tended).await {
            Ok(result) => {
                tracing::debug!(stone = %tended.stone_name, "Tended stone responded successfully");
                return Ok((result, tended));
            }
            Err(e) if e.is_retryable() => {
                // Connection failed - stone might be offline, try fallback
                tracing::debug!(stone = %tended.stone_name, error = %e, "Tended stone unreachable");
                if let Some(ref callback) = on_tended_offline {
                    callback(&tended.stone_name);
                }
            }
            Err(e) => {
                // Non-retryable error - stop here, don't try other stones
                tracing::error!(stone = %tended.stone_name, error = %e, "Operation failed on tended stone");
                return Err(anyhow::anyhow!("Stone '{}' error: {}", tended.stone_name, e));
            }
        }
    }

    // 2. Fallback: discover other stones (slow - only runs if tended failed or doesn't exist)
    tracing::debug!("Running fallback discovery...");
    let tended_endpoint = read_tending().ok().map(|t| t.endpoint);

    let discovered = match discovery::discover_moss_auto(discovery_timeout) {
        Ok(stones) => stones,
        Err(e) => {
            tracing::warn!(error = ?e, "Discovery failed");
            anyhow::bail!("No stones available: discovery failed");
        }
    };

    if discovered.is_empty() && tended_endpoint.is_none() {
        anyhow::bail!("No stones available: none discovered and no tending");
    }

    // Filter out the failed tended stone
    let fallbacks: Vec<StoneCandidate> = discovered
        .into_iter()
        .filter(|r| {
            if let Some(ref exclude) = tended_endpoint {
                &r.stone_endpoint != exclude
            } else {
                true
            }
        })
        .map(|r| StoneCandidate::from_discovery(&r))
        .collect();

    tracing::debug!(count = fallbacks.len(), "Trying fallback stones");

    // 3. Try each fallback
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
                // Connection failed, try next stone
                tracing::debug!(stone = %candidate.stone_name, error = %e, "Fallback stone unreachable");
                continue;
            }
            Err(e) => {
                // Non-retryable error - stop trying
                tracing::error!(stone = %candidate.stone_name, error = %e, "Operation failed on fallback stone");
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
pub fn discover_alternative_stone(timeout: std::time::Duration) -> Result<Option<StoneCandidate>> {
    use crate::discovery;

    let current_endpoint = read_tending().ok().map(|t| t.endpoint);

    tracing::debug!("Discovering alternative stones...");
    let discovered = discovery::discover_moss_auto(timeout)?;

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
