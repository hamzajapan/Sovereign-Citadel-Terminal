//! Blockchain abstraction layer.
//!
//! Provides traits for interacting with the Bitcoin network.

use async_trait::async_trait;
use bitcoin::{ScriptBuf, Transaction, Txid};
use scp_core::Result;

pub mod mock;

/// Trait for blockchain operations.
#[async_trait]
pub trait BlockchainProvider: Send + Sync {
    /// Broadcast a transaction.
    async fn broadcast_transaction(&self, tx: &Transaction) -> Result<Txid>;

    /// Get transaction depth (confirmations).
    async fn get_transaction_depth(&self, txid: &Txid) -> Result<Option<u32>>;

    /// Watch a script for transactions.
    async fn watch_script(&self, script: &ScriptBuf) -> Result<()>;

    /// Get current block height.
    async fn get_height(&self) -> Result<u32>;

    /// Get transaction.
    async fn get_transaction(&self, txid: &Txid) -> Result<Option<Transaction>>;
}
