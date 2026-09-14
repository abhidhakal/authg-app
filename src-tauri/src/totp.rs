use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha1 = Hmac<Sha1>;
type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtpCodeResult {
    pub code: String,
    pub seconds_remaining: u64,
    pub period: u64,
}

pub fn normalize_base32(secret: &str) -> String {
    secret
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_uppercase()
        .trim_end_matches('=')
        .to_string()
}

pub fn decode_secret(secret_str: &str) -> Result<Vec<u8>, String> {
    let clean = normalize_base32(secret_str);
    BASE32_NOPAD
        .decode(clean.as_bytes())
        .map_err(|e| format!("Invalid Base32 secret: {}", e))
}

pub fn compute_totp_at_time(
    secret_bytes: &[u8],
    time_seconds: u64,
    algorithm: &str,
    digits: u32,
    period: u64,
) -> Result<String, String> {
    if period == 0 {
        return Err("Period cannot be 0".to_string());
    }
    let counter = time_seconds / period;
    let counter_bytes = counter.to_be_bytes();

    let hmac_hash = match algorithm.to_ascii_uppercase().as_str() {
        "SHA1" | "" => {
            let mut mac = HmacSha1::new_from_slice(secret_bytes)
                .map_err(|e| format!("HMAC init error: {}", e))?;
            mac.update(&counter_bytes);
            mac.finalize().into_bytes().to_vec()
        }
        "SHA256" => {
            let mut mac = HmacSha256::new_from_slice(secret_bytes)
                .map_err(|e| format!("HMAC init error: {}", e))?;
            mac.update(&counter_bytes);
            mac.finalize().into_bytes().to_vec()
        }
        "SHA512" => {
            let mut mac = HmacSha512::new_from_slice(secret_bytes)
                .map_err(|e| format!("HMAC init error: {}", e))?;
            mac.update(&counter_bytes);
            mac.finalize().into_bytes().to_vec()
        }
        other => return Err(format!("Unsupported algorithm: {}", other)),
    };

    if hmac_hash.is_empty() {
        return Err("HMAC output is empty".to_string());
    }

    // Dynamic truncation per RFC 4226 section 5.3
    let offset = (hmac_hash.last().unwrap() & 0x0F) as usize;
    if offset + 4 > hmac_hash.len() {
        return Err("Invalid offset in HMAC output".to_string());
    }

    let binary_code = ((hmac_hash[offset] as u32 & 0x7F) << 24)
        | ((hmac_hash[offset + 1] as u32 & 0xFF) << 16)
        | ((hmac_hash[offset + 2] as u32 & 0xFF) << 8)
        | (hmac_hash[offset + 3] as u32 & 0xFF);

    let divisor = 10_u32.pow(digits);
    let otp = binary_code % divisor;

    Ok(format!("{:0width$}", otp, width = digits as usize))
}

pub fn generate_totp(
    secret_str: &str,
    algorithm: Option<&str>,
    digits: Option<u32>,
    period: Option<u64>,
) -> Result<OtpCodeResult, String> {
    let secret_bytes = decode_secret(secret_str)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();

    let algo = algorithm.unwrap_or("SHA1");
    let dig = digits.unwrap_or(6);
    let per = period.unwrap_or(30);

    let code = compute_totp_at_time(&secret_bytes, now, algo, dig, per)?;
    let elapsed = now % per;
    let seconds_remaining = per - elapsed;

    Ok(OtpCodeResult {
        code,
        seconds_remaining,
        period: per,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rfc6238_sha1_vectors() {
        // RFC 6238 Appendix B: Key is ASCII "12345678901234567890" (20 bytes)
        let key = b"12345678901234567890";
        // 59s -> 8-digit: 94287082 -> 6-digit: 287082
        assert_eq!(compute_totp_at_time(key, 59, "SHA1", 8, 30).unwrap(), "94287082");
        assert_eq!(compute_totp_at_time(key, 59, "SHA1", 6, 30).unwrap(), "287082");

        // 1111111109s -> 8-digit: 07081804 -> 6-digit: 081804
        assert_eq!(compute_totp_at_time(key, 1111111109, "SHA1", 8, 30).unwrap(), "07081804");
        assert_eq!(compute_totp_at_time(key, 1111111109, "SHA1", 6, 30).unwrap(), "081804");

        // 1111111111s -> 8-digit: 14050471 -> 6-digit: 050471
        assert_eq!(compute_totp_at_time(key, 1111111111, "SHA1", 8, 30).unwrap(), "14050471");
        assert_eq!(compute_totp_at_time(key, 1111111111, "SHA1", 6, 30).unwrap(), "050471");

        // 1234567890s -> 8-digit: 89005924 -> 6-digit: 005924
        assert_eq!(compute_totp_at_time(key, 1234567890, "SHA1", 8, 30).unwrap(), "89005924");
        assert_eq!(compute_totp_at_time(key, 1234567890, "SHA1", 6, 30).unwrap(), "005924");

        // 2000000000s -> 8-digit: 69279037 -> 6-digit: 279037
        assert_eq!(compute_totp_at_time(key, 2000000000, "SHA1", 8, 30).unwrap(), "69279037");
        assert_eq!(compute_totp_at_time(key, 2000000000, "SHA1", 6, 30).unwrap(), "279037");
    }
}
