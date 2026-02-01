//! Cryptographic type wrappers for SCP.
//!
//! **IMPORTANT**: This module provides TYPE DEFINITIONS only.
//! All actual cryptographic operations are delegated to `secp256k1-zkp`
//! or `rust-dlc`. We do NOT implement cryptographic primitives ourselves.

use crate::types::PublicKey;
use serde::{Deserialize, Serialize};

/// A Schnorr signature (BIP-340 compliant).
///
/// This is a wrapper around the underlying secp256k1 signature type.
/// We do not implement signature creation - that's done by `scp-signer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchnorrSignature {
    /// The raw signature bytes (64 bytes for Schnorr).
    bytes: Vec<u8>,
}

impl SchnorrSignature {
    /// Create from raw bytes.
    ///
    /// # Panics
    /// Panics if bytes length is not 64.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        if bytes.len() != 64 {
            panic!("Invalid Schnorr signature length: {}", bytes.len());
        }
        Self {
            bytes: bytes.to_vec(),
        }
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// An adaptor signature (pre-signature locked by an oracle's secret).
///
/// The mathematical relationship is:
/// ```text
/// adaptor_sig + oracle_secret = valid_signature
/// ```
///
/// This allows atomic settlement: when the oracle reveals the secret,
/// anyone can complete the signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptorSignature {
    /// The adaptor signature bytes.
    bytes: Vec<u8>,
    /// The oracle nonce this adaptor is locked to.
    oracle_nonce: OracleNonce,
}

impl AdaptorSignature {
    /// Create a new adaptor signature.
    pub fn new(bytes: Vec<u8>, oracle_nonce: OracleNonce) -> Self {
        Self {
            bytes,
            oracle_nonce,
        }
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Get the oracle nonce this adaptor is locked to.
    pub fn oracle_nonce(&self) -> &OracleNonce {
        &self.oracle_nonce
    }

    /// Complete the adaptor signature using the oracle's revealed secret.
    ///
    /// Returns a valid Schnorr signature if the secret is correct.
    ///
    /// # Note
    /// In production, this would use `secp256k1-zkp` functions.
    /// This is a placeholder demonstrating the interface.
    pub fn complete(&self, oracle_secret: &OracleSecret) -> Option<SchnorrSignature> {
        // In production: use secp256k1_zkp::adapt()
        // This is a placeholder - real implementation would be:
        // secp256k1_zkp::adapt(&self.bytes, &oracle_secret.scalar)

        // For now, we just validate the nonce matches
        if oracle_secret.nonce == self.oracle_nonce {
            // Placeholder: in reality, this would compute the actual signature
            Some(SchnorrSignature::from_bytes(&[0u8; 64]))
        } else {
            None
        }
    }
}

/// An oracle's public nonce commitment.
///
/// The oracle publishes this ahead of time. When the event occurs,
/// they reveal the corresponding secret scalar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleNonce {
    /// The nonce point (compressed public key format).
    point: Vec<u8>,
}

impl OracleNonce {
    /// Create from compressed point bytes.
    pub fn from_bytes(bytes: [u8; 33]) -> Self {
        Self {
            point: bytes.to_vec(),
        }
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.point
    }
}

/// An oracle's revealed secret scalar.
///
/// Published when the event outcome is determined.
/// Used to complete adaptor signatures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleSecret {
    /// The secret scalar (32 bytes).
    scalar: [u8; 32],
    /// The corresponding nonce (for verification).
    nonce: OracleNonce,
}

impl OracleSecret {
    /// Create a new oracle secret.
    pub fn new(scalar: [u8; 32], nonce: OracleNonce) -> Self {
        Self { scalar, nonce }
    }

    /// Get the secret scalar bytes.
    pub fn scalar(&self) -> &[u8; 32] {
        &self.scalar
    }

    /// Get the corresponding nonce.
    pub fn nonce(&self) -> &OracleNonce {
        &self.nonce
    }
}

/// An oracle attestation (signed outcome).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleAttestation {
    /// The oracle that signed this attestation.
    pub oracle_pubkey: PublicKey,
    /// The outcome value (e.g., price).
    pub outcome: Outcome,
    /// The revealed secret for this outcome.
    pub secret: OracleSecret,
    /// Signature over the outcome.
    pub signature: SchnorrSignature,
}

/// An outcome from an oracle attestation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// A binary outcome (true/false, yes/no).
    Binary(bool),
    /// A numeric outcome (e.g., BTC price).
    Numeric(i64),
    /// An enumerated outcome (category index).
    Enumerated(u32),
}

impl Outcome {
    /// Check if this outcome matches a target.
    pub fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Outcome::Binary(a), Outcome::Binary(b)) => a == b,
            (Outcome::Numeric(a), Outcome::Numeric(b)) => a == b,
            (Outcome::Enumerated(a), Outcome::Enumerated(b)) => a == b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oracle_nonce_creation() {
        let bytes = [0u8; 33];
        let nonce = OracleNonce::from_bytes(bytes);
        assert_eq!(nonce.as_bytes(), &bytes[..]);
    }

    #[test]
    fn test_outcome_matching() {
        let binary_true = Outcome::Binary(true);
        let binary_false = Outcome::Binary(false);
        let numeric = Outcome::Numeric(50000);

        assert!(binary_true.matches(&Outcome::Binary(true)));
        assert!(!binary_true.matches(&binary_false));
        assert!(!binary_true.matches(&numeric));
    }
}
