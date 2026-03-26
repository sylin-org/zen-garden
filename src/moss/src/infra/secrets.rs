//! Secrets management — delegates to Koi's encrypted vault.
//!
//! All credential storage (borrowed offering passwords, API keys, tokens)
//! is handled by `koi_crypto::vault::Vault`, which provides:
//!
//! - Platform credential store (Windows DPAPI, macOS Keychain, Linux Secret Service)
//!   for master key protection when available
//! - Machine-bound Argon2id derivation as fallback on headless systems
//! - AES-256-GCM encryption for individual secrets
//!
//! Pond CA secrets (passphrase, TOTP, FIDO2) are handled separately by
//! `koi-certmesh` via its unlock-slot envelope encryption.

pub use koi_crypto::vault::{Vault, VaultError};
