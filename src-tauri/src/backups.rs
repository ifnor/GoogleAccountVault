use serde::{Deserialize, Serialize};
use std::fs;
use crate::models::AccountRecord;
use crate::vault;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BackupFileInfo {
    pub filename: String,
    pub timestamp: String,
    pub size_bytes: u64,
}

pub fn list_backups() -> Result<Vec<BackupFileInfo>, String> {
    let backup_dir = vault::vault_data_dir().join("backups");
    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    let entries = fs::read_dir(&backup_dir).map_err(|e| format!("读取备份目录失败: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "gvault") {
            if let Ok(metadata) = entry.metadata() {
                let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let timestamp = if let Ok(time) = metadata.modified() {
                    let dt: chrono::DateTime<chrono::Local> = time.into();
                    dt.format("%Y-%m-%d %H:%M:%S").to_string()
                } else {
                    "—".to_string()
                };

                result.push(BackupFileInfo {
                    filename,
                    timestamp,
                    size_bytes: metadata.len(),
                });
            }
        }
    }

    // Sort newest first
    result.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(result)
}

pub fn restore_backup(filename: &str, master_password: &str) -> Result<Vec<AccountRecord>, String> {
    let backup_path = vault::vault_data_dir().join("backups").join(filename);
    if !backup_path.exists() {
        return Err(format!("备份文件不存在: {filename}"));
    }

    let blob = fs::read_to_string(&backup_path).map_err(|e| format!("读取备份失败: {e}"))?;
    let records = vault::decrypt_records(&blob, master_password)?;

    // Save as current vault
    vault::save_vault_atomic(&records, master_password, 30)?;
    Ok(records)
}

pub fn export_records_csv(records: &[AccountRecord]) -> String {
    let mut csv = String::from("邮箱,登录密码,分组,恢复邮箱,恢复手机,TOTP密钥,Google One状态,Google One方案,备用验证码,已用备用码,备注\n");
    for r in records {
        let line = format!(
            "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\"\n",
            r.email.replace('\"', "\"\""),
            r.password.replace('\"', "\"\""),
            r.group.replace('\"', "\"\""),
            r.recovery_email.replace('\"', "\"\""),
            r.phone.replace('\"', "\"\""),
            r.totp.replace('\"', "\"\""),
            r.google_one_status.replace('\"', "\"\""),
            r.google_one_plan.replace('\"', "\"\""),
            r.backup_codes.replace('\"', "\"\""),
            r.used_backup_codes.replace('\"', "\"\""),
            r.notes.replace('\"', "\"\"")
        );
        csv.push_str(&line);
    }
    csv
}
