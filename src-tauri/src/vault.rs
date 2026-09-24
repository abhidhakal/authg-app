use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

use crate::migration::OtpAccount;

pub const VAULT_MAGIC_V2: &[u8; 4] = b"AUG2";
pub const VAULT_MAGIC_V3: &[u8; 4] = b"AUG3";
pub const SALT_LEN: usize = 16;
pub const NONCE_LEN: usize = 12;
pub const PBKDF2_ROUNDS: u32 = 600_000;

/// Derives a 256-bit encryption key using PBKDF2-HMAC-SHA256 with 600,000 rounds and a random per-vault salt.
pub fn derive_key_pbkdf2(pin: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(pin.as_bytes(), salt, PBKDF2_ROUNDS, &mut key);
    key
}

/// Legacy single-round SHA256 key derivation for backward compatibility migration.
fn derive_legacy_key(pin: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(pin.as_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

fn write_private(path: &PathBuf, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create vault dir: {}", e))?;
    }
    fs::write(path, bytes).map_err(|e| format!("Failed to write vault file: {}", e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Returns true when the file on disk is an AUG3 vault, i.e. only readable with the Keychain key.
pub fn is_keychain_vault(path: &PathBuf) -> bool {
    fs::read(path).map(|d| d.starts_with(VAULT_MAGIC_V3)).unwrap_or(false)
}

/// AUG3: [AUG3: 4 bytes] + [Nonce: 12 bytes] + [AES-256-GCM Ciphertext + Tag].
/// The key is a random 256-bit key held in the OS keychain, so the file alone is useless.
pub fn save_vault_v3(path: &PathBuf, key: &[u8; 32], accounts: &[OtpAccount]) -> Result<(), String> {
    let json_data = serde_json::to_vec(accounts).map_err(|e| e.to_string())?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), json_data.as_ref())
        .map_err(|e| format!("Encryption failure: {}", e))?;

    let mut payload = Vec::with_capacity(4 + NONCE_LEN + ciphertext.len());
    payload.extend_from_slice(VAULT_MAGIC_V3);
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);
    write_private(path, &payload)
}

/// Loads any vault version. AUG3 needs `key`; older (PIN-derived) vaults are re-saved as AUG3 when a key is given.
pub fn load_vault(path: &PathBuf, key: Option<&[u8; 32]>, pin: &str) -> Result<Vec<OtpAccount>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read(path).map_err(|e| format!("Failed to read vault file: {}", e))?;
    if data.starts_with(VAULT_MAGIC_V3) {
        let key = key.ok_or("Vault is locked to the Keychain, but no Keychain key is available")?;
        if data.len() < 4 + NONCE_LEN + 16 {
            return Err("Corrupted vault file (incomplete envelope)".to_string());
        }
        let (nonce_bytes, ciphertext) = data[4..].split_at(NONCE_LEN);
        let decrypted = Aes256Gcm::new_from_slice(key)
            .map_err(|e| e.to_string())?
            .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
            .map_err(|_| "Could not decrypt vault with the Keychain key".to_string())?;
        return serde_json::from_slice(&decrypted).map_err(|e| format!("Vault deserialization error: {}", e));
    }
    let accounts = load_vault_from_path(path, pin)?;
    if let Some(key) = key {
        save_vault_v3(path, key, &accounts)?;
    }
    Ok(accounts)
}

/// Saves accounts into a cryptographically secured envelope on disk:
/// [AUG2: 4 bytes] + [Salt: 16 bytes] + [Nonce: 12 bytes] + [AES-256-GCM Ciphertext + Tag]
pub fn save_vault_to_path(path: &PathBuf, pin: &str, accounts: &[OtpAccount]) -> Result<(), String> {
    let json_data = serde_json::to_vec(accounts).map_err(|e| e.to_string())?;

    let mut salt = [0u8; SALT_LEN];
    rand::thread_rng().fill_bytes(&mut salt);

    let key = derive_key_pbkdf2(pin, &salt);
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, json_data.as_ref())
        .map_err(|e| format!("Encryption failure: {}", e))?;

    let mut final_payload = Vec::with_capacity(4 + SALT_LEN + NONCE_LEN + ciphertext.len());
    final_payload.extend_from_slice(VAULT_MAGIC_V2);
    final_payload.extend_from_slice(&salt);
    final_payload.extend_from_slice(&nonce_bytes);
    final_payload.extend_from_slice(&ciphertext);

    write_private(path, &final_payload)
}

/// Loads and decrypts accounts from the vault envelope.
/// Supports both AUG2 (PBKDF2 600,000 rounds) and legacy v1 vaults (auto-upgrading to AUG2 upon successful decryption).
pub fn load_vault_from_path(path: &PathBuf, pin: &str) -> Result<Vec<OtpAccount>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let data = fs::read(path).map_err(|e| format!("Failed to read vault file: {}", e))?;

    // Check for AUG2 Magic Envelope
    if data.starts_with(VAULT_MAGIC_V2) {
        let min_len = 4 + SALT_LEN + NONCE_LEN + 16; // 16 bytes is AES-GCM tag
        if data.len() < min_len {
            return Err("Corrupted vault file (incomplete envelope)".to_string());
        }

        let salt = &data[4..4 + SALT_LEN];
        let nonce_bytes = &data[4 + SALT_LEN..4 + SALT_LEN + NONCE_LEN];
        let ciphertext = &data[4 + SALT_LEN + NONCE_LEN..];

        let key = derive_key_pbkdf2(pin, salt);
        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
        let nonce = Nonce::from_slice(nonce_bytes);

        let decrypted = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| "Incorrect PIN or passcode. Could not decrypt vault.".to_string())?;

        let accounts: Vec<OtpAccount> =
            serde_json::from_slice(&decrypted).map_err(|e| format!("Vault deserialization error: {}", e))?;

        return Ok(accounts);
    }

    // Fallback: Legacy v1 unversioned vault format (nonce 12 bytes + ciphertext)
    if data.len() < NONCE_LEN + 16 {
        return Err("Corrupted vault file (too short)".to_string());
    }

    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    // Try legacy authg salt first
    let legacy_authg_key = derive_legacy_key(pin, b"authg_salt_v1_");
    let decrypted = match Aes256Gcm::new_from_slice(&legacy_authg_key)
        .map_err(|e| e.to_string())?
        .decrypt(nonce, ciphertext)
    {
        Ok(d) => d,
        Err(_) => {
            // Try legacy authdesk salt
            let legacy_authdesk_key = derive_legacy_key(pin, b"authdesk_salt_v1_");
            let cipher = Aes256Gcm::new_from_slice(&legacy_authdesk_key).map_err(|e| e.to_string())?;
            cipher
                .decrypt(nonce, ciphertext)
                .map_err(|_| "Incorrect PIN or passcode. Could not decrypt vault.".to_string())?
        }
    };

    let accounts: Vec<OtpAccount> =
        serde_json::from_slice(&decrypted).map_err(|e| format!("Vault deserialization error: {}", e))?;

    // Transparently upgrade legacy vault to AUG2 format on disk
    let _ = save_vault_to_path(path, pin, &accounts);

    Ok(accounts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn test_vault_roundtrip_pbkdf2() {
        let mut path = temp_dir();
        path.push("test_vault_authg_v2.enc");

        let sample_accounts = vec![OtpAccount {
            id: "github-user".to_string(),
            name: "user".to_string(),
            issuer: "GitHub".to_string(),
            secret: "JBSWY3DPEHPK3PXP".to_string(),
            algorithm: "SHA1".to_string(),
            digits: 6,
            period: 30,
            otp_type: "TOTP".to_string(),
            last_used: None,
        }];

        save_vault_to_path(&path, "1234", &sample_accounts).unwrap();
        let loaded = load_vault_from_path(&path, "1234").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].issuer, "GitHub");

        // Verify magic header presence
        let raw = fs::read(&path).unwrap();
        assert!(raw.starts_with(VAULT_MAGIC_V2));

        // Wrong pin must fail cryptographically
        assert!(load_vault_from_path(&path, "9999").is_err());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_vault_v3_keychain_key() {
        let mut path = temp_dir();
        path.push("test_vault_authg_v3.enc");
        let key = [7u8; 32];
        let js: Vec<OtpAccount> = serde_json::from_str(
            r#"[{"id":"a","name":"n","issuer":"GitHub","secret":"JBSWY3DPEHPK3PXP","algorithm":"SHA1","digits":6,"period":30,"otpType":"TOTP","lastUsed":5}]"#,
        )
        .expect("webview camelCase shape must deserialize");

        // An old AUG2 (empty PIN) vault upgrades to AUG3 on load
        save_vault_to_path(&path, "", &js).unwrap();
        let loaded = load_vault(&path, Some(&key), "").unwrap();
        assert_eq!(loaded[0].last_used, Some(5));
        assert!(fs::read(&path).unwrap().starts_with(VAULT_MAGIC_V3));
        assert!(is_keychain_vault(&path));

        // AUG3 reads back with the key, and refuses without it or with the wrong one
        assert_eq!(load_vault(&path, Some(&key), "").unwrap()[0].issuer, "GitHub");
        assert!(load_vault(&path, None, "").is_err());
        assert!(load_vault(&path, Some(&[8u8; 32]), "").is_err());

        // Serializes back as camelCase for the webview
        assert!(serde_json::to_string(&loaded).unwrap().contains("\"otpType\""));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_legacy_vault_migration() {
        let mut path = temp_dir();
        path.push("test_vault_legacy_migration.enc");

        let sample_accounts = vec![OtpAccount {
            id: "gitlab-user".to_string(),
            name: "dev".to_string(),
            issuer: "GitLab".to_string(),
            secret: "JBSWY3DPEHPK3PXP".to_string(),
            algorithm: "SHA1".to_string(),
            digits: 6,
            period: 30,
            otp_type: "TOTP".to_string(),
            last_used: None,
        }];

        // Create a simulated legacy v1 file
        let json_data = serde_json::to_vec(&sample_accounts).unwrap();
        let legacy_key = derive_legacy_key("5678", b"authg_salt_v1_");
        let cipher = Aes256Gcm::new_from_slice(&legacy_key).unwrap();
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher.encrypt(nonce, json_data.as_ref()).unwrap();

        let mut legacy_payload = Vec::new();
        legacy_payload.extend_from_slice(&nonce_bytes);
        legacy_payload.extend_from_slice(&ciphertext);
        fs::write(&path, legacy_payload).unwrap();

        // Load with legacy PIN -> should decrypt and auto-upgrade to AUG2
        let loaded = load_vault_from_path(&path, "5678").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].issuer, "GitLab");

        // Verify file is now upgraded with AUG2 header
        let raw = fs::read(&path).unwrap();
        assert!(raw.starts_with(VAULT_MAGIC_V2));

        // Now loading with the new format works
        let reloaded = load_vault_from_path(&path, "5678").unwrap();
        assert_eq!(reloaded[0].issuer, "GitLab");

        let _ = fs::remove_file(path);
    }
}
