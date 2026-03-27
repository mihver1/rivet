use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    pub version: u32,
    pub argon2_memory_cost: u32,
    pub argon2_time_cost: u32,
    pub argon2_parallelism: u32,
    pub salt: String, // hex-encoded
}

impl VaultConfig {
    pub fn new(salt: &[u8]) -> Self {
        Self {
            version: 1,
            argon2_memory_cost: 64 * 1024,
            argon2_time_cost: 3,
            argon2_parallelism: 4,
            salt: hex_encode(salt),
        }
    }

    pub fn salt_bytes(&self) -> Vec<u8> {
        hex_decode(&self.salt)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap_or(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_roundtrip() {
        let bytes = vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0xff];
        let encoded = hex_encode(&bytes);
        assert_eq!(encoded, "deadbeef00ff");
        let decoded = hex_decode(&encoded);
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn test_vault_config_salt_roundtrip() {
        let salt = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let config = VaultConfig::new(&salt);
        assert_eq!(config.salt_bytes(), salt);
    }
}
