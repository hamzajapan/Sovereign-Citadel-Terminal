//! DLC state machine implementation.
//!
//! Manages the lifecycle of a Discreet Log Contract:
//!
//! ```text
//! Offered → Accepted → Signed → Confirmed → Settled
//!                                    ↓
//!                                 Refunded (timeout)
//! ```
//!
//! All state transitions are persisted to disk before confirmation.

use crate::contract::{Contract, ContractAccept, ContractOffer, ContractSign};
use crate::storage::DlcStorage;
use scp_core::crypto::OracleAttestation;
use scp_core::{ContractId, Error, Result, Timestamp};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

/// The state of a DLC contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DlcState {
    /// Initial offer created, waiting for counterparty.
    Offered {
        offer: ContractOffer,
        created_at: Timestamp,
    },

    /// Counterparty accepted, waiting for signatures.
    Accepted {
        offer: ContractOffer,
        accept: ContractAccept,
        accepted_at: Timestamp,
    },

    /// Both parties signed, waiting for funding confirmation.
    Signed {
        contract: Contract,
        signed_at: Timestamp,
    },

    /// Funding transaction confirmed on-chain.
    Confirmed {
        contract: Contract,
        confirmed_at: Timestamp,
        block_height: u32,
    },

    /// Contract settled with oracle attestation.
    Settled {
        contract: Contract,
        attestation: OracleAttestation,
        settled_at: Timestamp,
        our_payout: u64, // satoshis
    },

    /// Contract refunded (timeout or mutual close).
    Refunded {
        contract: Contract,
        refunded_at: Timestamp,
        reason: RefundReason,
    },

    /// Contract failed or rejected.
    Failed {
        contract_id: ContractId,
        reason: String,
        failed_at: Timestamp,
    },
}

/// Reason for contract refund.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefundReason {
    /// Timeout expired without oracle attestation.
    Timeout,
    /// Mutual agreement to close.
    MutualClose,
    /// Oracle failed to attest.
    OracleFailure,
}

impl DlcState {
    /// Get the contract ID for this state.
    pub fn contract_id(&self) -> ContractId {
        match self {
            DlcState::Offered { offer, .. } => offer.contract_id,
            DlcState::Accepted { offer, .. } => offer.contract_id,
            DlcState::Signed { contract, .. } => contract.id,
            DlcState::Confirmed { contract, .. } => contract.id,
            DlcState::Settled { contract, .. } => contract.id,
            DlcState::Refunded { contract, .. } => contract.id,
            DlcState::Failed { contract_id, .. } => *contract_id,
        }
    }

    /// Get the state name as a string.
    pub fn name(&self) -> &'static str {
        match self {
            DlcState::Offered { .. } => "Offered",
            DlcState::Accepted { .. } => "Accepted",
            DlcState::Signed { .. } => "Signed",
            DlcState::Confirmed { .. } => "Confirmed",
            DlcState::Settled { .. } => "Settled",
            DlcState::Refunded { .. } => "Refunded",
            DlcState::Failed { .. } => "Failed",
        }
    }

    /// Check if this is a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            DlcState::Settled { .. } | DlcState::Refunded { .. } | DlcState::Failed { .. }
        )
    }

    /// Check if this state is active (funds are locked).
    pub fn is_active(&self) -> bool {
        matches!(self, DlcState::Signed { .. } | DlcState::Confirmed { .. })
    }
}

/// The DLC state machine.
///
/// Manages state transitions with persistence. Every transition is:
/// 1. Validated
/// 2. Persisted to disk
/// 3. Confirmed in memory
///
/// This ensures crash safety.
pub struct DlcStateMachine {
    storage: Arc<DlcStorage>,
}

impl DlcStateMachine {
    /// Create a new state machine with storage.
    pub fn new(storage: Arc<DlcStorage>) -> Self {
        Self { storage }
    }

    /// Create a new contract offer.
    pub fn create_offer(&self, offer: ContractOffer) -> Result<DlcState> {
        let contract_id = offer.contract_id;

        // Check if contract already exists
        if self.storage.get(&contract_id)?.is_some() {
            return Err(Error::ContractAlreadyExists(contract_id.to_string()));
        }

        let state = DlcState::Offered {
            offer,
            created_at: Timestamp::now(),
        };

        // Persist first, then confirm
        self.storage.save(&state)?;

        info!(contract_id = %contract_id, "Created contract offer");
        Ok(state)
    }

    /// Accept a contract offer.
    pub fn accept_offer(
        &self,
        contract_id: &ContractId,
        accept: ContractAccept,
    ) -> Result<DlcState> {
        let current = self.get_state(contract_id)?;

        // Validate transition
        let new_state = match current {
            DlcState::Offered { offer, .. } => {
                // Validate accept matches offer
                if accept.contract_id != offer.contract_id {
                    return Err(Error::InvalidContract(
                        "Accept contract_id doesn't match offer".to_string(),
                    ));
                }

                DlcState::Accepted {
                    offer,
                    accept,
                    accepted_at: Timestamp::now(),
                }
            }
            other => {
                return Err(Error::InvalidStateTransition {
                    from: other.name().to_string(),
                    to: "Accepted".to_string(),
                });
            }
        };

        // Persist first
        self.storage.save(&new_state)?;

        info!(contract_id = %contract_id, "Contract accepted");
        Ok(new_state)
    }

    /// Sign a contract (both parties have signed).
    pub fn sign_contract(&self, contract_id: &ContractId, sign: ContractSign) -> Result<DlcState> {
        let current = self.get_state(contract_id)?;

        let new_state = match current {
            DlcState::Accepted { offer, accept, .. } => {
                // Verify signatures
                // Note: In a real implementation, the message would be the Funding Transaction SIGHASH.
                // Here we verify against contract_id to demonstrate the cryptographic check.
                sign.verify(&offer.offerer, &accept.accepter, contract_id.as_bytes())
                    .map_err(Error::InvalidSignature)?;

                // Build the full contract
                let contract = Contract {
                    id: offer.contract_id,
                    offer,
                    accept,
                    offerer_signature: sign.offerer_signature,
                    accepter_signature: sign.accepter_signature,
                    funding_txid: None,
                    refund_timeout: sign.refund_timeout,
                };

                DlcState::Signed {
                    contract,
                    signed_at: Timestamp::now(),
                }
            }
            other => {
                return Err(Error::InvalidStateTransition {
                    from: other.name().to_string(),
                    to: "Signed".to_string(),
                });
            }
        };

        self.storage.save(&new_state)?;

        info!(contract_id = %contract_id, "Contract signed");
        Ok(new_state)
    }

    /// Confirm funding transaction is on-chain.
    pub fn confirm_funding(
        &self,
        contract_id: &ContractId,
        block_height: u32,
        funding_txid: [u8; 32],
    ) -> Result<DlcState> {
        let current = self.get_state(contract_id)?;

        let new_state = match current {
            DlcState::Signed { mut contract, .. } => {
                contract.funding_txid = Some(funding_txid);

                DlcState::Confirmed {
                    contract,
                    confirmed_at: Timestamp::now(),
                    block_height,
                }
            }
            other => {
                return Err(Error::InvalidStateTransition {
                    from: other.name().to_string(),
                    to: "Confirmed".to_string(),
                });
            }
        };

        self.storage.save(&new_state)?;

        info!(
            contract_id = %contract_id,
            block_height = block_height,
            "Funding confirmed"
        );
        Ok(new_state)
    }

    /// Settle the contract with an oracle attestation.
    pub fn settle(
        &self,
        contract_id: &ContractId,
        attestation: OracleAttestation,
        our_payout: u64,
    ) -> Result<DlcState> {
        let current = self.get_state(contract_id)?;

        let new_state = match current {
            DlcState::Confirmed { contract, .. } => DlcState::Settled {
                contract,
                attestation,
                settled_at: Timestamp::now(),
                our_payout,
            },
            other => {
                return Err(Error::InvalidStateTransition {
                    from: other.name().to_string(),
                    to: "Settled".to_string(),
                });
            }
        };

        self.storage.save(&new_state)?;

        info!(
            contract_id = %contract_id,
            payout = our_payout,
            "Contract settled"
        );
        Ok(new_state)
    }

    /// Refund the contract (timeout or mutual close).
    pub fn refund(&self, contract_id: &ContractId, reason: RefundReason) -> Result<DlcState> {
        let current = self.get_state(contract_id)?;

        let new_state = match current {
            DlcState::Confirmed { contract, .. } | DlcState::Signed { contract, .. } => {
                DlcState::Refunded {
                    contract,
                    refunded_at: Timestamp::now(),
                    reason: reason.clone(),
                }
            }
            other => {
                return Err(Error::InvalidStateTransition {
                    from: other.name().to_string(),
                    to: "Refunded".to_string(),
                });
            }
        };

        self.storage.save(&new_state)?;

        warn!(
            contract_id = %contract_id,
            reason = ?reason,
            "Contract refunded"
        );
        Ok(new_state)
    }

    /// Fail a contract.
    pub fn fail(&self, contract_id: &ContractId, reason: String) -> Result<DlcState> {
        let new_state = DlcState::Failed {
            contract_id: *contract_id,
            reason: reason.clone(),
            failed_at: Timestamp::now(),
        };

        self.storage.save(&new_state)?;

        warn!(contract_id = %contract_id, reason = %reason, "Contract failed");
        Ok(new_state)
    }

    /// Get the current state of a contract.
    pub fn get_state(&self, contract_id: &ContractId) -> Result<DlcState> {
        self.storage
            .get(contract_id)?
            .ok_or_else(|| Error::ContractNotFound(contract_id.to_string()))
    }

    /// List all contracts in a given state.
    pub fn list_by_state(&self, state_name: &str) -> Result<Vec<DlcState>> {
        self.storage.list_by_state(state_name)
    }

    /// List all active contracts.
    pub fn list_active(&self) -> Result<Vec<DlcState>> {
        let mut active = self.list_by_state("Signed")?;
        active.extend(self.list_by_state("Confirmed")?);
        Ok(active)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payout::PayoutCurve;
    use scp_core::{OracleInfo, PublicKey, Satoshi};
    use tempfile::tempdir;

    fn create_test_offer() -> ContractOffer {
        // Create a dummy public key for testing
        let secp = secp256k1::Secp256k1::new();
        let (_, pubkey) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());

        ContractOffer {
            contract_id: ContractId::from_data(b"test_contract"),
            offerer: PublicKey::new(pubkey),
            collateral: Satoshi::from_sat(1_000_000),
            payout_curve: PayoutCurve::Binary {
                win_amount: Satoshi::from_sat(2_000_000),
                lose_amount: Satoshi::ZERO,
            },
            oracle_info: OracleInfo {
                public_key: PublicKey::new(pubkey),
                name: "Test Oracle".to_string(),
                endpoint: None,
            },
            event_descriptor: "BTC/USD > 50000".to_string(),
            maturity: Timestamp::from_unix(1700000000),
        }
    }

    #[test]
    fn test_offer_to_accepted_transition() {
        let dir = tempdir().unwrap();
        let storage = Arc::new(DlcStorage::new(dir.path()).unwrap());
        let sm = DlcStateMachine::new(storage);

        let offer = create_test_offer();
        let contract_id = offer.contract_id;

        // Create offer
        let state = sm.create_offer(offer.clone()).unwrap();
        assert!(matches!(state, DlcState::Offered { .. }));

        // Accept offer
        let secp = secp256k1::Secp256k1::new();
        let (_, pubkey) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());

        let accept = ContractAccept {
            contract_id,
            accepter: PublicKey::new(pubkey),
            collateral: Satoshi::from_sat(1_000_000),
        };

        let state = sm.accept_offer(&contract_id, accept).unwrap();
        assert!(matches!(state, DlcState::Accepted { .. }));
    }

    #[test]
    fn test_invalid_transition() {
        let dir = tempdir().unwrap();
        let storage = Arc::new(DlcStorage::new(dir.path()).unwrap());
        let sm = DlcStateMachine::new(storage);

        let offer = create_test_offer();
        let contract_id = offer.contract_id;

        sm.create_offer(offer).unwrap();

        // Try to settle directly from Offered (invalid)
        let result = sm.settle(
            &contract_id,
            OracleAttestation {
                oracle_pubkey: PublicKey::new(
                    secp256k1::Secp256k1::new()
                        .generate_keypair(&mut secp256k1::rand::thread_rng())
                        .1,
                ),
                outcome: scp_core::crypto::Outcome::Binary(true),
                secret: scp_core::crypto::OracleSecret::new(
                    [0u8; 32],
                    scp_core::crypto::OracleNonce::from_bytes([0u8; 33]),
                ),
                signature: scp_core::crypto::SchnorrSignature::from_bytes(&[0u8; 64]),
            },
            1_000_000,
        );

        assert!(matches!(result, Err(Error::InvalidStateTransition { .. })));
    }
}
