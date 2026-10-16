use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use base64::Engine;
use chrono::Local;
use rand::RngCore;
use scrypt::{Params, scrypt};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::models::AccountRecord;

pub const AAD: &[u8] = b"GoogleAccountVault:1";

#[derive(Serialize, Deserialize, Debug)]
pub struct VaultEnvelope {
    pub version: u32,
    pub salt: String,
    pub nonce: String,
    pub data: String,
}

pub fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    // Python vault.py uses: Scrypt(salt=salt, length=32, n=2**15, r=8, p=1)
    // 2**15 = 32768 => log_n = 15
    let params = Params::new(15, 8, 1, 32).map_err(|e| format!("Scrypt 参数错误: {e}"))?;
    let mut key = [0u8; 32];
    scrypt(password.as_bytes(), salt, &params, &mut key)
        .map_err(|e| format!("Scrypt 派生密钥失败: {e}"))?;
    Ok(key)
}

pub fn encrypt_records(records: &[AccountRecord], password: &str) -> Result<String, String> {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let key = derive_key(password, &salt)?;

    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);

    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let raw = serde_json::to_vec(records).map_err(|e| e.to_string())?;

    let payload = Payload {
        msg: &raw,
        aad: AAD,
    };
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), payload)
        .map_err(|e| format!("AES-GCM 加密失败: {e}"))?;

    let envelope = VaultEnvelope {
        version: 1,
        salt: hex::encode(salt),
        nonce: hex::encode(nonce),
        data: base64::engine::general_purpose::STANDARD.encode(ciphertext),
    };

    serde_json::to_string_pretty(&envelope).map_err(|e| e.to_string())
}

pub fn encrypt_records_with_key(
    records: &[AccountRecord],
    key: &[u8; 32],
    salt: &[u8; 16],
) -> Result<String, String> {
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let raw = serde_json::to_vec(records).map_err(|e| e.to_string())?;

    let payload = Payload {
        msg: &raw,
        aad: AAD,
    };
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), payload)
        .map_err(|e| format!("AES-GCM 加密失败: {e}"))?;

    let envelope = VaultEnvelope {
        version: 1,
        salt: hex::encode(salt),
        nonce: hex::encode(nonce),
        data: base64::engine::general_purpose::STANDARD.encode(ciphertext),
    };

    serde_json::to_string_pretty(&envelope).map_err(|e| e.to_string())
}

pub fn decrypt_records(
    blob: &str,
    password: &str,
) -> Result<(Vec<AccountRecord>, [u8; 32], [u8; 16]), String> {
    let envelope: VaultEnvelope =
        serde_json::from_str(blob).map_err(|e| format!("解析保险库结构失败: {e}"))?;
    if envelope.version != 1 {
        return Err("不支持的保险库文件版本".to_string());
    }
    let salt_vec = hex::decode(&envelope.salt).map_err(|e| format!("Salt 解码失败: {e}"))?;
    if salt_vec.len() != 16 {
        return Err("保险库 Salt 长度不正确".to_string());
    }
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&salt_vec);

    let key = derive_key(password, &salt)?;
    let nonce = hex::decode(&envelope.nonce).map_err(|e| format!("Nonce 解码失败: {e}"))?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(&envelope.data)
        .map_err(|e| format!("Ciphertext Base64 解码失败: {e}"))?;

    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let payload = Payload {
        msg: &data,
        aad: AAD,
    };
    let decrypted = cipher
        .decrypt(Nonce::from_slice(&nonce), payload)
        .map_err(|_| "主密码错误，或文件已被篡改/损坏".to_string())?;

    let records: Vec<AccountRecord> =
        serde_json::from_slice(&decrypted).map_err(|e| format!("账号数据反序列化失败: {e}"))?;
    Ok((records, key, salt))
}

static VAULT_DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn init_vault_data_dir(path: PathBuf) {
    let _ = VAULT_DATA_DIR.set(path);
}

pub fn vault_data_dir() -> PathBuf {
    if let Some(dir) = VAULT_DATA_DIR.get() {
        return dir.clone();
    }

    #[cfg(target_os = "windows")]
    if let Some(local_app_data) = dirs::data_local_dir() {
        return local_app_data.join("GoogleAccountVault");
    }

    PathBuf::from(".")
}

pub fn vault_file_path() -> PathBuf {
    vault_data_dir().join("accounts.gvault")
}

pub fn save_vault_atomic_with_key(
    records: &[AccountRecord],
    key: &[u8; 32],
    salt: &[u8; 16],
    keep_backups: usize,
) -> Result<(), String> {
    let dir = vault_data_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败: {e}"))?;

    let main_path = vault_file_path();
    // Snapshot old file if exists
    if main_path.exists() {
        let backup_dir = dir.join("backups");
        let _ = fs::create_dir_all(&backup_dir);
        let timestamp = Local::now().format("%Y%m%d-%H%M%S").to_string();
        let backup_name = format!(
            "{}-{}.gvault",
            timestamp,
            &uuid::Uuid::new_v4().to_string()[..6]
        );
        let backup_file = backup_dir.join(backup_name);
        let _ = fs::copy(&main_path, &backup_file);

        // Prune old backups
        if let Ok(entries) = fs::read_dir(&backup_dir) {
            let mut files: Vec<PathBuf> = entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().map_or(false, |ext| ext == "gvault"))
                .collect();
            files.sort();
            files.reverse();
            let limit = keep_backups.max(1);
            if files.len() > limit {
                for old in &files[limit..] {
                    let _ = fs::remove_file(old);
                }
            }
        }
    }

    let encrypted_content = encrypt_records_with_key(records, key, salt)?;
    let temp_path = dir.join(format!(".tmp-{}", uuid::Uuid::new_v4()));
    fs::write(&temp_path, encrypted_content.as_bytes())
        .map_err(|e| format!("写入临时文件失败: {e}"))?;
    fs::rename(&temp_path, &main_path).map_err(|e| format!("更新主库文件失败: {e}"))?;
    Ok(())
}

pub fn save_vault_atomic(
    records: &[AccountRecord],
    password: &str,
    keep_backups: usize,
) -> Result<(), String> {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let key = derive_key(password, &salt)?;
    save_vault_atomic_with_key(records, &key, &salt, keep_backups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_roundtrip() {
        let sample = vec![AccountRecord {
            id: "test-uuid-1".to_string(),
            email: "alice@gmail.com".to_string(),
            password: "SecurePassword123!".to_string(),
            group: "工作".to_string(),
            google_one_status: "active".to_string(),
            google_one_plan: "2 TB".to_string(),
            ..Default::default()
        }];
        let password = "MyMasterPassword@2026";
        let blob = encrypt_records(&sample, password).expect("Encrypt failed");
        let (decrypted, key, salt) = decrypt_records(&blob, password).expect("Decrypt failed");
        assert_eq!(decrypted.len(), 1);
        assert_eq!(decrypted[0].email, "alice@gmail.com");
        assert_eq!(decrypted[0].password, "SecurePassword123!");
        assert_eq!(decrypted[0].google_one_status, "active");

        // Fast key encryption test
        let blob_fast = encrypt_records_with_key(&decrypted, &key, &salt).expect("Fast encrypt failed");
        let (decrypted_fast, _, _) = decrypt_records(&blob_fast, password).expect("Fast decrypt failed");
        assert_eq!(decrypted_fast.len(), 1);

        // Wrong password test
        let wrong = decrypt_records(&blob, "WrongPassword");
        assert!(wrong.is_err());
    }
}
