//! Position tracking for vault participants.

use scp_core::{ContractId, PublicKey, Satoshi};
use serde::{Deserialize, Serialize};

/// A position in the vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// The position owner.
    pub owner: PublicKey,
    /// Pool shares owned.
    pub shares: u64,
    /// Active contracts.
    pub active_contracts: Vec<ContractId>,
    /// Total deposited (historical).
    pub total_deposited: Satoshi,
    /// Total withdrawn (historical).
    pub total_withdrawn: Satoshi,
    /// Realized PnL in satoshis.
    pub realized_pnl: i64,
}

impl Position {
    /// Create a new empty position.
    pub fn new(owner: PublicKey) -> Self {
        Self {
            owner,
            shares: 0,
            active_contracts: Vec::new(),
            total_deposited: Satoshi::ZERO,
            total_withdrawn: Satoshi::ZERO,
            realized_pnl: 0,
        }
    }

    /// Record a deposit.
    pub fn record_deposit(&mut self, amount: Satoshi, shares: u64) {
        self.total_deposited = self.total_deposited + amount;
        self.shares += shares;
    }

    /// Record a withdrawal.
    pub fn record_withdrawal(&mut self, amount: Satoshi, shares: u64) {
        self.total_withdrawn = self.total_withdrawn + amount;
        self.shares = self.shares.saturating_sub(shares);
    }

    /// Add an active contract.
    pub fn add_contract(&mut self, contract_id: ContractId) {
        if !self.active_contracts.contains(&contract_id) {
            self.active_contracts.push(contract_id);
        }
    }

    /// Remove an active contract.
    pub fn remove_contract(&mut self, contract_id: &ContractId) {
        self.active_contracts.retain(|id| id != contract_id);
    }

    /// Record PnL from a settled contract.
    pub fn record_pnl(&mut self, pnl: i64) {
        self.realized_pnl += pnl;
    }

    /// Calculate metrics.
    pub fn metrics(&self, share_price: u64) -> PositionMetrics {
        let current_value = Satoshi::from_sat(self.shares * share_price);
        let net_deposits =
            self.total_deposited.as_sat() as i64 - self.total_withdrawn.as_sat() as i64;
        let unrealized_pnl = current_value.as_sat() as i64 - net_deposits;

        PositionMetrics {
            current_value,
            unrealized_pnl,
            realized_pnl: self.realized_pnl,
            total_pnl: unrealized_pnl + self.realized_pnl,
            num_active_contracts: self.active_contracts.len(),
            exposure: self.calculate_exposure(),
        }
    }

    /// Calculate current exposure (simplified).
    fn calculate_exposure(&self) -> f64 {
        // In a real implementation, this would sum up contract exposures
        self.active_contracts.len() as f64 * 0.1 // Placeholder
    }
}

/// Metrics for a position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionMetrics {
    /// Current value in satoshis.
    pub current_value: Satoshi,
    /// Unrealized PnL (current value - net deposits).
    pub unrealized_pnl: i64,
    /// Realized PnL from settled contracts.
    pub realized_pnl: i64,
    /// Total PnL.
    pub total_pnl: i64,
    /// Number of active contracts.
    pub num_active_contracts: usize,
    /// Current exposure (0.0 - 1.0).
    pub exposure: f64,
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
    fn test_position_lifecycle() {
        let mut position = Position::new(mock_pubkey());

        // Deposit
        position.record_deposit(Satoshi::from_sat(1_000_000), 100);
        assert_eq!(position.shares, 100);

        // Add contracts
        let contract_id = ContractId::from_data(b"test");
        position.add_contract(contract_id);
        assert_eq!(position.active_contracts.len(), 1);

        // Record PnL
        position.record_pnl(50_000);
        assert_eq!(position.realized_pnl, 50_000);

        // Withdraw
        position.record_withdrawal(Satoshi::from_sat(500_000), 50);
        assert_eq!(position.shares, 50);
    }
}
