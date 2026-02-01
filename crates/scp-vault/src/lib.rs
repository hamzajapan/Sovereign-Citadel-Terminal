//! # SCP Vault
//!
//! Liquidity vault system with delta-neutral hedging strategies.
//!
//! ## Architecture
//!
//! The vault receives signals from `scp-agent` via async channels
//! and requests signatures from `scp-signer`. It never has direct
//! access to private keys.
//!
//! ## Modules
//!
//! - [`pool`] - Liquidity pool management
//! - [`position`] - Position tracking
//! - [`delta_neutral`] - Delta-neutral hedging strategies
//! - [`deposit`] - Deposit and withdrawal logic
//! - [`signal_handler`] - Async signal receiver from Agent

pub mod delta_neutral;
pub mod deposit;
pub mod pool;
pub mod position;
pub mod signal_handler;
pub mod vault;

pub use delta_neutral::DeltaNeutralStrategy;
pub use deposit::{DepositRequest, WithdrawalRequest};
pub use pool::{LiquidityPool, PoolShare};
pub use position::{Position, PositionMetrics};
pub use signal_handler::SignalHandler;
pub use vault::LiquidityVault;
