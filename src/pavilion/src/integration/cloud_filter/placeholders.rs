//! Cloud Filter placeholder builders.
//!
//! Lifted from `src/moss/src/infra/cloud_filter/placeholders.rs`,
//! trimmed for Pavilion's simpler model: a single tended stone is the
//! source of truth for storage availability, so we don't track online/
//! offline transitions or maintain a registry of replica sets here. The
//! placeholder builders themselves are the same — they hand
//! `cloud-filter` the file/dir metadata Explorer needs to render entries
//! and trigger `fetch_data` on access.

use cloud_filter::metadata::Metadata;
use cloud_filter::placeholder_file::PlaceholderFile;
use nt_time::FileTime;

/// Availability of a storage as seen through the tended stone.
///
/// `online` is `true` whenever the garden's `list()` response includes
/// the storage with at least one replica. `stone_name` is the Primary
/// stone's display name (or `"unknown"` when the listing didn't pin one).
#[derive(Debug, Clone)]
pub struct StorageAvailability {
    pub online: bool,
    pub stone_name: String,
}

impl StorageAvailability {
    pub fn online(stone_name: impl Into<String>) -> Self {
        Self {
            online: true,
            stone_name: stone_name.into(),
        }
    }
}

/// Build a `PlaceholderFile` for a **storage root directory**.
///
/// Sets IN_SYNC when the storage is reachable. Encodes the stone name
/// in the blob so any future signaling layer can surface it without a
/// re-query.
pub fn build_storage_dir_placeholder(name: &str, avail: &StorageAvailability) -> PlaceholderFile {
    let now = FileTime::now();
    let blob = encode_storage_blob(name, avail);

    let ph = PlaceholderFile::new(name)
        .metadata(Metadata::directory().created(now).written(now).size(0))
        .blob(blob);

    if avail.online { ph.mark_in_sync() } else { ph }
}

/// Build a `PlaceholderFile` for a **file or sub-directory inside a storage**.
///
/// Always marked IN_SYNC — these are only created when the storage is
/// reachable, so the metadata is valid at creation time.
pub fn build_placeholder(name: &str, is_dir: bool, size: u64) -> PlaceholderFile {
    let now = FileTime::now();

    if is_dir {
        PlaceholderFile::new(name)
            .mark_in_sync()
            .metadata(Metadata::directory().created(now).written(now).size(0))
            .blob(name.into())
    } else {
        PlaceholderFile::new(name)
            .has_no_children()
            .mark_in_sync()
            .metadata(Metadata::file().created(now).written(now).size(size))
            .blob(name.into())
    }
}

/// Encode storage availability metadata as compact JSON bytes for the blob.
///
/// Schema: `{"n":"storage","s":"stone-golden-summit"}`.
fn encode_storage_blob(name: &str, avail: &StorageAvailability) -> Vec<u8> {
    let json = serde_json::json!({
        "n": name,
        "s": avail.stone_name,
    });
    json.to_string().into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_constructor_sets_fields() {
        let avail = StorageAvailability::online("stone-alpha");
        assert!(avail.online);
        assert_eq!(avail.stone_name, "stone-alpha");
    }

    #[test]
    fn online_constructor_accepts_owned_string() {
        // The `Into<String>` bound covers both &str and String inputs;
        // the rest of the call sites mix both.
        let stone: String = "stone-beta".to_string();
        let avail = StorageAvailability::online(stone);
        assert_eq!(avail.stone_name, "stone-beta");
    }

    #[test]
    fn blob_encodes_compact_json_with_n_and_s_fields() {
        let avail = StorageAvailability::online("stone-golden-summit");
        let blob = encode_storage_blob("storage", &avail);
        let text = std::str::from_utf8(&blob).expect("blob is utf-8");
        let json: serde_json::Value = serde_json::from_str(text).expect("blob is JSON");
        assert_eq!(json["n"], "storage");
        assert_eq!(json["s"], "stone-golden-summit");
    }

    #[test]
    fn blob_round_trips_special_characters_in_storage_name() {
        // Replica set names can include `::`; the JSON encoding must
        // preserve them without escaping artifacts.
        let avail = StorageAvailability::online("stone-alpha");
        let blob = encode_storage_blob("storage::personal", &avail);
        let json: serde_json::Value = serde_json::from_slice(&blob).expect("blob is JSON");
        assert_eq!(json["n"], "storage::personal");
    }

    #[test]
    fn build_storage_dir_placeholder_constructs_without_panic_when_online() {
        let avail = StorageAvailability::online("stone-alpha");
        // We can't introspect PlaceholderFile internals (CfApi types
        // expose no public accessors), but we can prove the builder
        // accepts the inputs and produces a value without panicking.
        let _ph = build_storage_dir_placeholder("storage", &avail);
    }

    #[test]
    fn build_storage_dir_placeholder_constructs_without_panic_when_offline() {
        let offline = StorageAvailability {
            online: false,
            stone_name: String::new(),
        };
        let _ph = build_storage_dir_placeholder("storage", &offline);
    }

    #[test]
    fn build_placeholder_constructs_for_files_and_directories() {
        let _file = build_placeholder("readme.txt", false, 42);
        let _dir = build_placeholder("photos", true, 0);
    }
}
