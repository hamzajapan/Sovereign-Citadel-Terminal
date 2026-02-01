//! Unified error types for the Sovereign Citadel Protocol.
//!
//! All crates in the workspace use these error types for consistency.

use thiserror::Error;

/// The unified result type for SCP operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in the Sovereign Citadel Protocol.
#[derive(Debug, Error)]
pub enum Error {
    // ========== Core Errors ==========
    /// Invalid amount (e.g., overflow, underflow).
    #[error("Invalid amount: {0}")]
    InvalidAmount(String),

    /// Invalid public key format.
    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),

    /// Serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(String),

    // ========== DLC Errors ==========
    /// Invalid DLC state transition.
    #[error("Invalid state transition: cannot go from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    /// Contract not found.
    #[error("Contract not found: {0}")]
    ContractNotFound(String),

    /// Contract already exists.
    #[error("Contract already exists: {0}")]
    ContractAlreadyExists(String),

    /// Invalid contract parameters.
    #[error("Invalid contract: {0}")]
    InvalidContract(String),

    /// Oracle error.
    #[error("Oracle error: {0}")]
    Oracle(String),

    /// Insufficient oracles for quorum.
    #[error("Insufficient oracles: need {required}, have {available}")]
    InsufficientOracles { required: usize, available: usize },

    /// Invalid signature.
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    /// Oracle error.
    #[error("Oracle error: {0}")]
    OracleError(String),

    /// Contract expired.
    #[error("Contract expired: {0}")]
    ContractExpired(String),

    // ========== Vault Errors ==========
    /// Insufficient liquidity.
    #[error("Insufficient liquidity: need {required} sats, have {available} sats")]
    InsufficientLiquidity { required: u64, available: u64 },

    /// Deposit too small.
    #[error("Deposit too small: minimum is {minimum} sats, got {actual} sats")]
    DepositTooSmall { minimum: u64, actual: u64 },

    /// Withdrawal exceeds balance.
    #[error("Withdrawal exceeds balance: requested {requested} sats, available {available} sats")]
    WithdrawalExceedsBalance { requested: u64, available: u64 },

    /// Circuit breaker active.
    #[error("Circuit breaker active: {reason}")]
    CircuitBreakerActive { reason: String },

    /// Position not found.
    #[error("Position not found for: {0}")]
    PositionNotFound(String),

    // ========== Signer Errors ==========
    /// Key not found.
    #[error("Key not found: {0}")]
    KeyNotFound(String),

    /// Signing policy violation.
    #[error("Signing policy violation: {0}")]
    PolicyViolation(String),

    /// Rate limit exceeded.
    #[error("Rate limit exceeded: max {limit} per minute, attempted {attempted}")]
    RateLimitExceeded { limit: u32, attempted: u32 },

    /// HSM/keystore error.
    #[error("Keystore error: {0}")]
    Keystore(String),

    // ========== Agent Errors ==========
    /// Sentiment provider error.
    #[error("Sentiment provider error: {0}")]
    SentimentProvider(String),

    /// Risk score out of range.
    #[error("Risk score out of range: {0} (must be 0.0 - 1.0)")]
    InvalidRiskScore(f64),

    /// Channel communication error.
    #[error("Channel error: {0}")]
    Channel(String),

    // ========== Persistence Errors ==========
    /// Database error.
    #[error("Database error: {0}")]
    Database(String),

    /// Data corruption detected.
    #[error("Data corruption: {0}")]
    DataCorruption(String),

    // ========== Economics Errors ==========
    /// Insufficient stake.
    #[error("Insufficient stake: need {required}, have {available}")]
    InsufficientStake { required: u64, available: u64 },

    /// Invalid token operation.
    #[error("Invalid token operation: {0}")]
    InvalidTokenOperation(String),

    // ========== Configuration Errors ==========
    /// Configuration error.
    #[error("Configuration error: {0}")]
    Config(String),

    /// Missing required configuration.
    #[error("Missing configuration: {0}")]
    MissingConfig(String),

    // ========== Network Errors ==========
    /// Blockchain provider error.
    #[error("Blockchain error: {0}")]
    Blockchain(String),

    // ========== External Errors ==========
    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic internal error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// Feature not implemented.
    #[error("Feature not implemented: {0}")]
    FeatureNotImplemented(String),
}

impl Error {
    /// Check if this error is recoverable.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Error::RateLimitExceeded { .. }
                | Error::CircuitBreakerActive { .. }
                | Error::InsufficientLiquidity { .. }
                | Error::Channel(_)
        )
    }

    /// Check if this error indicates a critical failure.
    pub fn is_critical(&self) -> bool {
        matches!(
            self,
            Error::DataCorruption(_)
                | Error::Keystore(_)
                | Error::InvalidSignature(_)
                | Error::PolicyViolation(_)
        )
    }

    /// Get an error code for logging/metrics.
    pub fn code(&self) -> &'static str {
        match self {
            Error::InvalidAmount(_) => "E001",
            Error::InvalidPublicKey(_) => "E002",
            Error::Serialization(_) => "E003",
            Error::InvalidStateTransition { .. } => "E101",
            Error::ContractNotFound(_) => "E102",
            Error::ContractAlreadyExists(_) => "E103",
            Error::InvalidContract(_) => "E104",
            Error::Oracle(_) => "E105",
            Error::InsufficientOracles { .. } => "E106",
            Error::InvalidSignature(_) => "E107",
            Error::ContractExpired(_) => "E108",
            Error::OracleError(_) => "E109",
            Error::InsufficientLiquidity { .. } => "E201",
            Error::DepositTooSmall { .. } => "E202",
            Error::WithdrawalExceedsBalance { .. } => "E203",
            Error::CircuitBreakerActive { .. } => "E204",
            Error::PositionNotFound(_) => "E205",
            Error::KeyNotFound(_) => "E301",
            Error::PolicyViolation(_) => "E302",
            Error::RateLimitExceeded { .. } => "E303",
            Error::Keystore(_) => "E304",
            Error::SentimentProvider(_) => "E401",
            Error::InvalidRiskScore(_) => "E402",
            Error::Channel(_) => "E403",
            Error::Database(_) => "E501",
            Error::DataCorruption(_) => "E502",
            Error::InsufficientStake { .. } => "E601",
            Error::InvalidTokenOperation(_) => "E602",
            Error::Config(_) => "E701",
            Error::MissingConfig(_) => "E702",
            Error::Io(_) => "E801",
            Error::Blockchain(_) => "E802",
            Error::FeatureNotImplemented(_) => "E900",
            Error::Internal(_) => "E999",
        }
    }
}

// Implement conversion from serde_json errors
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialization(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_recoverability() {
        let recoverable = Error::RateLimitExceeded {
            limit: 60,
            attempted: 61,
        };
        assert!(recoverable.is_recoverable());

        let critical = Error::DataCorruption("test".to_string());
        assert!(critical.is_critical());
        assert!(!critical.is_recoverable());
    }

    #[test]
    fn test_error_codes() {
        let err = Error::ContractNotFound("abc".to_string());
        assert_eq!(err.code(), "E102");
    }
}
