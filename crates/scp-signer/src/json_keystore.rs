use crate::keystore::{KeyId, KeyInfo, Keystore};
use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce}; // Or AeadCore::nonce_size()
use argon2::{
    password_hash::{rand_core::RngCore, PasswordHasher, SaltString},
    Argon2,
};
use scp_core::{Error, PublicKey, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock}; // Use Arc<RwLock> for internal mutability if needed, or just RwLock

use serde::{Deserialize, Serialize};

use zeroize::{Zeroize, ZeroizeOnDrop};

/// A single-file encrypted keystore.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct JsonKeystore {
    #[zeroize(skip)]
    path: PathBuf,
    #[zeroize(skip)]
    keys: RwLock<HashMap<KeyId, (KeyInfo, Vec<u8>)>>, 
    password: String,
}

#[derive(Serialize, Deserialize)]
struct EncryptedFile {
    salt: String, // Salt for Argon2
    nonce: Vec<u8>, // Nonce for AES-GCM
    ciphertext: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct VaultData {
    keys: HashMap<KeyId, (KeyInfo, Vec<u8>)>,
}

impl JsonKeystore {
    pub fn new(path: PathBuf, password: String) -> Result<Self> {
        let keystore = Self {
            path: path.clone(),
            keys: RwLock::new(HashMap::new()),
            password,
        };

        if path.exists() {
            keystore.load()?;
        } else {
            // New keystore, save immediately to create file
            keystore.save()?;
        }

        Ok(keystore)
    }

    fn derive_key(password: &str, salt: &str) -> Result<[u8; 32]> {
        let argon2 = Argon2::default();
        let salt = SaltString::from_b64(salt)
            .map_err(|e| Error::Keystore(format!("Invalid salt: {}", e)))?;
        
        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| Error::Keystore(format!("Key derivation failed: {}", e)))?;

        let output = hash.hash.ok_or_else(|| Error::Keystore("No hash output".into()))?;
        
        // Use first 32 bytes of hash as AES key
        let mut key = [0u8; 32];
        let bytes = output.as_bytes();
        if bytes.len() >= 32 {
            key.copy_from_slice(&bytes[..32]);
        } else {
            return Err(Error::Keystore("Hash too short".into()));
        }
        
        Ok(key)
    }

    fn load(&self) -> Result<()> {
        let content = std::fs::read(&self.path)
            .map_err(|e| Error::Keystore(format!("Failed to read keystore: {}", e)))?;
        
        let encrypted_file: EncryptedFile = serde_json::from_slice(&content)
            .map_err(|e| Error::Keystore(format!("Invalid keystore format: {}", e)))?;

        let key = Self::derive_key(&self.password, &encrypted_file.salt)?;
        let cipher = Aes256Gcm::new(&key.into());
        let nonce = Nonce::from_slice(&encrypted_file.nonce);

        let plaintext = cipher.decrypt(nonce, encrypted_file.ciphertext.as_ref())
            .map_err(|_| Error::Keystore("Decryption failed - Wrong password?".into()))?;

        let vault: VaultData = serde_json::from_slice(&plaintext)
            .map_err(|e| Error::Keystore(format!("Invalid vault data: {}", e)))?;

        *self.keys.write().unwrap() = vault.keys;
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let keys = self.keys.read().unwrap();
        let vault = VaultData {
            keys: keys.clone(),
        };
        let plaintext = serde_json::to_vec(&vault)
            .map_err(|e| Error::Serialization(e.to_string()))?;

        // Generate Salt
        let salt = SaltString::generate(&mut OsRng);
        let key = Self::derive_key(&self.password, salt.as_str())?;

        // Encrypt
        let cipher = Aes256Gcm::new(&key.into());
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 96-bits; unique per message
        
        let ciphertext = cipher.encrypt(&nonce, plaintext.as_ref())
            .map_err(|e| Error::Keystore(format!("Encryption failed: {}", e)))?;

        let file_data = EncryptedFile {
            salt: salt.as_str().to_string(),
            nonce: nonce.to_vec(),
            ciphertext,
        };

        let json = serde_json::to_vec_pretty(&file_data)
            .map_err(|e| Error::Serialization(e.to_string()))?;
        
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        
        std::fs::write(&self.path, json)
            .map_err(|e| Error::Keystore(format!("Failed to write keystore: {}", e)))?;
        
        Ok(())
    }

    fn generate_id() -> KeyId {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("key_{:x}", timestamp)
    }
}

impl Keystore for JsonKeystore {
    fn generate_key(&self, label: Option<String>) -> Result<KeyInfo> {
        let secp = secp256k1::Secp256k1::new();
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

        {
            let mut keys = self.keys.write().unwrap();
            keys.insert(id.clone(), (info.clone(), secret_key.secret_bytes().to_vec()));
        }
        self.save()?;
        
        Ok(info)
    }

    fn import_key(&self, secret_bytes: &[u8], label: Option<String>) -> Result<KeyInfo> {
        let secp = secp256k1::Secp256k1::new();
        let secret_key = secp256k1::SecretKey::from_slice(secret_bytes)
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

        {
            let mut keys = self.keys.write().unwrap();
            keys.insert(id.clone(), (info.clone(), secret_bytes.to_vec()));
        }
        self.save()?;

        Ok(info)
    }

    fn get_key_info(&self, id: &KeyId) -> Result<Option<KeyInfo>> {
        let keys = self.keys.read().unwrap();
        Ok(keys.get(id).map(|(info, _)| info.clone()))
    }

    fn list_keys(&self) -> Result<Vec<KeyInfo>> {
        let keys = self.keys.read().unwrap();
        Ok(keys.values().map(|(info, _)| info.clone()).collect())
    }

    fn sign(&self, key_id: &KeyId, message: &[u8]) -> Result<Vec<u8>> {
        let keys = self.keys.read().unwrap();
        let (_, secret_bytes) = keys.get(key_id).ok_or_else(|| Error::KeyNotFound(key_id.clone()))?;

        let secp = secp256k1::Secp256k1::new();
        let secret_key = secp256k1::SecretKey::from_slice(secret_bytes)
            .map_err(|e| Error::Keystore(e.to_string()))?;

        // Hash message
        let msg_hash = if message.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(message);
            arr
        } else {
            use bitcoin::hashes::{sha256, Hash};
            sha256::Hash::hash(message).to_byte_array()
        };
        let msg = secp256k1::Message::from_digest(msg_hash);

        let signature = secp.sign_ecdsa(&msg, &secret_key);
        Ok(signature.serialize_compact().to_vec())
    }

    fn sign_schnorr(&self, key_id: &KeyId, message: &[u8]) -> Result<Vec<u8>> {
        let keys = self.keys.read().unwrap();
        let (_, secret_bytes) = keys.get(key_id).ok_or_else(|| Error::KeyNotFound(key_id.clone()))?;

        let secp = secp256k1::Secp256k1::new();
        let secret_key = secp256k1::SecretKey::from_slice(secret_bytes)
            .map_err(|e| Error::Keystore(e.to_string()))?;
        let keypair = secp256k1::Keypair::from_secret_key(&secp, &secret_key);

        let msg_hash = if message.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(message);
            arr
        } else {
            use bitcoin::hashes::{sha256, Hash};
            sha256::Hash::hash(message).to_byte_array()
        };
        let msg = secp256k1::Message::from_digest(msg_hash);

        let signature = secp.sign_schnorr_no_aux_rand(&msg, &keypair);
        Ok(signature.as_ref().to_vec())
    }

    fn delete_key(&self, id: &KeyId) -> Result<()> {
        let mut keys = self.keys.write().unwrap();
        keys.remove(id);
        // Explicitly Save
        drop(keys); // Drop lock before save
        self.save()
    }
}
