//! Cricket adapters.
//!
//! The sole adapter is [`audio::AudioAdapter`] — a singleton that owns
//! the mixer, tune manifest, and companion on/off state, and plays
//! audio in response to presence events and handles hey-tell commands.

pub mod audio;

pub use audio::AudioFactory;
