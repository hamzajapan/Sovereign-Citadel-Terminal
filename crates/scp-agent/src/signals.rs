//! Signal emission to the Vault.

use scp_core::{AgentSignal, PublicKey, VaultEvent};
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Emits signals to the Vault and receives events.
pub struct SignalEmitter {
    /// Sender for signals to Vault.
    signal_tx: mpsc::Sender<AgentSignal>,
    /// Receiver for events from Vault.
    event_rx: mpsc::Receiver<VaultEvent>,
}

impl SignalEmitter {
    /// Create a new signal emitter.
    pub fn new(signal_tx: mpsc::Sender<AgentSignal>, event_rx: mpsc::Receiver<VaultEvent>) -> Self {
        Self {
            signal_tx,
            event_rx,
        }
    }

    /// Send a signal to the vault.
    pub async fn send(&self, signal: AgentSignal) -> Result<(), String> {
        debug!(signal = ?signal, "Sending signal to vault");
        self.signal_tx
            .send(signal)
            .await
            .map_err(|e| format!("Failed to send signal: {}", e))
    }

    /// Receive an event from the vault.
    pub async fn recv(&mut self) -> Option<VaultEvent> {
        self.event_rx.recv().await
    }

    /// Try to receive an event without blocking.
    pub fn try_recv(&mut self) -> Option<VaultEvent> {
        self.event_rx.try_recv().ok()
    }

    // === Convenience methods for common signals ===

    /// Widen the spread.
    pub async fn widen_spread(&self, factor: f64) -> Result<(), String> {
        self.send(AgentSignal::WidenSpread { factor }).await
    }

    /// Narrow the spread.
    pub async fn narrow_spread(&self, target: f64) -> Result<(), String> {
        self.send(AgentSignal::NarrowSpread { target }).await
    }

    /// Activate circuit breaker.
    pub async fn circuit_break(
        &self,
        reason: &str,
        duration_secs: Option<u64>,
    ) -> Result<(), String> {
        self.send(AgentSignal::CircuitBreaker {
            reason: reason.to_string(),
            duration_secs,
        })
        .await
    }

    /// Resume operations.
    pub async fn resume(&self) -> Result<(), String> {
        self.send(AgentSignal::Resume).await
    }

    /// Update a counterparty's risk score.
    pub async fn update_risk(
        &self,
        pubkey: PublicKey,
        score: f64,
        reason: &str,
    ) -> Result<(), String> {
        self.send(AgentSignal::UpdateRiskScore {
            pubkey,
            score,
            reason: reason.to_string(),
        })
        .await
    }

    /// Block a counterparty.
    pub async fn block_counterparty(&self, pubkey: PublicKey, reason: &str) -> Result<(), String> {
        warn!(pubkey = %pubkey, reason = %reason, "Blocking counterparty");
        self.send(AgentSignal::BlockCounterparty {
            pubkey,
            reason: reason.to_string(),
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_signal_emitter() {
        let (signal_tx, mut signal_rx) = mpsc::channel(16);
        let (event_tx, event_rx) = mpsc::channel(16);

        let emitter = SignalEmitter::new(signal_tx, event_rx);

        // Send a signal
        emitter.widen_spread(2.0).await.unwrap();

        // Receive it on the other end
        let signal = signal_rx.recv().await.unwrap();
        assert!(matches!(signal, AgentSignal::WidenSpread { factor } if factor == 2.0));

        // Drop to prevent blocking
        drop(event_tx);
    }
}
