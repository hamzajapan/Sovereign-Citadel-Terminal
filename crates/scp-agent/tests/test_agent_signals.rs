use scp_agent::sentiment::{MockSentimentProvider, SentimentProvider, SentimentScore};
use scp_agent::sentinel::{AgentConfig, CitadelAgent};
use scp_core::channels::{channel, AgentSignal, VaultEvent};
use std::sync::Arc;
use tokio::time::Duration;

#[tokio::test]
async fn test_agent_emits_signals_on_fear() {
    // 1. Setup Channels
    let ((agent_tx, mut vault_rx), (vault_tx, agent_rx)) = channel::create_default_channels();

    // 2. Setup Agent with Shared Mock Provider
    let config = AgentConfig {
        poll_interval_secs: 1, // Fast poll
        ..Default::default()
    };

    // Create provider we can control
    let provider = Arc::new(MockSentimentProvider::new(-0.8)); // Start with FEAR

    // Inject into Agent
    let mut agent = CitadelAgent::new_with_provider(config, provider.clone());
    agent.attach_channels(agent_tx, agent_rx);

    // 3. Spawn Agent
    tokio::spawn(async move {
        agent.run().await;
    });

    // 4. Expect WidenSpread Signal because sentiment is -0.8 (Fear)
    // -0.8 < -0.5 => Multiplier = 2.0
    // Factor 2.0 > 1.0 + 0.01 => Emit

    // We might need to wait/retry reading if channel is slow,
    // but at start it should emit immediately on first loop.

    // Use timeout to prevent hanging
    let signal = tokio::time::timeout(Duration::from_secs(2), vault_rx.recv()).await;

    assert!(signal.is_ok(), "Timed out waiting for signal");
    let event = signal.unwrap();
    assert!(event.is_some(), "Channel closed");

    match event.unwrap() {
        AgentSignal::WidenSpread { factor } => {
            println!("Received WidenSpread: {}", factor);
            assert!(factor >= 1.9, "Expected factor around 2.0");
        }
        s => panic!("Expected WidenSpread signal, got {:?}", s),
    }
}
