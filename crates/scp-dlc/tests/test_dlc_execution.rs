use scp_dlc::oracle::MockOracle;
use scp_dlc::state_machine::DlcStateMachine;
use scp_dlc::storage::DlcStorage;
use scp_dlc::DlcManager;
// use scp_chain::mock::MockBlockchain; // scp-chain mock module needs to be public?
use scp_chain;
use scp_chain::mock::MockBlockchain; // usage to be fixed if needed, or remove line if purely unused
                                     // use scp_chain::BlockchainProvider;
use scp_core::types::ContractId;
use scp_signer::{keystore::MemoryKeystore, policy::SigningPolicy, Signer};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_dlc_manager_initialization() {
    // 1. Setup Storage
    let dir = tempdir().unwrap();
    let storage = Arc::new(DlcStorage::new(dir.path()).unwrap());
    let sm = Arc::new(DlcStateMachine::new(storage));

    // 2. Setup Mocks
    let chain = Arc::new(MockBlockchain::new());
    let oracle = Arc::new(MockOracle::new());
    let keystore = Arc::new(MemoryKeystore::new());
    let policy = SigningPolicy::default_policy();
    let signer = Arc::new(Signer::new(keystore, policy));

    // 3. Init Manager
    let manager = DlcManager::new(chain, oracle, signer, sm);

    // 4. Verify wiring
    // Attempt funding broadcast on random ID (should fail)
    let random_id = ContractId::from_bytes([0u8; 32]);
    let result = manager.broadcast_funding(&random_id).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    println!("Got expected error: {}", err);
}
