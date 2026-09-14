use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

use crate::migration::OtpAccount;

const NONCE_LEN: usize = 12;

fn derive_key(pin: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"authg_salt_v1_");
    hasher.update(pin.as_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

fn derive_legacy_key(pin: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"authdesk_salt_v1_");
    hasher.update(pin.as_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

pub fn save_vault_to_path(path: &PathBuf, pin: &str, accounts: &[OtpAccount]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create vault dir: {}", e))?;
    }

    let json_data = serde_json::to_vec(accounts).map_err(|e| e.to_string())?;
    let key = derive_key(pin);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, json_data.as_ref())
        .map_err(|e| format!("Encryption failure: {}", e))?;

    let mut final_payload = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    final_payload.extend_from_slice(&nonce_bytes);
    final_payload.extend_from_slice(&ciphertext);

    fs::write(path, final_payload).map_err(|e| format!("Failed to write vault file: {}", e))?;
    Ok(())
}

pub fn load_vault_from_path(path: &PathBuf, pin: &str) -> Result<Vec<OtpAccount>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let data = fs::read(path).map_err(|e| format!("Failed to read vault file: {}", e))?;
    if data.len() < NONCE_LEN {
        return Err("Corrupted vault file (too short)".to_string());
    }

    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    // Try standard authg key derivation first
    let key = derive_key(pin);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;

    let decrypted = match cipher.decrypt(nonce, ciphertext) {
        Ok(d) => d,
        Err(_) => {
            // Fallback to legacy authdesk salt if existing vault
            let legacy_key = derive_legacy_key(pin);
            let legacy_cipher = Aes256Gcm::new_from_slice(&legacy_key).map_err(|e| e.to_string())?;
            legacy_cipher
                .decrypt(nonce, ciphertext)
                .map_err(|_| "Incorrect PIN or passcode. Could not decrypt vault.".to_string())?
        }
    };

    let accounts: Vec<OtpAccount> =
        serde_json::from_slice(&decrypted).map_err(|e| format!("Vault deserialization error: {}", e))?;

    Ok(accounts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn test_vault_roundtrip() {
        let mut path = temp_dir();
        path.push("test_vault_authg.enc");

        let sample_accounts = vec![OtpAccount {
            id: "github-user".to_string(),
            name: "user".to_string(),
            issuer: "GitHub".to_string(),
            secret: "JBSWY3DPEHPK3PXP".to_string(),
            algorithm: "SHA1".to_string(),
            digits: 6,
            period: 30,
            otp_type: "TOTP".to_string(),
        }];

        save_vault_to_path(&path, "1234", &sample_accounts).unwrap();
        let loaded = load_vault_from_path(&path, "1234").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].issuer, "GitHub");

        // Wrong pin must fail
        assert!(load_vault_from_path(&path, "wrong_pin").is_err());

        let _ = fs::remove_file(path);
    }
}
