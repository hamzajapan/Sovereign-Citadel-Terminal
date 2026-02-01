//! Liquidity Bootstrapping Offering (LBO).
//!
//! Fair launch mechanism where participants mint $CTDL by proving
//! liquidity provision to the protocol.

use crate::token::CtdlToken;
use scp_core::{ContractId, PublicKey, Result, Satoshi};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// LBO configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LboConfig {
    /// Start timestamp.
    pub start_time: u64,
    /// End timestamp.
    pub end_time: u64,
    /// Minimum contribution per participant.
    pub min_contribution: Satoshi,
    /// Maximum contribution per participant.
    pub max_contribution: Satoshi,
    /// CTDL tokens per satoshi of liquidity.
    pub tokens_per_sat: f64,
    /// Total token allocation for LBO.
    pub total_allocation: u64,
}

impl Default for LboConfig {
    fn default() -> Self {
        Self {
            start_time: 0,
            end_time: u64::MAX,
            min_contribution: Satoshi::from_sat(100_000), // 0.001 BTC
            max_contribution: Satoshi::from_btc(10.0),    // 10 BTC
            tokens_per_sat: 10.0,                         // 10 CTDL per sat
            total_allocation: 1_000_000_000 * 100_000_000 / 10, // 10% of supply
        }
    }
}

/// A liquidity contribution proof.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityProof {
    /// The contributor.
    pub contributor: PublicKey,
    /// The DLC contract ID (proof they provided liquidity).
    pub contract_id: ContractId,
    /// Amount of liquidity provided.
    pub amount: Satoshi,
    /// Timestamp of contribution.
    pub timestamp: u64,
    /// Whether this proof has been used to mint tokens.
    pub claimed: bool,
}

/// The LBO manager.
pub struct LiquidityBootstrapping {
    config: LboConfig,
    token: Arc<CtdlToken>,
    /// Contributions by participant.
    contributions: RwLock<HashMap<String, Vec<LiquidityProof>>>,
    /// Total raised.
    total_raised: RwLock<Satoshi>,
    /// Total tokens distributed.
    tokens_distributed: RwLock<u64>,
}

impl LiquidityBootstrapping {
    /// Create a new LBO.
    pub fn new(config: LboConfig, token: Arc<CtdlToken>) -> Self {
        Self {
            config,
            token,
            contributions: RwLock::new(HashMap::new()),
            total_raised: RwLock::new(Satoshi::ZERO),
            tokens_distributed: RwLock::new(0),
        }
    }

    /// Check if LBO is active.
    pub fn is_active(&self) -> bool {
        let now = now();
        now >= self.config.start_time && now <= self.config.end_time
    }

    /// Check if LBO has ended.
    pub fn has_ended(&self) -> bool {
        now() > self.config.end_time
    }

    /// Submit a liquidity proof.
    pub fn submit_proof(&self, proof: LiquidityProof) -> Result<()> {
        if !self.is_active() {
            return Err(scp_core::Error::InvalidContract(
                "LBO is not active".to_string(),
            ));
        }

        // Validate contribution bounds
        let participant_total = self.participant_total(&proof.contributor);
        let new_total = participant_total + proof.amount;

        if new_total < self.config.min_contribution {
            return Err(scp_core::Error::DepositTooSmall {
                minimum: self.config.min_contribution.as_sat(),
                actual: new_total.as_sat(),
            });
        }

        if new_total > self.config.max_contribution {
            return Err(scp_core::Error::InvalidAmount(format!(
                "Exceeds max contribution of {} sats",
                self.config.max_contribution.as_sat()
            )));
        }

        // Store the proof
        {
            let key = proof.contributor.to_string();
            let mut contributions = self.contributions.write().unwrap();
            contributions.entry(key).or_default().push(proof.clone());
        }

        // Update total raised
        {
            let mut total = self.total_raised.write().unwrap();
            *total = *total + proof.amount;
        }

        tracing::info!(
            contributor = %proof.contributor,
            amount = %proof.amount,
            "Liquidity proof submitted"
        );

        Ok(())
    }

    /// Claim tokens for a contributor.
    pub fn claim(&self, contributor: &PublicKey) -> Result<u64> {
        let key = contributor.to_string();

        // Calculate unclaimed tokens
        let (unclaimed_sats, proofs_to_mark) = {
            let contributions = self.contributions.read().unwrap();
            let proofs = contributions.get(&key).ok_or_else(|| {
                scp_core::Error::InvalidAmount("No contributions found".to_string())
            })?;

            let unclaimed: Vec<&LiquidityProof> = proofs.iter().filter(|p| !p.claimed).collect();
            if unclaimed.is_empty() {
                return Err(scp_core::Error::InvalidAmount(
                    "No unclaimed contributions".to_string(),
                ));
            }

            let total_sats: u64 = unclaimed.iter().map(|p| p.amount.as_sat()).sum();
            let indices: Vec<usize> = proofs
                .iter()
                .enumerate()
                .filter(|(_, p)| !p.claimed)
                .map(|(i, _)| i)
                .collect();

            (total_sats, indices)
        };

        // Calculate token amount
        let token_amount = (unclaimed_sats as f64 * self.config.tokens_per_sat) as u64;

        // Check allocation limit
        let current_distributed = *self.tokens_distributed.read().unwrap();
        if current_distributed + token_amount > self.config.total_allocation {
            return Err(scp_core::Error::InvalidAmount(
                "LBO allocation exhausted".to_string(),
            ));
        }

        // Mint tokens
        self.token.mint(contributor, token_amount)?;

        // Mark proofs as claimed
        {
            let mut contributions = self.contributions.write().unwrap();
            if let Some(proofs) = contributions.get_mut(&key) {
                for i in proofs_to_mark {
                    if let Some(proof) = proofs.get_mut(i) {
                        proof.claimed = true;
                    }
                }
            }
        }

        // Update distributed count
        {
            let mut distributed = self.tokens_distributed.write().unwrap();
            *distributed += token_amount;
        }

        tracing::info!(
            contributor = %contributor,
            tokens = token_amount,
            "LBO tokens claimed"
        );

        Ok(token_amount)
    }

    /// Get a participant's total contribution.
    pub fn participant_total(&self, contributor: &PublicKey) -> Satoshi {
        let key = contributor.to_string();
        let contributions = self.contributions.read().unwrap();

        contributions
            .get(&key)
            .map(|proofs| {
                let total: u64 = proofs.iter().map(|p| p.amount.as_sat()).sum();
                Satoshi::from_sat(total)
            })
            .unwrap_or(Satoshi::ZERO)
    }

    /// Get total raised.
    pub fn total_raised(&self) -> Satoshi {
        *self.total_raised.read().unwrap()
    }

    /// Get total tokens distributed.
    pub fn tokens_distributed(&self) -> u64 {
        *self.tokens_distributed.read().unwrap()
    }

    /// Get remaining allocation.
    pub fn remaining_allocation(&self) -> u64 {
        self.config.total_allocation - self.tokens_distributed()
    }

    /// Get LBO metrics.
    pub fn metrics(&self) -> LboMetrics {
        LboMetrics {
            is_active: self.is_active(),
            total_raised: self.total_raised(),
            tokens_distributed: self.tokens_distributed(),
            remaining_allocation: self.remaining_allocation(),
            participants: self.contributions.read().unwrap().len(),
        }
    }
}

/// LBO metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LboMetrics {
    pub is_active: bool,
    pub total_raised: Satoshi,
    pub tokens_distributed: u64,
    pub remaining_allocation: u64,
    pub participants: usize,
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
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let (_, pk) = secp.generate_keypair(&mut rand::thread_rng());
        PublicKey::new(pk)
    }

    #[test]
    fn test_lbo_contribution_and_claim() {
        let token = Arc::new(CtdlToken::new(1_000_000_000_000_000));
        let lbo = LiquidityBootstrapping::new(LboConfig::default(), token.clone());

        let contributor = mock_pubkey();

        // Submit proof
        let proof = LiquidityProof {
            contributor,
            contract_id: ContractId::from_data(b"test"),
            amount: Satoshi::from_sat(1_000_000),
            timestamp: now(),
            claimed: false,
        };

        lbo.submit_proof(proof).unwrap();

        assert_eq!(lbo.total_raised(), Satoshi::from_sat(1_000_000));

        // Claim tokens
        let tokens = lbo.claim(&contributor).unwrap();
        assert!(tokens > 0);

        // Check token balance
        let balance = token.balance_of(&contributor).unwrap();
        assert_eq!(balance.available, tokens);
    }

    #[test]
    fn test_contribution_limits() {
        let token = Arc::new(CtdlToken::new(1_000_000_000_000_000));
        let config = LboConfig {
            max_contribution: Satoshi::from_sat(1_000_000),
            ..Default::default()
        };
        let lbo = LiquidityBootstrapping::new(config, token);

        let contributor = mock_pubkey();

        // First contribution
        let proof1 = LiquidityProof {
            contributor,
            contract_id: ContractId::from_data(b"test1"),
            amount: Satoshi::from_sat(800_000),
            timestamp: now(),
            claimed: false,
        };
        lbo.submit_proof(proof1).unwrap();

        // Second contribution exceeds max
        let proof2 = LiquidityProof {
            contributor,
            contract_id: ContractId::from_data(b"test2"),
            amount: Satoshi::from_sat(300_000),
            timestamp: now(),
            claimed: false,
        };
        let result = lbo.submit_proof(proof2);
        assert!(result.is_err());
    }
}
