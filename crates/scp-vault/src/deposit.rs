//! Deposit and withdrawal request handling.

use scp_core::{ContractId, PublicKey, Satoshi, Timestamp};
use serde::{Deserialize, Serialize};

/// A deposit request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositRequest {
    /// Request ID.
    pub id: String,
    /// The depositor.
    pub depositor: PublicKey,
    /// Amount to deposit.
    pub amount: Satoshi,
    /// Whether to auto-hedge (delta-neutral).
    pub auto_hedge: bool,
    /// Timestamp of the request.
    pub created_at: Timestamp,
    /// Status.
    pub status: DepositStatus,
}

/// Deposit status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DepositStatus {
    /// Awaiting DLC creation.
    Pending,
    /// DLC created, awaiting funding.
    DlcCreated { contract_id: ContractId },
    /// Funded and confirmed.
    Confirmed,
    /// Failed.
    Failed { reason: String },
}

impl DepositRequest {
    /// Create a new deposit request.
    pub fn new(depositor: PublicKey, amount: Satoshi, auto_hedge: bool) -> Self {
        Self {
            id: Self::generate_id(),
            depositor,
            amount,
            auto_hedge,
            created_at: Timestamp::now(),
            status: DepositStatus::Pending,
        }
    }

    fn generate_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("dep_{:x}", ts)
    }

    /// Update status to DLC created.
    pub fn set_dlc_created(&mut self, contract_id: ContractId) {
        self.status = DepositStatus::DlcCreated { contract_id };
    }

    /// Mark as confirmed.
    pub fn confirm(&mut self) {
        self.status = DepositStatus::Confirmed;
    }

    /// Mark as failed.
    pub fn fail(&mut self, reason: String) {
        self.status = DepositStatus::Failed { reason };
    }
}

/// A withdrawal request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalRequest {
    /// Request ID.
    pub id: String,
    /// The withdrawer.
    pub withdrawer: PublicKey,
    /// Shares to burn.
    pub shares: u64,
    /// Estimated amount (at request time).
    pub estimated_amount: Satoshi,
    /// Timestamp of the request.
    pub created_at: Timestamp,
    /// Block height when withdrawal becomes valid.
    pub valid_at_block: u32,
    /// Status.
    pub status: WithdrawalStatus,
}

/// Withdrawal status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WithdrawalStatus {
    /// Waiting for delay period.
    Pending,
    /// Ready to execute.
    Ready,
    /// Executed successfully.
    Completed { actual_amount: Satoshi },
    /// Failed.
    Failed { reason: String },
    /// Cancelled by user.
    Cancelled,
}

impl WithdrawalRequest {
    /// Create a new withdrawal request.
    pub fn new(
        withdrawer: PublicKey,
        shares: u64,
        estimated_amount: Satoshi,
        delay_blocks: u32,
        current_block: u32,
    ) -> Self {
        Self {
            id: Self::generate_id(),
            withdrawer,
            shares,
            estimated_amount,
            created_at: Timestamp::now(),
            valid_at_block: current_block + delay_blocks,
            status: WithdrawalStatus::Pending,
        }
    }

    fn generate_id() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("wd_{:x}", ts)
    }

    /// Check if the withdrawal is ready.
    pub fn is_ready(&self, current_block: u32) -> bool {
        current_block >= self.valid_at_block
            && matches!(
                self.status,
                WithdrawalStatus::Pending | WithdrawalStatus::Ready
            )
    }

    /// Mark as ready.
    pub fn set_ready(&mut self) {
        self.status = WithdrawalStatus::Ready;
    }

    /// Complete the withdrawal.
    pub fn complete(&mut self, actual_amount: Satoshi) {
        self.status = WithdrawalStatus::Completed { actual_amount };
    }

    /// Fail the withdrawal.
    pub fn fail(&mut self, reason: String) {
        self.status = WithdrawalStatus::Failed { reason };
    }

    /// Cancel the withdrawal.
    pub fn cancel(&mut self) {
        self.status = WithdrawalStatus::Cancelled;
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
    fn test_deposit_lifecycle() {
        let mut request = DepositRequest::new(mock_pubkey(), Satoshi::from_sat(1_000_000), true);

        assert!(matches!(request.status, DepositStatus::Pending));

        let contract_id = ContractId::from_data(b"test");
        request.set_dlc_created(contract_id);
        assert!(matches!(request.status, DepositStatus::DlcCreated { .. }));

        request.confirm();
        assert!(matches!(request.status, DepositStatus::Confirmed));
    }

    #[test]
    fn test_withdrawal_delay() {
        let request = WithdrawalRequest::new(
            mock_pubkey(),
            100,
            Satoshi::from_sat(1_000_000),
            6,   // 6 block delay
            100, // current block
        );

        assert!(!request.is_ready(100)); // Not ready yet
        assert!(!request.is_ready(105)); // Still not ready
        assert!(request.is_ready(106)); // Now ready
    }
}
