//! $CTDL token and balance management.

use scp_core::{PublicKey, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Token denomination (1 CTDL = 10^8 units).
pub const CTDL_DECIMALS: u8 = 8;

/// The $CTDL token.
#[derive(Debug)]
pub struct CtdlToken {
    /// Total supply minted.
    total_supply: RwLock<u64>,
    /// Balances by holder.
    balances: RwLock<HashMap<String, CtdlBalance>>,
    /// Max supply cap.
    max_supply: u64,
}

/// A $CTDL balance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtdlBalance {
    /// The holder's public key.
    pub holder: PublicKey,
    /// Available (unlocked) balance.
    pub available: u64,
    /// Staked balance.
    pub staked: u64,
    /// Vesting balance (time-locked).
    pub vesting: u64,
}

impl CtdlBalance {
    /// Create a new empty balance.
    pub fn new(holder: PublicKey) -> Self {
        Self {
            holder,
            available: 0,
            staked: 0,
            vesting: 0,
        }
    }

    /// Total balance.
    pub fn total(&self) -> u64 {
        self.available + self.staked + self.vesting
    }
}

impl CtdlToken {
    /// Create a new token with max supply.
    pub fn new(max_supply: u64) -> Self {
        Self {
            total_supply: RwLock::new(0),
            balances: RwLock::new(HashMap::new()),
            max_supply,
        }
    }

    /// Get the total supply.
    pub fn total_supply(&self) -> u64 {
        *self.total_supply.read().unwrap()
    }

    /// Get the max supply.
    pub fn max_supply(&self) -> u64 {
        self.max_supply
    }

    /// Remaining mintable supply.
    pub fn remaining_supply(&self) -> u64 {
        self.max_supply - self.total_supply()
    }

    /// Mint tokens to an address.
    pub fn mint(&self, to: &PublicKey, amount: u64) -> Result<()> {
        let key = to.to_string();

        // Check supply cap
        {
            let mut supply = self.total_supply.write().unwrap();
            if *supply + amount > self.max_supply {
                return Err(scp_core::Error::InvalidAmount(
                    "Would exceed max supply".to_string(),
                ));
            }
            *supply += amount;
        }

        // Credit balance
        {
            let mut balances = self.balances.write().unwrap();
            let balance = balances.entry(key).or_insert_with(|| CtdlBalance::new(*to));
            balance.available += amount;
        }

        tracing::info!(to = %to, amount = amount, "Minted CTDL tokens");
        Ok(())
    }

    /// Transfer tokens.
    pub fn transfer(&self, from: &PublicKey, to: &PublicKey, amount: u64) -> Result<()> {
        let from_key = from.to_string();
        let to_key = to.to_string();

        let mut balances = self.balances.write().unwrap();

        // Check sender balance
        let sender = balances
            .get_mut(&from_key)
            .ok_or_else(|| scp_core::Error::InvalidAmount("Sender has no balance".to_string()))?;

        if sender.available < amount {
            return Err(scp_core::Error::InvalidAmount(
                "Insufficient available balance".to_string(),
            ));
        }

        sender.available -= amount;

        // Credit receiver
        let receiver = balances
            .entry(to_key)
            .or_insert_with(|| CtdlBalance::new(*to));
        receiver.available += amount;

        Ok(())
    }

    /// Get balance for an address.
    pub fn balance_of(&self, holder: &PublicKey) -> Option<CtdlBalance> {
        let key = holder.to_string();
        self.balances.read().unwrap().get(&key).cloned()
    }

    /// Lock tokens for staking.
    pub fn stake(&self, holder: &PublicKey, amount: u64) -> Result<()> {
        let key = holder.to_string();
        let mut balances = self.balances.write().unwrap();

        let balance = balances
            .get_mut(&key)
            .ok_or_else(|| scp_core::Error::InvalidAmount("Holder has no balance".to_string()))?;

        if balance.available < amount {
            return Err(scp_core::Error::InvalidAmount(
                "Insufficient available balance".to_string(),
            ));
        }

        balance.available -= amount;
        balance.staked += amount;

        Ok(())
    }

    /// Unstake tokens.
    pub fn unstake(&self, holder: &PublicKey, amount: u64) -> Result<()> {
        let key = holder.to_string();
        let mut balances = self.balances.write().unwrap();

        let balance = balances
            .get_mut(&key)
            .ok_or_else(|| scp_core::Error::InvalidAmount("Holder has no balance".to_string()))?;

        if balance.staked < amount {
            return Err(scp_core::Error::InvalidAmount(
                "Insufficient staked balance".to_string(),
            ));
        }

        balance.staked -= amount;
        balance.available += amount;

        Ok(())
    }

    /// Get all holders.
    pub fn holders(&self) -> Vec<PublicKey> {
        self.balances
            .read()
            .unwrap()
            .values()
            .map(|b| b.holder)
            .collect()
    }

    /// Get all stakers and their staked amounts.
    pub fn stakers(&self) -> Vec<(PublicKey, u64)> {
        self.balances
            .read()
            .unwrap()
            .values()
            .filter(|b| b.staked > 0)
            .map(|b| (b.holder, b.staked))
            .collect()
    }

    /// Get total staked supply.
    pub fn total_staked(&self) -> u64 {
        self.balances
            .read()
            .unwrap()
            .values()
            .map(|b| b.staked)
            .sum()
    }

    /// Load from file or create new.
    pub fn load_or_new(path: std::path::PathBuf, max_supply: u64) -> Arc<Self> {
        if path.exists() {
            let file = std::fs::File::open(&path).expect("Failed to open token file");
            let state: TokenState =
                serde_json::from_reader(file).expect("Failed to parse token file");

            Arc::new(Self {
                total_supply: RwLock::new(state.total_supply),
                balances: RwLock::new(state.balances),
                max_supply: state.max_supply,
            })
        } else {
            Arc::new(Self::new(max_supply))
        }
    }

    /// Save state to file.
    pub fn save(&self, path: &std::path::PathBuf) -> Result<()> {
        let state = TokenState {
            total_supply: *self.total_supply.read().unwrap(),
            balances: self.balances.read().unwrap().clone(),
            max_supply: self.max_supply,
        };

        let file = std::fs::File::create(path).map_err(scp_core::Error::Io)?;
        serde_json::to_writer_pretty(file, &state)
            .map_err(|e| scp_core::Error::Io(std::io::Error::other(e)))?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct TokenState {
    total_supply: u64,
    balances: HashMap<String, CtdlBalance>,
    max_supply: u64,
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
    fn test_mint_and_transfer() {
        let token = CtdlToken::new(1_000_000_000);

        let alice = mock_pubkey();
        let bob = mock_pubkey();

        // Mint to Alice
        token.mint(&alice, 1000).unwrap();
        assert_eq!(token.balance_of(&alice).unwrap().available, 1000);

        // Transfer to Bob
        token.transfer(&alice, &bob, 300).unwrap();
        assert_eq!(token.balance_of(&alice).unwrap().available, 700);
        assert_eq!(token.balance_of(&bob).unwrap().available, 300);
    }

    #[test]
    fn test_staking() {
        let token = CtdlToken::new(1_000_000_000);
        let holder = mock_pubkey();

        token.mint(&holder, 1000).unwrap();

        // Stake half
        token.stake(&holder, 500).unwrap();

        let balance = token.balance_of(&holder).unwrap();
        assert_eq!(balance.available, 500);
        assert_eq!(balance.staked, 500);
        assert_eq!(balance.total(), 1000);

        // Unstake
        token.unstake(&holder, 200).unwrap();
        let balance = token.balance_of(&holder).unwrap();
        assert_eq!(balance.available, 700);
        assert_eq!(balance.staked, 300);
    }

    #[test]
    fn test_supply_cap() {
        let token = CtdlToken::new(1000);
        let holder = mock_pubkey();

        token.mint(&holder, 1000).unwrap();

        // Trying to mint more should fail
        let result = token.mint(&holder, 1);
        assert!(result.is_err());
    }
}
