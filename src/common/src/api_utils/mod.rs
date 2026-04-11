//! API utilities for HTTP handlers
//!
//! Provides:
//! - Standard error response formatting
//! - Standard response wrappers (ApiResponse<T>)
//! - SSE (Server-Sent Events) streaming helpers
//! - Input sanitization for query parameters and names

pub mod errors;
pub mod responses;
pub mod sanitize;
pub mod sse;

pub use errors::{ApiErrorResponse, bad_request, error_response, internal_error, not_found};
pub use responses::ApiResponse;
pub use sanitize::{
    MAX_NAME_LENGTH, MAX_QUERY_LENGTH, MAX_TAG_LENGTH, SanitizeResult, is_suspicious,
    sanitize_fqn_input, sanitize_name, sanitize_path_segment, sanitize_query, sanitize_tag,
    validate_name,
};
pub use sse::{SseEvent, sse_stream};
