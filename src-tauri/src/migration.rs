use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::engine::general_purpose::URL_SAFE as BASE64_URL_SAFE;
use base64::Engine;
use data_encoding::BASE32_NOPAD;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtpAccount {
    pub id: String,
    pub name: String,
    pub issuer: String,
    pub secret: String, // Base32 encoded
    pub algorithm: String,
    pub digits: u32,
    pub period: u64,
    pub otp_type: String, // "TOTP" or "HOTP"
}

// Protobuf wire reader helper
struct ProtoReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ProtoReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn has_more(&self) -> bool {
        self.pos < self.data.len()
    }

    fn read_varint(&mut self) -> Result<u64, String> {
        let mut result: u64 = 0;
        let mut shift = 0;
        loop {
            if self.pos >= self.data.len() {
                return Err("Unexpected end of protobuf buffer while reading varint".to_string());
            }
            let b = self.data[self.pos];
            self.pos += 1;
            result |= ((b & 0x7F) as u64) << shift;
            if (b & 0x80) == 0 {
                break;
            }
            shift += 7;
            if shift >= 64 {
                return Err("Varint overflow".to_string());
            }
        }
        Ok(result)
    }

    fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], String> {
        if self.pos + len > self.data.len() {
            return Err("Unexpected end of protobuf buffer while reading length-delimited bytes".to_string());
        }
        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    fn skip_wire_type(&mut self, wire_type: u8) -> Result<(), String> {
        match wire_type {
            0 => {
                let _ = self.read_varint()?;
            }
            1 => {
                let _ = self.read_bytes(8)?;
            }
            2 => {
                let len = self.read_varint()? as usize;
                let _ = self.read_bytes(len)?;
            }
            5 => {
                let _ = self.read_bytes(4)?;
            }
            _ => return Err(format!("Unsupported protobuf wire type: {}", wire_type)),
        }
        Ok(())
    }
}

pub fn parse_google_migration_payload(bytes: &[u8]) -> Result<Vec<OtpAccount>, String> {
    let mut reader = ProtoReader::new(bytes);
    let mut accounts = Vec::new();

    while reader.has_more() {
        let tag = reader.read_varint()?;
        let field_num = tag >> 3;
        let wire_type = (tag & 0x07) as u8;

        if field_num == 1 && wire_type == 2 {
            // otp_parameters repeated submessage
            let sub_len = reader.read_varint()? as usize;
            let sub_bytes = reader.read_bytes(sub_len)?;
            let account = parse_otp_parameter(sub_bytes)?;
            accounts.push(account);
        } else {
            reader.skip_wire_type(wire_type)?;
        }
    }

    Ok(accounts)
}

fn parse_otp_parameter(bytes: &[u8]) -> Result<OtpAccount, String> {
    let mut reader = ProtoReader::new(bytes);

    let mut raw_secret: Vec<u8> = Vec::new();
    let mut name = String::new();
    let mut issuer = String::new();
    let mut algorithm = "SHA1".to_string();
    let mut digits = 6;
    let mut otp_type = "TOTP".to_string();

    while reader.has_more() {
        let tag = reader.read_varint()?;
        let field_num = tag >> 3;
        let wire_type = (tag & 0x07) as u8;

        match (field_num, wire_type) {
            (1, 2) => {
                // secret: bytes
                let len = reader.read_varint()? as usize;
                raw_secret = reader.read_bytes(len)?.to_vec();
            }
            (2, 2) => {
                // name: string
                let len = reader.read_varint()? as usize;
                let slice = reader.read_bytes(len)?;
                name = String::from_utf8_lossy(slice).to_string();
            }
            (3, 2) => {
                // issuer: string
                let len = reader.read_varint()? as usize;
                let slice = reader.read_bytes(len)?;
                issuer = String::from_utf8_lossy(slice).to_string();
            }
            (4, 0) => {
                // algorithm: enum
                let algo_val = reader.read_varint()?;
                algorithm = match algo_val {
                    2 => "SHA256".to_string(),
                    3 => "SHA512".to_string(),
                    4 => "MD5".to_string(),
                    _ => "SHA1".to_string(),
                };
            }
            (5, 0) => {
                // digits: enum (1 = 6 digits, 2 = 8 digits)
                let dig_val = reader.read_varint()?;
                digits = if dig_val == 2 { 8 } else { 6 };
            }
            (6, 0) => {
                // type: enum (1 = HOTP, 2 = TOTP)
                let type_val = reader.read_varint()?;
                otp_type = if type_val == 1 { "HOTP".to_string() } else { "TOTP".to_string() };
            }
            _ => {
                reader.skip_wire_type(wire_type)?;
            }
        }
    }

    if raw_secret.is_empty() {
        return Err("Encountered OTP parameter without secret key".to_string());
    }

    // Convert raw secret bytes to clean Base32
    let base32_secret = BASE32_NOPAD.encode(&raw_secret);

    // If issuer is blank, see if name contains "Issuer:Account"
    if issuer.is_empty() && name.contains(':') {
        let parts: Vec<&str> = name.splitn(2, ':').collect();
        issuer = parts[0].trim().to_string();
        name = parts[1].trim().to_string();
    }

    let id = format!(
        "{}-{}",
        issuer.to_lowercase().replace(' ', ""),
        name.to_lowercase().replace(' ', "")
    );

    Ok(OtpAccount {
        id,
        name,
        issuer,
        secret: base32_secret,
        algorithm,
        digits,
        period: 30,
        otp_type,
    })
}

pub fn parse_migration_uri(raw_uri: &str) -> Result<Vec<OtpAccount>, String> {
    let trimmed = raw_uri.trim();
    let data_str = if let Some(idx) = trimmed.find("data=") {
        &trimmed[idx + 5..]
    } else {
        trimmed
    };

    // Cut off additional URL parameters if any (e.g., &batch_size=...)
    let data_str = data_str.split('&').next().unwrap_or(data_str);

    // URL percent decode
    let unescaped = percent_decode_str(data_str);

    // Base64 decode (try standard, then URL safe)
    let binary_data = BASE64_STANDARD
        .decode(&unescaped)
        .or_else(|_| BASE64_URL_SAFE.decode(&unescaped))
        .or_else(|_| {
            let padded = pad_base64(&unescaped);
            BASE64_STANDARD.decode(&padded)
        })
        .map_err(|e| format!("Failed to base64 decode migration data: {}", e))?;

    parse_google_migration_payload(&binary_data)
}

fn pad_base64(s: &str) -> String {
    let mut res = s.to_string();
    while res.len() % 4 != 0 {
        res.push('=');
    }
    res
}

fn percent_decode_str(s: &str) -> String {
    let mut res = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(val) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                res.push(val);
                i += 3;
                continue;
            }
        }
        res.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&res).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_google_migration_sample() {
        let uri = "otpauth-migration://offline?data=CiwKCkhlbGxvId6tvu8SEHVzZXJAZXhhbXBsZS5jb20aBkdvb2dsZSABKAEwAhAB";
        let accounts = parse_migration_uri(uri).expect("Failed to parse migration uri");
        assert_eq!(accounts.len(), 1);
        let acc = &accounts[0];
        assert_eq!(acc.name, "user@example.com");
        assert_eq!(acc.issuer, "Google");
        assert_eq!(acc.digits, 6);
        assert_eq!(acc.algorithm, "SHA1");
        assert_eq!(acc.otp_type, "TOTP");
        assert!(!acc.secret.is_empty());
    }
}
