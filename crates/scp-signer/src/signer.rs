//! High-level signing interface.
//!
//! Combines keystore and policy enforcement into a unified interface.

use crate::keystore::{KeyId, KeyInfo, Keystore};
use crate::policy::{SigningPolicy, SigningRequest};
use scp_core::Result;
use std::sync::Arc;
use tracing::{info, warn};

/// The main signer interface.
///
/// Combines a keystore with policy enforcement. All signing operations
/// go through policy validation before execution.
pub struct Signer<K: Keystore> {
    keystore: Arc<K>,
    policy: SigningPolicy,
}

impl<K: Keystore> Signer<K> {
    /// Create a new signer.
    pub fn new(keystore: Arc<K>, policy: SigningPolicy) -> Self {
        Self { keystore, policy }
    }

    /// Generate a new key.
    pub fn generate_key(&self, label: Option<String>) -> Result<KeyInfo> {
        let info = self.keystore.generate_key(label)?;
        info!(key_id = %info.id, "Generated new signing key");
        Ok(info)
    }

    /// List all available keys.
    pub fn list_keys(&self) -> Result<Vec<KeyInfo>> {
        self.keystore.list_keys()
    }

    /// Get key info.
    pub fn get_key(&self, id: &KeyId) -> Result<Option<KeyInfo>> {
        self.keystore.get_key_info(id)
    }

    /// Sign a request after policy validation.
    ///
    /// This is the main entry point for all signing operations.
    pub fn sign(&self, request: SigningRequest) -> Result<Vec<u8>> {
        // Validate against policy
        if let Err(e) = self.policy.validate(&request) {
            warn!(
                key_id = %request.key_id,
                requester = %request.requester,
                error = %e,
                "Signing request rejected by policy"
            );
            return Err(e);
        }

        // Perform the signing
        let signature = self.keystore.sign(&request.key_id, &request.message)?;

        // Record the successful sign for rate limiting
        self.policy.record_sign(&request)?;

        info!(
            key_id = %request.key_id,
            requester = %request.requester,
            "Signed request successfully"
        );

        Ok(signature)
    }

    /// Sign a request using Schnorr (BIP-340) after policy validation.
    pub fn sign_schnorr(&self, request: SigningRequest) -> Result<Vec<u8>> {
        // Validate against policy
        if let Err(e) = self.policy.validate(&request) {
            warn!(
                key_id = %request.key_id,
                requester = %request.requester,
                error = %e,
                "Signing request rejected by policy"
            );
            return Err(e);
        }

        // Perform the signing
        let signature = self
            .keystore
            .sign_schnorr(&request.key_id, &request.message)?;

        // Record the successful sign for rate limiting
        self.policy.record_sign(&request)?;

        info!(
            key_id = %request.key_id,
            requester = %request.requester,
            "Signed Schnorr request successfully"
        );

        Ok(signature)
    }

    /// Check if a request would be allowed by policy (without signing).
    pub fn would_allow(&self, request: &SigningRequest) -> bool {
        self.policy.validate(request).is_ok()
    }

    /// Delete a key.
    pub fn delete_key(&self, id: &KeyId) -> Result<()> {
        self.keystore.delete_key(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keystore::FileKeystore;
    use crate::policy::{PolicyConfig, SigningOperation};
    use tempfile::tempdir;

    #[test]
    fn test_signer_workflow() {
        let dir = tempdir().unwrap();
        let keystore = Arc::new(FileKeystore::new(dir.path(), "test_password").unwrap());
        let policy = SigningPolicy::new(PolicyConfig::default());
        let signer = Signer::new(keystore, policy);

        // Generate a key
        let key_info = signer.generate_key(Some("test".to_string())).unwrap();

        // Create and sign a request
        let request = SigningRequest {
            operation: SigningOperation::Message,
            message: b"Hello, SCP!".to_vec(),
            key_id: key_info.id.clone(),
            requester: "vault".to_string(),
        };

        let signature = signer.sign(request).unwrap();
        assert_eq!(signature.len(), 64);
    }

    #[test]
    fn test_would_allow() {
        let dir = tempdir().unwrap();
        let keystore = Arc::new(FileKeystore::new(dir.path(), "test_password").unwrap());

        let mut config = PolicyConfig::default();
        config.blocked_requesters.push("bad_agent".to_string());

        let policy = SigningPolicy::new(config);
        let signer = Signer::new(keystore, policy);

        let good_request = SigningRequest {
            operation: SigningOperation::Message,
            message: vec![],
            key_id: "any".to_string(),
            requester: "vault".to_string(),
        };
        assert!(signer.would_allow(&good_request));

        let bad_request = SigningRequest {
            operation: SigningOperation::Message,
            message: vec![],
            key_id: "any".to_string(),
            requester: "bad_agent".to_string(),
        };
        assert!(!signer.would_allow(&bad_request));
    }
}
