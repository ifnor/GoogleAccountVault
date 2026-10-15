use std::fs;
use std::path::PathBuf;
use std::ptr::null_mut;
use crate::vault;

#[repr(C)]
struct DATA_BLOB {
    cb_data: u32,
    pb_data: *mut u8,
}

#[link(name = "crypt32")]
unsafe extern "system" {
    fn CryptProtectData(
        pDataIn: *const DATA_BLOB,
        szDataDescr: *const u16,
        pOptionalEntropy: *const DATA_BLOB,
        pvReserved: *mut std::ffi::c_void,
        pPromptStruct: *mut std::ffi::c_void,
        dwFlags: u32,
        pDataOut: *mut DATA_BLOB,
    ) -> i32;

    fn CryptUnprotectData(
        pDataIn: *const DATA_BLOB,
        ppszDataDescr: *mut *mut u16,
        pOptionalEntropy: *const DATA_BLOB,
        pvReserved: *mut std::ffi::c_void,
        pPromptStruct: *mut std::ffi::c_void,
        dwFlags: u32,
        pDataOut: *mut DATA_BLOB,
    ) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LocalFree(hMem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}

const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;

pub fn dpapi_protect(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut in_blob = DATA_BLOB {
        cb_data: data.len() as u32,
        pb_data: data.as_ptr() as *mut u8,
    };
    let mut out_blob = DATA_BLOB {
        cb_data: 0,
        pb_data: null_mut(),
    };

    let success = unsafe {
        CryptProtectData(
            &mut in_blob,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
    };

    if success == 0 {
        return Err("DPAPI 加密失败".to_string());
    }

    let mut result = vec![0u8; out_blob.cb_data as usize];
    unsafe {
        std::ptr::copy_nonoverlapping(out_blob.pb_data, result.as_mut_ptr(), out_blob.cb_data as usize);
        LocalFree(out_blob.pb_data as *mut std::ffi::c_void);
    }

    Ok(result)
}

pub fn dpapi_unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut in_blob = DATA_BLOB {
        cb_data: data.len() as u32,
        pb_data: data.as_ptr() as *mut u8,
    };
    let mut out_blob = DATA_BLOB {
        cb_data: 0,
        pb_data: null_mut(),
    };

    let success = unsafe {
        CryptUnprotectData(
            &mut in_blob,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
    };

    if success == 0 {
        return Err("DPAPI 解密失败 (可能当前非授权 Windows 用户或凭证已失效)".to_string());
    }

    let mut result = vec![0u8; out_blob.cb_data as usize];
    unsafe {
        std::ptr::copy_nonoverlapping(out_blob.pb_data, result.as_mut_ptr(), out_blob.cb_data as usize);
        LocalFree(out_blob.pb_data as *mut std::ffi::c_void);
    }

    Ok(result)
}

fn hello_cred_path() -> PathBuf {
    vault::vault_data_dir().join("hello.bin")
}

pub fn is_hello_enrolled() -> bool {
    hello_cred_path().exists()
}

pub fn disable_hello() -> Result<(), String> {
    let path = hello_cred_path();
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

pub fn save_hello_credentials(master_password: &str) -> Result<(), String> {
    let encrypted = dpapi_protect(master_password.as_bytes())?;
    let path = hello_cred_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(path, encrypted).map_err(|e| format!("保存 Windows Hello 凭据失败: {e}"))?;
    Ok(())
}

pub fn get_hello_credentials() -> Result<String, String> {
    let path = hello_cred_path();
    if !path.exists() {
        return Err("尚未启用 Windows Hello 解锁".to_string());
    }
    let data = fs::read(path).map_err(|e| format!("读取凭据失败: {e}"))?;
    let decrypted = dpapi_unprotect(&data)?;
    String::from_utf8(decrypted).map_err(|e| format!("解析凭据失败: {e}"))
}

use windows::core::HSTRING;
use windows::Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability};

pub async fn check_hello_availability() -> Result<bool, String> {
    let op = UserConsentVerifier::CheckAvailabilityAsync()
        .map_err(|e| format!("检查 Windows Hello 可用性失败: {e}"))?;
    let status = op.await.map_err(|e| format!("获取状态失败: {e}"))?;
    Ok(status == UserConsentVerifierAvailability::Available)
}

pub async fn verify_user_consent(prompt: &str) -> Result<bool, String> {
    let prompt_h = HSTRING::from(prompt);
    let op = UserConsentVerifier::RequestVerificationAsync(&prompt_h)
        .map_err(|e| format!("请求 Windows Hello 验证失败: {e}"))?;
    let result = op.await.map_err(|e| format!("验证执行失败: {e}"))?;
    Ok(result == UserConsentVerificationResult::Verified)
}
