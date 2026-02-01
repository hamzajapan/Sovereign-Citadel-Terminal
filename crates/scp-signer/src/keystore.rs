//! Secure key storage abstraction.
//!
//! Provides a trait-based abstraction for key storage backends,
//! allowing easy swapping between file-based storage (development)
//! and HSM-based storage (production).

use scp_core::{Error, PublicKey, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;

/// A key pair identifier.
pub type KeyId = String;

/// Information about a stored key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyInfo {
    /// The key identifier.
    pub id: KeyId,
    /// The public key.
    pub public_key: PublicKey,
    /// When the key was created (Unix timestamp).
    pub created_at: u64,
    /// Optional label for the key.
    pub label: Option<String>,
}

/// Trait for key storage backends.
///
/// Implementations must be thread-safe as keys may be accessed
/// from multiple async tasks.
pub trait Keystore: Send + Sync {
    /// Generate a new key pair.
    fn generate_key(&self, label: Option<String>) -> Result<KeyInfo>;

    /// Import an existing secret key.
    fn import_key(&self, secret_key: &[u8], label: Option<String>) -> Result<KeyInfo>;

    /// Get key info by ID.
    fn get_key_info(&self, id: &KeyId) -> Result<Option<KeyInfo>>;

    /// List all keys.
    fn list_keys(&self) -> Result<Vec<KeyInfo>>;

    /// Sign a message with the specified key.
    ///
    /// Returns the raw signature bytes.
    fn sign(&self, key_id: &KeyId, message: &[u8]) -> Result<Vec<u8>>;

    /// Sign a message with the specified key using Schnorr (BIP-340).
    ///
    /// Returns the raw signature bytes (64 bytes).
    fn sign_schnorr(&self, key_id: &KeyId, message: &[u8]) -> Result<Vec<u8>>;

    /// Delete a key.
    fn delete_key(&self, id: &KeyId) -> Result<()>;

    /// Check if a key exists.
    fn has_key(&self, id: &KeyId) -> bool {
        self.get_key_info(id).map(|k| k.is_some()).unwrap_or(false)
    }
}

/// File-based keystore for development and testing.
///
/// Keys are stored encrypted on disk. In production, use an HSM backend.
pub struct FileKeystore {
    /// Path to the keystore directory.
    path: std::path::PathBuf,
    /// In-memory cache of key info (secrets NOT cached).
    cache: RwLock<HashMap<KeyId, KeyInfo>>,
    /// Encryption key (derived from password).
    encryption_key: [u8; 32],
}

impl FileKeystore {
    /// Create a new file keystore.
    ///
    /// # Arguments
    /// * `path` - Directory to store key files
    /// * `password` - Password for encrypting keys
    pub fn new(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        std::fs::create_dir_all(&path).map_err(|e| Error::Keystore(e.to_string()))?;

        // Derive encryption key from password (simplified - use proper KDF in production)
        let mut encryption_key = [0u8; 32];
        let password_bytes = password.as_bytes();
        for (i, byte) in password_bytes.iter().enumerate() {
            encryption_key[i % 32] ^= byte;
        }

        let keystore = Self {
            path,
            cache: RwLock::new(HashMap::new()),
            encryption_key,
        };

        // Load existing keys into cache
        keystore.load_keys()?;

        Ok(keystore)
    }

    /// Load existing keys from disk into cache.
    fn load_keys(&self) -> Result<()> {
        let entries = std::fs::read_dir(&self.path).map_err(|e| Error::Keystore(e.to_string()))?;

        let mut cache = self
            .cache
            .write()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "key").unwrap_or(false) {
                if let Ok(info) = self.load_key_info(&path) {
                    cache.insert(info.id.clone(), info);
                }
            }
        }

        Ok(())
    }

    /// Load key info from a file (without the secret).
    fn load_key_info(&self, path: &Path) -> Result<KeyInfo> {
        let data = std::fs::read(path).map_err(|e| Error::Keystore(e.to_string()))?;
        let decrypted = self.decrypt(&data)?;
        let stored: StoredKey =
            serde_json::from_slice(&decrypted).map_err(|e| Error::Serialization(e.to_string()))?;
        Ok(stored.info)
    }

    /// Simple XOR encryption (use AES-GCM in production).
    fn encrypt(&self, data: &[u8]) -> Vec<u8> {
        data.iter()
            .enumerate()
            .map(|(i, b)| b ^ self.encryption_key[i % 32])
            .collect()
    }

    /// Simple XOR decryption.
    fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(self.encrypt(data)) // XOR is symmetric
    }

    /// Generate a unique key ID.
    fn generate_id() -> KeyId {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("key_{:x}", timestamp)
    }

    /// Get the file path for a key.
    fn key_path(&self, id: &KeyId) -> std::path::PathBuf {
        self.path.join(format!("{}.key", id))
    }
}

/// Internal structure for storing a key on disk.
#[derive(Serialize, Deserialize)]
struct StoredKey {
    info: KeyInfo,
    secret_key: Vec<u8>, // Encrypted
}

impl Keystore for FileKeystore {
    fn generate_key(&self, label: Option<String>) -> Result<KeyInfo> {
        use secp256k1::Secp256k1;

        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut secp256k1::rand::thread_rng());

        let id = Self::generate_id();
        let info = KeyInfo {
            id: id.clone(),
            public_key: PublicKey::new(public_key),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            label,
        };

        // Store to disk
        let stored = StoredKey {
            info: info.clone(),
            secret_key: secret_key.secret_bytes().to_vec(),
        };
        let serialized =
            serde_json::to_vec(&stored).map_err(|e| Error::Serialization(e.to_string()))?;
        let encrypted = self.encrypt(&serialized);
        std::fs::write(self.key_path(&id), encrypted)
            .map_err(|e| Error::Keystore(e.to_string()))?;

        // Update cache
        let mut cache = self
            .cache
            .write()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        cache.insert(id, info.clone());

        tracing::info!(key_id = %info.id, "Generated new key");
        Ok(info)
    }

    fn import_key(&self, secret_bytes: &[u8], label: Option<String>) -> Result<KeyInfo> {
        use secp256k1::{Secp256k1, SecretKey};

        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(secret_bytes)
            .map_err(|e| Error::InvalidPublicKey(e.to_string()))?;
        let public_key = secret_key.public_key(&secp);

        let id = Self::generate_id();
        let info = KeyInfo {
            id: id.clone(),
            public_key: PublicKey::new(public_key),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            label,
        };

        // Store to disk
        let stored = StoredKey {
            info: info.clone(),
            secret_key: secret_bytes.to_vec(),
        };
        let serialized =
            serde_json::to_vec(&stored).map_err(|e| Error::Serialization(e.to_string()))?;
        let encrypted = self.encrypt(&serialized);
        std::fs::write(self.key_path(&id), encrypted)
            .map_err(|e| Error::Keystore(e.to_string()))?;

        // Update cache
        let mut cache = self
            .cache
            .write()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        cache.insert(id, info.clone());

        tracing::info!(key_id = %info.id, "Imported key");
        Ok(info)
    }

    fn get_key_info(&self, id: &KeyId) -> Result<Option<KeyInfo>> {
        let cache = self
            .cache
            .read()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        Ok(cache.get(id).cloned())
    }

    fn list_keys(&self) -> Result<Vec<KeyInfo>> {
        let cache = self
            .cache
            .read()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        Ok(cache.values().cloned().collect())
    }

    fn sign(&self, key_id: &KeyId, message: &[u8]) -> Result<Vec<u8>> {
        use secp256k1::{Message, Secp256k1, SecretKey};

        // Load the secret key from disk
        let path = self.key_path(key_id);
        if !path.exists() {
            return Err(Error::KeyNotFound(key_id.clone()));
        }

        let encrypted = std::fs::read(&path).map_err(|e| Error::Keystore(e.to_string()))?;
        let decrypted = self.decrypt(&encrypted)?;
        let stored: StoredKey =
            serde_json::from_slice(&decrypted).map_err(|e| Error::Serialization(e.to_string()))?;

        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&stored.secret_key)
            .map_err(|e| Error::Keystore(e.to_string()))?;

        // Hash the message if it's not already 32 bytes
        let msg_hash = if message.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(message);
            arr
        } else {
            use bitcoin::hashes::{sha256, Hash};
            sha256::Hash::hash(message).to_byte_array()
        };

        let msg = Message::from_digest(msg_hash);
        let signature = secp.sign_ecdsa(&msg, &secret_key);

        tracing::debug!(key_id = %key_id, "Signed message");
        Ok(signature.serialize_compact().to_vec())
    }

    fn sign_schnorr(&self, key_id: &KeyId, message: &[u8]) -> Result<Vec<u8>> {
        use secp256k1::{Keypair, Message, Secp256k1, SecretKey};

        // Load the secret key from disk
        let path = self.key_path(key_id);
        if !path.exists() {
            return Err(Error::KeyNotFound(key_id.clone()));
        }

        let encrypted = std::fs::read(&path).map_err(|e| Error::Keystore(e.to_string()))?;
        let decrypted = self.decrypt(&encrypted)?;
        let stored: StoredKey =
            serde_json::from_slice(&decrypted).map_err(|e| Error::Serialization(e.to_string()))?;

        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&stored.secret_key)
            .map_err(|e| Error::Keystore(e.to_string()))?;
        let keypair = Keypair::from_secret_key(&secp, &secret_key);

        // Hash the message if it's not already 32 bytes
        let msg_hash = if message.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(message);
            arr
        } else {
            use bitcoin::hashes::{sha256, Hash};
            sha256::Hash::hash(message).to_byte_array()
        };

        let msg = Message::from_digest(msg_hash);
        let signature = secp.sign_schnorr_no_aux_rand(&msg, &keypair);

        tracing::debug!(key_id = %key_id, "Signed Schnorr message");
        Ok(signature.as_ref().to_vec())
    }

    fn delete_key(&self, id: &KeyId) -> Result<()> {
        let path = self.key_path(id);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| Error::Keystore(e.to_string()))?;
        }

        let mut cache = self
            .cache
            .write()
            .map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        cache.remove(id);

        tracing::info!(key_id = %id, "Deleted key");
        Ok(())
    }
}

/// InMemory keystore for testing.
#[derive(Debug, Clone, Default)]
pub struct MemoryKeystore {
    keys: std::sync::Arc<RwLock<HashMap<KeyId, KeyInfo>>>,
    secrets: std::sync::Arc<RwLock<HashMap<KeyId, Vec<u8>>>>,
}

impl MemoryKeystore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Keystore for MemoryKeystore {
    fn generate_key(&self, label: Option<String>) -> Result<KeyInfo> {
        let mut keys = self.keys.write().unwrap();
        let mut secrets = self.secrets.write().unwrap();

        // Mock generation
        let id = format!("key-{}", keys.len());
        let secret = vec![1u8; 32]; // Hardcoded 0x01...
                                    // Valid secp256k1 pubkey (0x02 + 32 bytes)
        let pk_bytes = [
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02,
        ];
        let secp_pk = bitcoin::secp256k1::PublicKey::from_slice(&pk_bytes).unwrap();
        let public_key = PublicKey::new(secp_pk);

        let info = KeyInfo {
            id: id.clone(),
            public_key,
            label,
            created_at: 0,
            // key_type: crate::keystore::KeyType::Internal, // Removed
        };

        keys.insert(id.clone(), info.clone());
        secrets.insert(id, secret);

        Ok(info)
    }

    fn import_key(&self, secret_bytes: &[u8], label: Option<String>) -> Result<KeyInfo> {
        let mut keys = self.keys.write().unwrap();
        let mut secrets = self.secrets.write().unwrap();

        let id = format!("import-{}", keys.len());
        // Valid secp256k1 pubkey
        let pk_bytes = [
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x02, 0x02,
        ];
        let secp_pk = bitcoin::secp256k1::PublicKey::from_slice(&pk_bytes).unwrap();
        let public_key = PublicKey::new(secp_pk);

        let info = KeyInfo {
            id: id.clone(),
            public_key,
            label,
            created_at: 0,
            // key_type: crate::keystore::KeyType::Imported, // Removed
        };
        keys.insert(id.clone(), info.clone());
        secrets.insert(id, secret_bytes.to_vec());
        Ok(info)
    }

    fn has_key(&self, id: &KeyId) -> bool {
        self.keys.read().unwrap().contains_key(id)
    }

    fn get_key_info(&self, id: &KeyId) -> Result<Option<KeyInfo>> {
        Ok(self.keys.read().unwrap().get(id).cloned())
    }

    fn list_keys(&self) -> Result<Vec<KeyInfo>> {
        Ok(self.keys.read().unwrap().values().cloned().collect())
    }

    fn sign(&self, key_id: &KeyId, _message: &[u8]) -> Result<Vec<u8>> {
        if self.has_key(key_id) {
            Ok(vec![0u8; 64]) // Mock Sig
        } else {
            Err(Error::KeyNotFound(key_id.clone()))
        }
    }

    fn sign_schnorr(&self, key_id: &KeyId, _message: &[u8]) -> Result<Vec<u8>> {
        if self.has_key(key_id) {
            Ok(vec![0u8; 64]) // Mock Sig
        } else {
            Err(Error::KeyNotFound(key_id.clone()))
        }
    }

    fn delete_key(&self, id: &KeyId) -> Result<()> {
        self.keys.write().unwrap().remove(id);
        self.secrets.write().unwrap().remove(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_and_sign() {
        let dir = tempdir().unwrap();
        let keystore = FileKeystore::new(dir.path(), "test_password").unwrap();

        // Generate a key
        let info = keystore.generate_key(Some("test key".to_string())).unwrap();
        assert!(info.label.is_some());

        // Sign a message
        let message = b"Hello, SCP!";
        let signature = keystore.sign(&info.id, message).unwrap();
        assert_eq!(signature.len(), 64); // ECDSA compact signature
    }

    #[test]
    fn test_list_keys() {
        let dir = tempdir().unwrap();
        let keystore = FileKeystore::new(dir.path(), "test_password").unwrap();

        // Generate multiple keys
        keystore.generate_key(Some("key1".to_string())).unwrap();
        keystore.generate_key(Some("key2".to_string())).unwrap();

        let keys = keystore.list_keys().unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_key_not_found() {
        let dir = tempdir().unwrap();
        let keystore = FileKeystore::new(dir.path(), "test_password").unwrap();

        let result = keystore.sign(&"nonexistent".to_string(), b"test");
        assert!(matches!(result, Err(Error::KeyNotFound(_))));
    }
}
