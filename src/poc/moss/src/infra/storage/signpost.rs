//! SMB signpost share — discovery billboard for network browsers (STORAGE-0009 Phase 6)
//!
//! Generates a lightweight Samba share configuration and `.url` shortcut files
//! that point to WebDAV endpoints for each managed storage. This makes storage
//! discoverable in Windows Explorer, macOS Finder, and Linux file managers via
//! mDNS (`_smb._tcp.local`).
//!
//! The signpost share is read-only and contains only `.url` files — it does NOT
//! serve actual storage content. All real I/O goes through the WebDAV/S3 APIs.
//!
//! ## Directory Layout
//!
//! ```text
//! {data_dir}/signpost/
//!   ├── Storage - {name1}.url
//!   ├── Storage - {name2}.url
//!   └── Zen Garden Dashboard.url
//! ```
//!
//! ## Samba Config Fragment
//!
//! Written to `{data_dir}/signpost/smb.conf.fragment` — intended for inclusion
//! via `include = /var/lib/zen-garden/signpost/smb.conf.fragment` in the system's
//! `smb.conf`. The stone provisioning or setup step is responsible for the include.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Signpost directory name under `data_dir`.
const SIGNPOST_DIR: &str = "signpost";

/// Samba config fragment filename.
const SMB_CONF_FRAGMENT: &str = "smb.conf.fragment";

/// Generate a `.url` shortcut file body (Windows INI-style).
///
/// Works in Windows Explorer, macOS Finder (via Spotlight), and most Linux file
/// managers that support `.url` / `.desktop` shortcuts.
fn url_shortcut(url: &str) -> String {
    format!("[InternetShortcut]\r\nURL={}\r\n", url)
}

/// Refresh the signpost directory: regenerate `.url` files and Samba config.
///
/// Call this after storage state changes (adopt, prepare, remove, rename, beacon).
/// Idempotent — safe to call repeatedly.
pub async fn refresh_signpost(
    stone_name: &str,
    api_port: u16,
    storages: &[(String, String)], // Vec<(storage_name, storage_id)>
) -> Result<()> {
    let signpost_dir =
        PathBuf::from(garden_common::constants::paths::data_dir()).join(SIGNPOST_DIR);

    // Ensure directory exists
    tokio::fs::create_dir_all(&signpost_dir)
        .await
        .context("Failed to create signpost directory")?;

    // Clean existing .url files
    if let Ok(mut rd) = tokio::fs::read_dir(&signpost_dir).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            let p: PathBuf = entry.path();
            if p.extension().is_some_and(|e| e == "url") {
                let _ = tokio::fs::remove_file(&p).await;
            }
        }
    }

    let base_url = format!("http://{}:{}", stone_name, api_port);

    // Generate per-storage .url files
    for (name, _id) in storages {
        let filename = format!("Storage - {}.url", sanitize_filename(name));
        let url = format!("{}/api/v1/stone/storage/banks/{}", base_url, name);
        let content = url_shortcut(&url);
        tokio::fs::write(signpost_dir.join(&filename), content.as_bytes()).await?;
    }

    // Dashboard shortcut
    let dashboard_url = url_shortcut(&base_url);
    tokio::fs::write(
        signpost_dir.join("Zen Garden Dashboard.url"),
        dashboard_url.as_bytes(),
    )
    .await?;

    // Generate Samba config fragment
    write_smb_fragment(&signpost_dir, stone_name).await?;

    info!(count = storages.len(), "Signpost share refreshed");
    Ok(())
}

/// Write the Samba config fragment for the signpost share.
async fn write_smb_fragment(signpost_dir: &Path, stone_name: &str) -> Result<()> {
    let share_name = format!("{} Storage", stone_name);
    let path_str = signpost_dir.to_string_lossy();

    let config = format!(
        r#"# Zen Garden signpost share — auto-generated, do not edit
# Include this in /etc/samba/smb.conf:
#   include = {path}

[{share}]
    comment = Zen Garden Storage Links
    path = {path}
    browseable = yes
    read only = yes
    guest ok = yes
    vfs objects = fruit streams_xattr
    fruit:encoding = native
"#,
        share = share_name,
        path = path_str,
    );

    let fragment_path = signpost_dir.join(SMB_CONF_FRAGMENT);
    tokio::fs::write(&fragment_path, config.as_bytes()).await?;
    debug!(path = %fragment_path.display(), "Samba config fragment written");
    Ok(())
}

/// Remove characters unsafe for filenames.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_shortcut_format() {
        let content = url_shortcut("http://stone-01:7185/api/v1/stone/storage/banks/my-data");
        assert!(content.starts_with("[InternetShortcut]\r\n"));
        assert!(content.contains("URL=http://stone-01:7185/api/v1/stone/storage/banks/my-data"));
        assert!(content.ends_with("\r\n"));
    }

    #[test]
    fn test_sanitize_filename_clean_name() {
        assert_eq!(sanitize_filename("my-seed-bank"), "my-seed-bank");
        assert_eq!(sanitize_filename("backup_2026"), "backup_2026");
    }

    #[test]
    fn test_sanitize_filename_replaces_unsafe_chars() {
        assert_eq!(sanitize_filename("path/to:file"), "path_to_file");
        assert_eq!(sanitize_filename("a*b?c\"d"), "a_b_c_d");
        assert_eq!(sanitize_filename("a<b>c|d"), "a_b_c_d");
        assert_eq!(sanitize_filename("back\\slash"), "back_slash");
    }

    #[test]
    fn test_sanitize_filename_preserves_spaces_and_dots() {
        assert_eq!(sanitize_filename("my storage.v2"), "my storage.v2");
    }

    #[tokio::test]
    async fn test_refresh_signpost_creates_url_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        let signpost_dir = tmp.path().join("signpost");
        tokio::fs::create_dir_all(&signpost_dir).await.unwrap();

        let storages = vec![
            ("photos".to_string(), "id-001".to_string()),
            ("backups".to_string(), "id-002".to_string()),
        ];

        // We can't use refresh_signpost directly (it uses data_dir()),
        // so test the individual pieces instead.

        // Write .url files manually as refresh_signpost would
        let base_url = "http://stone-test:7185";
        for (name, _id) in &storages {
            let filename = format!("Storage - {}.url", sanitize_filename(name));
            let url = format!("{}/api/v1/stone/storage/banks/{}", base_url, name);
            let content = url_shortcut(&url);
            tokio::fs::write(signpost_dir.join(&filename), content.as_bytes())
                .await
                .unwrap();
        }

        // Verify files were created
        let photos_path = signpost_dir.join("Storage - photos.url");
        assert!(photos_path.exists());
        let content = tokio::fs::read_to_string(&photos_path).await.unwrap();
        assert!(content.contains("stone-test:7185"));
        assert!(content.contains("/banks/photos"));

        let backups_path = signpost_dir.join("Storage - backups.url");
        assert!(backups_path.exists());
    }

    #[tokio::test]
    async fn test_smb_fragment_content() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_smb_fragment(tmp.path(), "stone-crystal")
            .await
            .unwrap();

        let fragment = tokio::fs::read_to_string(tmp.path().join(SMB_CONF_FRAGMENT))
            .await
            .unwrap();
        assert!(fragment.contains("[stone-crystal Storage]"));
        assert!(fragment.contains("browseable = yes"));
        assert!(fragment.contains("read only = yes"));
        assert!(fragment.contains("guest ok = yes"));
    }
}
