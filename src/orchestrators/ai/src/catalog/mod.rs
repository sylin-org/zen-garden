//! Provider abstraction boundary.
//!
//! Defines the `Provider` trait, canonical inference types, and the
//! `ProviderRegistry`. This module lives between the domain layer
//! (which never imports it) and the providers layer (which implements
//! the trait).

pub mod inference;
pub mod traits;

pub use inference::{
    BoxStream, ChatMessage, ChunkChoice, EmbedRequest, EmbedResponse, EmbeddingData,
    InferenceChunk, InferenceChoice, InferenceRequest, InferenceResponse, SpeechAudio,
    SpeechRequest, SpeechResponse, TranscribeRequest, TranscribeResponse, Usage,
};
pub use traits::{
    BenchmarkSample, BoxFuture, DiscoveryConfig, FormSchema, ProbeResult, Provider,
    ProviderContext, ProviderRegistry, Sample, ServiceModel, SyncProgress,
};
