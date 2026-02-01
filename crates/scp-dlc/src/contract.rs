//! DLC contract data structures.
//!
//! These structures represent the various stages of a DLC contract.

use crate::payout::PayoutCurve;
use scp_core::crypto::SchnorrSignature;
use scp_core::{ContractId, OracleInfo, PublicKey, Satoshi, Timestamp};
use serde::{Deserialize, Serialize};

/// A contract offer.
///
/// This is the first message in the DLC protocol, sent by the
/// party initiating the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractOffer {
    /// Unique contract identifier.
    pub contract_id: ContractId,
    /// The party making the offer.
    pub offerer: PublicKey,
    /// Collateral the offerer is putting up.
    pub collateral: Satoshi,
    /// The payout curve/structure.
    pub payout_curve: PayoutCurve,
    /// Oracle information for attestation.
    pub oracle_info: OracleInfo,
    /// Description of the event/condition.
    pub event_descriptor: String,
    /// When the contract matures.
    pub maturity: Timestamp,
}

impl ContractOffer {
    /// Calculate the total locked value (both sides).
    pub fn total_value(&self) -> Satoshi {
        // In a symmetric contract, both parties put up equal collateral
        self.collateral + self.collateral
    }

    /// Validate the offer.
    pub fn validate(&self) -> Result<(), String> {
        if self.collateral == Satoshi::ZERO {
            return Err("Collateral cannot be zero".to_string());
        }
        if self.event_descriptor.is_empty() {
            return Err("Event descriptor cannot be empty".to_string());
        }
        if self.maturity.is_past() {
            return Err("Maturity date is in the past".to_string());
        }
        Ok(())
    }
}

/// A contract acceptance.
///
/// Sent by the counterparty to accept an offer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractAccept {
    /// The contract being accepted.
    pub contract_id: ContractId,
    /// The accepting party.
    pub accepter: PublicKey,
    /// Collateral the accepter is putting up.
    pub collateral: Satoshi,
}

impl ContractAccept {
    /// Validate the acceptance against an offer.
    pub fn validate_against(&self, offer: &ContractOffer) -> Result<(), String> {
        if self.contract_id != offer.contract_id {
            return Err("Contract ID mismatch".to_string());
        }
        if self.collateral == Satoshi::ZERO {
            return Err("Collateral cannot be zero".to_string());
        }
        Ok(())
    }
}

/// Signatures for contract execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractSign {
    /// The contract being signed.
    pub contract_id: ContractId,
    /// Offerer's adaptor signatures.
    pub offerer_signature: SchnorrSignature,
    /// Accepter's adaptor signatures.
    pub accepter_signature: SchnorrSignature,
    /// Refund timeout (block height or timestamp).
    pub refund_timeout: Timestamp,
}

impl ContractSign {
    /// Verify signatures against public keys.
    ///
    /// # Arguments
    /// * `offerer_pk` - The public key of the offerer.
    /// * `accepter_pk` - The public key of the accepter.
    /// * `message` - The message that was signed (e.g., funding tx hash).
    pub fn verify(
        &self,
        offerer_pk: &PublicKey,
        accepter_pk: &PublicKey,
        message: &[u8],
    ) -> Result<(), String> {
        use secp256k1::{Message, Secp256k1, XOnlyPublicKey};

        let secp = Secp256k1::verification_only();

        // Convert message to digest
        let msg_hash = if message.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(message);
            arr
        } else {
            use bitcoin::hashes::{sha256, Hash};
            sha256::Hash::hash(message).to_byte_array()
        };
        let msg = Message::from_digest(msg_hash);

        // Verify offerer signature
        let offerer_xonly = XOnlyPublicKey::from(*offerer_pk.inner());
        let offerer_sig =
            secp256k1::schnorr::Signature::from_slice(self.offerer_signature.as_bytes())
                .map_err(|e| format!("Invalid offerer signature format: {}", e))?;

        secp.verify_schnorr(&offerer_sig, &msg, &offerer_xonly)
            .map_err(|e| format!("Invalid offerer signature: {}", e))?;

        // Verify accepter signature
        let accepter_xonly = XOnlyPublicKey::from(*accepter_pk.inner());
        let accepter_sig =
            secp256k1::schnorr::Signature::from_slice(self.accepter_signature.as_bytes())
                .map_err(|e| format!("Invalid accepter signature format: {}", e))?;

        secp.verify_schnorr(&accepter_sig, &msg, &accepter_xonly)
            .map_err(|e| format!("Invalid accepter signature: {}", e))?;

        Ok(())
    }
}

/// A fully formed contract.
///
/// Contains all information needed to execute or refund the contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contract {
    /// Unique identifier.
    pub id: ContractId,
    /// The original offer.
    pub offer: ContractOffer,
    /// The acceptance.
    pub accept: ContractAccept,
    /// Offerer's signature.
    pub offerer_signature: SchnorrSignature,
    /// Accepter's signature.
    pub accepter_signature: SchnorrSignature,
    /// Funding transaction ID (once funded).
    pub funding_txid: Option<[u8; 32]>,
    /// When the refund path becomes valid.
    pub refund_timeout: Timestamp,
}

impl Contract {
    /// Get the total locked value.
    pub fn total_value(&self) -> Satoshi {
        self.offer.collateral + self.accept.collateral
    }

    /// Check if the refund timeout has passed.
    pub fn is_refundable(&self) -> bool {
        self.refund_timeout.is_past()
    }

    /// Get the offerer's public key.
    pub fn offerer(&self) -> &PublicKey {
        &self.offer.offerer
    }

    /// Get the accepter's public key.
    pub fn accepter(&self) -> &PublicKey {
        &self.accept.accepter
    }
}

/// Contract summary for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSummary {
    pub id: ContractId,
    pub total_value: Satoshi,
    pub event: String,
    pub maturity: Timestamp,
    pub state: String,
}

impl From<&Contract> for ContractSummary {
    fn from(contract: &Contract) -> Self {
        Self {
            id: contract.id,
            total_value: contract.total_value(),
            event: contract.offer.event_descriptor.clone(),
            maturity: contract.offer.maturity,
            state: "Active".to_string(),
        }
    }
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
    fn test_offer_validation() {
        let offer = ContractOffer {
            contract_id: ContractId::from_data(b"test"),
            offerer: mock_pubkey(),
            collateral: Satoshi::from_sat(1_000_000),
            payout_curve: PayoutCurve::Binary {
                win_amount: Satoshi::from_sat(2_000_000),
                lose_amount: Satoshi::ZERO,
            },
            oracle_info: OracleInfo {
                public_key: mock_pubkey(),
                name: "Oracle".to_string(),
                endpoint: None,
            },
            event_descriptor: "BTC > 50k".to_string(),
            maturity: Timestamp::from_unix(u64::MAX), // Far future
        };

        assert!(offer.validate().is_ok());
    }

    #[test]
    fn test_zero_collateral_rejected() {
        let offer = ContractOffer {
            contract_id: ContractId::from_data(b"test"),
            offerer: mock_pubkey(),
            collateral: Satoshi::ZERO,
            payout_curve: PayoutCurve::Binary {
                win_amount: Satoshi::from_sat(1_000_000),
                lose_amount: Satoshi::ZERO,
            },
            oracle_info: OracleInfo {
                public_key: mock_pubkey(),
                name: "Oracle".to_string(),
                endpoint: None,
            },
            event_descriptor: "Test".to_string(),
            maturity: Timestamp::from_unix(u64::MAX),
        };

        assert!(offer.validate().is_err());
    }
}
