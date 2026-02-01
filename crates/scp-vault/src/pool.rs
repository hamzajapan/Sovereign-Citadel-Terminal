//! Liquidity pool management.
//!
//! Manages the shared liquidity pool where LPs deposit funds.

use scp_core::{Error, PublicKey, Result, Satoshi};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// A share in the liquidity pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolShare {
    /// The LP's public key.
    pub owner: PublicKey,
    /// Number of shares owned.
    pub shares: u64,
    /// When the shares were minted.
    pub minted_at: u64,
}

/// Pool configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Minimum deposit in satoshis.
    pub min_deposit: Satoshi,
    /// Maximum utilization ratio (0.0 - 1.0).
    pub max_utilization: f64,
    /// Base spread (bid-ask).
    pub base_spread: f64,
    /// Withdrawal delay in blocks.
    pub withdrawal_delay_blocks: u32,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_deposit: Satoshi::from_sat(100_000), // 0.001 BTC
            max_utilization: 0.80,
            base_spread: 0.02,
            withdrawal_delay_blocks: 6,
        }
    }
}

/// The liquidity pool.
pub struct LiquidityPool {
    config: PoolConfig,
    /// Total liquidity in satoshis.
    total_liquidity: RwLock<Satoshi>,
    /// Total shares outstanding.
    total_shares: RwLock<u64>,
    /// Shares by owner.
    shares: RwLock<HashMap<String, PoolShare>>,
    /// Currently utilized liquidity (locked in contracts).
    utilized: RwLock<Satoshi>,
    /// Current spread multiplier (adjusted by agent).
    spread_multiplier: RwLock<f64>,
    /// Circuit breaker active.
    circuit_breaker: RwLock<bool>,
    /// Event sender for agent notifications.
    event_tx: RwLock<Option<tokio::sync::mpsc::Sender<scp_core::VaultEvent>>>,
}

impl LiquidityPool {
    /// Create a new liquidity pool.
    pub fn new(config: PoolConfig) -> Self {
        Self {
            config,
            total_liquidity: RwLock::new(Satoshi::ZERO),
            total_shares: RwLock::new(0),
            shares: RwLock::new(HashMap::new()),
            utilized: RwLock::new(Satoshi::ZERO),
            spread_multiplier: RwLock::new(1.0),
            circuit_breaker: RwLock::new(false),
            event_tx: RwLock::new(None),
        }
    }

    pub fn set_event_sender(&self, tx: tokio::sync::mpsc::Sender<scp_core::VaultEvent>) {
        let mut w = self.event_tx.write().unwrap();
        *w = Some(tx);
    }

    async fn emit_event(&self, event: scp_core::VaultEvent) {
        let sender = self.event_tx.read().unwrap().clone();
        if let Some(tx) = sender {
            let _ = tx.send(event).await;
        }
    }

    /// Get the current share price in satoshis.
    pub fn share_price(&self) -> u64 {
        let liquidity = self.total_liquidity.read().unwrap();
        let shares = self.total_shares.read().unwrap();

        if *shares == 0 {
            1_000_000 // Initial price: 0.01 BTC per share
        } else {
            liquidity.as_sat() / *shares
        }
    }

    /// Deposit liquidity and receive shares.
    pub async fn deposit(&self, depositor: PublicKey, amount: Satoshi) -> Result<PoolShare> {
        // Check circuit breaker
        if *self.circuit_breaker.read().unwrap() {
            return Err(Error::CircuitBreakerActive {
                reason: "Deposits paused".to_string(),
            });
        }

        // Check minimum deposit
        if amount < self.config.min_deposit {
            return Err(Error::DepositTooSmall {
                minimum: self.config.min_deposit.as_sat(),
                actual: amount.as_sat(),
            });
        }

        let share_price = self.share_price();
        let new_shares = amount.as_sat() / share_price;

        // Update totals
        {
            let mut liquidity = self.total_liquidity.write().unwrap();
            *liquidity = liquidity
                .checked_add(amount)
                .ok_or_else(|| Error::InvalidAmount("Overflow in liquidity".to_string()))?;
        }

        {
            let mut total_shares = self.total_shares.write().unwrap();
            *total_shares += new_shares;
        }

        // Create or update share record
        let share = PoolShare {
            owner: depositor,
            shares: new_shares,
            minted_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        {
            let mut shares = self.shares.write().unwrap();
            let key = depositor.to_string();
            if let Some(existing) = shares.get_mut(&key) {
                existing.shares += new_shares;
            } else {
                shares.insert(key, share.clone());
            }
        }

        tracing::info!(
            depositor = %depositor,
            amount = %amount,
            shares = new_shares,
            "Deposit processed"
        );

        if amount > Satoshi::from_btc(1.0) {
            self.emit_event(scp_core::VaultEvent::LargeDeposit {
                from: depositor,
                amount,
            })
            .await;
        }

        Ok(share)
    }

    /// Withdraw liquidity by burning shares.
    pub async fn withdraw(&self, owner: &PublicKey, shares_to_burn: u64) -> Result<Satoshi> {
        let owner_key = owner.to_string();

        // Get current holdings
        let current_shares = {
            let shares = self.shares.read().unwrap();
            shares.get(&owner_key).map(|s| s.shares).unwrap_or(0)
        };

        if shares_to_burn > current_shares {
            return Err(Error::WithdrawalExceedsBalance {
                requested: shares_to_burn,
                available: current_shares,
            });
        }

        // Calculate withdrawal amount
        let share_price = self.share_price();
        let withdrawal_amount = Satoshi::from_sat(shares_to_burn * share_price);

        // Check available liquidity
        let available = self.available_liquidity();
        if withdrawal_amount > available {
            return Err(Error::InsufficientLiquidity {
                required: withdrawal_amount.as_sat(),
                available: available.as_sat(),
            });
        }

        // Update totals
        {
            let mut liquidity = self.total_liquidity.write().unwrap();
            *liquidity = liquidity
                .checked_sub(withdrawal_amount)
                .ok_or_else(|| Error::InvalidAmount("Underflow in liquidity".to_string()))?;
        }

        {
            let mut total_shares = self.total_shares.write().unwrap();
            *total_shares -= shares_to_burn;
        }

        // Update owner's shares
        {
            let mut shares = self.shares.write().unwrap();
            if let Some(share) = shares.get_mut(&owner_key) {
                share.shares -= shares_to_burn;
                if share.shares == 0 {
                    shares.remove(&owner_key);
                }
            }
        }

        tracing::info!(
            owner = %owner,
            shares = shares_to_burn,
            amount = %withdrawal_amount,
            "Withdrawal processed"
        );

        Ok(withdrawal_amount)
    }

    /// Get available (non-utilized) liquidity.
    pub fn available_liquidity(&self) -> Satoshi {
        let total = *self.total_liquidity.read().unwrap();
        let utilized = *self.utilized.read().unwrap();
        total.checked_sub(utilized).unwrap_or(Satoshi::ZERO)
    }

    /// Get current utilization ratio.
    pub fn utilization(&self) -> f64 {
        let total = self.total_liquidity.read().unwrap().as_sat() as f64;
        let utilized = self.utilized.read().unwrap().as_sat() as f64;
        if total == 0.0 {
            0.0
        } else {
            utilized / total
        }
    }

    /// Lock liquidity for a contract.
    pub fn lock_liquidity(&self, amount: Satoshi) -> Result<()> {
        let available = self.available_liquidity();
        if amount > available {
            return Err(Error::InsufficientLiquidity {
                required: amount.as_sat(),
                available: available.as_sat(),
            });
        }

        let mut utilized = self.utilized.write().unwrap();
        *utilized = utilized
            .checked_add(amount)
            .ok_or_else(|| Error::InvalidAmount("Overflow in utilized".to_string()))?;

        Ok(())
    }

    /// Release locked liquidity.
    pub fn release_liquidity(&self, amount: Satoshi) {
        let mut utilized = self.utilized.write().unwrap();
        *utilized = utilized.checked_sub(amount).unwrap_or(Satoshi::ZERO);
    }

    /// Get the current spread.
    pub fn current_spread(&self) -> f64 {
        self.config.base_spread * *self.spread_multiplier.read().unwrap()
    }

    /// Set the spread multiplier (called by agent signal handler).
    pub fn set_spread_multiplier(&self, multiplier: f64) {
        let mut spread = self.spread_multiplier.write().unwrap();
        *spread = multiplier.max(1.0); // Never go below base spread
        tracing::info!(multiplier = multiplier, "Spread multiplier updated");
    }

    /// Activate circuit breaker.
    pub fn activate_circuit_breaker(&self, reason: &str) {
        let mut cb = self.circuit_breaker.write().unwrap();
        *cb = true;
        tracing::warn!(reason = reason, "Circuit breaker activated");
    }

    /// Deactivate circuit breaker.
    pub fn deactivate_circuit_breaker(&self) {
        let mut cb = self.circuit_breaker.write().unwrap();
        *cb = false;
        tracing::info!("Circuit breaker deactivated");
    }

    /// Check if circuit breaker is active.
    pub fn is_circuit_breaker_active(&self) -> bool {
        *self.circuit_breaker.read().unwrap()
    }

    /// Get pool metrics.
    pub fn metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_liquidity: *self.total_liquidity.read().unwrap(),
            available_liquidity: self.available_liquidity(),
            utilization: self.utilization(),
            total_shares: *self.total_shares.read().unwrap(),
            share_price: self.share_price(),
            current_spread: self.current_spread(),
            circuit_breaker_active: self.is_circuit_breaker_active(),
        }
    }

    /// Get a specific LP's position.
    pub fn get_position(&self, owner: &PublicKey) -> Option<PoolShare> {
        let shares = self.shares.read().unwrap();
        shares.get(&owner.to_string()).cloned()
    }
    /// Process incoming signals from the agent.
    pub async fn process_signals(
        &self,
        mut rx: tokio::sync::mpsc::Receiver<scp_core::AgentSignal>,
    ) {
        use scp_core::AgentSignal;

        tracing::info!("Vault signal processor started");

        while let Some(signal) = rx.recv().await {
            match signal {
                AgentSignal::WidenSpread { factor } => {
                    self.set_spread_multiplier(factor);
                }
                AgentSignal::NarrowSpread { target } => {
                    let multiplier = target / self.config.base_spread;
                    self.set_spread_multiplier(multiplier);
                }
                AgentSignal::CircuitBreaker { reason, .. } => {
                    self.activate_circuit_breaker(&reason);
                }
                AgentSignal::Resume => {
                    self.deactivate_circuit_breaker();
                }
                AgentSignal::UpdateRiskScore { .. } => {
                    tracing::debug!("Received risk update");
                }
                _ => {} // Handle Resume, RebalanceHedge, BlockCounterparty if needed
            }
        }
    }
}

/// Pool metrics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolMetrics {
    pub total_liquidity: Satoshi,
    pub available_liquidity: Satoshi,
    pub utilization: f64,
    pub total_shares: u64,
    pub share_price: u64,
    pub current_spread: f64,
    pub circuit_breaker_active: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_pubkey() -> PublicKey {
        let secp = secp256k1::Secp256k1::new();
        let (_, pk) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());
        PublicKey::new(pk)
    }

    #[tokio::test]
    async fn test_deposit_and_withdraw() {
        let config = PoolConfig::default();
        let pool = LiquidityPool::new(config);
        let alice = mock_pubkey();

        // Deposit 1 BTC -> 100 shares (price 1M sat)
        let share = pool.deposit(alice, Satoshi::from_btc(1.0)).await.unwrap();
        assert_eq!(share.shares, 100);
        assert_eq!(pool.share_price(), 1_000_000);

        // Withdraw 50 shares -> 0.5 BTC
        let withdrawn = pool.withdraw(&alice, 50).await.unwrap();
        assert_eq!(withdrawn, Satoshi::from_btc(0.5));

        let pos = pool.get_position(&alice).unwrap();
        assert_eq!(pos.shares, 50);
    }

    #[tokio::test]
    async fn test_circuit_breaker_blocks_deposits() {
        let pool = LiquidityPool::new(PoolConfig::default());
        pool.activate_circuit_breaker("test");

        let res = pool.deposit(mock_pubkey(), Satoshi::from_btc(1.0)).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_utilization_tracking() {
        let pool = LiquidityPool::new(PoolConfig::default());
        let _ = pool
            .deposit(mock_pubkey(), Satoshi::from_btc(1.0))
            .await
            .unwrap();

        pool.lock_liquidity(Satoshi::from_btc(0.1)).unwrap();

        assert_eq!(pool.utilization(), 0.1);
        assert_eq!(pool.available_liquidity(), Satoshi::from_btc(0.9));

        // Release
        pool.release_liquidity(Satoshi::from_btc(0.1));
        assert_eq!(pool.utilization(), 0.0);
    }
}
