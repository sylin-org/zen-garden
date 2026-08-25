//! Ceremony phase implementations
//!
//! Each phase is a discrete step in a ceremony:
//! - collect: Create backup (harvest) before changes
//! - nourish: Pull new image, recreate container
//! - water: Start service, verify health, rollback if needed

pub mod collect;
pub mod nourish;
pub mod water;
