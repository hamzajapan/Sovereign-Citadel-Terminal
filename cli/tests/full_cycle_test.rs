use scp_agent::sentiment::MockSentimentProvider;
use scp_agent::sentinel::{AgentConfig, CitadelAgent};
use scp_chain::mock::MockBlockchain;
use scp_core::Satoshi;
use scp_dlc::manager::DlcManager;
use scp_dlc::oracle::MockOracle;
use scp_dlc::state_machine::DlcStateMachine;
use scp_dlc::storage::DlcStorage;
use scp_signer::keystore::MemoryKeystore;
use scp_signer::policy::SigningPolicy;
use scp_signer::signer::Signer;
use scp_vault::LiquidityVault;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::Duration;

#[tokio::test]
async fn test_the_sovereign_citadel_lifecycle() {
    // 0. Logging
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    tracing::info!("--- STARTING GOD TEST ---");

    // 1. Setup Infrastructure
    let chain = Arc::new(MockBlockchain::new());
    let oracle = Arc::new(MockOracle::new());

    // Keystore & Signer
    let keystore = Arc::new(MemoryKeystore::new());
    let policy = SigningPolicy::default_policy();
    let signer = Arc::new(Signer::new(keystore, policy));

    // DLC Storage (Temp Dir)
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(DlcStorage::new(temp_dir.path().to_path_buf()).unwrap());

    let sm = Arc::new(DlcStateMachine::new(storage));

    // 2. Setup Agent (The Brain)
    let ((agent_tx, vault_rx), (vault_tx, agent_rx)) =
        scp_core::channels::channel::create_default_channels();

    let agent_config = AgentConfig {
        poll_interval_secs: 1,
        ..Default::default()
    };

    // Inject Sentiment Provider to control "Fear"
    let sentiment_provider = Arc::new(MockSentimentProvider::new(0.0)); // Start Neutral
    let mut agent = CitadelAgent::new_with_provider(agent_config, sentiment_provider.clone());
    agent.attach_channels(agent_tx, agent_rx);

    // 3. Setup Vault (The Body)
    // LiquidityVault gets access to Manager.
    // Manager needs chain, oracle, signer, state_machine
    let dlc_manager = DlcManager::new(chain.clone(), oracle.clone(), signer.clone(), sm.clone());

    let (vault, handler) = LiquidityVault::new(dlc_manager, vault_rx, vault_tx);

    // 4. Start Engines
    tokio::spawn(async move {
        agent.run().await;
    });

    tokio::spawn(async move {
        handler.run().await;
    });

    // --- Scenario 1: The Risk Signal ---
    // Wait for initial stabilization
    tokio::time::sleep(Duration::from_millis(500)).await;

    tracing::info!(">>> Injecting FEAR sentiment...");
    sentiment_provider.set_sentiment(-0.6); // High Fear (not Extreme)

    // Poll loop to wait for spread change
    let pool = vault.get_pool();
    let mut spread_widened = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let s = pool.current_spread();
        // Base spread 0.02. Multiplier 2.0 -> 0.04.
        if s > 0.03 {
            tracing::info!("Spread widened to: {}", s);
            spread_widened = true;
            break;
        }
    }
    assert!(
        spread_widened,
        "Vault did not widen spread upon Fear signal"
    );

    // --- Scenario 2: The Trade ---
    // Generate User Key Logic
    let key_info = signer.generate_key(None).expect("Failed to generate key");
    let depositor = key_info.public_key;

    tracing::info!(">>> User Depositing Liquidity...");
    vault
        .process_deposit(depositor, Satoshi::from_btc(1.0))
        .await
        .unwrap();

    assert_eq!(pool.available_liquidity(), Satoshi::from_btc(1.0));

    // User Opens Position (Trade)
    tracing::info!(">>> User Opening Position...");
    // 0.1 BTC collateral
    let _contract_id = vault
        .open_position(depositor, Satoshi::from_btc(0.1), &[])
        .await
        .unwrap();

    // Verify Liquidity Locked
    // Locked should be 0.1
    // Available should be 0.9
    assert_eq!(pool.utilization(), 0.1);
    assert_eq!(pool.available_liquidity(), Satoshi::from_btc(0.9));

    tracing::info!(">>> GOD TEST SUCCESSFUL <<<");
}
