//! # SCP Signer
//!
//! Isolated key management with signing policies for the Sovereign Citadel Protocol.
//!
//! ## Security Model
//!
//! The signer is the ONLY component with access to private keys.
//! All other crates (especially `scp-agent`) must request signatures
//! through this interface, and all requests are validated against
//! signing policies before execution.
//!
//! ## Modules
//!
//! - [`keystore`] - Secure key storage abstraction
//! - [`signer`] - Signing operations
//! - [`policy`] - Signing policy enforcement

pub mod keystore;
pub mod policy;
pub mod signer;

pub use keystore::{FileKeystore, Keystore};
pub use policy::{SigningPolicy, SigningRequest};
pub use signer::Signer;
