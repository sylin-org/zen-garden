//! `FakeFactory` — a trivial adapter factory driven by a closure.

use crate::adapters::{Adapter, AdapterFactory};

type Builder = Box<dyn Fn() -> Box<dyn Adapter> + Send + Sync>;

/// Factory that produces a single adapter on every `discover` call via
/// a user-provided closure.
///
/// Because the supervisor dedupes by `AdapterInfo::id`, the closure is
/// typically only invoked once per adapter instance over the
/// factory's lifetime (the second call produces the adapter but
/// supervisor discards it since the id is already active).
pub struct FakeFactory {
    kind: &'static str,
    builder: Builder,
}

impl FakeFactory {
    /// Construct from a kind tag and a builder closure.
    ///
    /// The builder runs on every discovery tick. To share mutable
    /// state between multiple invocations (e.g. a records buffer),
    /// capture an `Arc` in the closure.
    pub fn new<F>(kind: &'static str, builder: F) -> Self
    where
        F: Fn() -> Box<dyn Adapter> + Send + Sync + 'static,
    {
        Self {
            kind,
            builder: Box::new(builder),
        }
    }
}

impl AdapterFactory for FakeFactory {
    fn kind(&self) -> &'static str {
        self.kind
    }

    fn discover(&self) -> Vec<Box<dyn Adapter>> {
        vec![(self.builder)()]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::recording_adapter::RecordingAdapter;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn kind_is_returned_from_constructor_arg() {
        let factory = FakeFactory::new("test.fake", || {
            let (a, _h) = RecordingAdapter::new("test.fake", "x", &[]);
            Box::new(a)
        });
        assert_eq!(factory.kind(), "test.fake");
    }

    #[test]
    fn discover_invokes_builder_and_returns_single_adapter() {
        let invocations = Arc::new(AtomicUsize::new(0));
        let inv = invocations.clone();
        let factory = FakeFactory::new("test.fake", move || {
            inv.fetch_add(1, Ordering::SeqCst);
            let (a, _h) = RecordingAdapter::new("test.fake", "x", &[]);
            Box::new(a)
        });

        let adapters = factory.discover();
        assert_eq!(adapters.len(), 1);
        assert_eq!(invocations.load(Ordering::SeqCst), 1);

        let adapters2 = factory.discover();
        assert_eq!(adapters2.len(), 1);
        assert_eq!(invocations.load(Ordering::SeqCst), 2);
    }
}
