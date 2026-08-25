//! Adapter factory trait — produces adapter instances for discovered
//! devices or endpoints.

use super::adapter::Adapter;
use crate::dependencies::SystemDependency;

/// Constructs [`Adapter`] instances for currently-present devices or
/// logical endpoints.
///
/// The [`Adapters`] supervisor calls [`AdapterFactory::discover`]
/// periodically (default every 5 seconds). Factories are stateless from
/// the supervisor's perspective — supervisor tracks which
/// `AdapterInfo::id` values are already running and only spawns new
/// candidates.
///
/// # Examples
///
/// A factory that produces one adapter per detected USB serial device:
///
/// ```ignore
/// struct MatrixFactory;
///
/// impl AdapterFactory for MatrixFactory {
///     fn kind(&self) -> &'static str { "firefly.matrix" }
///
///     fn required_dependencies(&self) -> &[SystemDependency] {
///         // No system deps; USB access only
///         &[]
///     }
///
///     fn discover(&self) -> Vec<Box<dyn Adapter>> {
///         scan_serial_ports()
///             .into_iter()
///             .filter(|p| is_rp2040_matrix(p))
///             .map(|p| Box::new(MatrixAdapter::new(p)) as Box<dyn Adapter>)
///             .collect()
///     }
/// }
/// ```
///
/// A singleton factory — always returns one adapter, supervisor dedupes:
///
/// ```ignore
/// struct AudioFactory;
///
/// impl AdapterFactory for AudioFactory {
///     fn kind(&self) -> &'static str { "cricket.audio" }
///     fn required_dependencies(&self) -> &[SystemDependency] {
///         &[LIBASOUND_DEPENDENCY]
///     }
///     fn discover(&self) -> Vec<Box<dyn Adapter>> {
///         vec![Box::new(AudioAdapter::new())]
///     }
/// }
/// ```
///
/// [`Adapters`]: super::Adapters
pub trait AdapterFactory: Send + Sync + 'static {
    /// Stable tag for this factory's adapter kind. Matches the `kind`
    /// field on instances produced by [`AdapterFactory::discover`].
    fn kind(&self) -> &'static str;

    /// System packages / binaries this factory's adapters depend on.
    ///
    /// Supervisor calls [`crate::dependencies::ensure_dependencies`] with
    /// the aggregated set across all registered factories before the
    /// first spawn. Factories with missing deps are skipped at spawn
    /// time; they remain registered and may be spawned later if their
    /// deps become available.
    fn required_dependencies(&self) -> &[SystemDependency] {
        &[]
    }

    /// Return adapter instances for currently-present devices/endpoints.
    ///
    /// Called by the supervisor on every discovery tick. Must be
    /// stateless from the supervisor's perspective — dedup is by
    /// `adapter.info().id` in the supervisor's active map.
    fn discover(&self) -> Vec<Box<dyn Adapter>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{AdapterInfo, AdapterProfile};
    use crate::garden::{Event, Pulse};
    use crate::moss_client::MossLocalClient;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    struct CountingFactory {
        kind: &'static str,
    }
    struct SimpleAdapter(&'static str, String);

    impl Adapter for SimpleAdapter {
        fn info(&self) -> AdapterInfo {
            AdapterInfo {
                kind: self.0,
                id: self.1.clone(),
                device: None,
            }
        }
        fn profile(&self) -> AdapterProfile {
            AdapterProfile::default()
        }
        fn run(
            self: Box<Self>,
            _events: mpsc::Receiver<Event>,
            _moss: Arc<MossLocalClient>,
            _pulse: Arc<Pulse>,
            shutdown: CancellationToken,
        ) -> super::super::adapter::BoxFuture<'static, ()> {
            Box::pin(async move {
                shutdown.cancelled().await;
            })
        }
    }

    impl AdapterFactory for CountingFactory {
        fn kind(&self) -> &'static str {
            self.kind
        }
        fn discover(&self) -> Vec<Box<dyn Adapter>> {
            vec![Box::new(SimpleAdapter(self.kind, "only".into()))]
        }
    }

    #[test]
    fn factory_is_object_safe() {
        let factories: Vec<Box<dyn AdapterFactory>> = vec![
            Box::new(CountingFactory { kind: "one" }),
            Box::new(CountingFactory { kind: "two" }),
        ];
        assert_eq!(factories.len(), 2);
        assert_eq!(factories[0].kind(), "one");
    }

    #[test]
    fn factory_required_dependencies_default_is_empty() {
        let f = CountingFactory { kind: "noop" };
        assert!(f.required_dependencies().is_empty());
    }

    #[test]
    fn factory_discover_returns_typed_adapters() {
        let f = CountingFactory { kind: "firefly.matrix" };
        let adapters = f.discover();
        assert_eq!(adapters.len(), 1);
        assert_eq!(adapters[0].info().kind, "firefly.matrix");
        assert_eq!(adapters[0].info().id, "only");
    }
}
