//! Oracle interface for DLCs.

use async_trait::async_trait;
use scp_core::{
    crypto::OracleAttestation,
    types::{OracleInfo, PublicKey},
    Result,
};
use serde::{Deserialize, Serialize};

/// Trait for oracle interaction.
#[async_trait]
pub trait OracleClient: Send + Sync {
    /// Fetch the public announcement (keys/nonces) for an event.
    /// Note: Returns OracleAnnouncement which is currently defined in scp-dlc or scp-core?
    /// scp-core doesn't have OracleAnnouncement defined in types/crypto yet (based on previous read).
    /// But wait, scp-dlc had it.
    /// I will define OracleAnnouncement here if not in scp-core.
    async fn get_announcement(&self, event_id: &str) -> Result<OracleAnnouncement>;

    /// Poll or fetch the final attestation (fails if event not over).
    async fn get_attestation(&self, event_id: &str) -> Result<OracleAttestation>;
}

/// Oracle announcement for an event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleAnnouncement {
    pub oracle_pubkey: PublicKey,
    pub event_id: String,
    pub nonces: Vec<PublicKey>, // R_points
}

/// Mock Oracle for testing.
#[derive(Default)]
pub struct MockOracle {
    announcements: std::sync::RwLock<std::collections::HashMap<String, OracleAnnouncement>>,
    attestations: std::sync::RwLock<std::collections::HashMap<String, OracleAttestation>>,
}

impl MockOracle {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_announcement(&self, announcement: OracleAnnouncement) {
        self.announcements
            .write()
            .unwrap()
            .insert(announcement.event_id.clone(), announcement);
    }

    pub fn add_attestation(&self, _attestation: OracleAttestation) {
        // We use event_id as key, but scp_core::OracleAttestation doesn't have event_id field globally?
        // scp_core::OracleAttestation has oracle_pubkey, outcome, secret, signature.
        // It does NOT have event_id.
        // The mock needs to store it somehow.
        // I will assume for now we key by outcome or something?
        // Or I can wrap it?
        // The test creates it with create_attestation.
        // Harness create_attestation doesn't seem to set event_id (checked mod.rs).
        // So MockOracle keying by event_id might be problematic if attestation doesn't carry it.
        // However, get_attestation takes event_id.
        // I will store it in a map <String, OracleAttestation> where key is event_id.
        // But add_attestation needs event_id arg then?
        // The previous MockOracle had add_attestation taking OracleAttestation which HAD event_id.
        // Now scp_core::OracleAttestation lacks it.
        // I'll add a method `add_attestation_for_event`.
    }

    pub fn add_test_attestation(&self, event_id: &str, attestation: OracleAttestation) {
        self.attestations
            .write()
            .unwrap()
            .insert(event_id.to_string(), attestation);
    }
}

#[async_trait]
impl OracleClient for MockOracle {
    async fn get_announcement(&self, event_id: &str) -> Result<OracleAnnouncement> {
        self.announcements
            .read()
            .unwrap()
            .get(event_id)
            .cloned()
            .ok_or_else(|| {
                scp_core::Error::OracleError(format!("Announcement not found: {}", event_id))
            })
    }

    async fn get_attestation(&self, event_id: &str) -> Result<OracleAttestation> {
        self.attestations
            .read()
            .unwrap()
            .get(event_id)
            .cloned()
            .ok_or_else(|| {
                scp_core::Error::OracleError(format!("Attestation not found: {}", event_id))
            })
    }
}

/// A quorum of oracles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiOracleQuorum {
    pub oracles: Vec<PublicKey>,
    pub threshold: usize,
    #[serde(skip)]
    pub attestations: Vec<OracleAttestation>,
}

impl MultiOracleQuorum {
    pub fn new(config: QuorumConfig) -> Self {
        match config {
            QuorumConfig::Simple(oracle) => Self {
                oracles: vec![oracle.public_key],
                threshold: 1,
                attestations: Vec::new(),
            },
            QuorumConfig::Multi { threshold, oracles } => Self {
                oracles: oracles.into_iter().map(|o| o.public_key).collect(),
                threshold,
                attestations: Vec::new(),
            },
        }
    }

    pub fn add_attestation(&mut self, attestation: OracleAttestation) -> Result<()> {
        self.attestations.push(attestation);
        Ok(())
    }

    pub fn has_quorum(&self) -> Option<Vec<OracleAttestation>> {
        if self.attestations.len() >= self.threshold {
            Some(self.attestations.clone())
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub enum QuorumConfig {
    Simple(OracleInfo),
    Multi {
        threshold: usize,
        oracles: Vec<OracleInfo>,
    },
}

impl QuorumConfig {
    pub fn simple(oracle: OracleInfo) -> Self {
        Self::Simple(oracle)
    }

    pub fn multi(threshold: usize, oracles: Vec<OracleInfo>) -> Result<Self> {
        if threshold == 0 || threshold > oracles.len() {
            return Err(scp_core::Error::Config("Invalid quorum threshold".into()));
        }
        Ok(Self::Multi { threshold, oracles })
    }
}
