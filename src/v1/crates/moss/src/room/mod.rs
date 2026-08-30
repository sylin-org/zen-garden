//! The room context (ADR-0015): the stone among stones — its voice,
//! the wire plane it speaks, and the presence cache it keeps.

pub mod voice;

pub use garden_room::{announce, config, dispatch, ingress, pipeline, probe, responder, topology};
