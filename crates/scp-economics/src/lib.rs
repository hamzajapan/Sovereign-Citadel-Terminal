//! # SCP Economics
//!
//! Token economics, Liquidity Bootstrapping Offering (LBO), and fee distribution.
//!
//! ## Key Features
//!
//! - **$CTDL Token**: Governance token minted via proof-of-liquidity
//! - **LBO Mechanism**: Fair launch via liquidity provision, no ICO
//! - **Real Yield**: 100% of fees distributed to stakers in satoshis
//!
//! ## Modules
//!
//! - [`token`] - $CTDL token and balances
//! - [`lbo`] - Liquidity Bootstrapping Offering
//! - [`fees`] - Fee collection and distribution
//! - [`staking`] - Staking and rewards

pub mod fees;
pub mod lbo;
pub mod staking;
pub mod token;

pub use fees::FeeDistributor;
pub use lbo::LiquidityBootstrapping;
pub use staking::{StakePosition, StakingPool};
pub use token::{CtdlBalance, CtdlToken};
