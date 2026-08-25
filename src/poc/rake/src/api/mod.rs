//! API client utilities
//!
//! Provides typed response parsing and HTTP helpers to eliminate
//! repetitive JSON extraction patterns in command handlers.

pub mod responses;

pub use responses::{
    extract_array, extract_bool, extract_data, extract_services, extract_string, ApiResult,
};
