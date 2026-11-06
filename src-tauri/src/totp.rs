use std::time::{SystemTime, UNIX_EPOCH};
use totp_rs::{Algorithm, Secret, TOTP};

use crate::models::{TotpQrResult, TotpResult};

pub fn normalize_totp_secret(raw_input: &str) -> Result<String, String> {
    let clean = raw_input.trim();
    if clean.is_empty() {
        return Err("TOTP 密钥为空".to_string());
    }

    if clean.to_lowercase().starts_with("otpauth://") {
        if let Ok(parsed) = tauri::Url::parse(clean) {
            for (k, v) in parsed.query_pairs() {
                if k.eq_ignore_ascii_case("secret") {
                    let norm: String = v
                        .chars()
                        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '=')
                        .collect::<String>()
                        .to_uppercase();
                    if !norm.is_empty() {
                        // Validate base32
                        let _ = Secret::Encoded(norm.clone())
                            .to_bytes()
                            .map_err(|e| format!("otpauth 中的 Base32 密钥格式错误: {e}"))?;
                        return Ok(norm);
                    }
                }
            }
        }
        let totp = TOTP::from_url(clean).map_err(|e| format!("无效的 otpauth 链接: {e}"))?;
        return Ok(totp.get_secret_base32());
    }

    let normalized: String = clean
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '=')
        .collect::<String>()
        .to_uppercase();

    let secret = Secret::Encoded(normalized.clone());
    let _ = secret
        .to_bytes()
        .map_err(|e| format!("Base32 密钥格式错误: {e}"))?;

    Ok(normalized)
}

pub fn generate_totp_qr(raw_input: &str, email: &str) -> Result<TotpQrResult, String> {
    let normalized_secret = normalize_totp_secret(raw_input)?;
    let secret_bytes = Secret::Encoded(normalized_secret.clone())
        .to_bytes()
        .map_err(|e| format!("Base32 密钥解码失败: {e}"))?;

    let account_name = if email.trim().is_empty() {
        "user@gmail.com".to_string()
    } else {
        email.trim().to_string()
    };

    let totp = TOTP::new_unchecked(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("Google".to_string()),
        account_name,
    );

    let otpauth_url = totp.get_url();
    let qr_base64 = totp
        .get_qr_base64()
        .map_err(|e| format!("生成 2FA 二维码失败: {e}"))?;

    Ok(TotpQrResult {
        qr_base64: format!("data:image/png;base64,{qr_base64}"),
        otpauth_url,
        secret: normalized_secret,
    })
}

pub fn calculate_totp(raw_input: &str) -> Result<TotpResult, String> {
    let normalized = normalize_totp_secret(raw_input)?;
    let secret = Secret::Encoded(normalized);
    let secret_bytes = secret
        .to_bytes()
        .map_err(|e| format!("Base32 密钥格式错误: {e}"))?;

    let totp = TOTP::new_unchecked(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        None,
        "".to_string(),
    );

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();

    let code = totp.generate(now);
    let remaining_seconds = 30 - (now % 30);

    Ok(TotpResult {
        code,
        remaining_seconds,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rfc6238_vector() {
        // Standard test secret "JBSWY3DPEHPK3PXP" (ASCII "Hello!\xde\xad\xbe\xef")
        let res = calculate_totp("JBSWY3DPEHPK3PXP");
        match res {
            Ok(val) => {
                assert_eq!(val.code.len(), 6);
                assert!(val.remaining_seconds <= 30);
            }
            Err(e) => panic!("TOTP error: {}", e),
        }
    }
}
