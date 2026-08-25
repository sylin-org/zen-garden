//! Uniform event envelope — the foundation type every companion-internal
//! communication uses.
//!
//! Every event in the system has the same shape: a GUIDv7 id, a timestamp,
//! a namespaced kind tag, and a type-erased typed payload. Presence events
//! from moss, HTTP commands, inter-adapter messages, and future
//! external-source messages all conform.
//!
//! See [companion-architecture.md §The event envelope] for the design
//! rationale and [COMPANION-0002] for the book ADR.
//!
//! # Usage
//!
//! Define a payload:
//!
//! ```
//! use std::any::Any;
//! use garden_companion_sdk::garden::EventPayload;
//!
//! #[derive(Debug, Clone)]
//! struct Greeting { pub who: String }
//!
//! impl EventPayload for Greeting {
//!     const KIND: &'static str = "example.greet.said";
//!     fn as_any(&self) -> &dyn Any { self }
//! }
//! ```
//!
//! Wrap it in an [`Event`] and dispatch in a consumer:
//!
//! ```
//! # use std::any::Any;
//! # use garden_companion_sdk::garden::{Event, EventPayload};
//! # #[derive(Debug, Clone)]
//! # struct Greeting { pub who: String }
//! # impl EventPayload for Greeting {
//! #     const KIND: &'static str = "example.greet.said";
//! #     fn as_any(&self) -> &dyn Any { self }
//! # }
//! let evt = Event::new(Greeting { who: "world".into() });
//!
//! evt.on::<Greeting>(|g| println!("Hello, {}!", g.who));
//!
//! assert!(evt.is::<Greeting>());
//! assert_eq!(evt.payload::<Greeting>().map(|g| g.who.as_str()), Some("world"));
//! ```
//!
//! [companion-architecture.md §The event envelope]: https://github.com/zen-garden/zen-garden/blob/dev/docs/specs/companion-architecture.md#the-event-envelope
//! [COMPANION-0002]: https://github.com/zen-garden/zen-garden/blob/dev/docs/decisions/COMPANION-0002-event-envelope.md

use chrono::{DateTime, Utc};
use std::any::Any;
use std::fmt::Debug;
use std::sync::Arc;

// Rust note (book discovery): an associated `const` on a trait makes that
// trait not object-safe — `dyn EventPayload` would be rejected by the
// compiler. Because `Event` must hold `Arc<dyn _>` to carry any payload
// type, we split the contract in two:
//
// - [`EventPayload`] is the user-facing trait. It carries the `KIND` and
//   `COALESCING` consts that make compile-time kind checks work (so
//   `Event::payload::<T>()` can read `T::KIND` directly). Users implement
//   this; it is NOT object-safe.
//
// - [`DynPayload`] is an object-safe runtime trait with method versions of
//   the same values. It is auto-implemented for every `EventPayload` via
//   the blanket impl below, and is what `Arc<dyn _>` actually stores.
//
// From the user's perspective only `EventPayload` exists; `DynPayload`
// surfaces only when you see the concrete type of `Event::payload`.
// This discovery is noted in COMPANION-0002.

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// Unique identifier for an event. A [GUIDv7](https://datatracker.ietf.org/doc/rfc9562/)
/// — time-ordered, globally unique.
///
/// Used as the primary key for [`Pulse`] deduplication, as a sort key for
/// replay, and as a correlation anchor for distributed tracing. Lexicographic
/// ordering of the underlying 128-bit representation matches creation order.
///
/// [`Pulse`]: https://github.com/zen-garden/zen-garden/blob/dev/docs/specs/companion-architecture.md#pulse-the-orchestrator
pub type EventId = uuid::Uuid;

/// Generate a new GUIDv7 [`EventId`].
///
/// Wraps [`uuid::Uuid::now_v7`]. Each call returns a fresh id whose timestamp
/// reflects the current wall-clock time (with nanosecond sub-field entropy to
/// ensure uniqueness across rapid successive calls).
#[inline]
pub fn new_event_id() -> EventId {
    uuid::Uuid::now_v7()
}

// ---------------------------------------------------------------------------
// Payload trait
// ---------------------------------------------------------------------------

/// Implemented by every event payload type.
///
/// A payload carries the event's data. It has a stable [`KIND`] tag (matched
/// against the envelope's `kind` field at dispatch time) and an optional
/// [`COALESCING`] flag (instructing [`Pulse`] that rapid bursts of this event
/// type may be collapsed to the latest value).
///
/// # Required implementation
///
/// Every impl must define [`KIND`], may override [`COALESCING`], and must
/// provide [`as_any`] — a one-liner returning `self`. The `as_any` method
/// exists because trait-object downcasting in stable Rust requires an
/// explicit `&dyn Any` handle; the method provides it without any
/// per-event allocation.
///
/// ```
/// use std::any::Any;
/// use garden_companion_sdk::garden::EventPayload;
///
/// #[derive(Debug)]
/// struct HealthChanged;
///
/// impl EventPayload for HealthChanged {
///     const KIND: &'static str = "core.stone.health.changed";
///     // COALESCING not overridden — defaults to false (each event delivered)
///     fn as_any(&self) -> &dyn Any { self }
/// }
/// ```
///
/// [`KIND`]: EventPayload::KIND
/// [`COALESCING`]: EventPayload::COALESCING
/// [`as_any`]: EventPayload::as_any
/// [`Pulse`]: https://github.com/zen-garden/zen-garden/blob/dev/docs/specs/companion-architecture.md#pulse-the-orchestrator
pub trait EventPayload: Any + Send + Sync + Debug {
    /// Stable identifier for this payload type. Must be namespaced per the
    /// [kind namespace convention] and must match the envelope's `kind`
    /// field on any [`Event`] wrapping this payload.
    ///
    /// Conventionally defined as a `const`:
    /// ```ignore
    /// const KIND: &'static str = "core.stone.health.changed";
    /// ```
    ///
    /// [kind namespace convention]: https://github.com/zen-garden/zen-garden/blob/dev/docs/specs/companion-architecture.md#kind-namespace-convention
    const KIND: &'static str;

    /// If `true`, [`Pulse`] may collapse rapid bursts of events of this kind
    /// to the latest value within a flush window.
    ///
    /// Set to `true` for **state-delta events** where only the newest value
    /// matters (e.g. `LoadUpdated`, `HealthChanged`). Leave at the default
    /// (`false`) for **discrete events** where every occurrence is meaningful
    /// (e.g. `Tended`, `ServiceStarted`).
    ///
    /// [`Pulse`]: https://github.com/zen-garden/zen-garden/blob/dev/docs/specs/companion-architecture.md#pulse-the-orchestrator
    const COALESCING: bool = false;

    /// Downcast handle. Implementations return `self`.
    ///
    /// This exists because trait-object downcasting from `&dyn EventPayload`
    /// to `&T` is not automatic in stable Rust — a method that yields
    /// `&dyn Any` is required. The implementation is always identical and
    /// zero-cost (`fn as_any(&self) -> &dyn Any { self }`).
    fn as_any(&self) -> &dyn Any;

    /// Runtime accessor for [`COALESCING`]. Default implementation returns
    /// `Self::COALESCING` so subscribers can read the flag through
    /// `&dyn DynPayload` without knowing the concrete type.
    ///
    /// Do not override. The default implementation is always correct.
    ///
    /// [`COALESCING`]: EventPayload::COALESCING
    fn is_coalescing(&self) -> bool {
        Self::COALESCING
    }
}

// ---------------------------------------------------------------------------
// Object-safe runtime trait (auto-implemented)
// ---------------------------------------------------------------------------

/// Object-safe runtime view of an [`EventPayload`]. Auto-implemented for
/// every [`EventPayload`] via a blanket impl; users never implement this
/// directly.
///
/// `Event::payload` is stored as `Arc<dyn DynPayload>` rather than
/// `Arc<dyn EventPayload>` because [`EventPayload`] carries associated
/// `const`s that make it not object-safe. `DynPayload` exposes the same
/// information through methods, which is object-safe.
///
/// ```ignore
/// // This is how Pulse will read event metadata through the trait object:
/// let payload: Arc<dyn DynPayload> = /* ... */;
/// let kind: &'static str = payload.kind();
/// let coalescing: bool = payload.is_coalescing();
/// ```
pub trait DynPayload: Any + Send + Sync + Debug {
    /// Runtime accessor for `EventPayload::KIND`.
    fn kind(&self) -> &'static str;

    /// Runtime accessor for `EventPayload::COALESCING`.
    fn is_coalescing(&self) -> bool;

    /// Downcast handle. Forwards to [`EventPayload::as_any`].
    fn as_any(&self) -> &dyn Any;
}

// Blanket impl: every EventPayload is automatically a DynPayload. Users
// implement only EventPayload; the runtime uses the auto-derived
// DynPayload through `Arc<dyn DynPayload>`.
impl<T: EventPayload> DynPayload for T {
    #[inline]
    fn kind(&self) -> &'static str {
        <T as EventPayload>::KIND
    }

    #[inline]
    fn is_coalescing(&self) -> bool {
        <T as EventPayload>::is_coalescing(self)
    }

    #[inline]
    fn as_any(&self) -> &dyn Any {
        <T as EventPayload>::as_any(self)
    }
}

// ---------------------------------------------------------------------------
// The envelope
// ---------------------------------------------------------------------------

/// Uniform event envelope. Every event in the companion integration
/// platform has this shape regardless of origin (SSE transport, HTTP
/// commands, inter-adapter, future external sources).
///
/// Construct via [`Event::new`] or, for deserialized events with an
/// original id / timestamp, [`Event::with_metadata`]. Downcast the payload
/// via [`Event::payload`] or the fluent [`Event::on`] helper.
#[derive(Clone, Debug)]
pub struct Event {
    /// GUIDv7 — time-ordered, globally unique.
    pub id: EventId,

    /// Wall-clock timestamp (UTC) when the event was created.
    pub timestamp: DateTime<Utc>,

    /// Namespaced kind identifier. Equals `P::KIND` for the payload type `P`
    /// this envelope was constructed with.
    pub kind: &'static str,

    /// Type-erased payload. Downcast via [`Event::payload`].
    ///
    /// Stored as `Arc<dyn DynPayload>` because [`EventPayload`] carries
    /// associated `const`s that prevent `dyn EventPayload`. The blanket
    /// impl of [`DynPayload`] for every `EventPayload` makes this a
    /// purely internal detail — construct an `Event` by passing any
    /// `EventPayload` to [`Event::new`].
    pub payload: Arc<dyn DynPayload>,
}

impl Event {
    /// Construct an event wrapping a typed payload. The `kind` field is
    /// populated from [`EventPayload::KIND`]. The id is a freshly-generated
    /// GUIDv7 and the timestamp is the current UTC wall-clock time.
    pub fn new<P: EventPayload>(payload: P) -> Self {
        Self {
            id: new_event_id(),
            timestamp: Utc::now(),
            kind: P::KIND,
            payload: Arc::new(payload),
        }
    }

    /// Construct an event with explicit id and timestamp. Used by
    /// transports that reconstruct events from a wire format (where the
    /// original id / timestamp must be preserved).
    pub fn with_metadata<P: EventPayload>(
        id: EventId,
        timestamp: DateTime<Utc>,
        payload: P,
    ) -> Self {
        Self {
            id,
            timestamp,
            kind: P::KIND,
            payload: Arc::new(payload),
        }
    }

    /// Downcast the payload to a specific type. Returns `None` if
    /// `self.kind` does not match `T::KIND`.
    pub fn payload<T: EventPayload>(&self) -> Option<&T> {
        if self.kind == T::KIND {
            self.payload.as_any().downcast_ref::<T>()
        } else {
            None
        }
    }

    /// True if this event's `kind` matches `T::KIND`.
    ///
    /// Faster than calling [`Event::payload`] and checking `is_some()` when
    /// only the yes/no answer is needed (no downcast performed).
    pub fn is<T: EventPayload>(&self) -> bool {
        self.kind == T::KIND
    }

    /// Fluent dispatch helper. Invokes `f` with the typed payload if the
    /// kind matches. Returns `&self` so multiple `.on::<T>(...)` calls can
    /// chain in adapter event loops.
    ///
    /// ```
    /// # use std::any::Any;
    /// # use garden_companion_sdk::garden::{Event, EventPayload};
    /// # #[derive(Debug)] struct A;
    /// # impl EventPayload for A { const KIND: &'static str = "test.a.one"; fn as_any(&self) -> &dyn Any { self } }
    /// # #[derive(Debug)] struct B;
    /// # impl EventPayload for B { const KIND: &'static str = "test.b.two"; fn as_any(&self) -> &dyn Any { self } }
    /// # let evt = Event::new(A);
    /// evt.on::<A>(|_a| { /* handle A */ })
    ///    .on::<B>(|_b| { /* handle B */ });
    /// ```
    pub fn on<T: EventPayload>(&self, f: impl FnOnce(&T)) -> &Self {
        if let Some(p) = self.payload::<T>() {
            f(p);
        }
        self
    }
}

// ---------------------------------------------------------------------------
// Kind validation (syntactic)
// ---------------------------------------------------------------------------

/// True if `kind` follows the [kind namespace convention].
///
/// The grammar accepted:
///
/// ```text
/// kind      := <part> "." <part> "." <part> ("." <part>)*
/// part      := <lowercase-ascii> (<lowercase-ascii> | <digit> | "-")*
/// lowercase-ascii := 'a' .. 'z'
/// digit           := '0' .. '9'
/// ```
///
/// Examples that are **valid**:
/// - `core.stone.health.changed`
/// - `firefly.command.brightness`
/// - `cricket.tune.selected`
/// - `core.storage.connected`
///
/// Examples that are **invalid**:
/// - `Stone.Health.Changed` — uppercase
/// - `health-changed` — no namespace (fewer than 3 parts)
/// - `core..health` — empty part
/// - `core.stone.héalth` — non-ASCII
/// - `core.stone.health!` — disallowed symbol
///
/// This is **syntactic** validation only. [`Pulse`] performs additional
/// **semantic** validation (is the namespace registered? does the payload
/// type match the kind?) at ingest time.
///
/// [kind namespace convention]: https://github.com/zen-garden/zen-garden/blob/dev/docs/specs/companion-architecture.md#kind-namespace-convention
/// [`Pulse`]: https://github.com/zen-garden/zen-garden/blob/dev/docs/specs/companion-architecture.md#pulse-the-orchestrator
pub fn is_valid_kind(kind: &str) -> bool {
    let parts: Vec<&str> = kind.split('.').collect();
    if parts.len() < 3 {
        return false;
    }
    parts
        .iter()
        .all(|p| !p.is_empty() && p.chars().all(is_valid_kind_char))
}

fn is_valid_kind_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
}

/// Extract the namespace prefix (first dot-separated part) from a kind.
///
/// Returns `None` if the kind has no `.` — a malformed kind has no valid
/// namespace.
///
/// ```
/// use garden_companion_sdk::garden::kind_namespace;
///
/// assert_eq!(kind_namespace("core.stone.health.changed"), Some("core"));
/// assert_eq!(kind_namespace("firefly.command.brightness"), Some("firefly"));
/// assert_eq!(kind_namespace("flat"), None);
/// ```
pub fn kind_namespace(kind: &str) -> Option<&str> {
    kind.split_once('.').map(|(ns, _)| ns)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    // --- Test payloads ---

    #[derive(Debug, Clone, PartialEq)]
    struct HealthChanged {
        to: &'static str,
    }

    impl EventPayload for HealthChanged {
        const KIND: &'static str = "core.stone.health.changed";
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct LoadUpdated {
        cpu: u8,
    }

    impl EventPayload for LoadUpdated {
        const KIND: &'static str = "core.stone.load.updated";
        const COALESCING: bool = true;
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    #[derive(Debug, Clone)]
    struct Tended;

    impl EventPayload for Tended {
        const KIND: &'static str = "core.stone.tended";
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // --- Envelope construction ---

    #[test]
    fn event_new_populates_kind_from_payload_const() {
        let evt = Event::new(HealthChanged { to: "thriving" });
        assert_eq!(evt.kind, HealthChanged::KIND);
        assert_eq!(evt.kind, "core.stone.health.changed");
    }

    #[test]
    fn event_new_assigns_fresh_id_and_current_timestamp() {
        let before = Utc::now();
        let evt = Event::new(Tended);
        let after = Utc::now();

        assert!(evt.timestamp >= before);
        assert!(evt.timestamp <= after);
    }

    #[test]
    fn event_new_ids_are_unique() {
        let a = Event::new(Tended);
        let b = Event::new(Tended);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn event_with_metadata_preserves_id_and_timestamp() {
        let id = new_event_id();
        let ts = Utc::now();
        let evt = Event::with_metadata(id, ts, HealthChanged { to: "wilting" });

        assert_eq!(evt.id, id);
        assert_eq!(evt.timestamp, ts);
        assert_eq!(evt.kind, HealthChanged::KIND);
    }

    // --- Typed downcast ---

    #[test]
    fn payload_returns_some_when_kind_matches() {
        let evt = Event::new(HealthChanged { to: "withering" });
        let downcast = evt.payload::<HealthChanged>();
        assert_eq!(downcast, Some(&HealthChanged { to: "withering" }));
    }

    #[test]
    fn payload_returns_none_when_kind_mismatches() {
        let evt = Event::new(HealthChanged { to: "thriving" });
        assert!(evt.payload::<LoadUpdated>().is_none());
        assert!(evt.payload::<Tended>().is_none());
    }

    #[test]
    fn is_agrees_with_payload_downcast() {
        let evt = Event::new(LoadUpdated { cpu: 42 });

        assert!(evt.is::<LoadUpdated>());
        assert!(!evt.is::<HealthChanged>());
        assert!(!evt.is::<Tended>());

        assert!(evt.payload::<LoadUpdated>().is_some());
        assert!(evt.payload::<HealthChanged>().is_none());
        assert!(evt.payload::<Tended>().is_none());
    }

    // --- Fluent dispatch ---

    #[test]
    fn on_invokes_closure_when_kind_matches() {
        let evt = Event::new(HealthChanged { to: "wilting" });
        let mut called_health = false;
        let mut called_load = false;

        evt.on::<HealthChanged>(|h| {
            called_health = true;
            assert_eq!(h.to, "wilting");
        })
        .on::<LoadUpdated>(|_| called_load = true);

        assert!(called_health);
        assert!(!called_load);
    }

    #[test]
    fn on_chains_return_self_for_fluent_composition() {
        let evt = Event::new(LoadUpdated { cpu: 90 });
        let mut seen_kinds: Vec<&'static str> = vec![];

        evt.on::<HealthChanged>(|_| seen_kinds.push(HealthChanged::KIND))
            .on::<LoadUpdated>(|_| seen_kinds.push(LoadUpdated::KIND))
            .on::<Tended>(|_| seen_kinds.push(Tended::KIND));

        assert_eq!(seen_kinds, vec![LoadUpdated::KIND]);
    }

    // --- GUIDv7 ordering ---

    #[test]
    fn guidv7_ids_sort_by_creation_time() {
        let first = new_event_id();
        thread::sleep(Duration::from_millis(2));
        let second = new_event_id();

        // GUIDv7 encodes the timestamp in the high bits, so lexicographic
        // ordering of the Uuid struct matches temporal ordering.
        assert!(second > first);
    }

    // --- Coalescing flag ---

    #[test]
    fn is_coalescing_reflects_const_value() {
        let load = Event::new(LoadUpdated { cpu: 10 });
        let tended = Event::new(Tended);
        let health = Event::new(HealthChanged { to: "thriving" });

        assert!(load.payload.is_coalescing());
        assert!(!tended.payload.is_coalescing());
        assert!(!health.payload.is_coalescing());
    }

    #[test]
    fn coalescing_const_is_accessible_without_instance() {
        const { assert!(LoadUpdated::COALESCING) };
        const { assert!(!Tended::COALESCING) };
        const { assert!(!HealthChanged::COALESCING) };
    }

    // --- Kind validation ---

    #[test]
    fn is_valid_kind_accepts_namespaced_kinds() {
        assert!(is_valid_kind("core.stone.health.changed"));
        assert!(is_valid_kind("firefly.command.brightness"));
        assert!(is_valid_kind("cricket.tune.selected"));
        assert!(is_valid_kind("core.storage.connected"));
        // three parts minimum
        assert!(is_valid_kind("a.b.c"));
        // digits and hyphens allowed in parts
        assert!(is_valid_kind("core.v2.load-updated"));
        assert!(is_valid_kind("ns1.domain2.event3"));
    }

    #[test]
    fn is_valid_kind_rejects_uppercase() {
        assert!(!is_valid_kind("Core.Stone.Health.Changed"));
        assert!(!is_valid_kind("CORE.STONE.HEALTH"));
        assert!(!is_valid_kind("core.Stone.health"));
    }

    #[test]
    fn is_valid_kind_rejects_fewer_than_three_parts() {
        assert!(!is_valid_kind("core"));
        assert!(!is_valid_kind("core.stone"));
        assert!(!is_valid_kind(""));
        assert!(!is_valid_kind("."));
        assert!(!is_valid_kind(".."));
    }

    #[test]
    fn is_valid_kind_rejects_empty_parts() {
        assert!(!is_valid_kind("core..health"));
        assert!(!is_valid_kind(".core.stone"));
        assert!(!is_valid_kind("core.stone."));
        assert!(!is_valid_kind("..."));
    }

    #[test]
    fn is_valid_kind_rejects_non_ascii_and_symbols() {
        assert!(!is_valid_kind("core.stone.héalth"));
        assert!(!is_valid_kind("core.stone.health!"));
        assert!(!is_valid_kind("core.stone.health@changed"));
        assert!(!is_valid_kind("core.stone.health/changed"));
        assert!(!is_valid_kind("core.stone.health_changed")); // underscore not allowed
        assert!(!is_valid_kind("core.stone.health changed")); // space not allowed
    }

    #[test]
    fn kind_namespace_extracts_first_part() {
        assert_eq!(kind_namespace("core.stone.health.changed"), Some("core"));
        assert_eq!(kind_namespace("firefly.command.brightness"), Some("firefly"));
        assert_eq!(kind_namespace("cricket.tune.selected"), Some("cricket"));
    }

    #[test]
    fn kind_namespace_returns_none_for_flat_strings() {
        assert_eq!(kind_namespace("flat"), None);
        assert_eq!(kind_namespace(""), None);
    }

    #[test]
    fn kind_namespace_handles_edge_cases() {
        // Leading dot → empty namespace
        assert_eq!(kind_namespace(".core.stone"), Some(""));
        // Only a dot → both parts empty; first is empty-string namespace
        assert_eq!(kind_namespace("."), Some(""));
    }

    #[test]
    fn event_kind_matches_its_payload_kind_const() {
        // The invariant every payload / envelope pair maintains.
        let a = Event::new(HealthChanged { to: "thriving" });
        assert_eq!(a.kind, HealthChanged::KIND);

        let b = Event::new(LoadUpdated { cpu: 0 });
        assert_eq!(b.kind, LoadUpdated::KIND);

        let c = Event::new(Tended);
        assert_eq!(c.kind, Tended::KIND);
    }

    #[test]
    fn every_concrete_kind_passes_validation() {
        // Every payload kind defined in-crate (currently just these test
        // types) follows the namespace convention. When Book V lands core
        // payloads, those will join this check.
        assert!(is_valid_kind(HealthChanged::KIND));
        assert!(is_valid_kind(LoadUpdated::KIND));
        assert!(is_valid_kind(Tended::KIND));
    }
}
