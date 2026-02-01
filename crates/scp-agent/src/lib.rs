//! # SCP Agent
//!
//! The AI Sentinel system for dynamic risk management.
//!
//! ## Security Note
//!
//! The Agent NEVER has access to private keys. It sends signals
//! to the Vault, which validates them before requesting signatures
//! from the isolated Signer.
//!
//! ## Architecture
//!
//! The Agent runs in its own Tokio task, continuously monitoring
//! market conditions and sentiment. It communicates with the Vault
//! via async channels, ensuring non-blocking operation.
//!
//! ## Modules
//!
//! - [`sentinel`] - Main agent coordinator
//! - [`sentiment`] - Sentiment analysis providers
//! - [`spread`] - Dynamic spread calculation
//! - [`risk`] - Risk scoring and reputation
//! - [`signals`] - Signal emission to Vault

pub mod risk;
pub mod rss_provider;
pub mod sentiment;
pub mod sentinel;
pub mod signals;
pub mod spread;

pub use risk::{ReputationScore, ToxicFlowDetector};
pub use sentiment::{MockSentimentProvider, SentimentProvider, SentimentScore};
pub use rss_provider::RssSentimentProvider;
pub use sentinel::{AgentConfig, CitadelAgent};
pub use spread::SpreadCalculator;
