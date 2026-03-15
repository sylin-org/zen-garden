//! Capability executor - manifest-driven capability discovery
//!
//! Discovers offering capabilities using manifest-defined commands.
//! Supports both managed (container) and adopted (native) modes.
//!
//! # Architecture
//!
//! ```text
//! CapabilityManifest (YAML)
//!   └── commands (per mode, per platform)
//!         ├── managed: docker exec {{container_name}} ...
//!         └── adopted: curl localhost:{{port}}/...
//!
//! Executor
//!   1. Load manifest for offering
//!   2. Determine service mode (managed vs adopted)
//!   3. Run appropriate command (templated)
//!   4. Transform output via helpers endpoint
//!   5. Return CapabilityCollection
//! ```
//!
//! # Example
//!
//! ```ignore
//! let executor = CapabilityExecutor::new(&docker, &http_client);
//! let caps = executor.list_capabilities(&service, &manifest).await?;
//! // caps = CapabilityCollection { type: "model", items: [...] }
//! ```

pub mod executor;

pub use executor::{CapabilityExecutor, CapabilityMutationResult, Executor};
