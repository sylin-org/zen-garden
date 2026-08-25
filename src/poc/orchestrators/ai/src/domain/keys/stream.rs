//! Streaming delta envelope fields.

use crate::domain::field_path::FieldPath;

/// Delta chunk payload. The concrete type depends on the primitive:
/// for `text.chat` it is the appended text; for audio it is a base64
/// binary frame.
pub const CHUNK: FieldPath = FieldPath::new("stream.chunk");
/// Monotonic sequence number within a stream, starting at 0.
pub const SEQUENCE: FieldPath = FieldPath::new("stream.sequence");
/// Total number of chunks expected, if known at open-time.
pub const TOTAL_CHUNKS: FieldPath = FieldPath::new("stream.total_chunks");
