use crate::BlockchainProvider;
use async_trait::async_trait;
use bitcoin::{consensus::encode::serialize_hex, ScriptBuf, Transaction, Txid};
use scp_core::{Error, Result};
use std::str::FromStr;

pub struct EsploraClient {
    client: reqwest::Client,
    base_url: String,
}

impl EsploraClient {
    pub fn new(base_url: &str) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }
}

#[derive(serde::Deserialize)]
struct EsploraTxStatus {
    confirmed: bool,
    block_height: Option<u32>,
}

#[async_trait]
impl BlockchainProvider for EsploraClient {
    async fn broadcast_transaction(&self, tx: &Transaction) -> Result<Txid> {
        let hex_tx = serialize_hex(tx);
        let url = format!("{}/tx", self.base_url);
        
        let resp = self.client.post(&url)
            .body(hex_tx)
            .send()
            .await
            .map_err(|e| Error::Blockchain(format!("Request failed: {}", e)))?;
            
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Blockchain(format!("Broadcast failed: {}", text)));
        }

        let txid_str = resp.text().await
            .map_err(|e| Error::Blockchain(format!("Failed to read txid: {}", e)))?;
            
        Ok(tx.compute_txid())
    }

    async fn get_transaction_depth(&self, txid: &Txid) -> Result<Option<u32>> {
        let url = format!("{}/tx/{}/status", self.base_url, txid);
        let resp = self.client.get(&url)
            .send()
            .await
            .map_err(|e| Error::Blockchain(format!("Request failed: {}", e)))?;

        if !resp.status().is_success() {
             return Err(Error::Blockchain(format!("Status check failed: {}", resp.status())));
        }

        let status: EsploraTxStatus = resp.json().await
            .map_err(|e| Error::Blockchain(format!("Failed to parse status: {}", e)))?;

        if let Some(height) = status.block_height {
            let tip = self.get_height().await?;
            Ok(Some(tip - height + 1))
        } else {
            Ok(None)
        }
    }

    async fn watch_script(&self, _script: &ScriptBuf) -> Result<()> {
        Ok(())
    }

    async fn get_height(&self) -> Result<u32> {
        let url = format!("{}/blocks/tip/height", self.base_url);
        let resp = self.client.get(&url)
            .send()
            .await
            .map_err(|e| Error::Blockchain(format!("Request failed: {}", e)))?; // Io error

        let text = resp.text().await
            .map_err(|e| Error::Blockchain(format!("Failed to read height: {}", e)))?;
            
        text.parse::<u32>()
            .map_err(|e| Error::Blockchain(format!("Invalid height: {}", e)))
    }

    async fn get_transaction(&self, txid: &Txid) -> Result<Option<Transaction>> {
        let url = format!("{}/tx/{}/hex", self.base_url, txid);
        let resp = self.client.get(&url)
            .send()
            .await
            .map_err(|e| Error::Blockchain(format!("Request failed: {}", e)))?;

        if resp.status() == 404 {
            return Ok(None);
        }

        if !resp.status().is_success() {
             return Err(Error::Blockchain(format!("Get TX failed: {}", resp.status())));
        }

        let hex = resp.text().await
            .map_err(|e| Error::Blockchain(format!("Failed to read hex: {}", e)))?;
            
        let bytes = hex::decode(hex)
             .map_err(|e| Error::Blockchain(format!("Invalid hex: {}", e)))?;
             
        let tx: Transaction = bitcoin::consensus::deserialize(&bytes)
             .map_err(|e| Error::Blockchain(format!("Invalid tx bytes: {}", e)))?;
             
        Ok(Some(tx))
    }
}
