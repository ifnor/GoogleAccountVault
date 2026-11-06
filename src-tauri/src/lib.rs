pub mod backups;
pub mod hello;
pub mod models;
pub mod totp;
pub mod utils;
pub mod vault;
pub mod webdav;

use models::{AccountRecord, TotpQrResult, TotpResult};
use parking_lot::RwLock;
use std::fs;
use tauri::State;

pub struct AppState {
    pub master_password: RwLock<Option<String>>,
    pub vault_key: RwLock<Option<([u8; 32], [u8; 16])>>,
    pub records: RwLock<Vec<AccountRecord>>,
}

#[tauri::command]
fn check_vault_exists() -> bool {
    vault::vault_file_path().exists()
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct HelloStatus {
    pub available: bool,
    pub enrolled: bool,
}

#[tauri::command]
async fn check_windows_hello_status() -> Result<HelloStatus, String> {
    let available = hello::check_hello_availability().await.unwrap_or(false);
    let enrolled = hello::is_hello_enrolled();
    Ok(HelloStatus {
        available,
        enrolled,
    })
}

#[tauri::command]
async fn enable_windows_hello(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let password = {
        let guard = state.master_password.read();
        guard
            .as_ref()
            .ok_or("请先解锁保险库后再启用 Windows Hello")?
            .clone()
    };
    let _ = window.show();
    let _ = window.set_focus();
    #[cfg(target_os = "windows")]
    let raw_hwnd = window.hwnd().ok().map(|h| h.0 as isize);
    #[cfg(not(target_os = "windows"))]
    let raw_hwnd: Option<isize> = None;
    let verified =
        hello::verify_user_consent("验证身份以启用 Windows Hello / PIN 快速解锁", raw_hwnd).await?;
    if !verified {
        return Err("身份验证未通过或已取消".to_string());
    }
    hello::save_hello_credentials(&password)?;
    Ok(())
}

#[tauri::command]
fn disable_windows_hello() -> Result<(), String> {
    hello::disable_hello()
}

#[tauri::command]
async fn unlock_vault_with_hello(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Vec<AccountRecord>, String> {
    let _ = window.show();
    let _ = window.set_focus();
    #[cfg(target_os = "windows")]
    let raw_hwnd = window.hwnd().ok().map(|h| h.0 as isize);
    #[cfg(not(target_os = "windows"))]
    let raw_hwnd: Option<isize> = None;
    let verified = hello::verify_user_consent(
        "验证 Windows Hello / PIN 身份以解锁 Google 账号管家",
        raw_hwnd,
    )
    .await?;
    if !verified {
        return Err("身份验证未通过或已取消".to_string());
    }
    let password = hello::get_hello_credentials()?;
    let path = vault::vault_file_path();
    if !path.exists() {
        return Err("保险库文件不存在，请先创建保险库".to_string());
    }
    let blob = fs::read_to_string(&path).map_err(|e| format!("读取保险库文件失败: {e}"))?;
    let (records, key, salt) = vault::decrypt_records(&blob, &password)?;

    *state.master_password.write() = Some(password);
    *state.vault_key.write() = Some((key, salt));
    *state.records.write() = records.clone();
    Ok(records.into_iter().filter(|r| r.deleted_at.is_none()).collect())
}

#[tauri::command]
fn unlock_vault(
    password: String,
    state: State<'_, AppState>,
) -> Result<Vec<AccountRecord>, String> {
    let path = vault::vault_file_path();
    if !path.exists() {
        return Err("保险库文件不存在，请先创建保险库".to_string());
    }
    let blob = fs::read_to_string(&path).map_err(|e| format!("读取保险库文件失败: {e}"))?;
    let (records, key, salt) = vault::decrypt_records(&blob, &password)?;

    *state.master_password.write() = Some(password);
    *state.vault_key.write() = Some((key, salt));
    *state.records.write() = records.clone();
    Ok(records.into_iter().filter(|r| r.deleted_at.is_none()).collect())
}

#[tauri::command]
fn create_vault(password: String, state: State<'_, AppState>) -> Result<(), String> {
    if password.trim().is_empty() {
        return Err("主密码不能为空".to_string());
    }
    let mut salt = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
    let key = vault::derive_key(&password, &salt)?;
    let records: Vec<AccountRecord> = Vec::new();
    vault::save_vault_atomic_with_key(&records, &key, &salt, 30)?;
    *state.master_password.write() = Some(password);
    *state.vault_key.write() = Some((key, salt));
    *state.records.write() = records;
    Ok(())
}

#[tauri::command]
fn lock_vault(state: State<'_, AppState>) -> Result<(), String> {
    *state.master_password.write() = None;
    *state.vault_key.write() = None;
    state.records.write().clear();
    Ok(())
}

#[tauri::command]
fn get_records(state: State<'_, AppState>) -> Result<Vec<AccountRecord>, String> {
    if state.master_password.read().is_none() {
        return Err("保险库已锁定".to_string());
    }
    let all = state.records.read();
    Ok(all.iter().filter(|r| r.deleted_at.is_none()).cloned().collect())
}

#[tauri::command]
fn save_record(
    mut record: AccountRecord,
    state: State<'_, AppState>,
) -> Result<AccountRecord, String> {
    let password_lock = state.master_password.read();
    let password = password_lock.as_ref().ok_or("保险库已锁定，无法保存")?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    if record.id.trim().is_empty() {
        record.id = uuid::Uuid::new_v4().to_string();
    }
    record.email = record.email.trim().to_string();
    record.group = record.group.trim().to_string();
    if record.status.trim().is_empty() {
        record.status = "待录入".to_string();
    }
    if record.antigravity_status.trim().is_empty() {
        record.antigravity_status = "待验证".to_string();
    }
    if record.google_one_plan.trim().is_empty() {
        record.google_one_plan = "Free".to_string();
    }
    record.updated_at = now;
    record.deleted_at = None;

    let mut records = state.records.write();
    if let Some(pos) = records.iter().position(|r| r.id == record.id) {
        if record.created_at == 0 {
            record.created_at = if records[pos].created_at != 0 {
                records[pos].created_at
            } else if records[pos].updated_at != 0 {
                records[pos].updated_at
            } else {
                now
            };
        }
        records[pos] = record.clone();
    } else {
        if record.created_at == 0 {
            record.created_at = now;
        }
        records.push(record.clone());
    }

    if let Some((key, salt)) = state.vault_key.read().as_ref() {
        vault::save_vault_atomic_with_key(&records, key, salt, 30)?;
    } else {
        vault::save_vault_atomic(&records, password, 30)?;
    }
    Ok(record)
}

#[tauri::command]
fn delete_record(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let password_lock = state.master_password.read();
    let password = password_lock.as_ref().ok_or("保险库已锁定，无法删除")?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let mut records = state.records.write();
    if let Some(pos) = records.iter().position(|r| r.id == id) {
        records[pos].deleted_at = Some(now);
        records[pos].updated_at = now;

        if let Some((key, salt)) = state.vault_key.read().as_ref() {
            vault::save_vault_atomic_with_key(&records, key, salt, 30)?;
        } else {
            vault::save_vault_atomic(&records, password, 30)?;
        }
    }
    Ok(())
}

#[tauri::command]
fn batch_update_records(
    ids: Vec<String>,
    group: Option<String>,
    status: Option<String>,
    antigravity_status: Option<String>,
    google_one_status: Option<String>,
    google_one_plan: Option<String>,
    is_pinned: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<AccountRecord>, String> {
    let password_lock = state.master_password.read();
    let password = password_lock.as_ref().ok_or("保险库已锁定，无法批量修改")?;

    if ids.is_empty() {
        let all = state.records.read();
        return Ok(all.iter().filter(|r| r.deleted_at.is_none()).cloned().collect());
    }

    let id_set: std::collections::HashSet<String> = ids.into_iter().collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let mut records = state.records.write();
    let mut changed = false;

    for r in records.iter_mut() {
        if r.deleted_at.is_none() && id_set.contains(&r.id) {
            if let Some(ref g) = group {
                r.group = g.trim().to_string();
            }
            if let Some(ref st) = status {
                r.status = st.trim().to_string();
            }
            if let Some(ref ags) = antigravity_status {
                r.antigravity_status = ags.trim().to_string();
            }
            if let Some(ref gos) = google_one_status {
                r.google_one_status = gos.trim().to_string();
            }
            if let Some(ref gop) = google_one_plan {
                r.google_one_plan = gop.trim().to_string();
            }
            if let Some(pin) = is_pinned {
                r.is_pinned = pin;
            }
            r.updated_at = now;
            changed = true;
        }
    }

    if changed {
        if let Some((key, salt)) = state.vault_key.read().as_ref() {
            vault::save_vault_atomic_with_key(&records, key, salt, 30)?;
        } else {
            vault::save_vault_atomic(&records, password, 30)?;
        }
    }

    Ok(records
        .iter()
        .filter(|r| r.deleted_at.is_none())
        .cloned()
        .collect())
}

#[tauri::command]
fn batch_delete_records(
    ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<Vec<AccountRecord>, String> {
    let password_lock = state.master_password.read();
    let password = password_lock.as_ref().ok_or("保险库已锁定，无法批量删除")?;

    let id_set: std::collections::HashSet<String> = ids.into_iter().collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let mut records = state.records.write();
    let mut changed = false;

    for r in records.iter_mut() {
        if r.deleted_at.is_none() && id_set.contains(&r.id) {
            r.deleted_at = Some(now);
            r.updated_at = now;
            changed = true;
        }
    }

    if changed {
        if let Some((key, salt)) = state.vault_key.read().as_ref() {
            vault::save_vault_atomic_with_key(&records, key, salt, 30)?;
        } else {
            vault::save_vault_atomic(&records, password, 30)?;
        }
    }

    Ok(records
        .iter()
        .filter(|r| r.deleted_at.is_none())
        .cloned()
        .collect())
}

#[tauri::command]
fn change_master_password(
    old_password: String,
    new_password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let current_pwd = {
        let guard = state.master_password.read();
        guard
            .as_ref()
            .ok_or("保险库已锁定，请先解锁")?
            .clone()
    };

    if old_password != current_pwd {
        return Err("原主密码输入不正确".to_string());
    }
    if new_password.trim().is_empty() {
        return Err("新主密码不能为空".to_string());
    }
    if new_password.len() < 4 {
        return Err("新主密码长度建议至少 4 位".to_string());
    }

    let mut salt = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
    let key = vault::derive_key(&new_password, &salt)?;

    let records = state.records.read().clone();
    vault::save_vault_atomic_with_key(&records, &key, &salt, 30)?;

    *state.master_password.write() = Some(new_password.clone());
    *state.vault_key.write() = Some((key, salt));

    // 如果已启用 Windows Hello，同步更新 DPAPI 绑定的主密码
    if hello::is_hello_enrolled() {
        let _ = hello::save_hello_credentials(&new_password);
    }

    Ok(())
}

#[tauri::command]
fn get_totp(secret: String) -> Result<TotpResult, String> {
    totp::calculate_totp(&secret)
}

#[tauri::command]
fn get_totp_qr(secret: String, email: String) -> Result<TotpQrResult, String> {
    totp::generate_totp_qr(&secret, &email)
}

#[tauri::command]
fn parse_totp_input(raw: String) -> Result<String, String> {
    totp::normalize_totp_secret(&raw)
}

#[tauri::command]
fn generate_pwd(length: usize, symbols: bool) -> Result<String, String> {
    utils::generate_password(length, symbols)
}

#[tauri::command]
fn generate_custom_pwd(options: utils::PasswordGenOptions) -> Result<String, String> {
    utils::generate_custom_password(options)
}

#[tauri::command]
async fn test_webdav_connection(config: webdav::WebDavConfig) -> Result<String, String> {
    webdav::test_connection(&config).await
}

#[tauri::command]
fn save_webdav_config(config: webdav::WebDavConfig) -> Result<(), String> {
    webdav::save_config(&config)
}

#[tauri::command]
fn load_webdav_config() -> Result<Option<webdav::WebDavConfig>, String> {
    webdav::load_config()
}

#[tauri::command]
async fn upload_webdav_vault(
    config: webdav::WebDavConfig,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if state.master_password.read().is_none() {
        return Err("保险库已锁定，无法上传".to_string());
    }
    let _ = webdav::upload_vault(&config).await?;
    Ok(())
}

#[tauri::command]
async fn sync_webdav_smart(
    state: State<'_, AppState>,
) -> Result<webdav::SmartSyncResult, String> {
    let password = {
        let password_lock = state.master_password.read();
        password_lock
            .as_ref()
            .cloned()
            .ok_or("请先解锁保险库，以便同步数据")?
    };

    let mut config = match webdav::load_config()? {
        Some(c) => c,
        None => {
            return Ok(webdav::SmartSyncResult {
                synced: false,
                message: "未配置 WebDAV 云端同步".to_string(),
                records_updated: false,
                records_count: 0,
                last_sync_time: 0,
            });
        }
    };

    let current_records = state.records.read().clone();
    let vault_key_copy = state.vault_key.read().clone();

    let (res, new_records_opt) = webdav::smart_sync_vault(
        &mut config,
        &current_records,
        &password,
        vault_key_copy.as_ref(),
    )
    .await?;

    if let Some(new_records) = new_records_opt {
        *state.records.write() = new_records;
    }

    Ok(res)
}

#[tauri::command]
async fn download_webdav_vault(
    config: webdav::WebDavConfig,
    state: State<'_, AppState>,
) -> Result<Vec<AccountRecord>, String> {
    let password = {
        let password_lock = state.master_password.read();
        password_lock
            .as_ref()
            .cloned()
            .ok_or("请先解锁保险库，以便验证并解密云端数据")?
    };
    let records = webdav::download_vault(&config, &password).await?;
    *state.records.write() = records.clone();
    Ok(records)
}

#[tauri::command]
fn list_vault_backups() -> Result<Vec<backups::BackupFileInfo>, String> {
    backups::list_backups()
}

#[tauri::command]
fn create_manual_vault_backup(state: State<'_, AppState>) -> Result<backups::BackupFileInfo, String> {
    if state.master_password.read().is_none() {
        return Err("请先解锁保险库".to_string());
    }
    backups::create_snapshot_now()
}

#[tauri::command]
fn restore_vault_backup(
    filename: String,
    state: State<'_, AppState>,
) -> Result<Vec<AccountRecord>, String> {
    let password_lock = state.master_password.read();
    let password = password_lock
        .as_ref()
        .ok_or("请先解锁保险库，以便解密历史快照")?;
    let records = backups::restore_backup(&filename, password)?;
    *state.records.write() = records.clone();
    Ok(records)
}

#[tauri::command]
fn export_records_csv_string(state: State<'_, AppState>) -> Result<String, String> {
    if state.master_password.read().is_none() {
        return Err("保险库已锁定，无法导出".to_string());
    }
    let records = state.records.read();
    Ok(backups::export_records_csv(&records))
}

#[tauri::command]
fn export_encrypted_vault_string(state: State<'_, AppState>) -> Result<String, String> {
    let password_lock = state.master_password.read();
    let password = password_lock.as_ref().ok_or("保险库已锁定，无法导出密文文件")?;

    let records = state.records.read();
    if let Some((key, salt)) = state.vault_key.read().as_ref() {
        vault::encrypt_records_with_key(&records, key, salt)
    } else {
        vault::encrypt_records(&records, password)
    }
}

#[tauri::command]
fn import_encrypted_vault_blob(
    blob: String,
    password_override: Option<String>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let password_lock = state.master_password.read();
    let current_password = password_lock
        .as_ref()
        .ok_or("保险库已锁定，请先解锁后再导入")?;
    let pwd = password_override.as_deref().unwrap_or(current_password);

    let (imported, _, _) = vault::decrypt_records(&blob, pwd)?;
    let count = imported.len();
    if count == 0 {
        return Ok(0);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let mut records = state.records.write();
    for mut r in imported {
        r.updated_at = now;
        r.deleted_at = None;
        if let Some(pos) = records
            .iter()
            .position(|existing| existing.email.eq_ignore_ascii_case(&r.email))
        {
            if r.created_at == 0 {
                r.created_at = if records[pos].created_at != 0 {
                    records[pos].created_at
                } else {
                    now
                };
            }
            records[pos] = r;
        } else {
            if r.created_at == 0 {
                r.created_at = now;
            }
            records.push(r);
        }
    }

    if let Some((key, salt)) = state.vault_key.read().as_ref() {
        vault::save_vault_atomic_with_key(&records, key, salt, 30)?;
    } else {
        vault::save_vault_atomic(&records, current_password, 30)?;
    }

    Ok(count)
}

#[tauri::command]
fn batch_import_records(
    new_records: Vec<AccountRecord>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let password_lock = state.master_password.read();
    let password = password_lock.as_ref().ok_or("保险库已锁定，无法导入")?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let mut records = state.records.write();
    let mut imported_count = 0;

    for mut r in new_records {
        let clean_email = r.email.trim().to_string();
        if clean_email.is_empty() {
            continue;
        }
        r.email = clean_email;
        if r.id.trim().is_empty() {
            r.id = uuid::Uuid::new_v4().to_string();
        }
        r.updated_at = now;
        r.deleted_at = None;
        if let Some(pos) = records
            .iter()
            .position(|existing| existing.email.eq_ignore_ascii_case(&r.email))
        {
            let existing = &mut records[pos];
            existing.updated_at = now;
            existing.deleted_at = None;
            if existing.created_at == 0 {
                existing.created_at = now;
            }
            if !r.password.trim().is_empty() {
                existing.password = r.password;
            }
            if !r.group.trim().is_empty() {
                existing.group = r.group;
            }
            if !r.recovery_email.trim().is_empty() {
                existing.recovery_email = r.recovery_email;
            }
            if !r.phone.trim().is_empty() {
                existing.phone = r.phone;
            }
            if !r.totp.trim().is_empty() {
                existing.totp = r.totp;
            }
            if !r.backup_codes.trim().is_empty() {
                existing.backup_codes = r.backup_codes;
            }
            if !r.notes.trim().is_empty() {
                existing.notes = r.notes;
            }
            if !r.status.trim().is_empty() {
                existing.status = r.status;
            }
            if !r.antigravity_status.trim().is_empty() {
                existing.antigravity_status = r.antigravity_status;
            }
            if !r.google_one_plan.trim().is_empty() {
                existing.google_one_plan = r.google_one_plan;
            }
            if !r.google_one_status.trim().is_empty() {
                existing.google_one_status = r.google_one_status;
            }
            if !r.expire_at.trim().is_empty() {
                existing.expire_at = r.expire_at;
            }
            if !r.proxy.trim().is_empty() {
                existing.proxy = r.proxy;
            }
            if r.is_pinned {
                existing.is_pinned = true;
            }
        } else {
            if r.status.trim().is_empty() {
                r.status = "待录入".to_string();
            }
            if r.antigravity_status.trim().is_empty() {
                r.antigravity_status = "待验证".to_string();
            }
            if r.google_one_plan.trim().is_empty() {
                r.google_one_plan = "Free".to_string();
            }
            if r.created_at == 0 {
                r.created_at = now;
            }
            records.push(r);
        }
        imported_count += 1;
    }

    if let Some((key, salt)) = state.vault_key.read().as_ref() {
        vault::save_vault_atomic_with_key(&records, key, salt, 30)?;
    } else {
        vault::save_vault_atomic(&records, password, 30)?;
    }
    Ok(imported_count)
}

#[cfg(target_os = "android")]
fn find_app_class<'a>(
    env: &mut jni::JNIEnv<'a>,
    context_obj: &jni::objects::JObject<'a>,
    class_name: &str,
) -> Result<jni::objects::JClass<'a>, String> {
    if let Ok(class_loader_val) =
        env.call_method(context_obj, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
    {
        if let Ok(class_loader) = class_loader_val.l() {
            let dotted = class_name.replace('/', ".");
            if let Ok(j_name) = env.new_string(&dotted) {
                let load_res = env.call_method(
                    &class_loader,
                    "loadClass",
                    "(Ljava/lang/String;)Ljava/lang/Class;",
                    &[jni::objects::JValue::from(&j_name)],
                );
                match load_res {
                    Ok(val) => {
                        if let Ok(cls) = val.l() {
                            return Ok(cls.into());
                        }
                    }
                    Err(_) => {
                        let _ = env.exception_clear();
                    }
                }
            }
        }
    }

    match env.find_class(class_name) {
        Ok(c) => Ok(c),
        Err(e) => {
            let _ = env.exception_clear();
            Err(format!("无法加载应用类 {class_name}: {e}"))
        }
    }
}

fn is_localhost_callback_url(u: &tauri::Url) -> bool {
    let scheme = u.scheme();
    if scheme != "http" && scheme != "https" {
        return false;
    }
    match u.host_str() {
        Some("localhost") | Some("127.0.0.1") | Some("[::1]") | Some("0.0.0.0") => true,
        _ => false,
    }
}

fn build_localhost_callback_eval_script(callback_url: &str, email: &str) -> String {
    let url_json = serde_json::to_string(callback_url).unwrap_or_else(|_| "\"\"".to_string());
    let email_json = serde_json::to_string(email).unwrap_or_else(|_| "\"\"".to_string());

    format!(
        r###"
        (function() {{
            const fullUrl = {url_json};
            const accountEmail = {email_json};
            let portStr = "";
            let codeParam = "";
            try {{
                const parsed = new URL(fullUrl);
                portStr = parsed.port ? ("端口: " + parsed.port) : "本地回环";
                codeParam = parsed.searchParams.get("code") || "";
                if (!codeParam && parsed.hash) {{
                    const hashParams = new URLSearchParams(parsed.hash.replace(/^#/, ""));
                    codeParam = hashParams.get("code") || hashParams.get("access_token") || "";
                }}
            }} catch (_) {{}}

            const escapeHtml = (str) => String(str)
                .replace(/&/g, "&amp;")
                .replace(/</g, "&lt;")
                .replace(/>/g, "&gt;")
                .replace(/"/g, "&quot;");

            const codeSectionHtml = codeParam ? `
                <div style="margin-top:16px;padding:14px 16px;background:#f8fafd;border-radius:16px;border:1px solid #dadce0;display:flex;align-items:center;justify-content:space-between;gap:12px;">
                    <div style="min-width:0;flex:1;text-align:left;">
                        <div style="font-size:12px;color:#5f6368;font-weight:500;">已自动提取授权码 (<code style="color:#1a73e8;font-family:Consolas,monospace;">code</code> 参数)</div>
                        <div id="gvaultCodeVal" style="font-family:Consolas,Monaco,monospace;font-size:12px;font-weight:600;color:#202124;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;margin-top:4px;user-select:all;">${{escapeHtml(codeParam)}}</div>
                    </div>
                    <button id="gvaultCopyCodeBtn" type="button" style="padding:7px 14px;background:#ffffff;color:#1a73e8;border:1px solid #dadce0;border-radius:12px;font-size:12px;font-weight:600;cursor:pointer;flex-shrink:0;transition:all 0.15s;">
                        仅复制 Code
                    </button>
                </div>
            ` : "";

            const html = `
                <div style="min-height:100vh;margin:0;padding:24px;background:#f0f4f9;display:flex;align-items:center;justify-content:center;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,'Microsoft YaHei',sans-serif;box-sizing:border-box;">
                    <div style="width:100%;max-width:520px;background:#ffffff;color:#202124;border:1px solid #dadce0;border-radius:24px;padding:28px;box-shadow:0 12px 32px rgba(0,0,0,0.08);box-sizing:border-box;">
                        <div style="display:flex;flex-direction:column;align-items:center;text-align:center;margin-bottom:20px;">
                            <div style="position:relative;margin-bottom:12px;">
                                <div style="width:56px;height:56px;border-radius:16px;background:#f8fafd;border:1px solid #dadce0;display:flex;align-items:center;justify-content:center;">
                                    <svg style="width:32px;height:32px;" viewBox="0 0 24 24" fill="none">
                                        <path d="M12 2a10 10 0 0 1 7.07 2.93l-2.83 2.83A6 6 0 0 0 12 6V2z" fill="#EA4335" />
                                        <path d="M19.07 4.93A10 10 0 0 1 22 12h-4a6 6 0 0 0-1.76-4.24l2.83-2.83z" fill="#FBBC05" />
                                        <path d="M22 12a10 10 0 0 1-10 10v-4a6 6 0 0 0 6-6h4z" fill="#34A853" />
                                        <path d="M12 22A10 10 0 0 1 2 12h4a6 6 0 0 0 6 6v4z" fill="#4285F4" />
                                        <path d="M2 12A10 10 0 0 1 12 2v4a6 6 0 0 0-6 6H2z" fill="#4285F4" opacity="0.85" />
                                    </svg>
                                </div>
                                <div style="position:absolute;bottom:-4px;right:-4px;width:20px;height:20px;border-radius:50%;background:#10b981;color:#ffffff;display:flex;align-items:center;justify-content:center;border:2px solid #ffffff;">
                                    <svg style="width:12px;height:12px;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3">
                                        <polyline points="20 6 9 17 4 12"></polyline>
                                    </svg>
                                </div>
                            </div>

                            <div style="display:inline-flex;align-items:center;padding:3px 12px;border-radius:999px;background:#ecfdf5;color:#047857;border:1px solid #a7f3d0;font-size:12px;font-weight:600;margin-bottom:6px;">
                                OAuth 本地回调已自动拦截
                            </div>
                            <h1 style="font-size:17px;font-weight:700;color:#202124;margin:2px 0;">已捕获本地回调地址 (localhost)</h1>
                            <p style="font-size:12px;color:#5f6368;margin:4px 0 0 0;">当前授权账号：<span style="font-weight:600;color:#1a73e8;">${{escapeHtml(accountEmail)}}</span></p>
                        </div>

                        <div>
                            <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;">
                                <label style="font-size:12px;font-weight:600;color:#3c4043;">完整回调地址 (Callback URL)</label>
                                <span style="font-size:12px;color:#80868b;font-family:Consolas,monospace;">${{escapeHtml(portStr)}}</span>
                            </div>

                            <textarea id="gvaultUrlInput" readonly rows="4" style="width:100%;padding:12px;background:#f8fafd;border-radius:16px;border:1px solid #dadce0;font-family:Consolas,Monaco,monospace;font-size:12px;color:#202124;outline:none;resize:none;line-height:1.55;word-break:break-all;box-sizing:border-box;">${{escapeHtml(fullUrl)}}</textarea>

                            <button id="gvaultCopyUrlBtn" type="button" style="margin-top:10px;width:100%;padding:11px 16px;background:#1a73e8;color:#ffffff;border:none;border-radius:12px;font-size:13px;font-weight:600;display:flex;align-items:center;justify-content:center;gap:8px;cursor:pointer;box-shadow:0 2px 6px rgba(26,115,232,0.25);transition:all 0.15s;">
                                <svg style="width:16px;height:16px;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                    <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                                    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                                </svg>
                                <span id="gvaultCopyBtnText">一键复制完整回调地址</span>
                            </button>
                        </div>

                        ${{codeSectionHtml}}
                    </div>
                </div>
            `;

            document.open();
            document.write("<!DOCTYPE html><html><head><meta charset='utf-8'><title>已捕获本地回调地址 - Google 账号管家</title></head><body style='margin:0;padding:0;'>" + html + "</body></html>");
            document.close();

            function copyTextFallback(text) {{
                if (navigator.clipboard && navigator.clipboard.writeText) {{
                    navigator.clipboard.writeText(text).catch(() => fallbackExecCopy(text));
                }} else {{
                    fallbackExecCopy(text);
                }}
            }}

            function fallbackExecCopy(text) {{
                const ta = document.createElement("textarea");
                ta.value = text;
                ta.style.position = "fixed";
                ta.style.opacity = "0";
                document.body.appendChild(ta);
                ta.focus();
                ta.select();
                try {{ document.execCommand("copy"); }} catch (_) {{}}
                document.body.removeChild(ta);
            }}

            const urlInput = document.getElementById("gvaultUrlInput");
            const copyUrlBtn = document.getElementById("gvaultCopyUrlBtn");
            const copyBtnText = document.getElementById("gvaultCopyBtnText");
            if (urlInput) {{
                urlInput.addEventListener("click", () => urlInput.select());
            }}
            if (copyUrlBtn) {{
                copyUrlBtn.addEventListener("click", () => {{
                    if (urlInput) urlInput.select();
                    copyTextFallback(fullUrl);
                    copyUrlBtn.style.background = "#059669";
                    if (copyBtnText) copyBtnText.innerText = "✓ 已成功复制完整回调地址！";
                    setTimeout(() => {{
                        copyUrlBtn.style.background = "#1a73e8";
                        if (copyBtnText) copyBtnText.innerText = "一键复制完整回调地址";
                    }}, 2000);
                }});
            }}

            const copyCodeBtn = document.getElementById("gvaultCopyCodeBtn");
            if (copyCodeBtn && codeParam) {{
                copyCodeBtn.addEventListener("click", () => {{
                    copyTextFallback(codeParam);
                    copyCodeBtn.innerText = "✓ 已复制";
                    copyCodeBtn.style.background = "#ecfdf5";
                    copyCodeBtn.style.color = "#047857";
                    copyCodeBtn.style.borderColor = "#6ee7b7";
                    setTimeout(() => {{
                        copyCodeBtn.innerText = "仅复制 Code";
                        copyCodeBtn.style.background = "#ffffff";
                        copyCodeBtn.style.color = "#1a73e8";
                        copyCodeBtn.style.borderColor = "#dadce0";
                    }}, 2000);
                }});
            }}
        }})();
        "###
    )
}

#[tauri::command]
async fn open_in_app_browser(
    _app: tauri::AppHandle,
    email: String,
    password: Option<String>,
    totp_secret: Option<String>,
    recovery_email: Option<String>,
    proxy: Option<String>,
    url: String,
) -> Result<(), String> {
    let clean_email = email.trim().to_lowercase();
    let email_uuid = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, clean_email.as_bytes());
    let safe_label = format!("browser-{}", &email_uuid.to_string()[..8]);

    let parsed_url = tauri::Url::parse(&url).map_err(|e| format!("无效网址: {e}"))?;

    let safe_email_json = serde_json::to_string(&email).unwrap_or_else(|_| "\"\"".to_string());
    let safe_pwd_json = serde_json::to_string(&password.as_deref().unwrap_or_default())
        .unwrap_or_else(|_| "\"\"".to_string());
    let safe_rec_email_json =
        serde_json::to_string(&recovery_email.as_deref().unwrap_or_default())
            .unwrap_or_else(|_| "\"\"".to_string());
    let current_totp = totp_secret
        .as_ref()
        .and_then(|s| totp::calculate_totp(s).ok())
        .map(|r| r.code)
        .unwrap_or_default();
    let safe_totp_json =
        serde_json::to_string(&current_totp).unwrap_or_else(|_| "\"\"".to_string());

    let autofill_script = format!(
        r#"
        (function() {{
            const targetEmail = {safe_email_json};
            const targetPassword = {safe_pwd_json};
            const targetTotp = {safe_totp_json};
            const targetRecEmail = {safe_rec_email_json};
            let emailFilled = false;
            let pwdFilled = false;
            let totpFilled = false;
            let recEmailFilled = false;

            function autoFill() {{
                if (!window.location.hostname.includes("google.com")) return;

                // 1. Fill Email / Phone
                if (targetEmail && !emailFilled) {{
                    const emailInput = document.getElementById("identifierId") || 
                                       document.querySelector('input[type="email"]') ||
                                       document.querySelector('input[name="identifier"]');
                    if (emailInput) {{
                        if (emailInput.value !== targetEmail) {{
                            emailInput.focus();
                            emailInput.value = targetEmail;
                            emailInput.dispatchEvent(new Event("input", {{ bubbles: true }}));
                            emailInput.dispatchEvent(new Event("change", {{ bubbles: true }}));
                        }}
                        emailFilled = true;
                    }}
                }}

                // 2. Fill Password
                if (targetPassword && !pwdFilled) {{
                    const pwdInput = document.querySelector('input[name="Passwd"]') ||
                                     document.querySelector('input[type="password"]');
                    if (pwdInput) {{
                        if (pwdInput.value !== targetPassword) {{
                            pwdInput.focus();
                            pwdInput.value = targetPassword;
                            pwdInput.dispatchEvent(new Event("input", {{ bubbles: true }}));
                            pwdInput.dispatchEvent(new Event("change", {{ bubbles: true }}));
                        }}
                        pwdFilled = true;
                    }}
                }}

                // 3. Fill 2FA TOTP code if present
                if (targetTotp && !totpFilled) {{
                    const totpInput = document.querySelector('input[name="totpPin"]') ||
                                      document.getElementById("idvPin") ||
                                      document.querySelector('input[type="tel"][maxlength="6"]') ||
                                      document.querySelector('input[autocomplete="one-time-code"]');
                    if (totpInput) {{
                        if (totpInput.value !== targetTotp) {{
                            totpInput.focus();
                            totpInput.value = targetTotp;
                            totpInput.dispatchEvent(new Event("input", {{ bubbles: true }}));
                            totpInput.dispatchEvent(new Event("change", {{ bubbles: true }}));
                        }}
                        totpFilled = true;
                    }}
                }}

                // 4. Fill Recovery Email if present
                if (targetRecEmail && !recEmailFilled) {{
                    const recInput = document.getElementById("knowledge-preregistered-email-response") ||
                                     document.querySelector('input[name="knowledgePreregisteredEmailResponse"]') ||
                                     document.querySelector('input[type="email"][aria-label*="辅助"]') ||
                                     document.querySelector('input[type="email"][aria-label*="recovery"]');
                    if (recInput) {{
                        if (recInput.value !== targetRecEmail) {{
                            recInput.focus();
                            recInput.value = targetRecEmail;
                            recInput.dispatchEvent(new Event("input", {{ bubbles: true }}));
                            recInput.dispatchEvent(new Event("change", {{ bubbles: true }}));
                        }}
                        recEmailFilled = true;
                    }}
                }}
            }}

            window.addEventListener("DOMContentLoaded", autoFill);
            window.addEventListener("load", autoFill);
            const observer = new MutationObserver(autoFill);
            if (document.documentElement) {{
                observer.observe(document.documentElement, {{ childList: true, subtree: true }});
            }}
            const timer = setInterval(autoFill, 400);
            setTimeout(() => {{
                clearInterval(timer);
                observer.disconnect();
            }}, 30000);
        }})();
        "#
    );

    #[cfg(target_os = "android")]
    {
        let _ = (
            safe_label,
            safe_email_json,
            safe_pwd_json,
            safe_rec_email_json,
            current_totp,
            safe_totp_json,
            autofill_script,
            proxy,
        );
        let ctx = ndk_context::android_context();
        let vm_ptr = ctx.vm();
        if vm_ptr.is_null() {
            return Err("获取 Android JavaVM 失败".to_string());
        }
        let context_ptr = ctx.context();
        if context_ptr.is_null() {
            return Err("获取 Android Context 失败".to_string());
        }

        let vm = unsafe { jni::JavaVM::from_raw(vm_ptr.cast()) }
            .map_err(|e| format!("初始化 JavaVM 句柄失败: {e}"))?;

        let mut env = vm
            .attach_current_thread()
            .map_err(|e| format!("附加当前线程至 JVM 失败: {e}"))?;

        let context_obj = unsafe { jni::objects::JObject::from_raw(context_ptr.cast()) };
        let email_j = env.new_string(&email).map_err(|e| e.to_string())?;
        let url_j = env.new_string(&url).map_err(|e| e.to_string())?;
        let pwd_j = env
            .new_string(password.unwrap_or_default())
            .map_err(|e| e.to_string())?;
        let totp_j = env
            .new_string(totp_secret.unwrap_or_default())
            .map_err(|e| e.to_string())?;
        let rec_email_j = env
            .new_string(recovery_email.unwrap_or_default())
            .map_err(|e| e.to_string())?;

        let class_res = find_app_class(&mut env, &context_obj, "com/google/accountvault/MainActivity");
        let opened = if let Ok(class) = class_res {
            let res = env.call_static_method(
                class,
                "openIsolatedBrowser",
                "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Z",
                &[
                    jni::objects::JValue::from(&context_obj),
                    jni::objects::JValue::from(&email_j),
                    jni::objects::JValue::from(&url_j),
                    jni::objects::JValue::from(&pwd_j),
                    jni::objects::JValue::from(&totp_j),
                    jni::objects::JValue::from(&rec_email_j),
                ],
            );
            match res {
                Ok(v) => v.z().unwrap_or(false),
                Err(e) => {
                    eprintln!("JNI 调用 openIsolatedBrowser 异常: {e}");
                    let _ = env.exception_clear();
                    false
                }
            }
        } else {
            eprintln!("find_app_class MainActivity 失败");
            false
        };

        if !opened {
            // Graceful fallback to default browser if activity launch fails
            let _ = tauri_plugin_opener::open_url(parsed_url.as_str(), None::<&str>);
        }

        Ok(())
    }

    #[cfg(not(target_os = "android"))]
    {
        let app = &_app;
        // If window already exists, focus it and navigate to the requested URL
        if let Some(w) = app.get_webview_window(&safe_label) {
            let _ = w.show();
            let _ = w.set_focus();
            if is_localhost_callback_url(&parsed_url) {
                let script = build_localhost_callback_eval_script(parsed_url.as_str(), &email);
                let _ = w.eval(&script);
            } else {
                let _ = w.navigate(parsed_url);
                let _ = w.eval(&autofill_script);
            }
            return Ok(());
        }

        let session_dir = vault::vault_data_dir().join("sessions").join(&safe_label);
        let _ = std::fs::create_dir_all(&session_dir);
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0";

        let clean_proxy = proxy.as_deref().unwrap_or("").trim().to_string();
        let window_title = if !clean_proxy.is_empty() {
            format!("Google 独立会话 - {} [独立代理: {}]", email, clean_proxy)
        } else {
            format!("Google 独立会话 - {}", email)
        };

        let nav_app = app.clone();
        let nav_label = safe_label.clone();
        let nav_email = email.clone();

        let mut builder = tauri::WebviewWindowBuilder::new(
            app,
            &safe_label,
            tauri::WebviewUrl::External(parsed_url),
        )
        .title(window_title)
        .inner_size(1050.0, 750.0)
        .center()
        .user_agent(ua)
        .devtools(true)
        .initialization_script(&autofill_script)
        .data_directory(session_dir)
        .on_navigation(move |target_url| {
            if is_localhost_callback_url(target_url) {
                let captured = target_url.as_str().to_string();
                let script = build_localhost_callback_eval_script(&captured, &nav_email);
                let app_handle = nav_app.clone();
                let label_clone = nav_label.clone();
                std::thread::spawn(move || {
                    use tauri::Manager;
                    if let Some(win) = app_handle.get_webview_window(&label_clone) {
                        let _ = win.eval(&script);
                    }
                });
                return false;
            }
            true
        });

        if !clean_proxy.is_empty() {
            let normalized_proxy = if clean_proxy.contains("://") {
                clean_proxy.clone()
            } else {
                format!("http://{}", clean_proxy)
            };
            if let Ok(proxy_url) = tauri::Url::parse(&normalized_proxy) {
                builder = builder.proxy_url(proxy_url);
            }
            #[cfg(target_os = "windows")]
            {
                builder = builder.additional_browser_args(&format!("--proxy-server={}", clean_proxy));
            }
        }

        let _window = builder
            .build()
            .map_err(|e| format!("创建独立窗口失败: {e}"))?;
        #[cfg(debug_assertions)]
        let _ = _window.open_devtools();
        Ok(())
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
    pub size: u64,
    pub asset_type: String, // "setup", "portable", "apk", "other"
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_name: String,
    pub release_notes: String,
    pub published_at: String,
    pub release_url: String,
    pub download_url: Option<String>,
    pub setup_url: Option<String>,
    pub portable_url: Option<String>,
    pub apk_url: Option<String>,
    pub current_install_type: String,
    pub assets: Vec<ReleaseAsset>,
}

fn detect_install_type() -> &'static str {
    #[cfg(target_os = "android")]
    {
        "android"
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                if parent.join("uninstall.exe").exists()
                    || parent.join("Uninstall Google 账号管家.exe").exists()
                {
                    return "setup";
                }
                let path_str = parent.to_string_lossy().to_lowercase();
                if path_str.contains("program files") || path_str.contains("programs") {
                    return "setup";
                }
            }
        }
        "portable"
    }
    #[cfg(not(any(target_os = "windows", target_os = "android")))]
    {
        "desktop"
    }
}

fn parse_semver(v: &str) -> (u32, u32, u32) {
    let clean = v.trim().trim_start_matches(|c| c == 'v' || c == 'V');
    let parts: Vec<&str> = clean.split('.').collect();
    let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts
        .get(2)
        .and_then(|s| s.split('-').next().unwrap_or("0").parse().ok())
        .unwrap_or(0);
    (major, minor, patch)
}

#[tauri::command]
async fn check_github_update(
    repo: Option<String>,
    mirror: Option<String>,
) -> Result<UpdateInfo, String> {
    let target_repo = repo.unwrap_or_else(|| "ifnor/GoogleAccountVault".to_string());
    let direct_api_url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        target_repo.trim()
    );

    let mirror_prefix: Option<String> = mirror.and_then(|m| {
        let trimmed = m.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("official") {
            None
        } else if trimmed.eq_ignore_ascii_case("gh-proxy")
            || trimmed.eq_ignore_ascii_case("gh_proxy")
            || trimmed.contains("gh-proxy.com")
        {
            Some("https://gh-proxy.com/".to_string())
        } else if trimmed.eq_ignore_ascii_case("ghproxy") || trimmed.contains("ghproxy.net") {
            Some("https://ghproxy.net/".to_string())
        } else {
            let pfx = if trimmed.ends_with('/') {
                trimmed.to_string()
            } else {
                format!("{}/", trimmed)
            };
            Some(pfx)
        }
    });

    let client = reqwest::Client::builder()
        .user_agent("google-account-vault/3.1.17")
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| format!("构建 HTTP 客户端失败: {e}"))?;

    let resp_res = if let Some(ref pfx) = mirror_prefix {
        let proxied_api_url = format!("{}{}", pfx, direct_api_url);
        match client.get(&proxied_api_url).send().await {
            Ok(r) if r.status().is_success() => Ok(r),
            _ => client.get(&direct_api_url).send().await,
        }
    } else {
        client.get(&direct_api_url).send().await
    };

    let resp = resp_res.map_err(|e| format!("请求 GitHub Releases 失败: {e}"))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(UpdateInfo {
            current_version: "3.1.17".to_string(),
            latest_version: "3.1.17".to_string(),
            has_update: false,
            release_name: "暂无发布版本".to_string(),
            release_notes: format!("仓库 {} 尚未发布任何公开 Release", target_repo),
            published_at: String::new(),
            release_url: format!("https://github.com/{}", target_repo),
            download_url: None,
            setup_url: None,
            portable_url: None,
            apk_url: None,
            current_install_type: detect_install_type().to_string(),
            assets: Vec::new(),
        });
    }

    if !resp.status().is_success() {
        return Err(format!("GitHub API 响应异常: HTTP {}", resp.status()));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取 GitHub 响应文本失败: {e}"))?;
    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("解析 Release JSON 失败: {e}"))?;

    let tag = json["tag_name"].as_str().unwrap_or("3.1.17").to_string();
    let name = json["name"].as_str().unwrap_or(&tag).to_string();
    let notes = json["body"].as_str().unwrap_or("").to_string();
    let published_at = json["published_at"].as_str().unwrap_or("").to_string();
    let html_url = json["html_url"]
        .as_str()
        .unwrap_or(&format!("https://github.com/{}/releases", target_repo))
        .to_string();

    let mut asset_list = Vec::new();
    let mut setup_url = None;
    let mut portable_url = None;
    let mut apk_url = None;

    if let Some(assets) = json["assets"].as_array() {
        for asset in assets {
            if let (Some(asset_name), Some(raw_browser_url)) = (
                asset["name"].as_str(),
                asset["browser_download_url"].as_str(),
            ) {
                let browser_url = if let Some(ref pfx) = mirror_prefix {
                    format!("{}{}", pfx, raw_browser_url)
                } else {
                    raw_browser_url.to_string()
                };

                let size = asset["size"].as_u64().unwrap_or(0);
                let name_lower = asset_name.to_lowercase();
                let asset_type = if name_lower.ends_with(".apk") {
                    apk_url = Some(browser_url.clone());
                    "apk"
                } else if (name_lower.contains("setup") || name_lower.contains("installer"))
                    && name_lower.ends_with(".exe")
                {
                    setup_url = Some(browser_url.clone());
                    "setup"
                } else if name_lower.contains("portable") && name_lower.ends_with(".exe") {
                    portable_url = Some(browser_url.clone());
                    "portable"
                } else if name_lower.ends_with(".exe") {
                    if setup_url.is_none() {
                        setup_url = Some(browser_url.clone());
                    }
                    "setup"
                } else {
                    "other"
                };

                asset_list.push(ReleaseAsset {
                    name: asset_name.to_string(),
                    download_url: browser_url,
                    size,
                    asset_type: asset_type.to_string(),
                });
            }
        }
    }

    let install_type = detect_install_type();
    let download_url = match install_type {
        "android" => apk_url.clone(),
        "portable" => portable_url.clone().or_else(|| setup_url.clone()),
        _ => setup_url.clone().or_else(|| portable_url.clone()),
    };

    let current = (3, 1, 17);
    let latest = parse_semver(&tag);
    let has_update = latest > current;

    Ok(UpdateInfo {
        current_version: "3.1.17".to_string(),
        latest_version: tag,
        has_update,
        release_name: name,
        release_notes: notes,
        published_at,
        release_url: html_url,
        download_url,
        setup_url,
        portable_url,
        apk_url,
        current_install_type: install_type.to_string(),
        assets: asset_list,
    })
}

#[tauri::command]
fn open_external_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| format!("打开系统浏览器链接失败: {e}"))
}

#[cfg(target_os = "android")]
fn install_apk_android(apk_path: &str) -> Result<(), String> {
    let ctx = ndk_context::android_context();
    let vm_ptr = ctx.vm();
    if vm_ptr.is_null() {
        return Err("获取 Android JavaVM 失败".to_string());
    }
    let context_ptr = ctx.context();
    if context_ptr.is_null() {
        return Err("获取 Android Context 失败".to_string());
    }

    let vm = unsafe { jni::JavaVM::from_raw(vm_ptr.cast()) }
        .map_err(|e| format!("初始化 JavaVM 句柄失败: {e}"))?;

    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("附加当前线程至 JVM 失败: {e}"))?;

    let context_obj = unsafe { jni::objects::JObject::from_raw(context_ptr.cast()) };
    let path_jstring = env
        .new_string(apk_path)
        .map_err(|e| format!("创建 APK 路径字符串失败: {e}"))?;

    let class = find_app_class(&mut env, &context_obj, "com/google/accountvault/MainActivity")
        .map_err(|e| format!("未找到 MainActivity 类: {e}"))?;

    let res = env
        .call_static_method(
            class,
            "installApk",
            "(Landroid/content/Context;Ljava/lang/String;)Z",
            &[
                jni::objects::JValue::from(&context_obj),
                jni::objects::JValue::from(&path_jstring),
            ],
        )
        .map_err(|e| format!("调用 MainActivity.installApk 失败: {e}"))?;

    let success = res.z().unwrap_or(false);
    if !success {
        return Err("调用系统安装器失败，请检查文件权限或允许安装未知应用".to_string());
    }

    Ok(())
}

#[tauri::command]
async fn start_auto_update(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use futures_util::StreamExt;
    use std::io::Write;
    use tauri::Emitter;

    let client = reqwest::Client::builder()
        .user_agent("google-account-vault/3.1.17")
        .build()
        .map_err(|e| format!("初始化下载客户端失败: {e}"))?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("发起下载请求失败: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("下载响应错误: HTTP {}", resp.status()));
    }

    let total_bytes = resp.content_length().unwrap_or(0);
    let url_lower = url.to_lowercase();
    let is_apk = url_lower.ends_with(".apk") || url_lower.contains(".apk?");
    let is_setup = url_lower.contains("setup") || (!url_lower.contains("portable") && !is_apk);

    let temp_dir = {
        #[cfg(target_os = "android")]
        {
            use tauri::Manager;
            app.path()
                .app_cache_dir()
                .or_else(|_| app.path().cache_dir())
                .unwrap_or_else(|_| std::env::temp_dir())
        }
        #[cfg(not(target_os = "android"))]
        {
            std::env::temp_dir()
        }
    };

    let _ = std::fs::create_dir_all(&temp_dir);

    let temp_file_path = if is_apk {
        temp_dir.join("GoogleAccountVault_Update.apk")
    } else if is_setup {
        temp_dir.join("GoogleAccountVault_Update_Setup.exe")
    } else {
        temp_dir.join("GoogleAccountVault_Update_Portable.exe")
    };

    let mut file = std::fs::File::create(&temp_file_path)
        .map_err(|e| format!("创建更新临时文件失败: {e}"))?;

    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_percent: u32 = 0;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| format!("下载更新数据包中断: {e}"))?;
        file.write_all(&chunk)
            .map_err(|e| format!("写入更新文件失败: {e}"))?;
        downloaded += chunk.len() as u64;

        if total_bytes > 0 {
            let percent = ((downloaded as f64 / total_bytes as f64) * 100.0) as u32;
            if percent != last_percent {
                last_percent = percent;
                let _ = app.emit(
                    "update-download-progress",
                    serde_json::json!({
                        "downloaded": downloaded,
                        "total": total_bytes,
                        "percent": percent
                    }),
                );
            }
        }
    }

    file.flush()
        .map_err(|e| format!("保存更新文件失败: {e}"))?;
    drop(file);

    let _ = app.emit(
        "update-download-progress",
        serde_json::json!({
            "downloaded": downloaded,
            "total": downloaded,
            "percent": 100
        }),
    );

    #[cfg(target_os = "windows")]
    {
        if is_setup {
            std::process::Command::new(&temp_file_path)
                .spawn()
                .map_err(|e| format!("启动安装升级程序失败: {e}"))?;
            app.exit(0);
        } else {
            if let Ok(current_exe) = std::env::current_exe() {
                let current_exe_str = current_exe.to_string_lossy().to_string();
                let temp_str = temp_file_path.to_string_lossy().to_string();
                let cmd_arg = format!(
                    "ping 127.0.0.1 -n 2 > nul & copy /y \"{}\" \"{}\" & start \"\" \"{}\"",
                    temp_str, current_exe_str, current_exe_str
                );
                std::process::Command::new("cmd.exe")
                    .args(["/C", &cmd_arg])
                    .spawn()
                    .map_err(|e| format!("启动便携程序替换进程失败: {e}"))?;
                app.exit(0);
            } else {
                std::process::Command::new(&temp_file_path)
                    .spawn()
                    .map_err(|e| format!("启动更新程序失败: {e}"))?;
                app.exit(0);
            }
        }
    }

    #[cfg(target_os = "android")]
    {
        let path_str = temp_file_path.to_string_lossy().to_string();
        install_apk_android(&path_str)?;
    }

    #[cfg(not(any(target_os = "windows", target_os = "android")))]
    {
        use tauri_plugin_opener::OpenerExt;
        let _ = app.opener().open_url(&url, None::<&str>);
    }

    Ok(())
}

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState {
        master_password: RwLock::new(None),
        vault_key: RwLock::new(None),
        records: RwLock::new(Vec::new()),
    };

    let builder = tauri::Builder::default();

    #[cfg(any(target_os = "android", target_os = "ios"))]
    let builder = builder.plugin(tauri_plugin_biometric::init());

    builder
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(|app| {
            let data_dir = {
                #[cfg(target_os = "windows")]
                {
                    dirs::data_local_dir()
                        .map(|p| p.join("GoogleAccountVault"))
                        .or_else(|| app.path().app_data_dir().ok())
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                }
                #[cfg(not(target_os = "windows"))]
                {
                    app.path()
                        .app_data_dir()
                        .unwrap_or_else(|_| std::path::PathBuf::from("."))
                }
            };

            let _ = std::fs::create_dir_all(&data_dir);
            vault::init_vault_data_dir(data_dir);

            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.open_devtools();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_vault_exists,
            unlock_vault,
            create_vault,
            lock_vault,
            get_records,
            save_record,
            delete_record,
            batch_update_records,
            batch_delete_records,
            change_master_password,
            get_totp,
            get_totp_qr,
            parse_totp_input,
            generate_pwd,
            open_in_app_browser,
            test_webdav_connection,
            save_webdav_config,
            load_webdav_config,
            upload_webdav_vault,
            download_webdav_vault,
            list_vault_backups,
            create_manual_vault_backup,
            restore_vault_backup,
            export_records_csv_string,
            export_encrypted_vault_string,
            import_encrypted_vault_blob,
            batch_import_records,
            check_windows_hello_status,
            enable_windows_hello,
            disable_windows_hello,
            unlock_vault_with_hello,
            generate_custom_pwd,
            check_github_update,
            open_external_url,
            start_auto_update,
            sync_webdav_smart
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
