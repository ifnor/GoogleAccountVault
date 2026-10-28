use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AccountRecord {
    pub id: String,
    pub email: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub recovery_email: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub totp: String,
    #[serde(default)]
    pub verification: String,
    #[serde(default)]
    pub backup_codes: String,
    #[serde(default)]
    pub used_backup_codes: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub google_one_plan: String,
    #[serde(default)]
    pub google_one_used: String,
    #[serde(default)]
    pub google_one_status: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub deleted_at: Option<i64>,
    #[serde(flatten)]
    pub extra: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VaultSettings {
    pub lock_minutes: u32,
    pub clipboard_seconds: u32,
    pub backup_keep: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpResult {
    pub code: String,
    pub remaining_seconds: u64,
}
