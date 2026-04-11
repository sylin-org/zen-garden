//! Cross-platform read/write of environment variables for adopted (bare-metal)
//! services.
//!
//! Each platform has its own mechanism:
//! - **Linux**: `/etc/default/{service}` (KEY=VALUE env file)
//! - **Windows**: Machine-scoped env vars in the registry
//! - **macOS**: `launchctl setenv` / `launchctl getenv`
//!
//! See ADR MOSS-0005 for the full design.

#[cfg(any(target_os = "linux", target_os = "macos"))]
use anyhow::Context;
use anyhow::Result;
use std::collections::HashMap;

/// Read the current values of the given environment variable names
/// for an adopted bare-metal service.
///
/// Returns only the vars that are currently set; missing vars are omitted.
pub async fn read_env(service_name: &str, var_names: &[String]) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for name in var_names {
        if let Some(val) = read_var(service_name, name).await {
            result.insert(name.clone(), val);
        }
    }
    result
}

/// Write a set of environment variables for an adopted bare-metal service.
///
/// A `None` value deletes the variable (reverts to default).
pub async fn write_env(service_name: &str, vars: &HashMap<String, Option<String>>) -> Result<()> {
    for (name, value) in vars {
        match value {
            Some(val) => write_var(service_name, name, val).await?,
            None => delete_var(service_name, name).await?,
        }
    }
    Ok(())
}

// ── Linux ───────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
async fn read_var(service_name: &str, var_name: &str) -> Option<String> {
    // Try /etc/default/{service} first (standard Debian/Ubuntu pattern)
    let env_file = format!("/etc/default/{service_name}");
    if let Ok(content) = tokio::fs::read_to_string(&env_file).await {
        if let Some(val) = parse_env_file_var(&content, var_name) {
            return Some(val);
        }
    }

    // Fall back to systemd override
    let override_file = format!("/etc/systemd/system/{service_name}.service.d/zen-garden-env.conf");
    if let Ok(content) = tokio::fs::read_to_string(&override_file).await {
        if let Some(val) = parse_systemd_env_var(&content, var_name) {
            return Some(val);
        }
    }

    None
}

#[cfg(target_os = "linux")]
async fn write_var(service_name: &str, var_name: &str, value: &str) -> Result<()> {
    let env_file = format!("/etc/default/{service_name}");

    // If /etc/default/{service} exists, update it (merge semantics)
    if tokio::fs::metadata(&env_file).await.is_ok() {
        let content = tokio::fs::read_to_string(&env_file)
            .await
            .unwrap_or_default();
        let updated = upsert_env_file_var(&content, var_name, value);
        tokio::fs::write(&env_file, updated)
            .await
            .with_context(|| format!("failed to write {env_file}"))?;
        return Ok(());
    }

    // Otherwise, use a systemd drop-in override
    let override_dir = format!("/etc/systemd/system/{service_name}.service.d");
    tokio::fs::create_dir_all(&override_dir)
        .await
        .with_context(|| format!("failed to create {override_dir}"))?;

    let override_file = format!("{override_dir}/zen-garden-env.conf");
    let existing = tokio::fs::read_to_string(&override_file)
        .await
        .unwrap_or_default();
    let updated = upsert_systemd_env_var(&existing, var_name, value);
    tokio::fs::write(&override_file, updated)
        .await
        .with_context(|| format!("failed to write {override_file}"))?;

    // Reload systemd so it picks up the new override
    let _ = tokio::process::Command::new("systemctl")
        .arg("daemon-reload")
        .output()
        .await;

    Ok(())
}

#[cfg(target_os = "linux")]
async fn delete_var(service_name: &str, var_name: &str) -> Result<()> {
    // Remove from /etc/default/{service} if present
    let env_file = format!("/etc/default/{service_name}");
    if let Ok(content) = tokio::fs::read_to_string(&env_file).await {
        let updated = remove_env_file_var(&content, var_name);
        tokio::fs::write(&env_file, updated)
            .await
            .with_context(|| format!("failed to write {env_file}"))?;
    }

    // Remove from systemd override if present
    let override_file = format!("/etc/systemd/system/{service_name}.service.d/zen-garden-env.conf");
    if let Ok(content) = tokio::fs::read_to_string(&override_file).await {
        let updated = remove_systemd_env_var(&content, var_name);
        tokio::fs::write(&override_file, updated)
            .await
            .with_context(|| format!("failed to write {override_file}"))?;
        let _ = tokio::process::Command::new("systemctl")
            .arg("daemon-reload")
            .output()
            .await;
    }

    Ok(())
}

// ── Linux env-file helpers ──────────────────────────────────────

/// Parse a KEY=VALUE env file and extract a specific variable.
#[cfg(target_os = "linux")]
fn parse_env_file_var(content: &str, var_name: &str) -> Option<String> {
    let prefix = format!("{var_name}=");
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if let Some(val) = trimmed.strip_prefix(&prefix) {
            // Strip optional quotes
            return Some(unquote(val));
        }
    }
    None
}

/// Upsert a variable in a KEY=VALUE env file (preserves other lines).
#[cfg(target_os = "linux")]
fn upsert_env_file_var(content: &str, var_name: &str, value: &str) -> String {
    let prefix = format!("{var_name}=");
    let new_line = format!("{var_name}={value}");
    let mut found = false;
    let mut lines: Vec<String> = content
        .lines()
        .map(|line| {
            if line.trim().starts_with(&prefix) {
                found = true;
                new_line.clone()
            } else {
                line.to_string()
            }
        })
        .collect();
    if !found {
        lines.push(new_line);
    }
    let mut result = lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Remove a variable from a KEY=VALUE env file.
#[cfg(target_os = "linux")]
fn remove_env_file_var(content: &str, var_name: &str) -> String {
    let prefix = format!("{var_name}=");
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().starts_with(&prefix))
        .collect();
    let mut result = lines.join("\n");
    if !result.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    result
}

// ── Linux systemd override helpers ──────────────────────────────

/// Parse a systemd drop-in for `Environment=VAR=VALUE` lines.
#[cfg(target_os = "linux")]
fn parse_systemd_env_var(content: &str, var_name: &str) -> Option<String> {
    let prefix = format!("Environment={var_name}=");
    let prefix_quoted = format!("Environment=\"{var_name}=");
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix(&prefix) {
            return Some(unquote(val));
        }
        if let Some(rest) = trimmed.strip_prefix(&prefix_quoted) {
            // Environment="VAR=value"
            return Some(unquote(rest.trim_end_matches('"')));
        }
    }
    None
}

/// Upsert a variable in a systemd drop-in override file.
#[cfg(target_os = "linux")]
fn upsert_systemd_env_var(content: &str, var_name: &str, value: &str) -> String {
    let prefix = format!("Environment={var_name}=");
    let prefix_quoted = format!("Environment=\"{var_name}=");
    let new_line = format!("Environment={var_name}={value}");

    if content.is_empty() {
        return format!("[Service]\n{new_line}\n");
    }

    let mut found = false;
    let lines: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with(&prefix) || trimmed.starts_with(&prefix_quoted) {
                found = true;
                new_line.clone()
            } else {
                line.to_string()
            }
        })
        .collect();

    let mut result = lines.join("\n");
    if !found {
        // Append after [Service] section or at end
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(&new_line);
        result.push('\n');
    }
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Remove a variable from a systemd drop-in override file.
#[cfg(target_os = "linux")]
fn remove_systemd_env_var(content: &str, var_name: &str) -> String {
    let prefix = format!("Environment={var_name}=");
    let prefix_quoted = format!("Environment=\"{var_name}=");
    let lines: Vec<&str> = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with(&prefix) && !trimmed.starts_with(&prefix_quoted)
        })
        .collect();
    let mut result = lines.join("\n");
    if !result.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    result
}

/// Strip surrounding quotes from a value.
#[cfg(target_os = "linux")]
fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

// ── Windows ─────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
async fn read_var(_service_name: &str, var_name: &str) -> Option<String> {
    super::registry::read_machine_env(var_name)
}

#[cfg(target_os = "windows")]
async fn write_var(_service_name: &str, var_name: &str, value: &str) -> Result<()> {
    super::registry::write_machine_env(var_name, value)
}

#[cfg(target_os = "windows")]
async fn delete_var(_service_name: &str, var_name: &str) -> Result<()> {
    super::registry::delete_machine_env(var_name)
}

// ── macOS ───────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
async fn read_var(_service_name: &str, var_name: &str) -> Option<String> {
    let output = tokio::process::Command::new("launchctl")
        .args(["getenv", var_name])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if val.is_empty() { None } else { Some(val) }
}

#[cfg(target_os = "macos")]
async fn write_var(_service_name: &str, var_name: &str, value: &str) -> Result<()> {
    let output = tokio::process::Command::new("launchctl")
        .args(["setenv", var_name, value])
        .output()
        .await
        .context("failed to run launchctl setenv")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("launchctl setenv failed: {stderr}");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn delete_var(_service_name: &str, var_name: &str) -> Result<()> {
    let output = tokio::process::Command::new("launchctl")
        .args(["unsetenv", var_name])
        .output()
        .await
        .context("failed to run launchctl unsetenv")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::debug!(var = %var_name, error = %stderr, "launchctl unsetenv failed (may not exist)");
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;

    #[test]
    fn parse_env_file() {
        let content = "# comment\nFOO=bar\nBAZ=42\n";
        assert_eq!(parse_env_file_var(content, "FOO"), Some("bar".into()));
        assert_eq!(parse_env_file_var(content, "BAZ"), Some("42".into()));
        assert_eq!(parse_env_file_var(content, "MISSING"), None);
    }

    #[test]
    fn parse_quoted_env_file() {
        let content = "FOO=\"hello world\"\nBAR='single'\n";
        assert_eq!(
            parse_env_file_var(content, "FOO"),
            Some("hello world".into())
        );
        assert_eq!(parse_env_file_var(content, "BAR"), Some("single".into()));
    }

    #[test]
    fn upsert_env_file_new() {
        let content = "EXISTING=yes\n";
        let result = upsert_env_file_var(content, "NEW", "value");
        assert!(result.contains("EXISTING=yes"));
        assert!(result.contains("NEW=value"));
    }

    #[test]
    fn upsert_env_file_replace() {
        let content = "FOO=old\nBAR=keep\n";
        let result = upsert_env_file_var(content, "FOO", "new");
        assert!(result.contains("FOO=new"));
        assert!(result.contains("BAR=keep"));
        assert!(!result.contains("FOO=old"));
    }

    #[test]
    fn remove_env_file() {
        let content = "FOO=1\nBAR=2\nBAZ=3\n";
        let result = remove_env_file_var(content, "BAR");
        assert!(result.contains("FOO=1"));
        assert!(!result.contains("BAR=2"));
        assert!(result.contains("BAZ=3"));
    }

    #[test]
    fn systemd_override_roundtrip() {
        let empty = "";
        let with_one = upsert_systemd_env_var(empty, "OLLAMA_NUM_PARALLEL", "4");
        assert!(with_one.contains("[Service]"));
        assert!(with_one.contains("Environment=OLLAMA_NUM_PARALLEL=4"));

        let parsed = parse_systemd_env_var(&with_one, "OLLAMA_NUM_PARALLEL");
        assert_eq!(parsed, Some("4".into()));

        let with_two = upsert_systemd_env_var(&with_one, "OLLAMA_HOST", "0.0.0.0:11434");
        assert!(with_two.contains("OLLAMA_NUM_PARALLEL=4"));
        assert!(with_two.contains("OLLAMA_HOST=0.0.0.0:11434"));

        let after_remove = remove_systemd_env_var(&with_two, "OLLAMA_NUM_PARALLEL");
        assert!(!after_remove.contains("OLLAMA_NUM_PARALLEL"));
        assert!(after_remove.contains("OLLAMA_HOST"));
    }
}
