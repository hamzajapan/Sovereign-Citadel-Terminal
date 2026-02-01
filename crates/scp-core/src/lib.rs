//! # SCP Core
//!
//! Shared types, cryptographic primitives, and async channel definitions
//! for the Sovereign Citadel Protocol.
//!
//! ## Modules
//!
//! - [`types`] - Core Bitcoin types (Satoshi, ContractId, etc.)
//! - [`crypto`] - Cryptographic wrappers (AdaptorSignature, SchnorrSignature)
//! - [`channels`] - Async message types for inter-crate communication
//! - [`error`] - Unified error types

pub mod channels;
pub mod crypto;
pub mod error;
pub mod types;

pub use channels::{AgentSignal, VaultEvent};
pub use error::{Error, Result};
pub use types::*;
