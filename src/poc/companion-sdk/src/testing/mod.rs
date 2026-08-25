//! Testing utilities for companion integration tests.
//!
//! This module is part of the public SDK and is intended for use by
//! downstream companion crates (`garden-firefly`, `garden-cricket`,
//! future companions) in their integration test suites. The types here
//! are deliberately simple: small, well-typed fixtures that compose
//! with the real production types from [`crate::garden`], [`crate::adapters`],
//! and [`crate::companion`].
//!
//! Ships under COMPANION-0010 (Book IX of COMPANION-0001).
//!
//! # Typical use
//!
//! ```ignore
//! use garden_companion_sdk::testing::{TestHarness, RecordingAdapter};
//! use garden_companion_sdk::{Event, EventPayload};
//!
//! #[tokio::test]
//! async fn my_scenario() {
//!     let (records_handle, factory) = RecordingAdapter::factory(
//!         "test.record", "only", &["core.stone.tended"]
//!     );
//!
//!     let harness = TestHarness::new("test-companion")
//!         .with_adapter_factory(factory)
//!         .start()
//!         .await;
//!
//!     harness.publish(MyPayload { /* ... */ });
//!     tokio::time::sleep(std::time::Duration::from_millis(50)).await;
//!
//!     let received = records_handle.lock().unwrap();
//!     assert_eq!(received.len(), 1);
//!
//!     harness.shutdown().await.unwrap();
//! }
//! ```

pub mod fake_factory;
pub mod harness;
pub mod mock_transport;
pub mod recording_adapter;

pub use fake_factory::FakeFactory;
pub use harness::{RunningHarness, TestHarness};
pub use mock_transport::{MOCK_EMITTED_KINDS, MockTransport};
pub use recording_adapter::RecordingAdapter;
