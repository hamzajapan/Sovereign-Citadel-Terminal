//! Delta-neutral hedging strategies.
//!
//! Implements the core strategy from the whitepaper:
//! 1. User deposits BTC
//! 2. Vault automatically opens a matching short position
//! 3. User is delta-neutral (protected from price changes)
//! 4. User earns funding rate from speculators

use scp_core::{ContractId, PublicKey, Satoshi};
use serde::{Deserialize, Serialize};

/// A delta-neutral position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaNeutralPosition {
    /// Position owner.
    pub owner: PublicKey,
    /// The long (deposit) amount.
    pub long_amount: Satoshi,
    /// The short contract ID.
    pub short_contract_id: Option<ContractId>,
    /// Current delta (should be ~0 when hedged).
    pub delta: f64,
    /// Accumulated funding earned.
    pub funding_earned: Satoshi,
    /// Entry price (BTC/USD).
    pub entry_price: f64,
}

impl DeltaNeutralPosition {
    /// Create a new position.
    pub fn new(owner: PublicKey, amount: Satoshi, entry_price: f64) -> Self {
        Self {
            owner,
            long_amount: amount,
            short_contract_id: None,
            delta: 1.0, // Unhedged initially
            funding_earned: Satoshi::ZERO,
            entry_price,
        }
    }

    /// Attach a short contract for hedging.
    pub fn attach_short(&mut self, contract_id: ContractId) {
        self.short_contract_id = Some(contract_id);
        self.delta = 0.0; // Now hedged
    }

    /// Add funding earnings.
    pub fn add_funding(&mut self, amount: Satoshi) {
        self.funding_earned = self.funding_earned + amount;
    }

    /// Check if fully hedged.
    pub fn is_hedged(&self) -> bool {
        self.delta.abs() < 0.01 && self.short_contract_id.is_some()
    }
}

/// The delta-neutral strategy manager.
pub struct DeltaNeutralStrategy {
    /// Funding rate per period (e.g., per 8 hours).
    funding_rate: f64,
    /// Current BTC/USD price.
    current_price: f64,
}

impl DeltaNeutralStrategy {
    /// Create a new strategy manager.
    pub fn new(initial_price: f64) -> Self {
        Self {
            funding_rate: 0.0001, // 0.01% per period
            current_price: initial_price,
        }
    }

    /// Update the current price.
    pub fn update_price(&mut self, price: f64) {
        self.current_price = price;
    }

    /// Set the funding rate.
    pub fn set_funding_rate(&mut self, rate: f64) {
        self.funding_rate = rate;
    }

    /// Calculate the short position size needed to hedge.
    pub fn calculate_hedge_size(&self, long_amount: Satoshi) -> Satoshi {
        // For a 1:1 hedge, short the same amount
        // In practice, this might include a buffer for slippage
        long_amount
    }

    /// Calculate funding payment for a position.
    pub fn calculate_funding(&self, position: &DeltaNeutralPosition) -> Satoshi {
        if position.short_contract_id.is_none() {
            return Satoshi::ZERO;
        }

        // Funding = position size * funding rate
        let funding_sats = (position.long_amount.as_sat() as f64 * self.funding_rate) as u64;
        Satoshi::from_sat(funding_sats)
    }

    /// Calculate USD value of a position.
    pub fn usd_value(&self, position: &DeltaNeutralPosition) -> f64 {
        if position.is_hedged() {
            // Delta-neutral: USD value stays constant at entry
            position.long_amount.as_btc() * position.entry_price
        } else {
            // Unhedged: current market value
            position.long_amount.as_btc() * self.current_price
        }
    }

    /// Calculate BTC value of a position.
    pub fn btc_value(&self, position: &DeltaNeutralPosition) -> f64 {
        if position.is_hedged() {
            // Delta-neutral: BTC value fluctuates inversely with price
            let entry_usd = position.long_amount.as_btc() * position.entry_price;
            entry_usd / self.current_price
        } else {
            position.long_amount.as_btc()
        }
    }

    /// Get the current funding rate.
    pub fn funding_rate(&self) -> f64 {
        self.funding_rate
    }

    /// Get the current price.
    pub fn current_price(&self) -> f64 {
        self.current_price
    }
}

/// Result of a hedge operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HedgeResult {
    /// Whether the hedge was successful.
    pub success: bool,
    /// The contract ID if a short was opened.
    pub contract_id: Option<ContractId>,
    /// New delta after hedging.
    pub new_delta: f64,
    /// Any error message.
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_pubkey() -> PublicKey {
        let secp = secp256k1::Secp256k1::new();
        let (_, pk) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());
        PublicKey::new(pk)
    }

    #[test]
    fn test_delta_neutral_position() {
        let owner = mock_pubkey();
        let mut position = DeltaNeutralPosition::new(owner, Satoshi::from_btc(1.0), 50000.0);

        // Initially unhedged
        assert!(!position.is_hedged());
        assert_eq!(position.delta, 1.0);

        // Attach short
        let contract_id = ContractId::from_data(b"short");
        position.attach_short(contract_id);
        assert!(position.is_hedged());
        assert_eq!(position.delta, 0.0);
    }

    #[test]
    fn test_strategy_calculations() {
        let strategy = DeltaNeutralStrategy::new(50000.0);

        let position = DeltaNeutralPosition::new(mock_pubkey(), Satoshi::from_btc(1.0), 50000.0);

        // Hedge size should match long
        let hedge = strategy.calculate_hedge_size(position.long_amount);
        assert_eq!(hedge, Satoshi::from_btc(1.0));
    }

    #[test]
    fn test_usd_value_stability() {
        let mut strategy = DeltaNeutralStrategy::new(50000.0);

        let owner = mock_pubkey();
        let mut position = DeltaNeutralPosition::new(owner, Satoshi::from_btc(1.0), 50000.0);
        position.attach_short(ContractId::from_data(b"short"));

        // USD value should stay constant
        let initial_usd = strategy.usd_value(&position);

        // Price goes up 20%
        strategy.update_price(60000.0);
        let new_usd = strategy.usd_value(&position);

        assert!((initial_usd - new_usd).abs() < 0.01);
    }
}
