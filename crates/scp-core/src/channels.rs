//! Async channel message types for inter-crate communication.
//!
//! This module defines the messages passed between `scp-agent` and `scp-vault`
//! using `tokio::sync::mpsc` channels for non-blocking communication.
//!
//! ## Data Flow
//!
//! ```text
//! ┌──────────────┐    VaultEvent    ┌──────────────┐
//! │  scp-agent   │◄─────────────────│  scp-vault   │
//! │              │                  │              │
//! │              │─────────────────▶│              │
//! └──────────────┘   AgentSignal    └──────────────┘
//! ```
//!
//! Both run in separate Tokio tasks, never blocking each other.

use crate::types::{ContractId, PublicKey, Satoshi};
use serde::{Deserialize, Serialize};

/// Signals sent from the AI Agent to the Vault.
///
/// The Agent analyzes market conditions and sends commands to adjust
/// vault behavior. The Vault treats these as suggestions and validates
/// them against safety rules before execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentSignal {
    /// Widen the bid-ask spread.
    ///
    /// Used when volatility or negative sentiment is detected.
    /// Factor is multiplicative (1.5 = 50% wider spread).
    WidenSpread {
        /// Spread multiplier (1.0 = no change, 2.0 = double).
        factor: f64,
    },

    /// Narrow the spread back toward normal.
    NarrowSpread {
        /// Target spread percentage (0.0 - 1.0).
        target: f64,
    },

    /// Activate circuit breaker - pause all new contract creation.
    ///
    /// Existing contracts continue to operate normally.
    CircuitBreaker {
        /// Reason for the circuit break (for logging).
        reason: String,
        /// Duration in seconds (None = until manually resumed).
        duration_secs: Option<u64>,
    },

    /// Resume normal operations after circuit breaker.
    Resume,

    /// Update the risk/reputation score for a counterparty.
    ///
    /// Low scores may result in higher collateral requirements
    /// or rejection of contract offers.
    UpdateRiskScore {
        /// The counterparty's public key.
        pubkey: PublicKey,
        /// New risk score (0.0 = highest risk, 1.0 = lowest risk).
        score: f64,
        /// Reason for the score update.
        reason: String,
    },

    /// Suggest rebalancing delta-neutral positions.
    RebalanceHedge {
        /// Suggested adjustment amount.
        delta_adjustment: i64,
    },

    /// Request the vault to pause accepting deposits from a specific address.
    BlockCounterparty {
        /// The counterparty to block.
        pubkey: PublicKey,
        /// Reason for blocking.
        reason: String,
    },
}

impl AgentSignal {
    /// Check if this is a critical signal requiring immediate action.
    pub fn is_critical(&self) -> bool {
        matches!(
            self,
            AgentSignal::CircuitBreaker { .. } | AgentSignal::BlockCounterparty { .. }
        )
    }

    /// Get a human-readable description.
    pub fn description(&self) -> String {
        match self {
            AgentSignal::WidenSpread { factor } => {
                format!("Widen spread by {:.1}x", factor)
            }
            AgentSignal::NarrowSpread { target } => {
                format!("Narrow spread to {:.2}%", target * 100.0)
            }
            AgentSignal::CircuitBreaker { reason, .. } => {
                format!("Circuit breaker: {}", reason)
            }
            AgentSignal::Resume => "Resume normal operations".to_string(),
            AgentSignal::UpdateRiskScore { pubkey, score, .. } => {
                format!("Update risk score for {} to {:.2}", pubkey, score)
            }
            AgentSignal::RebalanceHedge { delta_adjustment } => {
                format!("Rebalance hedge by {}", delta_adjustment)
            }
            AgentSignal::BlockCounterparty { pubkey, reason } => {
                format!("Block {}: {}", pubkey, reason)
            }
        }
    }
}

/// Events sent from the Vault to the AI Agent.
///
/// The Vault notifies the Agent about significant activities
/// that may require risk assessment or response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VaultEvent {
    /// A large deposit was received.
    LargeDeposit {
        /// The depositor's public key.
        from: PublicKey,
        /// The deposit amount.
        amount: Satoshi,
    },

    /// A large withdrawal was requested.
    LargeWithdrawal {
        /// The withdrawer's public key.
        to: PublicKey,
        /// The withdrawal amount.
        amount: Satoshi,
    },

    /// Potential toxic flow detected (informed trader).
    SuspiciousActivity {
        /// The counterparty public key.
        counterparty: PublicKey,
        /// Description of the suspicious pattern.
        details: String,
        /// Confidence level (0.0 - 1.0).
        confidence: f64,
    },

    /// A new contract offer was received.
    ContractOfferReceived {
        /// The contract identifier.
        contract_id: ContractId,
        /// The counterparty.
        counterparty: PublicKey,
        /// The collateral amount.
        collateral: Satoshi,
    },

    /// A contract was settled.
    ContractSettled {
        /// The contract identifier.
        contract_id: ContractId,
        /// Our profit/loss (positive = profit).
        pnl: i64,
    },

    /// An oracle attestation was received.
    OracleAttestation {
        /// The affected contract.
        contract_id: ContractId,
        /// The outcome value.
        outcome_value: i64,
    },

    /// The vault's total exposure changed significantly.
    ExposureChange {
        /// New total exposure in satoshis.
        total_exposure: Satoshi,
        /// Change from previous (can be negative).
        delta: i64,
    },

    /// Current pool metrics update (periodic).
    PoolMetrics {
        /// Total liquidity in the pool.
        total_liquidity: Satoshi,
        /// Current utilization rate (0.0 - 1.0).
        utilization: f64,
        /// Current spread.
        current_spread: f64,
    },
}

impl VaultEvent {
    /// Check if this event represents a potential risk.
    pub fn is_risk_event(&self) -> bool {
        matches!(
            self,
            VaultEvent::LargeDeposit { .. }
                | VaultEvent::LargeWithdrawal { .. }
                | VaultEvent::SuspiciousActivity { .. }
        )
    }

    /// Get the priority level (1 = low, 5 = critical).
    pub fn priority(&self) -> u8 {
        match self {
            VaultEvent::SuspiciousActivity { confidence, .. } if *confidence > 0.8 => 5,
            VaultEvent::SuspiciousActivity { .. } => 4,
            VaultEvent::LargeWithdrawal { .. } => 3,
            VaultEvent::LargeDeposit { .. } => 3,
            VaultEvent::ExposureChange { .. } => 2,
            VaultEvent::ContractSettled { .. } => 2,
            VaultEvent::ContractOfferReceived { .. } => 2,
            VaultEvent::OracleAttestation { .. } => 2,
            VaultEvent::PoolMetrics { .. } => 1,
        }
    }
}

/// Channel builder for Agent-Vault communication.
pub mod channel {
    use super::{AgentSignal, VaultEvent};
    use tokio::sync::mpsc;

    /// Default channel buffer size.
    pub const DEFAULT_BUFFER_SIZE: usize = 256;

    pub type AgentChannels = (
        (mpsc::Sender<AgentSignal>, mpsc::Receiver<AgentSignal>),
        (mpsc::Sender<VaultEvent>, mpsc::Receiver<VaultEvent>),
    );

    /// Create a pair of channels for Agent-Vault communication.
    ///
    /// Returns:
    /// - `(agent_tx, vault_rx)` for Agent → Vault signals
    /// - `(vault_tx, agent_rx)` for Vault → Agent events
    pub fn create_channels(buffer_size: usize) -> AgentChannels {
        let (agent_tx, vault_rx) = mpsc::channel(buffer_size);
        let (vault_tx, agent_rx) = mpsc::channel(buffer_size);
        ((agent_tx, vault_rx), (vault_tx, agent_rx))
    }

    /// Create channels with default buffer size.
    pub fn create_default_channels() -> AgentChannels {
        create_channels(DEFAULT_BUFFER_SIZE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_signal_criticality() {
        let widen = AgentSignal::WidenSpread { factor: 1.5 };
        assert!(!widen.is_critical());

        let circuit = AgentSignal::CircuitBreaker {
            reason: "test".to_string(),
            duration_secs: None,
        };
        assert!(circuit.is_critical());
    }

    #[test]
    fn test_vault_event_priority() {
        let suspicious = VaultEvent::SuspiciousActivity {
            counterparty: PublicKey::new(
                secp256k1::PublicKey::from_slice(&[
                    0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
                ])
                .unwrap(),
            ),
            details: "test".to_string(),
            confidence: 0.9,
        };
        assert_eq!(suspicious.priority(), 5);
    }

    #[tokio::test]
    async fn test_channel_creation() {
        let ((agent_tx, mut vault_rx), (vault_tx, mut agent_rx)) =
            channel::create_default_channels();

        // Send signal from agent to vault
        agent_tx
            .send(AgentSignal::WidenSpread { factor: 1.5 })
            .await
            .unwrap();

        // Receive in vault
        let signal = vault_rx.recv().await.unwrap();
        assert!(matches!(signal, AgentSignal::WidenSpread { factor } if factor == 1.5));

        // Send event from vault to agent
        vault_tx
            .send(VaultEvent::PoolMetrics {
                total_liquidity: Satoshi::from_sat(1_000_000),
                utilization: 0.5,
                current_spread: 0.02,
            })
            .await
            .unwrap();

        // Receive in agent
        let event = agent_rx.recv().await.unwrap();
        assert!(matches!(event, VaultEvent::PoolMetrics { .. }));
    }
}
