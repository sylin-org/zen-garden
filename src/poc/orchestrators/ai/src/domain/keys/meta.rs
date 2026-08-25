//! Response metadata — present on every response envelope.

use crate::domain::field_path::FieldPath;

pub const CORRELATION_ID: FieldPath = FieldPath::new("meta.correlation_id");
pub const REQUEST_ID: FieldPath = FieldPath::new("meta.request_id");
pub const ACTION: FieldPath = FieldPath::new("meta.action");
pub const PROVIDER: FieldPath = FieldPath::new("meta.provider");
pub const MODEL: FieldPath = FieldPath::new("meta.model");
pub const MODE: FieldPath = FieldPath::new("meta.mode");
pub const IDEMPOTENT: FieldPath = FieldPath::new("meta.idempotent");

pub const RESOLUTION_PATH: FieldPath = FieldPath::new("meta.resolution.path");
pub const REQUESTED_PROVIDER: FieldPath = FieldPath::new("meta.resolution.requested_provider");
pub const REQUESTED_MODEL: FieldPath = FieldPath::new("meta.resolution.requested_model");
pub const IGNORED_FIELDS: FieldPath = FieldPath::new("meta.ignored_fields");

pub mod values {
    pub const MODE_SYNC: &str = "sync";
    pub const MODE_ASYNC: &str = "async";
    pub const MODE_STREAM: &str = "stream";
}
