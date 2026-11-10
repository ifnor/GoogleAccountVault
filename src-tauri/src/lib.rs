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

fn is_localhost_callback_str(raw: &str) -> bool {
    if let Ok(u) = tauri::Url::parse(raw) {
        return is_localhost_callback_url(&u);
    }
    false
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn build_localhost_callback_html(callback_url: &str, email: &str) -> String {
    let mut port_str = "本地回环".to_string();
    let mut code_param = String::new();

    if let Ok(parsed) = tauri::Url::parse(callback_url) {
        if let Some(p) = parsed.port() {
            port_str = format!("端口: {p}");
        }
        for (k, v) in parsed.query_pairs() {
            if k == "code" && !v.is_empty() {
                code_param = v.to_string();
                break;
            }
        }
        if code_param.is_empty() {
            if let Some(frag) = parsed.fragment() {
                if let Ok(frag_url) = tauri::Url::parse(&format!("http://localhost/?{frag}")) {
                    for (k, v) in frag_url.query_pairs() {
                        if (k == "code" || k == "access_token") && !v.is_empty() {
                            code_param = v.to_string();
                            break;
                        }
                    }
                }
            }
        }
    }

    let safe_url_html = html_escape(callback_url);
    let safe_email_html = html_escape(email);
    let safe_port_html = html_escape(&port_str);
    let safe_code_html = html_escape(&code_param);
    let url_json = serde_json::to_string(callback_url).unwrap_or_else(|_| "\"\"".to_string());
    let code_json = serde_json::to_string(&code_param).unwrap_or_else(|_| "\"\"".to_string());

    let code_section_html = if !code_param.is_empty() {
        format!(
            r###"
            <div style="margin-top:16px;padding:14px 16px;background:#f8fafd;border-radius:16px;border:1px solid #dadce0;display:flex;align-items:center;justify-content:space-between;gap:12px;">
                <div style="min-width:0;flex:1;text-align:left;">
                    <div style="font-size:12px;color:#5f6368;font-weight:500;">已自动提取授权码 (<code style="color:#1a73e8;font-family:Consolas,monospace;">code</code> 参数)</div>
                    <div id="gvaultCodeVal" style="font-family:Consolas,Monaco,monospace;font-size:12px;font-weight:600;color:#202124;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;margin-top:4px;user-select:all;">{safe_code_html}</div>
                </div>
                <button id="gvaultCopyCodeBtn" type="button" onclick="copyCode()" style="padding:7px 14px;background:#ffffff;color:#1a73e8;border:1px solid #dadce0;border-radius:12px;font-size:12px;font-weight:600;cursor:pointer;flex-shrink:0;transition:all 0.15s;">
                    仅复制 Code
                </button>
            </div>
            "###
        )
    } else {
        String::new()
    };

    format!(
        r###"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>已捕获回调地址 - Google 账号管家</title>
</head>
<body style="min-height:100vh;margin:0;padding:24px;background:#f0f4f9;display:flex;align-items:center;justify-content:center;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,'Microsoft YaHei',sans-serif;box-sizing:border-box;">
    <div style="width:100%;max-width:520px;background:#ffffff;color:#202124;border:1px solid #dadce0;border-radius:24px;padding:28px;box-shadow:0 12px 32px rgba(0,0,0,0.08);box-sizing:border-box;">
        <div style="display:flex;flex-direction:column;align-items:center;text-align:center;margin-bottom:20px;">
            <div style="margin-bottom:10px;display:flex;align-items:center;justify-content:center;">
                <svg style="width:42px;height:42px;" viewBox="0 0 24 24" fill="none">
                    <path d="M12 2a10 10 0 0 1 7.07 2.93l-2.83 2.83A6 6 0 0 0 12 6V2z" fill="#EA4335" />
                    <path d="M19.07 4.93A10 10 0 0 1 22 12h-4a6 6 0 0 0-1.76-4.24l2.83-2.83z" fill="#FBBC05" />
                    <path d="M22 12a10 10 0 0 1-10 10v-4a6 6 0 0 0 6-6h4z" fill="#34A853" />
                    <path d="M12 22A10 10 0 0 1 2 12h4a6 6 0 0 0 6 6v4z" fill="#4285F4" />
                    <path d="M2 12A10 10 0 0 1 12 2v4a6 6 0 0 0-6 6H2z" fill="#4285F4" opacity="0.85" />
                </svg>
            </div>

            <h1 style="font-size:17px;font-weight:700;color:#202124;margin:2px 0;">已捕获回调地址</h1>
            <p style="font-size:12px;color:#5f6368;margin:4px 0 0 0;">当前授权账号：<span style="font-weight:600;color:#1a73e8;">{safe_email_html}</span></p>
            <div id="gvaultLocalSyncNote" style="margin-top:10px;width:100%;padding:8px 12px;border-radius:12px;background:#f0fdf4;border:1px solid #bbf7d0;font-size:12px;color:#166534;line-height:1.45;box-sizing:border-box;">
                ⚡ 正在同步将授权结果送达监听服务…
            </div>
        </div>

        <div>
            <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;">
                <label style="font-size:12px;font-weight:600;color:#3c4043;">完整回调地址 (Callback URL)</label>
                <span style="font-size:12px;color:#80868b;font-family:Consolas,monospace;">{safe_port_html}</span>
            </div>

            <textarea id="gvaultUrlInput" readonly rows="4" onclick="this.select()" style="width:100%;padding:12px;background:#f8fafd;border-radius:16px;border:1px solid #dadce0;font-family:Consolas,Monaco,monospace;font-size:12px;color:#202124;outline:none;resize:none;line-height:1.55;word-break:break-all;box-sizing:border-box;">{safe_url_html}</textarea>

            <button id="gvaultCopyUrlBtn" type="button" onclick="copyFull()" style="margin-top:10px;width:100%;padding:11px 16px;background:#1a73e8;color:#ffffff;border:none;border-radius:12px;font-size:13px;font-weight:600;display:flex;align-items:center;justify-content:center;gap:8px;cursor:pointer;box-shadow:0 2px 6px rgba(26,115,232,0.25);transition:all 0.15s;">
                <svg style="width:16px;height:16px;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                </svg>
                <span id="gvaultCopyBtnText">一键复制完整回调地址</span>
            </button>

            <button id="gvaultJumpBtn" type="button" onclick="jumpToCallback()" style="margin-top:10px;width:100%;padding:10px 16px;background:#ffffff;color:#1a73e8;border:1px solid #dadce0;border-radius:12px;font-size:13px;font-weight:600;display:flex;align-items:center;justify-content:center;gap:8px;cursor:pointer;transition:all 0.15s;">
                <svg style="width:15px;height:15px;" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path>
                    <polyline points="15 3 21 3 21 9"></polyline>
                    <line x1="10" y1="14" x2="21" y2="3"></line>
                </svg>
                <span id="gvaultJumpBtnText">跳转回调地址</span>
            </button>
        </div>

        {code_section_html}
    </div>

    <script>
        let fullUrl = {url_json};
        let codeParam = {code_json};
        let jumpResetTimer = null;

        (function syncLocationHash() {{
            try {{
                if (window.location && /^https?:\/\/(localhost|127\.0\.0\.1|\[::1\]|0\.0\.0\.0)(:\d+)?(\/|$)/i.test(window.location.href)) {{
                    if (window.location.href.length > fullUrl.length && !window.location.href.includes("__gvault_")) {{
                        fullUrl = window.location.href;
                        const urlInput = document.getElementById("gvaultUrlInput");
                        if (urlInput) urlInput.value = fullUrl;
                    }}
                }}
            }} catch (_) {{}}
        }})();

        window.__gvaultSetLocalStatus = function(localOk, isManual) {{
            const note = document.getElementById("gvaultLocalSyncNote");
            const jumpBtn = document.getElementById("gvaultJumpBtn");
            const jumpBtnText = document.getElementById("gvaultJumpBtnText");
            if (localOk) {{
                if (note) {{
                    note.style.background = "#ecfdf5";
                    note.style.borderColor = "#6ee7b7";
                    note.style.color = "#065f46";
                    note.innerHTML = "✓ 回调已完成，已同步将授权结果送达监听服务";
                }}
                if (isManual && jumpBtn && jumpBtnText) {{
                    jumpBtn.style.background = "#ecfdf5";
                    jumpBtn.style.color = "#047857";
                    jumpBtn.style.borderColor = "#6ee7b7";
                    jumpBtnText.innerText = "✓ 已完成回调地址跳转！";
                }}
            }} else {{
                if (note) {{
                    note.style.background = "#eff6ff";
                    note.style.borderColor = "#bfdbfe";
                    note.style.color = "#1e40af";
                    note.innerHTML = "ℹ️ 未检测到本机开启该端口监听，请直接点击下方按钮复制回调地址或 Code";
                }}
                if (isManual && jumpBtn && jumpBtnText) {{
                    jumpBtn.style.background = "#eff6ff";
                    jumpBtn.style.color = "#1e40af";
                    jumpBtn.style.borderColor = "#bfdbfe";
                    jumpBtnText.innerText = "✓ 已触发回调（本机未监听该端口）";
                }}
            }}
            if (isManual && jumpBtn && jumpBtnText) {{
                if (jumpResetTimer) clearTimeout(jumpResetTimer);
                jumpResetTimer = setTimeout(() => {{
                    jumpBtn.style.background = "#ffffff";
                    jumpBtn.style.color = "#1a73e8";
                    jumpBtn.style.borderColor = "#dadce0";
                    jumpBtnText.innerText = "跳转回调地址";
                }}, 2200);
            }}
        }};

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

        function copyFull() {{
            const urlInput = document.getElementById("gvaultUrlInput");
            const copyUrlBtn = document.getElementById("gvaultCopyUrlBtn");
            const copyBtnText = document.getElementById("gvaultCopyBtnText");
            if (urlInput) urlInput.select();
            copyTextFallback(fullUrl);
            if (copyUrlBtn) copyUrlBtn.style.background = "#059669";
            if (copyBtnText) copyBtnText.innerText = "✓ 已成功复制完整回调地址！";
            setTimeout(() => {{
                if (copyUrlBtn) copyUrlBtn.style.background = "#1a73e8";
                if (copyBtnText) copyBtnText.innerText = "一键复制完整回调地址";
            }}, 2000);
        }}

        function jumpToCallback() {{
            const jumpBtn = document.getElementById("gvaultJumpBtn");
            const jumpBtnText = document.getElementById("gvaultJumpBtnText");
            const note = document.getElementById("gvaultLocalSyncNote");
            if (jumpBtnText) jumpBtnText.innerText = "正在触发回调地址…";
            if (note) {{
                note.style.background = "#f0fdf4";
                note.style.borderColor = "#bbf7d0";
                note.style.color = "#166534";
                note.innerHTML = "⚡ 正在同步将授权结果送达监听服务…";
            }}
            try {{
                const oldFrame = document.getElementById("gvaultManualTriggerFrame");
                if (oldFrame && oldFrame.parentNode) oldFrame.parentNode.removeChild(oldFrame);
                const pFrame = document.createElement("iframe");
                pFrame.id = "gvaultManualTriggerFrame";
                pFrame.style.display = "none";
                pFrame.src = "http://localhost/__gvault_manual_callback__?t=" + Date.now();
                document.body.appendChild(pFrame);
            }} catch (_) {{}}
            if (jumpResetTimer) clearTimeout(jumpResetTimer);
            jumpResetTimer = setTimeout(() => {{
                if (jumpBtn) {{
                    jumpBtn.style.background = "#ffffff";
                    jumpBtn.style.color = "#1a73e8";
                    jumpBtn.style.borderColor = "#dadce0";
                }}
                if (jumpBtnText && jumpBtnText.innerText === "正在触发回调地址…") {{
                    jumpBtnText.innerText = "跳转回调地址";
                }}
            }}, 3500);
        }}

        function copyCode() {{
            const copyCodeBtn = document.getElementById("gvaultCopyCodeBtn");
            copyTextFallback(codeParam);
            if (copyCodeBtn) {{
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
            }}
        }}
    </script>
</body>
</html>"###
    )
}

#[cfg(target_os = "windows")]
fn trigger_localhost_callback_in_background(
    window: tauri::WebviewWindow,
    callback_url: String,
    is_manual: bool,
) {
    tauri::async_runtime::spawn(async move {
        let client_res = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(std::time::Duration::from_millis(1200))
            .timeout(std::time::Duration::from_secs(8))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                let host = attempt.url().host_str().unwrap_or("");
                if host.eq_ignore_ascii_case("localhost")
                    || host == "127.0.0.1"
                    || host == "::1"
                    || host == "[::1]"
                {
                    if attempt.previous().len() >= 5 {
                        attempt.stop()
                    } else {
                        attempt.follow()
                    }
                } else {
                    attempt.stop()
                }
            }))
            .build();

        let local_ok = if let Ok(client) = client_res {
            let mut ok = false;
            if let Ok(mut parsed) = tauri::Url::parse(&callback_url) {
                if parsed.host_str().map(|h| h.eq_ignore_ascii_case("localhost")).unwrap_or(false) {
                    let host_header = match parsed.port() {
                        Some(p) => format!("localhost:{p}"),
                        None => "localhost".to_string(),
                    };
                    if parsed.set_host(Some("127.0.0.1")).is_ok() {
                        if client
                            .get(parsed.as_str())
                            .header("Host", host_header)
                            .send()
                            .await
                            .is_ok()
                        {
                            ok = true;
                        }
                    }
                }
            }
            if !ok {
                ok = client.get(&callback_url).send().await.is_ok();
            }
            ok
        } else {
            false
        };

        let js = format!(
            "if (window.__gvaultSetLocalStatus) {{ window.__gvaultSetLocalStatus({}, {}); }}",
            if local_ok { "true" } else { "false" },
            if is_manual { "true" } else { "false" }
        );
        std::thread::spawn(move || {
            let delays: &[u64] = if is_manual {
                &[0, 100]
            } else {
                &[0, 120, 400, 1000]
            };
            for &delay_ms in delays {
                if delay_ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                }
                let _ = window.eval(&js);
            }
        });
    });
}

#[cfg(target_os = "windows")]
fn attach_webview2_localhost_interceptor(
    window: &tauri::WebviewWindow,
    email: String,
    initial_localhost_url: Option<String>,
) {
    let win_for_webview = window.clone();
    let _ = window.with_webview(move |webview| {
        unsafe {
            use webview2_com::Microsoft::Web::WebView2::Win32::{
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT, ICoreWebView2, ICoreWebView2_2,
                ICoreWebView2NewWindowRequestedEventArgs,
                ICoreWebView2WebResourceRequestedEventArgs,
            };
            use webview2_com::{
                NewWindowRequestedEventHandler, WebResourceRequestedEventHandler,
            };
            use windows::Win32::Foundation::E_FAIL;
            use windows::Win32::UI::Shell::SHCreateMemStream;
            use windows::core::{Interface, PCWSTR, PWSTR};

            let Ok(core_wv2) = webview.controller().CoreWebView2() else {
                return;
            };

            // 拦截独立浏览器内部的 window.open / target="_blank" 弹窗，保持在当前隔离会话内打开
            let new_win_handler = NewWindowRequestedEventHandler::create(Box::new(
                move |wv: Option<ICoreWebView2>,
                      args: Option<ICoreWebView2NewWindowRequestedEventArgs>| {
                    let Some(args) = args else { return Ok(()) };
                    let mut uri_pwstr = PWSTR::null();
                    if args.Uri(&mut uri_pwstr).is_err() || uri_pwstr.is_null() {
                        return Ok(());
                    }
                    let uri_str = uri_pwstr.to_string().unwrap_or_default();
                    windows::Win32::System::Com::CoTaskMemFree(Some(uri_pwstr.0 as *const _));

                    if uri_str.starts_with("http://") || uri_str.starts_with("https://") {
                        // 若是依赖 window.opener postMessage 的内嵌授权弹窗则保留同沙箱弹窗，其余全部在当前隔离窗口内打开
                        let needs_opener = uri_str.contains("redirect_uri=postmessage")
                            || uri_str.contains("storagerelay%3A")
                            || uri_str.contains("storagerelay:");
                        if !needs_opener {
                            let _ = args.SetHandled(true.into());
                            if let Some(wv) = wv {
                                let wide_uri: Vec<u16> =
                                    uri_str.encode_utf16().chain(std::iter::once(0)).collect();
                                let _ = wv.Navigate(PCWSTR(wide_uri.as_ptr()));
                            }
                        }
                    }
                    Ok(())
                },
            ));
            let mut new_win_token = 0i64;
            let _ = core_wv2.add_NewWindowRequested(&new_win_handler, &mut new_win_token);

            for pattern in [
                "*",
                "http://localhost/*",
                "http://localhost:*/*",
                "https://localhost/*",
                "https://localhost:*/*",
                "http://127.0.0.1/*",
                "http://127.0.0.1:*/*",
                "https://127.0.0.1/*",
                "https://127.0.0.1:*/*",
            ] {
                let wide: Vec<u16> = pattern.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = core_wv2.AddWebResourceRequestedFilter(
                    PCWSTR(wide.as_ptr()),
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT,
                );
            }

            let env_res = core_wv2
                .cast::<ICoreWebView2_2>()
                .and_then(|wv2_2| wv2_2.Environment());

            let email_for_handler = email.clone();
            let win_for_handler = win_for_webview.clone();
            let latest_callback_url = std::sync::Arc::new(std::sync::Mutex::new(
                initial_localhost_url.clone().unwrap_or_default(),
            ));
            let last_auto_triggered = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
            let handler = WebResourceRequestedEventHandler::create(Box::new(
                move |wv: Option<ICoreWebView2>,
                      args: Option<ICoreWebView2WebResourceRequestedEventArgs>| {
                    let Some(args) = args else { return Ok(()) };
                    let Ok(req) = args.Request() else { return Ok(()) };
                    let mut uri_pwstr = PWSTR::null();
                    if req.Uri(&mut uri_pwstr).is_err() || uri_pwstr.is_null() {
                        return Ok(());
                    }
                    let uri_str = uri_pwstr.to_string().unwrap_or_default();
                    windows::Win32::System::Com::CoTaskMemFree(Some(uri_pwstr.0 as *const _));

                    if uri_str.contains("__gvault_manual_callback__") {
                        let cb_url = latest_callback_url
                            .lock()
                            .ok()
                            .map(|g| g.clone())
                            .unwrap_or_default();
                        if !cb_url.is_empty() {
                            trigger_localhost_callback_in_background(
                                win_for_handler.clone(),
                                cb_url,
                                true,
                            );
                        }
                        if let Ok(ref env) = env_res {
                            if let Some(stream) = SHCreateMemStream(Some(b"ok")) {
                                let status_wide: Vec<u16> = "OK".encode_utf16().chain(std::iter::once(0)).collect();
                                let headers_wide: Vec<u16> = "Content-Type: text/plain\r\nCache-Control: no-store"
                                    .encode_utf16()
                                    .chain(std::iter::once(0))
                                    .collect();
                                if let Ok(resp) = env.CreateWebResourceResponse(
                                    &stream,
                                    200,
                                    PCWSTR(status_wide.as_ptr()),
                                    PCWSTR(headers_wide.as_ptr()),
                                ) {
                                    let _ = args.SetResponse(&resp);
                                }
                            }
                        }
                        return Ok(());
                    }

                    if is_localhost_callback_str(&uri_str) {
                        if let Ok(mut guard) = latest_callback_url.lock() {
                            *guard = uri_str.clone();
                        }
                        let should_auto = if let Ok(mut guard) = last_auto_triggered.lock() {
                            if *guard != uri_str {
                                *guard = uri_str.clone();
                                true
                            } else {
                                false
                            }
                        } else {
                            true
                        };
                        if should_auto {
                            trigger_localhost_callback_in_background(
                                win_for_handler.clone(),
                                uri_str.clone(),
                                false,
                            );
                        }

                        let html = build_localhost_callback_html(&uri_str, &email_for_handler);
                        if let Ok(ref env) = env_res {
                            let bytes = html.as_bytes();
                            if let Some(stream) = SHCreateMemStream(Some(bytes)) {
                                let status_wide: Vec<u16> = "OK".encode_utf16().chain(std::iter::once(0)).collect();
                                let headers_wide: Vec<u16> = "Content-Type: text/html; charset=utf-8\r\nCache-Control: no-store"
                                    .encode_utf16()
                                    .chain(std::iter::once(0))
                                    .collect();
                                if let Ok(resp) = env.CreateWebResourceResponse(
                                    &stream,
                                    200,
                                    PCWSTR(status_wide.as_ptr()),
                                    PCWSTR(headers_wide.as_ptr()),
                                ) {
                                    let _ = args.SetResponse(&resp);
                                    return Ok(());
                                }
                            }
                        }
                        if let Some(wv) = wv {
                            let html_wide: Vec<u16> = html.encode_utf16().chain(std::iter::once(0)).collect();
                            let _ = wv.NavigateToString(PCWSTR(html_wide.as_ptr()));
                            return Err(windows::core::Error::from_hresult(E_FAIL));
                        }
                    }
                    Ok(())
                },
            ));

            let mut token = 0i64;
            let _ = core_wv2.add_WebResourceRequested(&handler, &mut token);

            if let Some(init_url) = initial_localhost_url {
                trigger_localhost_callback_in_background(
                    win_for_webview.clone(),
                    init_url.clone(),
                    false,
                );
                let html = build_localhost_callback_html(&init_url, &email);
                let html_wide: Vec<u16> = html.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = core_wv2.NavigateToString(PCWSTR(html_wide.as_ptr()));
            }
        }
    });
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

    let mut parsed_url = tauri::Url::parse(&url).map_err(|e| format!("无效网址: {e}"))?;
    if !clean_email.is_empty()
        && parsed_url
            .host_str()
            .map(|h| h.eq_ignore_ascii_case("accounts.google.com"))
            .unwrap_or(false)
    {
        let path = parsed_url.path();
        let is_oauth_or_login = path.contains("/oauth")
            || path.contains("/signin")
            || path.contains("/ServiceLogin")
            || path.contains("/AccountChooser")
            || path.contains("/v3/signin");
        let has_login_hint = parsed_url
            .query_pairs()
            .any(|(k, _)| k.eq_ignore_ascii_case("login_hint"));
        if is_oauth_or_login && !has_login_hint {
            parsed_url
                .query_pairs_mut()
                .append_pair("login_hint", email.trim());
        }
    }

    let safe_email_json = serde_json::to_string(&email).unwrap_or_else(|_| "\"\"".to_string());
    let safe_pwd_json = serde_json::to_string(&password.as_deref().unwrap_or_default())
        .unwrap_or_else(|_| "\"\"".to_string());
    let safe_rec_email_json =
        serde_json::to_string(&recovery_email.as_deref().unwrap_or_default())
            .unwrap_or_else(|_| "\"\"".to_string());
    let normalized_totp_secret = totp_secret
        .as_ref()
        .and_then(|s| totp::normalize_totp_secret(s).ok())
        .unwrap_or_default();
    let safe_totp_secret_json =
        serde_json::to_string(&normalized_totp_secret).unwrap_or_else(|_| "\"\"".to_string());

    let autofill_script = format!(
        r#"
        (function() {{
            const targetEmail = {safe_email_json};
            const targetPassword = {safe_pwd_json};
            const targetTotpSecret = {safe_totp_secret_json};
            const targetRecEmail = {safe_rec_email_json};
            let emailFilled = false;
            let pwdFilled = false;
            let totpFilled = false;
            let lastAutoFilledTotp = "";
            let recEmailFilled = false;

            function base32ToBytes(b32) {{
                const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
                const clean = (b32 || "").replace(/[^A-Z2-7]/gi, "").toUpperCase();
                let bits = 0, value = 0;
                const out = [];
                for (let i = 0; i < clean.length; i++) {{
                    const idx = alphabet.indexOf(clean[i]);
                    if (idx === -1) continue;
                    value = (value << 5) | idx;
                    bits += 5;
                    if (bits >= 8) {{
                        out.push((value >>> (bits - 8)) & 0xff);
                        bits -= 8;
                    }}
                }}
                return new Uint8Array(out);
            }}

            function sha1Bytes(msg) {{
                const ml = msg.length * 8;
                const withOne = msg.length + 1;
                const padLen = (withOne % 64 <= 56 ? 56 : 120) - (withOne % 64);
                const totalLen = withOne + padLen + 8;
                const buf = new Uint8Array(totalLen);
                buf.set(msg, 0);
                buf[msg.length] = 0x80;
                const dv = new DataView(buf.buffer);
                dv.setUint32(totalLen - 4, ml >>> 0, false);
                dv.setUint32(totalLen - 8, Math.floor(ml / 4294967296) >>> 0, false);

                let h0 = 0x67452301, h1 = 0xefcdab89, h2 = 0x98badcfe, h3 = 0x10325476, h4 = 0xc3d2e1f0;
                const w = new Int32Array(80);
                for (let i = 0; i < totalLen; i += 64) {{
                    for (let j = 0; j < 16; j++) {{
                        w[j] = dv.getInt32(i + j * 4, false);
                    }}
                    for (let j = 16; j < 80; j++) {{
                        const n = w[j - 3] ^ w[j - 8] ^ w[j - 14] ^ w[j - 16];
                        w[j] = (n << 1) | (n >>> 31);
                    }}
                    let a = h0, b = h1, c = h2, d = h3, e = h4;
                    for (let j = 0; j < 80; j++) {{
                        let f, k;
                        if (j < 20) {{
                            f = (b & c) | ((~b) & d);
                            k = 0x5a827999;
                        }} else if (j < 40) {{
                            f = b ^ c ^ d;
                            k = 0x6ed9eba1;
                        }} else if (j < 60) {{
                            f = (b & c) | (b & d) | (c & d);
                            k = 0x8f1bbcdc;
                        }} else {{
                            f = b ^ c ^ d;
                            k = 0xca62c1d6;
                        }}
                        const temp = (((a << 5) | (a >>> 27)) + f + e + k + w[j]) | 0;
                        e = d; d = c; c = (b << 30) | (b >>> 2); b = a; a = temp;
                    }}
                    h0 = (h0 + a) | 0;
                    h1 = (h1 + b) | 0;
                    h2 = (h2 + c) | 0;
                    h3 = (h3 + d) | 0;
                    h4 = (h4 + e) | 0;
                }}
                const out = new Uint8Array(20);
                const outDv = new DataView(out.buffer);
                outDv.setInt32(0, h0, false);
                outDv.setInt32(4, h1, false);
                outDv.setInt32(8, h2, false);
                outDv.setInt32(12, h3, false);
                outDv.setInt32(16, h4, false);
                return out;
            }}

            function computeRealtimeTotp(secret) {{
                if (!secret) return "";
                let key = base32ToBytes(secret);
                if (!key.length) return "";
                if (key.length > 64) key = sha1Bytes(key);
                const ipad = new Uint8Array(64);
                const opad = new Uint8Array(64);
                for (let i = 0; i < 64; i++) {{
                    const b = i < key.length ? key[i] : 0;
                    ipad[i] = b ^ 0x36;
                    opad[i] = b ^ 0x5c;
                }}
                const counter = Math.floor(Date.now() / 30000);
                const msg = new Uint8Array(8);
                const msgDv = new DataView(msg.buffer);
                msgDv.setUint32(0, Math.floor(counter / 4294967296) >>> 0, false);
                msgDv.setUint32(4, counter >>> 0, false);

                const innerInput = new Uint8Array(72);
                innerInput.set(ipad, 0);
                innerInput.set(msg, 64);
                const innerHash = sha1Bytes(innerInput);

                const outerInput = new Uint8Array(84);
                outerInput.set(opad, 0);
                outerInput.set(innerHash, 64);
                const hmac = sha1Bytes(outerInput);

                const offset = hmac[19] & 0x0f;
                const binCode = ((hmac[offset] & 0x7f) << 24) |
                                ((hmac[offset + 1] & 0xff) << 16) |
                                ((hmac[offset + 2] & 0xff) << 8) |
                                (hmac[offset + 3] & 0xff);
                return String(binCode % 1000000).padStart(6, "0");
            }}

            function autoFill() {{
                if (!window.location.hostname.includes("google.com")) return;

                // 1. Fill Email / Phone
                if (targetEmail && !emailFilled) {{
                    const emailInput = document.getElementById("identifierId") || 
                                       document.querySelector('input[type="email"]') ||
                                       document.querySelector('input[name="identifier"]');
                    if (emailInput && emailInput.offsetParent !== null) {{
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
                    if (pwdInput && pwdInput.offsetParent !== null) {{
                        if (pwdInput.value !== targetPassword) {{
                            pwdInput.focus();
                            pwdInput.value = targetPassword;
                            pwdInput.dispatchEvent(new Event("input", {{ bubbles: true }}));
                            pwdInput.dispatchEvent(new Event("change", {{ bubbles: true }}));
                        }}
                        pwdFilled = true;
                    }}
                }}

                // 3. Fill 2FA TOTP code in real time (and auto-refresh when 30s period rolls over)
                if (targetTotpSecret) {{
                    const totpInput = document.querySelector('input[name="totpPin"]') ||
                                      document.getElementById("idvPin") ||
                                      document.querySelector('input[type="tel"][maxlength="6"]') ||
                                      document.querySelector('input[autocomplete="one-time-code"]');
                    if (totpInput && totpInput.offsetParent !== null) {{
                        const liveTotp = computeRealtimeTotp(targetTotpSecret);
                        if (liveTotp) {{
                            const shouldUpdate = !totpFilled || (totpInput.value === lastAutoFilledTotp && totpInput.value !== liveTotp);
                            if (shouldUpdate && totpInput.value !== liveTotp) {{
                                totpInput.focus();
                                totpInput.value = liveTotp;
                                totpInput.dispatchEvent(new Event("input", {{ bubbles: true }}));
                                totpInput.dispatchEvent(new Event("change", {{ bubbles: true }}));
                                lastAutoFilledTotp = liveTotp;
                                totpFilled = true;
                            }}
                        }}
                    }}
                }}

                // 4. Fill Recovery Email if present
                if (targetRecEmail && !recEmailFilled) {{
                    const recInput = document.getElementById("knowledge-preregistered-email-response") ||
                                     document.querySelector('input[name="knowledgePreregisteredEmailResponse"]') ||
                                     document.querySelector('input[type="email"][aria-label*="辅助"]') ||
                                     document.querySelector('input[type="email"][aria-label*="recovery"]');
                    if (recInput && recInput.offsetParent !== null) {{
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

            let lastActiveInput = null;
            document.addEventListener("focusin", (e) => {{
                const el = e.target;
                if (el && el.tagName === "INPUT" && el.type !== "hidden" && el.type !== "checkbox" && el.type !== "radio") {{
                    lastActiveInput = el;
                }}
            }}, true);

            function fillValueIntoInput(el, val) {{
                if (!el || !val) return false;
                try {{
                    el.focus();
                    const nativeSetter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value")?.set;
                    if (nativeSetter) {{
                        nativeSetter.call(el, val);
                    }} else {{
                        el.value = val;
                    }}
                    el.dispatchEvent(new Event("input", {{ bubbles: true }}));
                    el.dispatchEvent(new Event("change", {{ bubbles: true }}));
                    return true;
                }} catch (_) {{
                    return false;
                }}
            }}

            async function copyToClipboardSafe(text) {{
                if (!text) return false;
                try {{
                    await navigator.clipboard.writeText(text);
                    return true;
                }} catch (_) {{
                    try {{
                        const ta = document.createElement("textarea");
                        ta.value = text;
                        ta.style.position = "fixed";
                        ta.style.opacity = "0";
                        document.body.appendChild(ta);
                        ta.select();
                        document.execCommand("copy");
                        ta.remove();
                        return true;
                    }} catch (_) {{
                        return false;
                    }}
                }}
            }}

            function mountFloatingCapsule() {{
                if (window !== window.top) return;
                if (document.getElementById("__gvault_capsule_host__")) return;
                if (document.getElementById("urlText") && document.title.includes("授权回调")) return;
                if (!document.body && !document.documentElement) return;

                const host = document.createElement("div");
                host.id = "__gvault_capsule_host__";
                host.style.cssText = "position:fixed;z-index:2147483647;right:16px;bottom:24px;pointer-events:none;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;";
                const savedTop = sessionStorage.getItem("__gvault_cap_top__");
                if (savedTop) {{
                    const y = parseFloat(savedTop);
                    if (!isNaN(y) && y >= 12 && y <= window.innerHeight - 60) {{
                        host.style.bottom = "auto";
                        host.style.top = y + "px";
                    }}
                }}

                const shadow = host.attachShadow({{ mode: "open" }});
                shadow.innerHTML = `
                    <style>
                        * {{ box-sizing: border-box; user-select: none; -webkit-user-select: none; }}
                        .wrap {{ pointer-events: auto; position: relative; display: flex; flex-direction: column; align-items: flex-end; }}
                        .pill {{
                            display: inline-flex; align-items: center; gap: 6px;
                            height: 36px; padding: 0 13px; border-radius: 999px;
                            background: rgba(255, 255, 255, 0.96); color: #202124;
                            border: 1px solid rgba(26, 115, 232, 0.32);
                            box-shadow: 0 4px 16px rgba(15, 23, 42, 0.14);
                            font-size: 12px; font-weight: 600; cursor: pointer;
                            backdrop-filter: blur(10px); transition: box-shadow 0.15s, border-color 0.15s;
                        }}
                        .pill:hover {{ border-color: #1a73e8; box-shadow: 0 6px 20px rgba(26, 115, 232, 0.22); }}
                        .grip {{ color: #9aa0a6; font-size: 13px; margin-right: 1px; cursor: grab; }}
                        .code {{ font-family: ui-monospace, SFMono-Regular, Consolas, monospace; color: #1a73e8; font-weight: 700; letter-spacing: 0.5px; }}
                        .sec {{ font-size: 11px; color: #5f6368; font-weight: 500; }}
                        .panel {{
                            display: none; width: 288px; border-radius: 18px; padding: 12px;
                            background: rgba(255, 255, 255, 0.98); color: #202124;
                            border: 1px solid #dadce0;
                            box-shadow: 0 12px 32px rgba(15, 23, 42, 0.18);
                            backdrop-filter: blur(12px);
                        }}
                        .panel.open {{ display: block; }}
                        .hdr {{ display: flex; align-items: center; justify-content: space-between; gap: 8px; padding-bottom: 9px; border-bottom: 1px solid #f1f3f4; margin-bottom: 9px; }}
                        .hdr-left {{ display: flex; align-items: center; gap: 7px; min-width: 0; flex: 1; cursor: pointer; }}
                        .g-badge {{
                            width: 24px; height: 24px; border-radius: 7px; background: #e8f0fe; color: #1a73e8;
                            display: flex; align-items: center; justify-content: center; font-weight: 800; font-size: 12px; flex-shrink: 0;
                        }}
                        .email {{ font-size: 11.5px; font-weight: 700; color: #202124; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }}
                        .sub {{ font-size: 10px; color: #5f6368; }}
                        .close-btn {{
                            width: 24px; height: 24px; border-radius: 999px; border: none; background: #f1f3f4;
                            color: #5f6368; font-size: 12px; cursor: pointer; display: flex; align-items: center; justify-content: center; flex-shrink: 0;
                        }}
                        .close-btn:hover {{ background: #e8eaed; color: #202124; }}
                        .grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 6px; margin-bottom: 8px; }}
                        .card {{
                            padding: 7px 9px; border-radius: 11px; border: 1px solid #e8eaed; background: #f8fafd;
                            cursor: pointer; text-align: left; transition: all 0.12s; display: flex; flex-direction: column; gap: 2px;
                        }}
                        .card:hover {{ border-color: #1a73e8; background: #e8f0fe; }}
                        .card.full {{ grid-column: span 2; }}
                        .c-lbl {{ font-size: 10px; color: #5f6368; font-weight: 600; display: flex; align-items: center; justify-content: space-between; }}
                        .c-val {{ font-size: 11.5px; font-weight: 700; color: #202124; font-family: ui-monospace, SFMono-Regular, Consolas, monospace; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }}
                        .tools {{ display: grid; grid-template-columns: repeat(4, 1fr); gap: 5px; }}
                        .t-btn {{
                            padding: 6px 2px; border-radius: 9px; border: 1px solid #e8eaed; background: #ffffff;
                            color: #3c4043; font-size: 10.5px; font-weight: 600; cursor: pointer; text-align: center; transition: all 0.12s;
                        }}
                        .t-btn:hover {{ background: #f1f3f4; border-color: #dadce0; color: #1a73e8; }}
                        .toast {{
                            position: absolute; bottom: calc(100% + 8px); right: 0;
                            background: rgba(32, 33, 36, 0.92); color: #fff; padding: 5px 11px;
                            border-radius: 999px; font-size: 11px; font-weight: 600; white-space: nowrap;
                            box-shadow: 0 4px 12px rgba(0,0,0,0.18); opacity: 0; pointer-events: none; transition: opacity 0.15s;
                        }}
                        .toast.show {{ opacity: 1; }}
                    </style>
                    <div class="wrap">
                        <div class="toast" id="capToast"></div>
                        <div class="pill" id="capPill" title="点击展开快捷凭据面板（支持上下拖拽）">
                            <span class="grip">⠿</span>
                            <span id="pillText">🔑 快捷凭据</span>
                        </div>
                        <div class="panel" id="capPanel">
                            <div class="hdr">
                                <div class="hdr-left" id="copyEmailHdr" title="点击填入并复制邮箱">
                                    <div class="g-badge">G</div>
                                    <div style="min-width:0;flex:1">
                                        <div class="email" id="hdrEmail"></div>
                                        <div class="sub">点击卡片自动填入并复制</div>
                                    </div>
                                </div>
                                <button class="close-btn" id="capClose" title="收起">✕</button>
                            </div>
                            <div class="grid">
                                <button class="card" id="btnFillPwd">
                                    <div class="c-lbl"><span>🔑 登录密码</span></div>
                                    <div class="c-val" id="valPwd"></div>
                                </button>
                                <button class="card" id="btnFillTotp">
                                    <div class="c-lbl"><span>⏱️ 2FA 验证码</span><span id="valTotpSec"></span></div>
                                    <div class="c-val" id="valTotp" style="color:#1a73e8"></div>
                                </button>
                                <button class="card full" id="btnFillRec" style="display:none">
                                    <div class="c-lbl"><span>🛡️ 辅助邮箱</span></div>
                                    <div class="c-val" id="valRec"></div>
                                </button>
                            </div>
                            <div class="tools">
                                <button class="t-btn" id="btnBack" title="返回上一页">← 后退</button>
                                <button class="t-btn" id="btnReload" title="刷新当前页面">↻ 刷新</button>
                                <button class="t-btn" id="btnRefill" title="重新执行智能填充">⚡ 重填</button>
                                <button class="t-btn" id="btnCopyUrl" title="复制当前页面网址">🔗 网址</button>
                            </div>
                        </div>
                    </div>
                `;

                (document.body || document.documentElement).appendChild(host);

                const pill = shadow.getElementById("capPill");
                const pillText = shadow.getElementById("pillText");
                const panel = shadow.getElementById("capPanel");
                const toastEl = shadow.getElementById("capToast");
                const hdrEmail = shadow.getElementById("hdrEmail");
                const valPwd = shadow.getElementById("valPwd");
                const valTotp = shadow.getElementById("valTotp");
                const valTotpSec = shadow.getElementById("valTotpSec");
                const btnFillRec = shadow.getElementById("btnFillRec");
                const valRec = shadow.getElementById("valRec");

                hdrEmail.textContent = targetEmail || "未指定邮箱";
                valPwd.textContent = targetPassword ? "••••••••" : "未记录";
                if (targetRecEmail) {{
                    btnFillRec.style.display = "flex";
                    valRec.textContent = targetRecEmail;
                }}

                let toastTimer = null;
                function showCapToast(msg) {{
                    toastEl.textContent = msg;
                    toastEl.classList.add("show");
                    if (toastTimer) clearTimeout(toastTimer);
                    toastTimer = setTimeout(() => toastEl.classList.remove("show"), 1800);
                }}

                let autoCollapseTimer = null;
                function resetAutoCollapse() {{
                    if (autoCollapseTimer) clearTimeout(autoCollapseTimer);
                    autoCollapseTimer = setTimeout(() => {{
                        panel.classList.remove("open");
                        pill.style.display = "inline-flex";
                    }}, 8000);
                }}

                function openPanel() {{
                    pill.style.display = "none";
                    panel.classList.add("open");
                    resetAutoCollapse();
                }}

                function closePanel() {{
                    if (autoCollapseTimer) clearTimeout(autoCollapseTimer);
                    panel.classList.remove("open");
                    pill.style.display = "inline-flex";
                }}

                let startY = 0, startTop = 0, dragging = false;
                pill.addEventListener("mousedown", (e) => {{
                    if (e.button !== 0) return;
                    startY = e.clientY;
                    startTop = host.getBoundingClientRect().top;
                    dragging = false;

                    const onMove = (me) => {{
                        const dy = me.clientY - startY;
                        if (Math.abs(dy) > 5) {{
                            dragging = true;
                            const nextTop = Math.max(12, Math.min(window.innerHeight - 52, startTop + dy));
                            host.style.bottom = "auto";
                            host.style.top = nextTop + "px";
                            sessionStorage.setItem("__gvault_cap_top__", String(nextTop));
                        }}
                    }};
                    const onUp = () => {{
                        window.removeEventListener("mousemove", onMove);
                        window.removeEventListener("mouseup", onUp);
                        if (!dragging) {{
                            openPanel();
                        }}
                    }};
                    window.addEventListener("mousemove", onMove);
                    window.addEventListener("mouseup", onUp);
                }});

                shadow.getElementById("capClose").addEventListener("click", (e) => {{
                    e.stopPropagation();
                    closePanel();
                }});

                shadow.getElementById("copyEmailHdr").addEventListener("click", async () => {{
                    resetAutoCollapse();
                    if (!targetEmail) return;
                    await copyToClipboardSafe(targetEmail);
                    const emailEl = document.getElementById("identifierId") || document.querySelector('input[type="email"]') || lastActiveInput;
                    const filled = fillValueIntoInput(emailEl, targetEmail);
                    showCapToast(filled ? "✓ 已填入并复制邮箱" : "✓ 已复制邮箱");
                }});

                shadow.getElementById("btnFillPwd").addEventListener("click", async () => {{
                    resetAutoCollapse();
                    if (!targetPassword) {{
                        showCapToast("当前账号未记录密码");
                        return;
                    }}
                    await copyToClipboardSafe(targetPassword);
                    const pwdEl = document.querySelector('input[name="Passwd"]') || document.querySelector('input[type="password"]') || lastActiveInput;
                    const filled = fillValueIntoInput(pwdEl, targetPassword);
                    showCapToast(filled ? "✓ 已填入并复制密码" : "✓ 已复制密码");
                }});

                shadow.getElementById("btnFillTotp").addEventListener("click", async () => {{
                    resetAutoCollapse();
                    const code = computeRealtimeTotp(targetTotpSecret);
                    if (!code) {{
                        showCapToast("当前账号未配置 2FA 密钥");
                        return;
                    }}
                    await copyToClipboardSafe(code);
                    const totpEl = document.querySelector('input[name="totpPin"]') ||
                                   document.getElementById("idvPin") ||
                                   document.querySelector('input[type="tel"][maxlength="6"]') ||
                                   document.querySelector('input[autocomplete="one-time-code"]') ||
                                   lastActiveInput;
                    const filled = fillValueIntoInput(totpEl, code);
                    showCapToast(filled ? `✓ 已填入并复制验证码 ${{code}}` : `✓ 已复制验证码 ${{code}}`);
                }});

                btnFillRec.addEventListener("click", async () => {{
                    resetAutoCollapse();
                    if (!targetRecEmail) return;
                    await copyToClipboardSafe(targetRecEmail);
                    const recEl = document.getElementById("knowledge-preregistered-email-response") ||
                                  document.querySelector('input[name="knowledgePreregisteredEmailResponse"]') ||
                                  document.querySelector('input[type="email"]') ||
                                  lastActiveInput;
                    const filled = fillValueIntoInput(recEl, targetRecEmail);
                    showCapToast(filled ? "✓ 已填入并复制辅助邮箱" : "✓ 已复制辅助邮箱");
                }});

                shadow.getElementById("btnBack").addEventListener("click", () => {{
                    window.history.back();
                }});
                shadow.getElementById("btnReload").addEventListener("click", () => {{
                    window.location.reload();
                }});
                shadow.getElementById("btnRefill").addEventListener("click", () => {{
                    resetAutoCollapse();
                    emailFilled = false;
                    pwdFilled = false;
                    totpFilled = false;
                    recEmailFilled = false;
                    autoFill();
                    showCapToast("⚡ 已重新执行自动填充");
                }});
                shadow.getElementById("btnCopyUrl").addEventListener("click", async () => {{
                    resetAutoCollapse();
                    await copyToClipboardSafe(window.location.href);
                    showCapToast("✓ 已复制当前页面网址");
                }});

                function refreshCapsuleTotp() {{
                    if (targetTotpSecret) {{
                        const code = computeRealtimeTotp(targetTotpSecret);
                        const rem = 30 - (Math.floor(Date.now() / 1000) % 30);
                        if (code) {{
                            const fmt = code.slice(0, 3) + " " + code.slice(3);
                            pillText.innerHTML = `⏱️ <span class="code">${{fmt}}</span> <span class="sec">(${{rem}}s)</span>`;
                            valTotp.textContent = fmt;
                            valTotpSec.textContent = rem + "s";
                            return;
                        }}
                    }}
                    pillText.textContent = targetRecEmail ? "🛡️ 辅助凭据" : "🔑 快捷凭据";
                    valTotp.textContent = "未配置";
                    valTotpSec.textContent = "";
                }}

                refreshCapsuleTotp();
                setInterval(refreshCapsuleTotp, 1000);
            }}

            function tickAll() {{
                autoFill();
                mountFloatingCapsule();
            }}

            window.addEventListener("DOMContentLoaded", tickAll);
            window.addEventListener("load", tickAll);
            const observer = new MutationObserver(tickAll);
            if (document.documentElement) {{
                observer.observe(document.documentElement, {{ childList: true, subtree: true }});
            }}
            const timer = setInterval(tickAll, 400);
            setTimeout(() => {{
                clearInterval(timer);
                observer.disconnect();
            }}, 600000);
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
            normalized_totp_secret,
            safe_totp_secret_json,
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
        let url_j = env.new_string(parsed_url.as_str()).map_err(|e| e.to_string())?;
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
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            let _ = w.navigate(parsed_url);
            let _ = w.eval(&autofill_script);
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

        let initial_localhost_url = if is_localhost_callback_url(&parsed_url) {
            Some(parsed_url.as_str().to_string())
        } else {
            None
        };

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
        .data_directory(session_dir);

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
                builder = builder.additional_browser_args(&format!(
                    "--proxy-server={} --proxy-bypass-list=<-loopback>;localhost;127.0.0.1;[::1]",
                    clean_proxy
                ));
            }
        }

        let _window = builder
            .build()
            .map_err(|e| format!("创建独立窗口失败: {e}"))?;
        let _ = _window.unminimize();
        let _ = _window.show();
        let _ = _window.set_focus();
        #[cfg(target_os = "windows")]
        attach_webview2_localhost_interceptor(&_window, email.clone(), initial_localhost_url);
        #[cfg(debug_assertions)]
        let _ = _window.open_devtools();
        Ok(())
    }
}

#[tauri::command]
async fn clear_account_browser_session(
    app: tauri::AppHandle,
    email: String,
) -> Result<String, String> {
    let clean_email = email.trim().to_lowercase();
    let email_uuid = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, clean_email.as_bytes());
    let safe_label = format!("browser-{}", &email_uuid.to_string()[..8]);

    #[cfg(not(target_os = "android"))]
    {
        if let Some(w) = app.get_webview_window(&safe_label) {
            let _ = w.close();
            std::thread::sleep(std::time::Duration::from_millis(350));
        }

        let session_dir = vault::vault_data_dir().join("sessions").join(&safe_label);
        if session_dir.exists() {
            let mut removed = false;
            for _ in 0..8 {
                if std::fs::remove_dir_all(&session_dir).is_ok() {
                    removed = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            if !removed {
                let eb_default = session_dir.join("EBWebView").join("Default");
                for sub in [
                    "Network",
                    "Local Storage",
                    "Session Storage",
                    "IndexedDB",
                    "Cache",
                    "Code Cache",
                    "Service Worker",
                    "Cookies",
                    "Cookies-journal",
                ] {
                    let target = eb_default.join(sub);
                    let _ = if target.is_dir() {
                        std::fs::remove_dir_all(&target)
                    } else {
                        std::fs::remove_file(&target)
                    };
                }
            }
        }
        Ok(format!("已清理账号 [{}] 的独立会话 Cookie 与本地缓存", email))
    }

    #[cfg(target_os = "android")]
    {
        let _ = (app, safe_label);
        Ok(format!("请在独立浏览器底部抽屉中点击「清理沙箱」清理 [{}] 的会话", email))
    }
}

mod takeover_engine {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;

    static TAKEOVER_SESSION_ID: AtomicU64 = AtomicU64::new(0);
    static TAKEOVER_ACTIVE: AtomicBool = AtomicBool::new(false);
    static CAPTURED_TAKEOVER_URL: Mutex<Option<String>> = Mutex::new(None);
    static CLI_STARTUP_CHECKED: AtomicBool = AtomicBool::new(false);

    pub fn extract_url_from_text(text: &str, require_oauth_hint: bool) -> Option<String> {
        let lower = text.to_lowercase();
        if lower.contains("--type=")
            || lower.contains("--no-startup-window")
            || lower.contains("--watcher")
            || lower.contains("crashpad")
            || lower.contains("clients2.google.com/cr/report")
        {
            return None;
        }

        let candidate_slice = if let Some(idx) = lower.find("--single-argument") {
            let after = &text[idx + "--single-argument".len()..];
            after.trim().trim_matches(|c| c == '"' || c == '\'')
        } else if let Some(idx) = lower.find("https://").or_else(|| lower.find("http://")) {
            let rest = &text[idx..];
            let end = rest
                .find(|c: char| {
                    c == '"' || c == '\'' || c == ' ' || c == '\t' || c == '\r' || c == '\n'
                })
                .unwrap_or(rest.len());
            &rest[..end]
        } else {
            return None;
        };

        let unescaped = candidate_slice.replace("^&", "&");
        let parsed = tauri::Url::parse(&unescaped).ok()?;
        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return None;
        }
        let host = parsed.host_str().unwrap_or_default().to_lowercase();
        if host.is_empty() {
            return None;
        }

        if require_oauth_hint {
            let u_low = unescaped.to_lowercase();
            let has_hint = u_low.contains("accounts.google.com")
                || u_low.contains("google.com")
                || u_low.contains("oauth")
                || u_low.contains("client_id=")
                || u_low.contains("redirect_uri=")
                || u_low.contains("signin")
                || u_low.contains("login")
                || u_low.contains("auth");
            if !has_hint {
                return None;
            }
        }

        Some(unescaped)
    }

    pub fn set_captured_url(url: String) {
        if let Ok(mut guard) = CAPTURED_TAKEOVER_URL.lock() {
            if guard.is_none() {
                *guard = Some(url);
            }
        }
        TAKEOVER_ACTIVE.store(false, Ordering::SeqCst);
    }

    pub fn take_captured_url() -> Option<String> {
        if !CLI_STARTUP_CHECKED.swap(true, Ordering::SeqCst) {
            for arg in std::env::args().skip(1) {
                if let Some(u) = extract_url_from_text(&arg, false) {
                    TAKEOVER_ACTIVE.store(false, Ordering::SeqCst);
                    return Some(u);
                }
            }
        }
        CAPTURED_TAKEOVER_URL.lock().ok().and_then(|mut g| g.take())
    }

    pub fn stop_takeover() {
        TAKEOVER_ACTIVE.store(false, Ordering::SeqCst);
        TAKEOVER_SESSION_ID.fetch_add(1, Ordering::SeqCst);
    }

    pub fn start_takeover(duration_secs: u64) {
        if let Ok(mut guard) = CAPTURED_TAKEOVER_URL.lock() {
            *guard = None;
        }
        let session_id = TAKEOVER_SESSION_ID.fetch_add(1, Ordering::SeqCst) + 1;
        TAKEOVER_ACTIVE.store(true, Ordering::SeqCst);

        #[cfg(target_os = "windows")]
        windows_sniffer::spawn_sniffer_thread(session_id, duration_secs);
        #[cfg(not(target_os = "windows"))]
        let _ = (session_id, duration_secs);
    }

    pub fn is_active() -> bool {
        TAKEOVER_ACTIVE.load(Ordering::SeqCst)
    }

    #[cfg(target_os = "windows")]
    mod windows_sniffer {
        use super::{
            extract_url_from_text, set_captured_url, TAKEOVER_ACTIVE, TAKEOVER_SESSION_ID,
        };
        use std::collections::HashSet;
        use std::sync::atomic::Ordering;
        use std::time::{Duration, Instant};

        const TH32CS_SNAPPROCESS: u32 = 0x00000002;
        const INVALID_HANDLE_VALUE: isize = -1;
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const PROCESS_TERMINATE: u32 = 0x0001;
        const PROCESS_COMMAND_LINE_INFORMATION: u32 = 60;
        const CF_UNICODETEXT: u32 = 13;

        #[repr(C)]
        struct PROCESSENTRY32W {
            dw_size: u32,
            cnt_usage: u32,
            th32_process_id: u32,
            th32_default_heap_id: usize,
            th32_module_id: u32,
            cnt_threads: u32,
            th32_parent_process_id: u32,
            pc_pri_class_base: i32,
            dw_flags: u32,
            sz_exe_file: [u16; 260],
        }

        #[repr(C)]
        struct UNICODE_STRING {
            length: u16,
            maximum_length: u16,
            buffer: *const u16,
        }

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> isize;
            fn Process32FirstW(h_snapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;
            fn Process32NextW(h_snapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;
            fn OpenProcess(
                dw_desired_access: u32,
                b_inherit_handle: i32,
                dw_process_id: u32,
            ) -> isize;
            fn TerminateProcess(h_process: isize, u_exit_code: u32) -> i32;
            fn CloseHandle(h_object: isize) -> i32;
            fn GlobalLock(h_mem: isize) -> *const u16;
            fn GlobalUnlock(h_mem: isize) -> i32;
        }

        #[link(name = "ntdll")]
        unsafe extern "system" {
            fn NtQueryInformationProcess(
                process_handle: isize,
                process_information_class: u32,
                process_information: *mut u8,
                process_information_length: u32,
                return_length: *mut u32,
            ) -> i32;
        }

        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetClipboardSequenceNumber() -> u32;
            fn OpenClipboard(h_wnd_new_owner: isize) -> i32;
            fn GetClipboardData(u_format: u32) -> isize;
            fn CloseClipboard() -> i32;
        }

        fn read_clipboard_text() -> Option<String> {
            unsafe {
                if OpenClipboard(0) == 0 {
                    return None;
                }
                let result = (|| {
                    let h_data = GetClipboardData(CF_UNICODETEXT);
                    if h_data == 0 {
                        return None;
                    }
                    let ptr = GlobalLock(h_data);
                    if ptr.is_null() {
                        return None;
                    }
                    let mut len = 0usize;
                    while len < 8192 && *ptr.add(len) != 0 {
                        len += 1;
                    }
                    let slice = std::slice::from_raw_parts(ptr, len);
                    let s = String::from_utf16_lossy(slice);
                    let _ = GlobalUnlock(h_data);
                    Some(s)
                })();
                let _ = CloseClipboard();
                result
            }
        }

        fn is_browser_exe_name(exe: &str) -> bool {
            matches!(
                exe,
                "msedge.exe"
                    | "chrome.exe"
                    | "brave.exe"
                    | "firefox.exe"
                    | "vivaldi.exe"
                    | "opera.exe"
                    | "arc.exe"
                    | "centbrowser.exe"
                    | "chromium.exe"
                    | "thorium.exe"
                    | "waterfox.exe"
                    | "librewolf.exe"
                    | "360chrome.exe"
                    | "qqbrowser.exe"
                    | "sogouexplorer.exe"
            )
        }

        fn query_process_cmdline(h_proc: isize, buf: &mut Vec<u8>) -> Option<String> {
            unsafe {
                let mut ret_len: u32 = 0;
                let status = NtQueryInformationProcess(
                    h_proc,
                    PROCESS_COMMAND_LINE_INFORMATION,
                    buf.as_mut_ptr(),
                    buf.len() as u32,
                    &mut ret_len,
                );
                if status < 0 {
                    return None;
                }
                if (ret_len as usize) < std::mem::size_of::<UNICODE_STRING>() {
                    return None;
                }
                let ustr = &*(buf.as_ptr() as *const UNICODE_STRING);
                if ustr.length == 0 || ustr.buffer.is_null() {
                    return None;
                }
                let char_count = (ustr.length / 2) as usize;
                let slice = std::slice::from_raw_parts(ustr.buffer, char_count);
                Some(String::from_utf16_lossy(slice))
            }
        }

        fn snapshot_all_pids() -> HashSet<u32> {
            let mut set = HashSet::with_capacity(512);
            unsafe {
                let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
                if snap == INVALID_HANDLE_VALUE || snap == 0 {
                    return set;
                }
                let mut entry: PROCESSENTRY32W = std::mem::zeroed();
                entry.dw_size = std::mem::size_of::<PROCESSENTRY32W>() as u32;
                if Process32FirstW(snap, &mut entry) != 0 {
                    loop {
                        set.insert(entry.th32_process_id);
                        if Process32NextW(snap, &mut entry) == 0 {
                            break;
                        }
                    }
                }
                let _ = CloseHandle(snap);
            }
            set
        }

        pub fn spawn_sniffer_thread(session_id: u64, duration_secs: u64) {
            std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(duration_secs);
                let self_pid = std::process::id();
                let mut seen_pids = snapshot_all_pids();
                seen_pids.insert(self_pid);

                let mut last_clip_seq = unsafe { GetClipboardSequenceNumber() };
                let initial_clip_text = read_clipboard_text().unwrap_or_default();
                let mut cmd_buf = vec![0u8; 32768];

                while Instant::now() < deadline {
                    if !TAKEOVER_ACTIVE.load(Ordering::SeqCst)
                        || TAKEOVER_SESSION_ID.load(Ordering::SeqCst) != session_id
                    {
                        return;
                    }

                    // 1. 剪贴板 OAuth 链接雷达（仅当剪贴板序列号变化时读取）
                    let cur_clip_seq = unsafe { GetClipboardSequenceNumber() };
                    if cur_clip_seq != last_clip_seq {
                        last_clip_seq = cur_clip_seq;
                        if let Some(clip_str) = read_clipboard_text() {
                            let trimmed = clip_str.trim();
                            if !trimmed.is_empty() && trimmed != initial_clip_text.trim() {
                                let is_pure_url = !trimmed.contains(char::is_whitespace)
                                    && (trimmed.starts_with("http://")
                                        || trimmed.starts_with("https://"));
                                if let Some(url) = extract_url_from_text(trimmed, !is_pure_url) {
                                    set_captured_url(url);
                                    return;
                                }
                            }
                        }
                    }

                    // 2. 毫秒级新进程命令行嗅探（拦截外部 IDE/CLI 唤起默认浏览器）
                    unsafe {
                        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
                        if snap != INVALID_HANDLE_VALUE && snap != 0 {
                            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
                            entry.dw_size = std::mem::size_of::<PROCESSENTRY32W>() as u32;
                            if Process32FirstW(snap, &mut entry) != 0 {
                                loop {
                                    let pid = entry.th32_process_id;
                                    if !seen_pids.contains(&pid) {
                                        seen_pids.insert(pid);
                                        let exe_len = entry
                                            .sz_exe_file
                                            .iter()
                                            .position(|&c| c == 0)
                                            .unwrap_or(entry.sz_exe_file.len());
                                        let exe_name =
                                            String::from_utf16_lossy(&entry.sz_exe_file[..exe_len])
                                                .to_lowercase();
                                        let is_browser = is_browser_exe_name(&exe_name);

                                        let mut h_proc = OpenProcess(
                                            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
                                            0,
                                            pid,
                                        );
                                        let can_terminate = if h_proc == 0 {
                                            h_proc = OpenProcess(
                                                PROCESS_QUERY_LIMITED_INFORMATION,
                                                0,
                                                pid,
                                            );
                                            false
                                        } else {
                                            true
                                        };

                                        if h_proc != 0 {
                                            if let Some(cmd) =
                                                query_process_cmdline(h_proc, &mut cmd_buf)
                                            {
                                                if let Some(url) =
                                                    extract_url_from_text(&cmd, !is_browser)
                                                {
                                                    if is_browser && can_terminate {
                                                        let _ = TerminateProcess(h_proc, 0);
                                                    }
                                                    let _ = CloseHandle(h_proc);
                                                    let _ = CloseHandle(snap);
                                                    set_captured_url(url);
                                                    return;
                                                }
                                            }
                                            let _ = CloseHandle(h_proc);
                                        }
                                    }

                                    if Process32NextW(snap, &mut entry) == 0 {
                                        break;
                                    }
                                }
                            }
                            let _ = CloseHandle(snap);
                        }
                    }

                    std::thread::sleep(Duration::from_millis(12));
                }

                if TAKEOVER_SESSION_ID.load(Ordering::SeqCst) == session_id {
                    TAKEOVER_ACTIVE.store(false, Ordering::SeqCst);
                }
            });
        }
    }
}

#[tauri::command]
fn start_browser_takeover(duration_secs: Option<u64>) -> Result<(), String> {
    takeover_engine::start_takeover(duration_secs.unwrap_or(60));
    Ok(())
}

#[tauri::command]
fn stop_browser_takeover() -> Result<(), String> {
    takeover_engine::stop_takeover();
    Ok(())
}

#[tauri::command]
fn poll_browser_takeover(clipboard_text: Option<String>) -> Result<Option<String>, String> {
    if let Some(url) = takeover_engine::take_captured_url() {
        return Ok(Some(url));
    }
    if takeover_engine::is_active() {
        if let Some(clip) = clipboard_text {
            let trimmed = clip.trim();
            if !trimmed.is_empty() {
                let is_pure_url = !trimmed.contains(char::is_whitespace)
                    && (trimmed.starts_with("http://") || trimmed.starts_with("https://"));
                if let Some(url) = takeover_engine::extract_url_from_text(trimmed, !is_pure_url) {
                    takeover_engine::stop_takeover();
                    return Ok(Some(url));
                }
            }
        }
    }
    Ok(None)
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

const CURRENT_APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn parse_semver(v: &str) -> (u32, u32, u32) {
    let trimmed = v.trim();
    let clean = trimmed
        .find(|c: char| c.is_ascii_digit())
        .map(|idx| &trimmed[idx..])
        .unwrap_or(trimmed);
    let parts: Vec<&str> = clean.split('.').collect();
    let major = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts
        .get(2)
        .and_then(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .ok()
        })
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
        .user_agent(concat!("google-account-vault/", env!("CARGO_PKG_VERSION")))
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
            current_version: CURRENT_APP_VERSION.to_string(),
            latest_version: CURRENT_APP_VERSION.to_string(),
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

    let tag = json["tag_name"]
        .as_str()
        .unwrap_or(CURRENT_APP_VERSION)
        .to_string();
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

    let current = parse_semver(CURRENT_APP_VERSION);
    let latest = parse_semver(&tag);
    let has_update = latest > current;

    Ok(UpdateInfo {
        current_version: CURRENT_APP_VERSION.to_string(),
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
        .user_agent(concat!("google-account-vault/", env!("CARGO_PKG_VERSION")))
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
            clear_account_browser_session,
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
            sync_webdav_smart,
            start_browser_takeover,
            stop_browser_takeover,
            poll_browser_takeover
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
