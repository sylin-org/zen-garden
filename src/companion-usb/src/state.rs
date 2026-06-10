//! [`DeviceState`] — the [`super::UsbSerialDevice`] state machine.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceState {
    /// Just opened; no evaluation started.
    New,
    /// Orchestrator claimed the device for probing.
    Evaluating,
    /// Probe succeeded; an adapter of `kind` is driving the device.
    Accepted { kind: String },
    /// Probe failed; device is known-unusable until physical detach.
    Rejected { reason: String },
    /// fd closed; observers should unwind. Terminal.
    Disposed,
}

#[derive(Debug, Error)]
pub enum StateError {
    #[error("invalid transition from {from:?} to {to}")]
    InvalidTransition { from: DeviceState, to: &'static str },
}

impl DeviceState {
    /// Legal transition from `self` to `Evaluating`.
    pub(super) fn can_begin_evaluation(&self) -> bool {
        matches!(self, DeviceState::New)
    }

    pub(super) fn can_accept(&self) -> bool {
        matches!(self, DeviceState::Evaluating)
    }

    pub(super) fn can_reject(&self) -> bool {
        matches!(self, DeviceState::Evaluating | DeviceState::New)
    }

    pub(super) fn is_disposed(&self) -> bool {
        matches!(self, DeviceState::Disposed)
    }
}
