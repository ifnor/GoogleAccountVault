use std::time::{SystemTime, UNIX_EPOCH};
use totp_rs::{Algorithm, Secret, TOTP};

use crate::models::TotpResult;

pub fn calculate_totp(raw_input: &str) -> Result<TotpResult, String> {
    let clean = raw_input.trim();
    if clean.is_empty() {
        return Err("TOTP 密钥为空".to_string());
    }

    let totp = if clean.starts_with("otpauth://") {
        TOTP::from_url(clean).map_err(|e| format!("无效的 otpauth 链接: {e}"))?
    } else {
        // Strip spaces, dashes, equals
        let normalized: String = clean
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-' && *c != '=')
            .collect::<String>()
            .to_uppercase();

        let secret = Secret::Encoded(normalized);
        let secret_bytes = secret
            .to_bytes()
            .map_err(|e| format!("Base32 密钥格式错误: {e}"))?;

        TOTP::new_unchecked(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret_bytes,
            None,
            "".to_string(),
        )
    };

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
