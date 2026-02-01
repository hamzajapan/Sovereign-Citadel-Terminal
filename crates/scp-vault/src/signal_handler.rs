//! Async signal handler for Agent-Vault communication.
//!
//! Receives signals from `scp-agent` and applies them to the vault.
//! Runs in its own Tokio task, never blocking vault operations.

use crate::pool::LiquidityPool;
use scp_core::{AgentSignal, VaultEvent};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Handles signals from the AI Agent.
pub struct SignalHandler {
    pool: Arc<LiquidityPool>,
    /// Receiver for agent signals.
    signal_rx: mpsc::Receiver<AgentSignal>,
    /// Sender for vault events.
    event_tx: mpsc::Sender<VaultEvent>,
}

impl SignalHandler {
    /// Create a new signal handler.
    pub fn new(
        pool: Arc<LiquidityPool>,
        signal_rx: mpsc::Receiver<AgentSignal>,
        event_tx: mpsc::Sender<VaultEvent>,
    ) -> Self {
        Self {
            pool,
            signal_rx,
            event_tx,
        }
    }

    /// Run the signal handler loop.
    ///
    /// This should be spawned as a separate Tokio task.
    pub async fn run(mut self) {
        info!("Signal handler started");

        while let Some(signal) = self.signal_rx.recv().await {
            debug!(signal = ?signal, "Received agent signal");
            self.handle_signal(signal).await;
        }

        info!("Signal handler stopped (channel closed)");
    }

    /// Handle a single signal.
    async fn handle_signal(&self, signal: AgentSignal) {
        match signal {
            AgentSignal::WidenSpread { factor } => {
                self.pool.set_spread_multiplier(factor);
                info!(factor = factor, "Spread widened");
            }

            AgentSignal::NarrowSpread { target } => {
                // Calculate multiplier to achieve target
                let base_spread = 0.02; // Should come from config
                let multiplier = target / base_spread;
                self.pool.set_spread_multiplier(multiplier.max(1.0));
                info!(target = target, "Spread narrowed");
            }

            AgentSignal::CircuitBreaker {
                reason,
                duration_secs,
            } => {
                self.pool.activate_circuit_breaker(&reason);
                warn!(reason = %reason, duration = ?duration_secs, "Circuit breaker activated");

                // If duration specified, schedule deactivation
                if let Some(duration) = duration_secs {
                    let pool = self.pool.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(duration)).await;
                        pool.deactivate_circuit_breaker();
                        info!("Circuit breaker auto-deactivated");
                    });
                }
            }

            AgentSignal::Resume => {
                self.pool.deactivate_circuit_breaker();
                info!("Operations resumed");
            }

            AgentSignal::UpdateRiskScore {
                pubkey,
                score,
                reason,
            } => {
                // In a real implementation, this would update a risk database
                info!(
                    pubkey = %pubkey,
                    score = score,
                    reason = %reason,
                    "Risk score updated"
                );
            }

            AgentSignal::RebalanceHedge { delta_adjustment } => {
                // In a real implementation, this would trigger rebalancing
                info!(delta = delta_adjustment, "Hedge rebalance requested");
            }

            AgentSignal::BlockCounterparty { pubkey, reason } => {
                warn!(
                    pubkey = %pubkey,
                    reason = %reason,
                    "Counterparty blocked"
                );
            }
        }
    }

    /// Send a vault event to the agent.
    pub async fn emit_event(&self, event: VaultEvent) {
        if let Err(e) = self.event_tx.send(event).await {
            warn!(error = %e, "Failed to send vault event");
        }
    }
}

/// Builder for creating the signal handler with channels.
pub struct SignalHandlerBuilder {
    buffer_size: usize,
}

impl SignalHandlerBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self { buffer_size: 256 }
    }

    /// Set the channel buffer size.
    pub fn buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Build the signal handler and return the channels.
    ///
    /// Returns:
    /// - The signal handler
    /// - Sender for agent to send signals
    /// - Receiver for agent to receive events
    pub fn build(
        self,
        pool: Arc<LiquidityPool>,
    ) -> (
        SignalHandler,
        mpsc::Sender<AgentSignal>,
        mpsc::Receiver<VaultEvent>,
    ) {
        let (signal_tx, signal_rx) = mpsc::channel(self.buffer_size);
        let (event_tx, event_rx) = mpsc::channel(self.buffer_size);

        let handler = SignalHandler::new(pool, signal_rx, event_tx);

        (handler, signal_tx, event_rx)
    }
}

impl Default for SignalHandlerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::PoolConfig;

    #[tokio::test]
    async fn test_widen_spread_signal() {
        let pool = Arc::new(LiquidityPool::new(PoolConfig::default()));
        let (handler, signal_tx, _event_rx) = SignalHandlerBuilder::new().build(pool.clone());

        // Spawn the handler
        let handle = tokio::spawn(handler.run());

        // Send a signal
        signal_tx
            .send(AgentSignal::WidenSpread { factor: 2.0 })
            .await
            .unwrap();

        // Give it time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Check that spread was widened
        assert!(pool.current_spread() > 0.02);

        // Close channel to stop handler
        drop(signal_tx);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn test_circuit_breaker_signal() {
        let pool = Arc::new(LiquidityPool::new(PoolConfig::default()));
        let (handler, signal_tx, _event_rx) = SignalHandlerBuilder::new().build(pool.clone());

        let handle = tokio::spawn(handler.run());

        signal_tx
            .send(AgentSignal::CircuitBreaker {
                reason: "test".to_string(),
                duration_secs: None,
            })
            .await
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        assert!(pool.is_circuit_breaker_active());

        // Resume
        signal_tx.send(AgentSignal::Resume).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        assert!(!pool.is_circuit_breaker_active());

        drop(signal_tx);
        let _ = handle.await;
    }
}
