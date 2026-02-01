//! Risk scoring and reputation management.

use scp_core::PublicKey;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

/// A reputation score for a counterparty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationScore {
    /// The counterparty.
    pub pubkey: PublicKey,
    /// Overall score (0.0 = highest risk, 1.0 = lowest risk).
    pub score: f64,
    /// Number of successful contracts.
    pub successful_contracts: u32,
    /// Number of failed/disputed contracts.
    pub failed_contracts: u32,
    /// Total volume transacted.
    pub total_volume_sats: u64,
    /// Last activity timestamp.
    pub last_activity: u64,
    /// Flags/notes.
    pub flags: Vec<String>,
}

impl ReputationScore {
    /// Create a new reputation for an unknown counterparty.
    pub fn new(pubkey: PublicKey) -> Self {
        Self {
            pubkey,
            score: 0.5, // Neutral starting score
            successful_contracts: 0,
            failed_contracts: 0,
            total_volume_sats: 0,
            last_activity: 0,
            flags: Vec::new(),
        }
    }

    /// Record a successful contract.
    pub fn record_success(&mut self, volume_sats: u64) {
        self.successful_contracts += 1;
        self.total_volume_sats += volume_sats;
        self.last_activity = now();
        self.recalculate_score();
    }

    /// Record a failed contract.
    pub fn record_failure(&mut self, reason: &str) {
        self.failed_contracts += 1;
        self.last_activity = now();
        self.flags.push(format!("Failed: {}", reason));
        self.recalculate_score();
    }

    /// Add a flag/note.
    pub fn add_flag(&mut self, flag: String) {
        self.flags.push(flag);
    }

    /// Recalculate the score based on history.
    fn recalculate_score(&mut self) {
        let total = self.successful_contracts + self.failed_contracts;
        if total == 0 {
            self.score = 0.5;
            return;
        }

        // Base score from success rate
        let success_rate = self.successful_contracts as f64 / total as f64;

        // Boost for volume (logarithmic)
        let volume_factor = if self.total_volume_sats > 0 {
            (self.total_volume_sats as f64).log10() / 10.0 // Normalize
        } else {
            0.0
        };

        // Penalty for flags
        let flag_penalty = (self.flags.len() as f64 * 0.05).min(0.3);

        self.score = (success_rate * 0.7 + volume_factor.min(0.2) - flag_penalty).clamp(0.0, 1.0);
    }

    /// Check if this counterparty is high risk.
    pub fn is_high_risk(&self) -> bool {
        self.score < 0.3 || self.failed_contracts > self.successful_contracts
    }

    /// Check if this counterparty is trusted.
    pub fn is_trusted(&self) -> bool {
        self.score > 0.7 && self.successful_contracts >= 3
    }
}

/// Detector for toxic flow (informed traders).
pub struct ToxicFlowDetector {
    /// Recent trades by counterparty for pattern detection.
    recent_trades: RwLock<HashMap<String, Vec<TradeRecord>>>,
    /// Threshold for marking as toxic.
    toxicity_threshold: f64,
}

/// A record of a trade for analysis.
#[derive(Debug, Clone)]
struct TradeRecord {
    /// Trade direction (true = long, false = short).
    _is_long: bool,
    /// Outcome (profit in sats, can be negative).
    outcome: i64,
    /// Timestamp.
    timestamp: u64,
}

impl ToxicFlowDetector {
    /// Create a new detector.
    pub fn new(toxicity_threshold: f64) -> Self {
        Self {
            recent_trades: RwLock::new(HashMap::new()),
            toxicity_threshold,
        }
    }

    /// Record a trade.
    pub fn record_trade(&self, pubkey: &PublicKey, is_long: bool, outcome: i64) {
        let key = pubkey.to_string();
        let record = TradeRecord {
            _is_long: is_long,
            outcome,
            timestamp: now(),
        };

        let mut trades = self.recent_trades.write().unwrap();
        trades.entry(key).or_default().push(record);
    }

    /// Analyze a counterparty for toxic flow patterns.
    pub fn analyze(&self, pubkey: &PublicKey) -> ToxicFlowAnalysis {
        let key = pubkey.to_string();
        let trades = self.recent_trades.read().unwrap();

        let records = match trades.get(&key) {
            Some(r) if r.len() >= 5 => r,
            _ => {
                return ToxicFlowAnalysis {
                    is_toxic: false,
                    toxicity_score: 0.0,
                    win_rate: 0.5,
                    reason: None,
                }
            }
        };

        // Calculate win rate
        let wins = records.iter().filter(|r| r.outcome > 0).count();
        let win_rate = wins as f64 / records.len() as f64;

        // Toxic if win rate is suspiciously high
        let is_toxic = win_rate > self.toxicity_threshold;
        let toxicity_score = if win_rate > 0.5 {
            (win_rate - 0.5) * 2.0 // Normalize to 0-1
        } else {
            0.0
        };

        let reason = if is_toxic {
            Some(format!(
                "Win rate {:.1}% exceeds threshold",
                win_rate * 100.0
            ))
        } else {
            None
        };

        ToxicFlowAnalysis {
            is_toxic,
            toxicity_score,
            win_rate,
            reason,
        }
    }

    /// Clear old records.
    pub fn prune(&self, max_age_secs: u64) {
        let cutoff = now().saturating_sub(max_age_secs);
        let mut trades = self.recent_trades.write().unwrap();

        for records in trades.values_mut() {
            records.retain(|r| r.timestamp > cutoff);
        }

        // Remove empty entries
        trades.retain(|_, v| !v.is_empty());
    }
}

/// Result of toxic flow analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToxicFlowAnalysis {
    /// Whether the counterparty appears to be toxic.
    pub is_toxic: bool,
    /// Toxicity score (0.0 - 1.0).
    pub toxicity_score: f64,
    /// Observed win rate.
    pub win_rate: f64,
    /// Reason for toxicity classification.
    pub reason: Option<String>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
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
    fn test_reputation_scoring() {
        let mut rep = ReputationScore::new(mock_pubkey());
        assert_eq!(rep.score, 0.5);

        // Record successes
        for _ in 0..5 {
            rep.record_success(1_000_000);
        }
        assert!(rep.score > 0.5);

        // Record a failure
        rep.record_failure("timeout");
        assert!(rep.score < 1.0);
    }

    #[test]
    fn test_toxic_flow_detection() {
        let detector = ToxicFlowDetector::new(0.75);
        let pk = mock_pubkey();

        // Record a series of wins
        for _ in 0..10 {
            detector.record_trade(&pk, true, 100_000);
        }

        let analysis = detector.analyze(&pk);
        assert!(analysis.is_toxic);
        assert!(analysis.win_rate > 0.9);
    }

    #[test]
    fn test_not_toxic_with_mixed_results() {
        let detector = ToxicFlowDetector::new(0.75);
        let pk = mock_pubkey();

        // Record mixed results
        for i in 0..10 {
            let outcome = if i % 2 == 0 { 100_000 } else { -100_000 };
            detector.record_trade(&pk, true, outcome);
        }

        let analysis = detector.analyze(&pk);
        assert!(!analysis.is_toxic);
    }
}
