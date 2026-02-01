//! Persistent storage for DLC state.
//!
//! Uses `sled` as an embedded database for crash-safe state persistence.
//! All state transitions are written to disk before being confirmed.

use crate::state_machine::DlcState;
use scp_core::{ContractId, Error, Result};
use sled::Db;
use std::path::Path;
use tracing::{debug, error};

/// Storage for DLC contract states.
///
/// Every state is persisted to disk before being confirmed in memory.
/// This ensures that a crash never results in loss of funds.
pub struct DlcStorage {
    db: Db,
}

impl DlcStorage {
    /// Tree name for contract states.
    const STATES_TREE: &'static str = "dlc_states";
    /// Tree name for state index (for listing by state).
    const STATE_INDEX_TREE: &'static str = "dlc_state_index";

    /// Open or create storage at the given path.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = sled::open(path.as_ref().join("dlc.sled"))
            .map_err(|e| Error::Database(e.to_string()))?;

        debug!(path = ?path.as_ref(), "Opened DLC storage");
        Ok(Self { db })
    }

    /// Save a DLC state.
    ///
    /// This writes to disk synchronously before returning.
    pub fn save(&self, state: &DlcState) -> Result<()> {
        let contract_id = state.contract_id();
        let key = contract_id.as_bytes();

        // Serialize the state
        let value = serde_json::to_vec(state).map_err(|e| Error::Serialization(e.to_string()))?;

        // Get the states tree
        let states = self
            .db
            .open_tree(Self::STATES_TREE)
            .map_err(|e| Error::Database(e.to_string()))?;

        // Get the old state for index cleanup
        let old_state: Option<DlcState> = states
            .get(key)
            .map_err(|e| Error::Database(e.to_string()))?
            .and_then(|v| serde_json::from_slice(&v).ok());

        // Insert the new state
        states
            .insert(key, value)
            .map_err(|e| Error::Database(e.to_string()))?;

        // Update the state index
        self.update_index(&contract_id, old_state.as_ref(), Some(state))?;

        // Flush to ensure durability
        self.db
            .flush()
            .map_err(|e| Error::Database(e.to_string()))?;

        debug!(contract_id = %contract_id, state = state.name(), "Saved DLC state");
        Ok(())
    }

    /// Get a DLC state by contract ID.
    pub fn get(&self, contract_id: &ContractId) -> Result<Option<DlcState>> {
        let states = self
            .db
            .open_tree(Self::STATES_TREE)
            .map_err(|e| Error::Database(e.to_string()))?;

        match states.get(contract_id.as_bytes()) {
            Ok(Some(value)) => {
                let state: DlcState = serde_json::from_slice(&value)
                    .map_err(|e| Error::Serialization(e.to_string()))?;
                Ok(Some(state))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(Error::Database(e.to_string())),
        }
    }

    /// Delete a DLC state.
    pub fn delete(&self, contract_id: &ContractId) -> Result<()> {
        let states = self
            .db
            .open_tree(Self::STATES_TREE)
            .map_err(|e| Error::Database(e.to_string()))?;

        // Get the current state for index cleanup
        let old_state: Option<DlcState> = states
            .get(contract_id.as_bytes())
            .map_err(|e| Error::Database(e.to_string()))?
            .and_then(|v| serde_json::from_slice(&v).ok());

        // Remove from states
        states
            .remove(contract_id.as_bytes())
            .map_err(|e| Error::Database(e.to_string()))?;

        // Update index
        if let Some(state) = old_state.as_ref() {
            self.update_index(contract_id, Some(state), None)?;
        }

        self.db
            .flush()
            .map_err(|e| Error::Database(e.to_string()))?;
        Ok(())
    }

    /// List all contracts by state name.
    pub fn list_by_state(&self, state_name: &str) -> Result<Vec<DlcState>> {
        let index = self
            .db
            .open_tree(Self::STATE_INDEX_TREE)
            .map_err(|e| Error::Database(e.to_string()))?;

        let prefix = format!("{}:", state_name);
        let mut results = Vec::new();

        for entry in index.scan_prefix(prefix.as_bytes()) {
            let (key, _) = entry.map_err(|e| Error::Database(e.to_string()))?;

            // Extract contract ID from the key (format: "state_name:contract_id_hex")
            let key_str = String::from_utf8_lossy(&key);
            if let Some(contract_id_hex) = key_str.strip_prefix(&prefix) {
                // Decode hex to bytes
                if let Ok(bytes) = hex_decode(contract_id_hex) {
                    if bytes.len() == 32 {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&bytes);
                        let contract_id = ContractId::from_bytes(arr);
                        if let Ok(Some(state)) = self.get(&contract_id) {
                            results.push(state);
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// List all contracts.
    pub fn list_all(&self) -> Result<Vec<DlcState>> {
        let states = self
            .db
            .open_tree(Self::STATES_TREE)
            .map_err(|e| Error::Database(e.to_string()))?;

        let mut results = Vec::new();
        for entry in states.iter() {
            let (_, value) = entry.map_err(|e| Error::Database(e.to_string()))?;
            match serde_json::from_slice(&value) {
                Ok(state) => results.push(state),
                Err(e) => {
                    error!(error = %e, "Failed to deserialize DLC state");
                }
            }
        }

        Ok(results)
    }

    /// Count contracts by state.
    pub fn count_by_state(&self, state_name: &str) -> Result<usize> {
        let index = self
            .db
            .open_tree(Self::STATE_INDEX_TREE)
            .map_err(|e| Error::Database(e.to_string()))?;

        let prefix = format!("{}:", state_name);
        let count = index.scan_prefix(prefix.as_bytes()).count();
        Ok(count)
    }

    /// Update the state index.
    fn update_index(
        &self,
        contract_id: &ContractId,
        old_state: Option<&DlcState>,
        new_state: Option<&DlcState>,
    ) -> Result<()> {
        let index = self
            .db
            .open_tree(Self::STATE_INDEX_TREE)
            .map_err(|e| Error::Database(e.to_string()))?;

        let contract_id_hex = hex_encode(contract_id.as_bytes());

        // Remove old index entry
        if let Some(old) = old_state {
            let old_key = format!("{}:{}", old.name(), contract_id_hex);
            index
                .remove(old_key.as_bytes())
                .map_err(|e| Error::Database(e.to_string()))?;
        }

        // Add new index entry
        if let Some(new) = new_state {
            let new_key = format!("{}:{}", new.name(), contract_id_hex);
            index
                .insert(new_key.as_bytes(), &[1u8])
                .map_err(|e| Error::Database(e.to_string()))?;
        }

        Ok(())
    }

    /// Compact the database (for maintenance).
    pub fn compact(&self) -> Result<()> {
        // Sled auto-compacts, but we can trigger a flush
        self.db
            .flush()
            .map(|_| ())
            .map_err(|e| Error::Database(e.to_string()))
    }
}

/// Simple hex encoding.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Simple hex decoding.
fn hex_decode(s: &str) -> std::result::Result<Vec<u8>, ()> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::ContractOffer;
    use crate::payout::PayoutCurve;
    use scp_core::{OracleInfo, PublicKey, Satoshi, Timestamp};
    use tempfile::tempdir;

    fn mock_offer() -> ContractOffer {
        let secp = secp256k1::Secp256k1::new();
        let (_, pk) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());

        ContractOffer {
            contract_id: ContractId::from_data(b"test_storage"),
            offerer: PublicKey::new(pk),
            collateral: Satoshi::from_sat(1_000_000),
            payout_curve: PayoutCurve::Binary {
                win_amount: Satoshi::from_sat(2_000_000),
                lose_amount: Satoshi::ZERO,
            },
            oracle_info: OracleInfo {
                public_key: PublicKey::new(pk),
                name: "Test".to_string(),
                endpoint: None,
            },
            event_descriptor: "Test event".to_string(),
            maturity: Timestamp::from_unix(u64::MAX),
        }
    }

    #[test]
    fn test_save_and_get() {
        let dir = tempdir().unwrap();
        let storage = DlcStorage::new(dir.path()).unwrap();

        let offer = mock_offer();
        let state = DlcState::Offered {
            offer: offer.clone(),
            created_at: Timestamp::now(),
        };

        storage.save(&state).unwrap();

        let retrieved = storage.get(&offer.contract_id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "Offered");
    }

    #[test]
    fn test_list_by_state() {
        let dir = tempdir().unwrap();
        let storage = DlcStorage::new(dir.path()).unwrap();

        // Create multiple contracts
        for i in 0..3 {
            let mut offer = mock_offer();
            offer.contract_id = ContractId::from_data(format!("contract_{}", i).as_bytes());
            let state = DlcState::Offered {
                offer,
                created_at: Timestamp::now(),
            };
            storage.save(&state).unwrap();
        }

        let offered = storage.list_by_state("Offered").unwrap();
        assert_eq!(offered.len(), 3);
    }

    #[test]
    fn test_delete() {
        let dir = tempdir().unwrap();
        let storage = DlcStorage::new(dir.path()).unwrap();

        let offer = mock_offer();
        let state = DlcState::Offered {
            offer: offer.clone(),
            created_at: Timestamp::now(),
        };

        storage.save(&state).unwrap();
        assert!(storage.get(&offer.contract_id).unwrap().is_some());

        storage.delete(&offer.contract_id).unwrap();
        assert!(storage.get(&offer.contract_id).unwrap().is_none());
    }
}
