//! Ollama offering adapter.
//!
//! Implements the [`Offering`](crate::catalog::Offering) trait for Ollama
//! instances. Encapsulates all Ollama-specific protocol knowledge: HTTP client
//! for `/api/tags`, `/api/ps`, `/api/show`, NDJSON streaming, model pull
//! protocol, and benchmark payloads.
//!
//! Harvested from `zen-garden-ollama-orchestrator` infra/ollama_client.rs,
//! api/proxy.rs, and tasks/benchmark.rs.

// pub mod client;
// pub mod proxy;
// pub mod benchmark;
// pub mod types;
