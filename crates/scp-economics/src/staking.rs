//! Staking pool and rewards management.

use crate::token::CtdlToken;
use scp_core::{PublicKey, Result, Satoshi};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const REWARD_PRECISION: u128 = 1_000_000_000_000; // 1e12

/// Configuration for the staking pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingConfig {
    /// Minimum stake amount.
    pub min_stake: u64,
    /// Lock-up period in seconds.
    pub lockup_period_secs: u64,
    /// Early withdrawal penalty (0.0 - 1.0).
    pub early_withdrawal_penalty: f64,
}

impl Default for StakingConfig {
    fn default() -> Self {
        Self {
            min_stake: 100,                 // 100 CTDL minimum
            lockup_period_secs: 604800,     // 7 days
            early_withdrawal_penalty: 0.05, // 5% penalty
        }
    }
}

/// A stake position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakePosition {
    /// The staker.
    pub staker: PublicKey,
    /// Amount staked.
    pub amount: u64,
    /// When the stake was created.
    pub staked_at: u64,
    /// When the stake can be withdrawn without penalty.
    pub unlocks_at: u64,
    /// Rewards debt (internal accounting).
    pub reward_debt: u128,
    /// Pending rewards not yet claimed.
    pub pending_rewards: Satoshi,
}

impl StakePosition {
    /// Create a new stake position.
    pub fn new(staker: PublicKey, lockup_secs: u64) -> Self {
        let now = now();
        Self {
            staker,
            amount: 0,
            staked_at: now,
            unlocks_at: now + lockup_secs,
            reward_debt: 0,
            pending_rewards: Satoshi::ZERO,
        }
    }

    /// Check if the stake is locked.
    pub fn is_locked(&self) -> bool {
        now() < self.unlocks_at
    }
}

/// The staking pool.
pub struct StakingPool {
    config: StakingConfig,
    token: Arc<CtdlToken>,
    /// Positions by staker.
    positions: RwLock<HashMap<String, StakePosition>>,
    /// Accumulated rewards per share (scaled by 1e12).
    acc_reward_per_share: RwLock<u128>,
    /// Total rewards distributed ever.
    total_distributed: RwLock<Satoshi>,
}

impl StakingPool {
    /// Create a new staking pool.
    pub fn new(config: StakingConfig, token: Arc<CtdlToken>) -> Self {
        Self {
            config,
            token,
            positions: RwLock::new(HashMap::new()),
            acc_reward_per_share: RwLock::new(0),
            total_distributed: RwLock::new(Satoshi::ZERO),
        }
    }

    /// Distribute new rewards to the pool.
    pub fn distribute(&self, amount: Satoshi) -> Result<()> {
        if amount == Satoshi::ZERO {
            return Ok(());
        }

        let total_staked = self.total_staked();
        if total_staked == 0 {
            return Ok(());
        }

        let mut acc = self.acc_reward_per_share.write().unwrap();
        // acc += amount * precision / total_staked
        let additional = (amount.as_sat() as u128)
            .checked_mul(REWARD_PRECISION)
            .unwrap()
            .checked_div(total_staked as u128)
            .unwrap();

        *acc += additional;

        let mut total = self.total_distributed.write().unwrap();
        *total = *total + amount;

        tracing::info!(amount = %amount, new_acc = *acc, "Distributed rewards to pool");
        Ok(())
    }

    /// Stake tokens.
    pub fn stake(&self, staker: &PublicKey, amount: u64) -> Result<StakePosition> {
        if amount < self.config.min_stake {
            let key = staker.to_string();
            let positions = self.positions.read().unwrap();
            if let Some(pos) = positions.get(&key) {
                if pos.amount + amount < self.config.min_stake {
                    return Err(scp_core::Error::InvalidAmount(format!(
                        "Total stake < min {}",
                        self.config.min_stake
                    )));
                }
            } else {
                return Err(scp_core::Error::InvalidAmount(format!(
                    "Minimum stake is {}",
                    self.config.min_stake
                )));
            }
        }

        // Lock tokens
        self.token.stake(staker, amount)?;

        let key = staker.to_string();
        let mut positions = self.positions.write().unwrap();
        let acc = *self.acc_reward_per_share.read().unwrap();

        let position = positions
            .entry(key.clone())
            .or_insert_with(|| StakePosition::new(*staker, self.config.lockup_period_secs));

        // Settle pending rewards
        if position.amount > 0 {
            let pending =
                ((position.amount as u128 * acc) - position.reward_debt) / REWARD_PRECISION;
            position.pending_rewards = position.pending_rewards + Satoshi::from_sat(pending as u64);
        }

        // Update amount
        position.amount += amount;
        position.reward_debt = position.amount as u128 * acc;

        // Reset lock (extensions reset timer)
        position.unlocks_at = now() + self.config.lockup_period_secs;

        tracing::info!(staker = %staker, amount = amount, "Stake added");
        Ok(position.clone())
    }

    /// Unstake tokens.
    pub fn unstake(&self, staker: &PublicKey, amount: u64) -> Result<u64> {
        let key = staker.to_string();
        let acc = *self.acc_reward_per_share.read().unwrap();

        let (net_amount, _reward_to_claim) = {
            let mut positions = self.positions.write().unwrap();
            let position = positions.get_mut(&key).ok_or_else(|| {
                scp_core::Error::InvalidAmount("No stake position found".to_string())
            })?;

            if amount > position.amount {
                return Err(scp_core::Error::InvalidAmount(
                    "Insufficient staked balance".to_string(),
                ));
            }

            // Settle rewards
            let pending =
                ((position.amount as u128 * acc) - position.reward_debt) / REWARD_PRECISION;
            position.pending_rewards = position.pending_rewards + Satoshi::from_sat(pending as u64);

            let penalty = if position.is_locked() {
                (amount as f64 * self.config.early_withdrawal_penalty) as u64
            } else {
                0
            };

            let net = amount - penalty;

            position.amount -= amount;
            position.reward_debt = position.amount as u128 * acc; // Reset debt for new amount

            if position.amount == 0 && position.pending_rewards == Satoshi::ZERO {
                positions.remove(&key);
            }

            (net, Satoshi::ZERO)
        };

        // Unlock tokens (net amount returned to user)
        self.token.unstake(staker, amount)?;

        Ok(net_amount)
    }

    /// Claim pending rewards.
    pub fn claim(&self, staker: &PublicKey) -> Result<Satoshi> {
        let key = staker.to_string();
        let acc = *self.acc_reward_per_share.read().unwrap();

        let mut positions = self.positions.write().unwrap();
        let position = positions
            .get_mut(&key)
            .ok_or_else(|| scp_core::Error::InvalidAmount("No position found".to_string()))?;

        // Settle
        let pending = ((position.amount as u128 * acc) - position.reward_debt) / REWARD_PRECISION;
        position.pending_rewards = position.pending_rewards + Satoshi::from_sat(pending as u64);
        position.reward_debt = position.amount as u128 * acc;

        let claimable = position.pending_rewards;
        if claimable == Satoshi::ZERO {
            return Err(scp_core::Error::InvalidAmount(
                "No rewards to claim".to_string(),
            ));
        }

        position.pending_rewards = Satoshi::ZERO;

        tracing::info!(staker = %staker, amount = %claimable, "Rewards claimed");
        Ok(claimable)
    }

    /// Get a staker's position.
    pub fn position(&self, staker: &PublicKey) -> Option<StakePosition> {
        let key = staker.to_string();
        let positions = self.positions.read().unwrap();
        let acc = *self.acc_reward_per_share.read().unwrap();

        positions.get(&key).map(|p| {
            let mut p = p.clone();
            // Calculate pending for display
            let pending = ((p.amount as u128 * acc) - p.reward_debt) / REWARD_PRECISION;
            p.pending_rewards = p.pending_rewards + Satoshi::from_sat(pending as u64);
            p
        })
    }

    /// Get total staked in the pool.
    pub fn total_staked(&self) -> u64 {
        self.positions
            .read()
            .unwrap()
            .values()
            .map(|p| p.amount)
            .sum()
    }

    /// Get metrics.
    pub fn metrics(&self) -> StakingMetrics {
        let positions = self.positions.read().unwrap();
        let total_staked: u64 = positions.values().map(|p| p.amount).sum();

        StakingMetrics {
            total_stakers: positions.len(),
            total_staked,
            total_rewards_distributed: *self.total_distributed.read().unwrap(),
        }
    }

    /// Load from file or create new.
    pub fn load_or_new(
        path: std::path::PathBuf,
        config: StakingConfig,
        token: Arc<CtdlToken>,
    ) -> Self {
        if path.exists() {
            let file = std::fs::File::open(&path).expect("Failed to open staking file");
            let state: StakingState =
                serde_json::from_reader(file).expect("Failed to parse staking file");

            Self {
                config,
                token,
                positions: RwLock::new(state.positions),
                acc_reward_per_share: RwLock::new(state.acc_reward_per_share),
                total_distributed: RwLock::new(state.total_distributed),
            }
        } else {
            Self::new(config, token)
        }
    }

    /// Save state to file.
    pub fn save(&self, path: &std::path::PathBuf) -> Result<()> {
        let state = StakingState {
            positions: self.positions.read().unwrap().clone(),
            acc_reward_per_share: *self.acc_reward_per_share.read().unwrap(),
            total_distributed: *self.total_distributed.read().unwrap(),
        };

        let file = std::fs::File::create(path).map_err(scp_core::Error::Io)?;
        serde_json::to_writer_pretty(file, &state)
            .map_err(|e| scp_core::Error::Io(std::io::Error::other(e)))?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct StakingState {
    positions: HashMap<String, StakePosition>,
    acc_reward_per_share: u128,
    total_distributed: Satoshi,
}

/// Staking pool metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingMetrics {
    pub total_stakers: usize,
    pub total_staked: u64,
    pub total_rewards_distributed: Satoshi,
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

    fn mock_pubkey() -> PublicKey {
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let (_, pk) = secp.generate_keypair(&mut rand::thread_rng());
        PublicKey::new(pk)
    }

    #[test]
    fn test_stake_and_unstake() {
        let token = Arc::new(CtdlToken::new(1_000_000_000));
        let pool = StakingPool::new(StakingConfig::default(), token.clone());

        let staker = mock_pubkey();
        token.mint(&staker, 1000).unwrap();

        // Stake
        let position = pool.stake(&staker, 500).unwrap();
        assert_eq!(position.amount, 500);
        assert!(position.is_locked());

        // Unstake (with penalty logic check)
        let returned = pool.unstake(&staker, 500).unwrap();
        let config = StakingConfig::default();
        let expected = 500 - (500.0 * config.early_withdrawal_penalty) as u64;
        assert_eq!(returned, expected);

        // Position should be gone
        assert!(pool.position(&staker).is_none());
    }

    #[test]
    fn test_rewards_distribution() {
        let token = Arc::new(CtdlToken::new(1_000_000_000));
        let pool = StakingPool::new(StakingConfig::default(), token.clone());

        let staker1 = mock_pubkey();
        let staker2 = mock_pubkey();

        token.mint(&staker1, 1000).unwrap();
        token.mint(&staker2, 1000).unwrap();

        pool.stake(&staker1, 100).unwrap();
        pool.stake(&staker2, 300).unwrap(); // 1/4 vs 3/4

        // Distribute 1000 sats
        pool.distribute(Satoshi::from_sat(1000)).unwrap();

        let pos1 = pool.position(&staker1).unwrap();
        let pos2 = pool.position(&staker2).unwrap();

        assert_eq!(pos1.pending_rewards, Satoshi::from_sat(250));
        assert_eq!(pos2.pending_rewards, Satoshi::from_sat(750));

        // Claim
        let claimed = pool.claim(&staker1).unwrap();
        assert_eq!(claimed, Satoshi::from_sat(250));

        let pos1 = pool.position(&staker1).unwrap();
        assert_eq!(pos1.pending_rewards, Satoshi::ZERO);
    }
}
