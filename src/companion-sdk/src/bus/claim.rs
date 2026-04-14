//! Claim engine — picks the winning [`AdapterRegistration`] for a
//! device based on descriptor specificity.
//!
//! Rules (COMPANION-0012):
//!
//! 1. Evaluate every registration's predicate against the parsed
//!    [`Identification`].
//! 2. Keep only registrations whose resource class matches the device
//!    class and whose predicate returned `Some(score)`.
//! 3. Rank by score descending. Ties broken by registration-list
//!    order (earlier registration wins). This matches "specificity
//!    wins; documented order for tie-breaks" from the ADR.
//! 4. The winner is invoked with the opened device and descriptor.
//!    Rest are not contacted.

use super::descriptor::Identification;
use super::registration::AdapterRegistration;
use super::resource::ResourceClass;

/// Outcome of a claim round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimOutcome<'a> {
    /// A registration claimed the device. The caller invokes
    /// `registrations[index].build(opened_device, identification)`.
    Claimed {
        /// Index into the registration list passed in.
        index: usize,
        /// Name of the winning registration (copy of `.name` for
        /// logging/telemetry without reaching back through the index).
        name: &'a str,
        /// Specificity score that won.
        score: u32,
    },
    /// No registration matched. Caller emits `unclaimed` telemetry and
    /// places the port on backoff.
    Unmatched,
}

/// Pick the winner from `registrations` given the device's class and
/// parsed identification.
///
/// `device_class` is the enumerator-sourced class of the device
/// being claimed (e.g. `ResourceClass::UsbSerial { vid, pid }` with
/// concrete values). Each registration's `resource` is compared for
/// compatibility before its predicate runs.
pub fn pick_winner<'a>(
    registrations: &'a [AdapterRegistration],
    device_class: &ResourceClass,
    identification: &Identification,
) -> ClaimOutcome<'a> {
    let mut best: Option<(usize, &AdapterRegistration, u32)> = None;
    for (i, reg) in registrations.iter().enumerate() {
        if !class_compatible(&reg.resource, device_class) {
            continue;
        }
        let Some(score) = reg.score(identification) else {
            continue;
        };
        match &best {
            Some((_, _, best_score)) if score <= *best_score => {
                // Keep current best; ties go to earlier-registered.
                continue;
            }
            _ => {
                best = Some((i, reg, score));
            }
        }
    }
    match best {
        Some((i, reg, score)) => ClaimOutcome::Claimed {
            index: i,
            name: reg.name,
            score,
        },
        None => ClaimOutcome::Unmatched,
    }
}

/// `true` when a registration's declared resource class accepts the
/// device's actual class. Same variant required; USB VID/PID filters
/// on the registration must match the device's concrete VID/PID.
fn class_compatible(registration: &ResourceClass, device: &ResourceClass) -> bool {
    match (registration, device) {
        (
            ResourceClass::UsbSerial {
                vid: rv,
                pid: rp,
            },
            ResourceClass::UsbSerial {
                vid: Some(dv),
                pid: Some(dp),
            },
        ) => rv.is_none_or(|v| v == *dv) && rp.is_none_or(|p| p == *dp),
        // Registration scoped to a specific VID/PID, but enumerator
        // didn't surface concrete ids — treat as incompatible so we
        // don't guess.
        (ResourceClass::UsbSerial { .. }, _) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::descriptor::Identification;
    use super::super::predicate::Predicate;
    use super::super::resource::ResourceClass;
    use super::*;
    use crate::adapters::{Adapter, AdapterInfo, AdapterProfile, adapter::BoxFuture};
    use crate::garden::{Event, Garden, Pulse};
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    struct StubAdapter(&'static str);
    impl Adapter for StubAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                kind: self.0,
                id: "stub".into(),
                device: None,
            }
        }
        fn profile(&self) -> AdapterProfile {
            AdapterProfile::default()
        }
        fn run(
            self: Box<Self>,
            _e: mpsc::Receiver<Event>,
            _g: Arc<Garden>,
            _p: Arc<Pulse>,
            _s: CancellationToken,
        ) -> BoxFuture<'static, ()> {
            Box::pin(async {})
        }
    }

    fn reg(name: &'static str, interest: Predicate) -> AdapterRegistration {
        AdapterRegistration::new(
            name,
            ResourceClass::UsbSerial {
                vid: Some(0x1a86),
                pid: None,
            },
            interest,
            move |_opened, _id| Box::new(StubAdapter(name)),
        )
    }

    fn device_class() -> ResourceClass {
        ResourceClass::UsbSerial {
            vid: Some(0x1a86),
            pid: Some(0x55d4),
        }
    }

    fn identification(fields: serde_json::Value) -> Identification {
        Identification::from_json("firefly", fields).unwrap()
    }

    #[test]
    fn picks_highest_score() {
        let v1_reg = reg(
            "oled-v1",
            Predicate::AllOf(vec![
                Predicate::eq("family", "firefly"),
                Predicate::eq("variant", "oled"),
            ]),
        );
        let v2_reg = reg(
            "oled-v2",
            Predicate::AllOf(vec![
                Predicate::eq("family", "firefly"),
                Predicate::eq("variant", "oled"),
                Predicate::has_capability("dashboard"),
            ]),
        );
        let regs = vec![v1_reg, v2_reg];
        let id = identification(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "family": "firefly",
            "variant": "oled",
            "capabilities": ["dashboard"],
        }));
        let outcome = pick_winner(&regs, &device_class(), &id);
        match outcome {
            ClaimOutcome::Claimed { name, index, score } => {
                assert_eq!(name, "oled-v2");
                assert_eq!(index, 1);
                assert_eq!(score, 3);
            }
            _ => panic!("expected claimed"),
        }
    }

    #[test]
    fn tie_goes_to_earlier_registered() {
        let a = reg("a", Predicate::eq("family", "firefly"));
        let b = reg("b", Predicate::eq("family", "firefly"));
        let regs = vec![a, b];
        let id = identification(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "family": "firefly",
        }));
        match pick_winner(&regs, &device_class(), &id) {
            ClaimOutcome::Claimed { name, .. } => assert_eq!(name, "a"),
            _ => panic!("expected claimed"),
        }
    }

    #[test]
    fn no_match_returns_unmatched() {
        let r = reg("no-match", Predicate::eq("variant", "nonexistent"));
        let regs = vec![r];
        let id = identification(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "variant": "oled",
        }));
        assert_eq!(
            pick_winner(&regs, &device_class(), &id),
            ClaimOutcome::Unmatched
        );
    }

    #[test]
    fn class_filter_rejects_wrong_vid() {
        let r = AdapterRegistration::new(
            "wrong-vid",
            ResourceClass::UsbSerial {
                vid: Some(0x2e8a),
                pid: None,
            },
            Predicate::eq("family", "firefly"),
            |_, _| Box::new(StubAdapter("wrong-vid")),
        );
        let regs = vec![r];
        let id = identification(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "family": "firefly",
        }));
        assert_eq!(
            pick_winner(&regs, &device_class(), &id),
            ClaimOutcome::Unmatched
        );
    }

    #[test]
    fn device_missing_concrete_vid_pid_rejects() {
        let r = reg("any-ch340", Predicate::eq("family", "firefly"));
        let regs = vec![r];
        let id = identification(json!({
            "device_id": "01938abc-de01-7234-89ab-cdef01234567",
            "family": "firefly",
        }));
        let vague = ResourceClass::UsbSerial {
            vid: None,
            pid: None,
        };
        assert_eq!(
            pick_winner(&regs, &vague, &id),
            ClaimOutcome::Unmatched
        );
    }
}
