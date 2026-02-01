//! Mock blockchain provider for testing.

use crate::BlockchainProvider;
use async_trait::async_trait;
use bitcoin::{ScriptBuf, Transaction, Txid};
use scp_core::Result;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// A mock blockchain.
#[derive(Debug, Default)]
pub struct MockBlockchain {
    /// Broadcasted transactions.
    pub mempool: Arc<RwLock<HashMap<Txid, Transaction>>>,
    /// Confirmed transactions (txid -> height).
    pub confirmed: Arc<RwLock<HashMap<Txid, u32>>>,
    /// Watched scripts.
    pub watched: Arc<RwLock<Vec<ScriptBuf>>>,
    /// Current height.
    pub height: Arc<RwLock<u32>>,
}

impl MockBlockchain {
    /// Create a new mock blockchain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mine a block (increment height).
    pub fn mine_block(&self) {
        let mut h = self.height.write().unwrap();
        *h += 1;
    }

    /// Confirm a transaction at current height.
    pub fn confirm_transaction(&self, txid: &Txid) {
        let h = *self.height.read().unwrap();
        self.confirmed.write().unwrap().insert(*txid, h);
    }
}

#[async_trait]
impl BlockchainProvider for MockBlockchain {
    async fn broadcast_transaction(&self, tx: &Transaction) -> Result<Txid> {
        let txid = tx.compute_txid();
        tracing::info!("Broadcasting tx: {}", txid);
        self.mempool.write().unwrap().insert(txid, tx.clone());
        Ok(txid)
    }

    async fn get_transaction_depth(&self, txid: &Txid) -> Result<Option<u32>> {
        let height = *self.height.read().unwrap();
        let confirmed = self.confirmed.read().unwrap();

        if let Some(conf_height) = confirmed.get(txid) {
            Ok(Some(height - conf_height + 1))
        } else {
            Ok(None)
        }
    }

    async fn watch_script(&self, script: &ScriptBuf) -> Result<()> {
        self.watched.write().unwrap().push(script.clone());
        Ok(())
    }

    async fn get_height(&self) -> Result<u32> {
        Ok(*self.height.read().unwrap())
    }

    async fn get_transaction(&self, txid: &Txid) -> Result<Option<Transaction>> {
        let mempool = self.mempool.read().unwrap();
        // In a real mock, we might separate mempool and chain storage,
        // but for now retrieve from mempool
        Ok(mempool.get(txid).cloned())
    }
}
