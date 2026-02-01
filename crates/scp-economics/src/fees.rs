//! Fee collection and distribution.
//!
//! Collects trading fees and distributes them to stakers in satoshis.

use crate::staking::StakingPool;
use crate::token::CtdlToken;
use scp_core::{Result, Satoshi};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

/// Fee configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeConfig {
    /// Trading fee rate (0.0 - 1.0).
    pub trading_fee_rate: f64,
    /// Minimum fee in satoshis.
    pub min_fee: u64,
    /// Distribution frequency in seconds.
    pub distribution_interval_secs: u64,
}

impl Default for FeeConfig {
    fn default() -> Self {
        Self {
            trading_fee_rate: 0.001,           // 0.1%
            min_fee: 100,                      // 100 sats
            distribution_interval_secs: 28800, // 8 hours
        }
    }
}

/// The fee distributor.
pub struct FeeDistributor {
    config: FeeConfig,
    _token: Arc<CtdlToken>,
    /// Accumulated fees pending distribution.
    pending_fees: RwLock<Satoshi>,
    /// Total fees ever collected.
    total_collected: RwLock<Satoshi>,
    /// Total fees ever distributed.
    total_distributed: RwLock<Satoshi>,
    /// Last distribution timestamp.
    last_distribution: RwLock<u64>,
}

impl FeeDistributor {
    /// Create a new fee distributor.
    pub fn new(config: FeeConfig, token: Arc<CtdlToken>) -> Self {
        Self {
            config,
            _token: token,
            pending_fees: RwLock::new(Satoshi::ZERO),
            total_collected: RwLock::new(Satoshi::ZERO),
            total_distributed: RwLock::new(Satoshi::ZERO),
            last_distribution: RwLock::new(now()),
        }
    }

    /// Calculate fee for a trade.
    pub fn calculate_fee(&self, volume: Satoshi) -> Satoshi {
        let fee = (volume.as_sat() as f64 * self.config.trading_fee_rate) as u64;
        Satoshi::from_sat(fee.max(self.config.min_fee))
    }

    /// Collect a fee.
    pub fn collect_fee(&self, amount: Satoshi) {
        let mut pending = self.pending_fees.write().unwrap();
        *pending = *pending + amount;

        let mut total = self.total_collected.write().unwrap();
        *total = *total + amount;

        tracing::debug!(amount = %amount, "Fee collected");
    }

    /// Check if distribution is due.
    pub fn is_distribution_due(&self) -> bool {
        let last = *self.last_distribution.read().unwrap();
        now() - last >= self.config.distribution_interval_secs
    }

    /// Distribute accumulated fees to stakers via the StakingPool.
    pub fn distribute(&self, staking_pool: &StakingPool) -> Result<Satoshi> {
        let pending = {
            let mut pending = self.pending_fees.write().unwrap();
            let amount = *pending;
            *pending = Satoshi::ZERO;
            amount
        };

        if pending == Satoshi::ZERO {
            return Ok(Satoshi::ZERO);
        }

        // Delegate to staking pool
        staking_pool.distribute(pending)?;

        // Update totals
        {
            let mut total = self.total_distributed.write().unwrap();
            *total = *total + pending;
        }

        {
            let mut last = self.last_distribution.write().unwrap();
            *last = now();
        }

        tracing::info!(amount = %pending, "Fees distributed to staking pool");
        Ok(pending)
    }

    /// Get fee metrics.
    pub fn metrics(&self) -> FeeMetrics {
        FeeMetrics {
            pending_fees: *self.pending_fees.read().unwrap(),
            total_collected: *self.total_collected.read().unwrap(),
            total_distributed: *self.total_distributed.read().unwrap(),
            last_distribution: *self.last_distribution.read().unwrap(),
        }
    }

    /// Load from file or create new.
    pub fn load_or_new(path: std::path::PathBuf, config: FeeConfig, token: Arc<CtdlToken>) -> Self {
        if path.exists() {
            let file = std::fs::File::open(&path).expect("Failed to open fees file");
            let state: FeeState = serde_json::from_reader(file).expect("Failed to parse fees file");

            Self {
                config,
                _token: token,
                pending_fees: RwLock::new(state.pending_fees),
                total_collected: RwLock::new(state.total_collected),
                total_distributed: RwLock::new(state.total_distributed),
                last_distribution: RwLock::new(state.last_distribution),
            }
        } else {
            Self::new(config, token)
        }
    }

    /// Save state to file.
    pub fn save(&self, path: &std::path::PathBuf) -> Result<()> {
        let state = FeeState {
            pending_fees: *self.pending_fees.read().unwrap(),
            total_collected: *self.total_collected.read().unwrap(),
            total_distributed: *self.total_distributed.read().unwrap(),
            last_distribution: *self.last_distribution.read().unwrap(),
        };

        let file = std::fs::File::create(path).map_err(scp_core::Error::Io)?;
        serde_json::to_writer_pretty(file, &state)
            .map_err(|e| scp_core::Error::Io(std::io::Error::other(e)))?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct FeeState {
    pending_fees: Satoshi,
    total_collected: Satoshi,
    total_distributed: Satoshi,
    last_distribution: u64,
}

/// Fee metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeMetrics {
    pub pending_fees: Satoshi,
    pub total_collected: Satoshi,
    pub total_distributed: Satoshi,
    pub last_distribution: u64,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::staking::{StakingConfig, StakingPool};
    use scp_core::PublicKey;

    fn mock_pubkey() -> PublicKey {
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let (_, pk) = secp.generate_keypair(&mut rand::thread_rng());
        PublicKey::new(pk)
    }

    #[test]
    fn test_fee_distribution() {
        let token = Arc::new(CtdlToken::new(1_000_000_000));
        let pool = StakingPool::new(StakingConfig::default(), token.clone());
        let distributor = FeeDistributor::new(FeeConfig::default(), token.clone());

        // Create stakers
        let staker1 = mock_pubkey();
        token.mint(&staker1, 1000).unwrap();
        pool.stake(&staker1, 500).unwrap();

        // Collect fees
        distributor.collect_fee(Satoshi::from_sat(10_000));

        // Distribute
        let distributed = distributor.distribute(&pool).unwrap();
        assert_eq!(distributed, Satoshi::from_sat(10_000));

        // Start checking pool rewards integration
        let claimable = pool.claim(&staker1).unwrap();
        assert_eq!(claimable, Satoshi::from_sat(10_000));
    }
}
