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

pub use errors::{bad_request, error_response, internal_error, not_found, ApiErrorResponse};
pub use responses::ApiResponse;
pub use sanitize::{
    is_suspicious, sanitize_name, sanitize_name_allow_colon, sanitize_path_segment, sanitize_query,
    sanitize_tag, validate_name, SanitizeResult, MAX_NAME_LENGTH, MAX_QUERY_LENGTH, MAX_TAG_LENGTH,
};
pub use sse::{sse_stream, SseEvent};
