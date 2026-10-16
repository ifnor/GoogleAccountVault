use crate::models::AccountRecord;
use crate::vault;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WebDavConfig {
    pub url: String,
    pub username: String,
    pub password: String,
}

pub fn webdav_config_path() -> PathBuf {
    vault::vault_data_dir().join("webdav.json")
}

pub fn load_config() -> Result<Option<WebDavConfig>, String> {
    let path = webdav_config_path();
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path).map_err(|e| format!("读取 WebDAV 配置失败: {e}"))?;
    let config: WebDavConfig =
        serde_json::from_str(&data).map_err(|e| format!("解析 WebDAV 配置失败: {e}"))?;
    Ok(Some(config))
}

pub fn save_config(config: &WebDavConfig) -> Result<(), String> {
    let dir = vault::vault_data_dir();
    let _ = fs::create_dir_all(&dir);
    let path = webdav_config_path();
    let data =
        serde_json::to_string_pretty(config).map_err(|e| format!("序列化 WebDAV 配置失败: {e}"))?;
    fs::write(&path, data).map_err(|e| format!("保存 WebDAV 配置失败: {e}"))?;
    Ok(())
}

fn normalize_base_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("WebDAV 地址必须以 http:// 或 https:// 开头".to_string());
    }
    if trimmed.ends_with('/') {
        Ok(trimmed.to_string())
    } else {
        Ok(format!("{trimmed}/"))
    }
}

fn file_url(base: &str) -> String {
    format!("{base}accounts.gvault")
}

fn build_headers(config: &WebDavConfig) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let token = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        format!("{}:{}", config.username.trim(), config.password.trim()).as_bytes(),
    );
    if let Ok(val) = HeaderValue::from_str(&format!("Basic {token}")) {
        headers.insert(AUTHORIZATION, val);
    }
    headers
}

pub async fn test_connection(config: &WebDavConfig) -> Result<String, String> {
    let base = normalize_base_url(&config.url)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .default_headers(build_headers(config))
        .build()
        .map_err(|e| format!("创建客户端失败: {e}"))?;

    // Try MKCOL or GET/PROPFIND on base
    let resp = client
        .request(
            reqwest::Method::from_bytes(b"PROPFIND").unwrap_or(reqwest::Method::GET),
            &base,
        )
        .header("Depth", "0")
        .send()
        .await;

    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            if status >= 200 && status < 300 || status == 207 || status == 405 {
                Ok("WebDAV 连接测试成功！".to_string())
            } else if status == 401 {
                Err("身份验证失败 (HTTP 401)，请核对账号与应用密码".to_string())
            } else {
                Ok(format!("WebDAV 服务器响应正常 (HTTP {status})"))
            }
        }
        Err(e) => Err(format!("连接 WebDAV 服务器失败: {e}")),
    }
}

pub async fn upload_vault(config: &WebDavConfig) -> Result<(), String> {
    let vault_path = vault::vault_file_path();
    if !vault_path.exists() {
        return Err("本地保险库文件不存在，无法上传".to_string());
    }
    let blob = fs::read(&vault_path).map_err(|e| format!("读取本地保险库失败: {e}"))?;

    let base = normalize_base_url(&config.url)?;
    let target = file_url(&base);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .default_headers(build_headers(config))
        .build()
        .map_err(|e| format!("创建客户端失败: {e}"))?;

    // Attempt best-effort MKCOL
    let _ = client
        .request(
            reqwest::Method::from_bytes(b"MKCOL").unwrap_or(reqwest::Method::POST),
            &base,
        )
        .send()
        .await;

    let resp = client
        .put(&target)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(blob)
        .send()
        .await
        .map_err(|e| format!("上传请求发送失败: {e}"))?;

    let status = resp.status().as_u16();
    if status >= 200 && status < 300 {
        Ok(())
    } else if status == 401 {
        Err("上传失败: 认证未通过 (HTTP 401)".to_string())
    } else {
        Err(format!("上传失败，服务器返回 HTTP {status}"))
    }
}

pub async fn download_vault(
    config: &WebDavConfig,
    master_password: &str,
) -> Result<Vec<AccountRecord>, String> {
    let base = normalize_base_url(&config.url)?;
    let target = file_url(&base);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .default_headers(build_headers(config))
        .build()
        .map_err(|e| format!("创建客户端失败: {e}"))?;

    let resp = client
        .get(&target)
        .send()
        .await
        .map_err(|e| format!("下载请求发送失败: {e}"))?;

    let status = resp.status().as_u16();
    if status == 404 {
        return Err("云端尚未保存任何保险库文件 (404 Not Found)".to_string());
    } else if status == 401 {
        return Err("下载失败: 认证未通过 (HTTP 401)".to_string());
    } else if status < 200 || status >= 300 {
        return Err(format!("下载失败，服务器返回 HTTP {status}"));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取云端响应数据失败: {e}"))?;
    let blob =
        String::from_utf8(bytes.to_vec()).map_err(|_| "云端文件不是有效的文本格式".to_string())?;

    // Try decrypting with master_password to verify integrity before overwriting local file
    let records = vault::decrypt_records(&blob, master_password)?;

    // Atomically save downloaded vault locally
    vault::save_vault_atomic(&records, master_password, 30)?;
    Ok(records)
}
