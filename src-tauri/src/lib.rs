pub mod models;
pub mod totp;
pub mod utils;
pub mod vault;
pub mod webdav;
pub mod backups;
pub mod hello;

use std::fs;
use models::{AccountRecord, TotpResult};
use parking_lot::RwLock;
use tauri::State;

pub struct AppState {
    pub master_password: RwLock<Option<String>>,
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
    Ok(HelloStatus { available, enrolled })
}

#[tauri::command]
async fn enable_windows_hello(state: State<'_, AppState>) -> Result<(), String> {
    let password = {
        let guard = state.master_password.read();
        guard.as_ref().ok_or("请先解锁保险库后再启用 Windows Hello")?.clone()
    };
    let verified = hello::verify_user_consent("验证身份以启用 Windows Hello / PIN 快速解锁").await?;
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
async fn unlock_vault_with_hello(state: State<'_, AppState>) -> Result<Vec<AccountRecord>, String> {
    let verified = hello::verify_user_consent("验证 Windows Hello / PIN 身份以解锁 Google 账号管家").await?;
    if !verified {
        return Err("身份验证未通过或已取消".to_string());
    }
    let password = hello::get_hello_credentials()?;
    let path = vault::vault_file_path();
    if !path.exists() {
        return Err("保险库文件不存在，请先创建保险库".to_string());
    }
    let blob = fs::read_to_string(&path).map_err(|e| format!("读取保险库文件失败: {e}"))?;
    let records = vault::decrypt_records(&blob, &password)?;

    *state.master_password.write() = Some(password);
    *state.records.write() = records.clone();
    Ok(records)
}

#[tauri::command]
fn unlock_vault(password: String, state: State<'_, AppState>) -> Result<Vec<AccountRecord>, String> {
    let path = vault::vault_file_path();
    if !path.exists() {
        return Err("保险库文件不存在，请先创建保险库".to_string());
    }
    let blob = fs::read_to_string(&path).map_err(|e| format!("读取保险库文件失败: {e}"))?;
    let records = vault::decrypt_records(&blob, &password)?;

    *state.master_password.write() = Some(password);
    *state.records.write() = records.clone();
    Ok(records)
}

#[tauri::command]
fn create_vault(password: String, state: State<'_, AppState>) -> Result<(), String> {
    if password.trim().is_empty() {
        return Err("主密码不能为空".to_string());
    }
    let records: Vec<AccountRecord> = Vec::new();
    vault::save_vault_atomic(&records, &password, 30)?;
    *state.master_password.write() = Some(password);
    *state.records.write() = records;
    Ok(())
}

#[tauri::command]
fn lock_vault(state: State<'_, AppState>) -> Result<(), String> {
    *state.master_password.write() = None;
    state.records.write().clear();
    Ok(())
}

#[tauri::command]
fn get_records(state: State<'_, AppState>) -> Result<Vec<AccountRecord>, String> {
    if state.master_password.read().is_none() {
        return Err("保险库已锁定".to_string());
    }
    Ok(state.records.read().clone())
}

#[tauri::command]
fn save_record(mut record: AccountRecord, state: State<'_, AppState>) -> Result<AccountRecord, String> {
    let password_lock = state.master_password.read();
    let password = password_lock.as_ref().ok_or("保险库已锁定，无法保存")?;

    if record.id.trim().is_empty() {
        record.id = uuid::Uuid::new_v4().to_string();
    }
    record.email = record.email.trim().to_string();
    record.group = record.group.trim().to_string();

    let mut records = state.records.write();
    if let Some(pos) = records.iter().position(|r| r.id == record.id) {
        records[pos] = record.clone();
    } else {
        records.push(record.clone());
    }

    vault::save_vault_atomic(&records, password, 30)?;
    Ok(record)
}

#[tauri::command]
fn delete_record(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let password_lock = state.master_password.read();
    let password = password_lock.as_ref().ok_or("保险库已锁定，无法删除")?;

    let mut records = state.records.write();
    let initial_len = records.len();
    records.retain(|r| r.id != id);

    if records.len() != initial_len {
        vault::save_vault_atomic(&records, password, 30)?;
    }
    Ok(())
}

#[tauri::command]
fn get_totp(secret: String) -> Result<TotpResult, String> {
    totp::calculate_totp(&secret)
}

#[tauri::command]
fn generate_pwd(length: usize, symbols: bool) -> Result<String, String> {
    utils::generate_password(length, symbols)
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
async fn upload_webdav_vault(config: webdav::WebDavConfig, state: State<'_, AppState>) -> Result<(), String> {
    if state.master_password.read().is_none() {
        return Err("保险库已锁定，无法上传".to_string());
    }
    webdav::upload_vault(&config).await
}

#[tauri::command]
async fn download_webdav_vault(config: webdav::WebDavConfig, state: State<'_, AppState>) -> Result<Vec<AccountRecord>, String> {
    let password = {
        let password_lock = state.master_password.read();
        password_lock.as_ref().cloned().ok_or("请先解锁保险库，以便验证并解密云端数据")?
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
fn restore_vault_backup(filename: String, state: State<'_, AppState>) -> Result<Vec<AccountRecord>, String> {
    let password_lock = state.master_password.read();
    let password = password_lock.as_ref().ok_or("请先解锁保险库，以便解密历史快照")?;
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
fn batch_import_records(new_records: Vec<AccountRecord>, state: State<'_, AppState>) -> Result<usize, String> {
    let password_lock = state.master_password.read();
    let password = password_lock.as_ref().ok_or("保险库已锁定，无法导入")?;

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
        if let Some(pos) = records.iter().position(|existing| existing.email.eq_ignore_ascii_case(&r.email)) {
            let existing = &mut records[pos];
            if !r.password.trim().is_empty() { existing.password = r.password; }
            if !r.group.trim().is_empty() { existing.group = r.group; }
            if !r.recovery_email.trim().is_empty() { existing.recovery_email = r.recovery_email; }
            if !r.phone.trim().is_empty() { existing.phone = r.phone; }
            if !r.totp.trim().is_empty() { existing.totp = r.totp; }
            if !r.backup_codes.trim().is_empty() { existing.backup_codes = r.backup_codes; }
            if !r.notes.trim().is_empty() { existing.notes = r.notes; }
        } else {
            records.push(r);
        }
        imported_count += 1;
    }

    vault::save_vault_atomic(&records, password, 30)?;
    Ok(imported_count)
}

#[tauri::command]
async fn open_in_app_browser(
    app: tauri::AppHandle,
    email: String,
    password: Option<String>,
    totp_secret: Option<String>,
    url: String,
) -> Result<(), String> {
    let clean_email = email.trim().to_lowercase();
    let email_uuid = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, clean_email.as_bytes());
    let safe_label = format!("browser-{}", &email_uuid.to_string()[..8]);
    
    let parsed_url = tauri::Url::parse(&url).map_err(|e| format!("无效网址: {e}"))?;

    let safe_email_json = serde_json::to_string(&email).unwrap_or_else(|_| "\"\"".to_string());
    let safe_pwd_json = serde_json::to_string(&password.unwrap_or_default()).unwrap_or_else(|_| "\"\"".to_string());
    let current_totp = totp_secret
        .as_ref()
        .and_then(|s| totp::calculate_totp(s).ok())
        .map(|r| r.code)
        .unwrap_or_default();
    let safe_totp_json = serde_json::to_string(&current_totp).unwrap_or_else(|_| "\"\"".to_string());

    let autofill_script = format!(
        r#"
        (function() {{
            const targetEmail = {safe_email_json};
            const targetPassword = {safe_pwd_json};
            const targetTotp = {safe_totp_json};
            let emailFilled = false;
            let pwdFilled = false;
            let totpFilled = false;

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

    // If window already exists, focus it and navigate to the requested URL
    if let Some(w) = app.get_webview_window(&safe_label) {
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.navigate(parsed_url);
        let _ = w.eval(&autofill_script);
        return Ok(());
    }

    let session_dir = vault::vault_data_dir().join("sessions").join(&safe_label);
    let _ = std::fs::create_dir_all(&session_dir);
    let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0";

    let builder = tauri::WebviewWindowBuilder::new(&app, &safe_label, tauri::WebviewUrl::External(parsed_url))
        .title(format!("Google 独立会话 - {}", email))
        .inner_size(1050.0, 750.0)
        .center()
        .user_agent(ua)
        .devtools(true)
        .initialization_script(&autofill_script)
        .data_directory(session_dir);

    let window = builder.build().map_err(|e| format!("创建独立窗口失败: {e}"))?;
    #[cfg(debug_assertions)]
    let _ = window.open_devtools();
    Ok(())
}

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState {
        master_password: RwLock::new(None),
        records: RwLock::new(Vec::new()),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(|app| {
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
            get_totp,
            generate_pwd,
            open_in_app_browser,
            test_webdav_connection,
            save_webdav_config,
            load_webdav_config,
            upload_webdav_vault,
            download_webdav_vault,
            list_vault_backups,
            restore_vault_backup,
            export_records_csv_string,
            batch_import_records,
            check_windows_hello_status,
            enable_windows_hello,
            disable_windows_hello,
            unlock_vault_with_hello
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
