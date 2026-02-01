//! Signing policy enforcement.
//!
//! All signing requests are validated against policies before execution.
//! This prevents unauthorized or malicious signing operations.

use scp_core::{ContractId, Error, Result, Satoshi};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// A request to sign something.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningRequest {
    /// Type of signing operation.
    pub operation: SigningOperation,
    /// The message/data to sign.
    #[serde(skip)]
    pub message: Vec<u8>,
    /// The key to use.
    pub key_id: String,
    /// Requester identifier (for audit).
    pub requester: String,
}

/// Types of signing operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SigningOperation {
    /// Sign a DLC contract offer.
    ContractOffer { contract_id: ContractId },
    /// Sign a DLC contract acceptance.
    ContractAccept { contract_id: ContractId },
    /// Sign a DLC contract execution.
    ContractExecute {
        contract_id: ContractId,
        amount: Satoshi,
    },
    /// Sign a refund transaction.
    Refund {
        contract_id: ContractId,
        amount: Satoshi,
    },
    /// Sign a withdrawal.
    Withdrawal { amount: Satoshi },
    /// Sign a deposit acknowledgment.
    Deposit { amount: Satoshi },
    /// Generic message signing (for authentication, etc.).
    Message,
}

/// Signing policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// Maximum signs per minute (rate limiting).
    pub max_signs_per_minute: u32,
    /// Maximum amount per single signature (in satoshis).
    pub max_amount_per_sign: u64,
    /// Maximum total amount per hour.
    pub max_amount_per_hour: u64,
    /// Allowed operations (if empty, all are allowed).
    pub allowed_operations: Vec<String>,
    /// Blocked requester identifiers.
    pub blocked_requesters: Vec<String>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            max_signs_per_minute: 60,
            max_amount_per_sign: 100_000_000_000,   // 1000 BTC
            max_amount_per_hour: 1_000_000_000_000, // 10000 BTC
            allowed_operations: vec![],
            blocked_requesters: vec![],
        }
    }
}

/// Signing policy enforcer.
pub struct SigningPolicy {
    config: PolicyConfig,
    /// Recent signing times for rate limiting.
    recent_signs: RwLock<Vec<Instant>>,
    /// Amount signed in the current hour.
    hourly_amounts: RwLock<HashMap<u64, u64>>, // hour -> total sats
}

impl SigningPolicy {
    /// Create a new signing policy.
    pub fn new(config: PolicyConfig) -> Self {
        Self {
            config,
            recent_signs: RwLock::new(Vec::new()),
            hourly_amounts: RwLock::new(HashMap::new()),
        }
    }

    /// Create with default policy.
    pub fn default_policy() -> Self {
        Self::new(PolicyConfig::default())
    }

    /// Validate a signing request against the policy.
    pub fn validate(&self, request: &SigningRequest) -> Result<()> {
        // Check if requester is blocked
        if self.config.blocked_requesters.contains(&request.requester) {
            return Err(Error::PolicyViolation(format!(
                "Requester '{}' is blocked",
                request.requester
            )));
        }

        // Check operation allowlist (if configured)
        if !self.config.allowed_operations.is_empty() {
            let op_name = self.operation_name(&request.operation);
            if !self.config.allowed_operations.contains(&op_name) {
                return Err(Error::PolicyViolation(format!(
                    "Operation '{}' is not allowed",
                    op_name
                )));
            }
        }

        // Check rate limit
        self.check_rate_limit()?;

        // Check amount limits
        if let Some(amount) = self.extract_amount(&request.operation) {
            self.check_amount_limit(amount)?;
        }

        Ok(())
    }

    /// Record a successful signing (for rate limiting).
    pub fn record_sign(&self, request: &SigningRequest) -> Result<()> {
        // Record time for rate limiting
        let mut recent = self
            .recent_signs
            .write()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        recent.push(Instant::now());

        // Prune old entries (older than 1 minute)
        let cutoff = Instant::now() - Duration::from_secs(60);
        recent.retain(|t| *t > cutoff);

        // Record amount for hourly limit
        if let Some(amount) = self.extract_amount(&request.operation) {
            let hour = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                / 3600;

            let mut hourly = self
                .hourly_amounts
                .write()
                .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
            *hourly.entry(hour).or_insert(0) += amount;

            // Prune old hours
            let current_hour = hour;
            hourly.retain(|h, _| *h >= current_hour - 1);
        }

        Ok(())
    }

    /// Check the rate limit.
    fn check_rate_limit(&self) -> Result<()> {
        let recent = self
            .recent_signs
            .read()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        let cutoff = Instant::now() - Duration::from_secs(60);
        let recent_count = recent.iter().filter(|t| **t > cutoff).count() as u32;

        if recent_count >= self.config.max_signs_per_minute {
            return Err(Error::RateLimitExceeded {
                limit: self.config.max_signs_per_minute,
                attempted: recent_count + 1,
            });
        }

        Ok(())
    }

    /// Check amount limits.
    fn check_amount_limit(&self, amount: u64) -> Result<()> {
        // Check per-sign limit
        if amount > self.config.max_amount_per_sign {
            return Err(Error::PolicyViolation(format!(
                "Amount {} exceeds per-sign limit of {}",
                amount, self.config.max_amount_per_sign
            )));
        }

        // Check hourly limit
        let hour = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            / 3600;

        let hourly = self
            .hourly_amounts
            .read()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        let current_total = hourly.get(&hour).copied().unwrap_or(0);

        if current_total + amount > self.config.max_amount_per_hour {
            return Err(Error::PolicyViolation(format!(
                "Hourly limit exceeded: {} + {} > {}",
                current_total, amount, self.config.max_amount_per_hour
            )));
        }

        Ok(())
    }

    /// Extract the amount from an operation (if applicable).
    fn extract_amount(&self, op: &SigningOperation) -> Option<u64> {
        match op {
            SigningOperation::ContractExecute { amount, .. } => Some(amount.as_sat()),
            SigningOperation::Refund { amount, .. } => Some(amount.as_sat()),
            SigningOperation::Withdrawal { amount } => Some(amount.as_sat()),
            SigningOperation::Deposit { amount } => Some(amount.as_sat()),
            _ => None,
        }
    }

    /// Get the operation name for allowlist checking.
    fn operation_name(&self, op: &SigningOperation) -> String {
        match op {
            SigningOperation::ContractOffer { .. } => "contract_offer".to_string(),
            SigningOperation::ContractAccept { .. } => "contract_accept".to_string(),
            SigningOperation::ContractExecute { .. } => "contract_execute".to_string(),
            SigningOperation::Refund { .. } => "refund".to_string(),
            SigningOperation::Withdrawal { .. } => "withdrawal".to_string(),
            SigningOperation::Deposit { .. } => "deposit".to_string(),
            SigningOperation::Message => "message".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy_allows_signing() {
        let policy = SigningPolicy::default_policy();
        let request = SigningRequest {
            operation: SigningOperation::Message,
            message: vec![1, 2, 3],
            key_id: "test_key".to_string(),
            requester: "vault".to_string(),
        };

        assert!(policy.validate(&request).is_ok());
    }

    #[test]
    fn test_blocked_requester() {
        let mut config = PolicyConfig::default();
        config
            .blocked_requesters
            .push("malicious_agent".to_string());

        let policy = SigningPolicy::new(config);
        let request = SigningRequest {
            operation: SigningOperation::Message,
            message: vec![1, 2, 3],
            key_id: "test_key".to_string(),
            requester: "malicious_agent".to_string(),
        };

        let result = policy.validate(&request);
        assert!(matches!(result, Err(Error::PolicyViolation(_))));
    }

    #[test]
    fn test_amount_limit() {
        let mut config = PolicyConfig::default();
        config.max_amount_per_sign = 1_000_000; // 0.01 BTC

        let policy = SigningPolicy::new(config);
        let request = SigningRequest {
            operation: SigningOperation::Withdrawal {
                amount: Satoshi::from_sat(2_000_000),
            },
            message: vec![],
            key_id: "test_key".to_string(),
            requester: "vault".to_string(),
        };

        let result = policy.validate(&request);
        assert!(matches!(result, Err(Error::PolicyViolation(_))));
    }
}
