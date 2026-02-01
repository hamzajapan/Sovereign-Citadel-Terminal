//! Common test utilities and harness.

use scp_core::crypto::{OracleAttestation, OracleNonce, OracleSecret, Outcome, SchnorrSignature};
use scp_core::PublicKey;
use scp_dlc::state_machine::DlcStateMachine;
use scp_dlc::storage::DlcStorage;
use std::sync::Arc;
use tempfile::TempDir;

/// Test party.
    pub name: String,
    pub keypair: secp256k1::Keypair,
    pub pubkey: PublicKey,
}

/// Test harness for integration tests.
pub struct TestHarness {
    pub state_machine: DlcStateMachine,
    #[allow(dead_code)]
    temp_dir: TempDir,
    secp: secp256k1::Secp256k1<secp256k1::All>,
}

impl TestHarness {
    /// Create a new test harness.
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = Arc::new(DlcStorage::new(temp_dir.path()).expect("Failed to create storage"));
        let state_machine = DlcStateMachine::new(storage);
        let secp = secp256k1::Secp256k1::new();

        Self {
            state_machine,
            temp_dir,
            secp,
        }
    }

    /// Create a test party.
    pub fn create_party(&self, name: &str) -> TestParty {
        let (secret, pubkey) = self.secp.generate_keypair(&mut secp256k1::rand::thread_rng());
        let keypair = secp256k1::Keypair::from_secret_key(&self.secp, &secret);
        TestParty {
            name: name.to_string(),
            keypair,
            pubkey: PublicKey::new(pubkey),
        }
    }

    /// Sign a message.
    pub fn sign_message(&self, party: &TestParty, message: &[u8]) -> SchnorrSignature {
         use secp256k1::{Message, Hash};
         
         let msg_hash = if message.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(message);
            arr
        } else {
            use bitcoin::hashes::{sha256, Hash};
            sha256::Hash::hash(message).to_byte_array()
        };
        
        let msg = Message::from_digest(msg_hash);
        let signature = self.secp.sign_schnorr(&msg, &party.keypair);
        
        SchnorrSignature::from_bytes(signature.as_ref())
    }

    /// Create a test oracle.
    pub fn create_oracle(&self, name: &str) -> TestParty {
        self.create_party(name)
    }

    /// Create a mock signature.
    pub fn mock_signature(&self) -> SchnorrSignature {
        SchnorrSignature::from_bytes(&[0u8; 64])
    }

    /// Create a mock attestation.
    pub fn create_attestation(&self, oracle: &TestParty, value: i64) -> OracleAttestation {
        OracleAttestation {
            oracle_pubkey: oracle.pubkey,
            outcome: Outcome::Numeric(value),
            secret: OracleSecret::new(
                [0u8; 32],
                OracleNonce::from_bytes([0u8; 33]),
            ),
            signature: self.mock_signature(),
        }
    }
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::new()
    }
}
