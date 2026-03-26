use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: Uuid,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: AuthMethod,
    pub tags: Vec<String>,
    pub group_ids: Vec<Uuid>,
    pub jump_host: Option<Uuid>,
    pub options: SshOptions,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Connection {
    pub fn new(name: impl Into<String>, host: impl Into<String>, username: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            host: host.into(),
            port: 22,
            username: username.into(),
            auth: AuthMethod::Agent,
            tags: Vec::new(),
            group_ids: Vec::new(),
            jump_host: None,
            options: SshOptions::default(),
            notes: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AuthMethod {
    Password(String),
    PrivateKey {
        key_data: Vec<u8>,
        passphrase: Option<String>,
    },
    KeyFile {
        path: PathBuf,
        passphrase: Option<String>,
    },
    Agent,
    Certificate {
        cert_path: PathBuf,
        key_path: PathBuf,
    },
    Interactive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
}

impl Group {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: None,
            color: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshOptions {
    pub keepalive_interval: Option<u32>,
    pub keepalive_count_max: Option<u32>,
    pub compression: bool,
    pub connect_timeout: Option<u32>,
    pub extra_args: Vec<String>,
}

impl Default for SshOptions {
    fn default() -> Self {
        Self {
            keepalive_interval: Some(30),
            keepalive_count_max: Some(3),
            compression: false,
            connect_timeout: Some(10),
            extra_args: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_new() {
        let conn = Connection::new("test-server", "10.0.1.50", "deploy");
        assert_eq!(conn.name, "test-server");
        assert_eq!(conn.host, "10.0.1.50");
        assert_eq!(conn.username, "deploy");
        assert_eq!(conn.port, 22);
        assert!(matches!(conn.auth, AuthMethod::Agent));
        assert!(conn.tags.is_empty());
        assert!(conn.group_ids.is_empty());
        assert!(conn.jump_host.is_none());
        assert!(conn.notes.is_none());
    }

    #[test]
    fn test_ssh_options_default() {
        let opts = SshOptions::default();
        assert_eq!(opts.keepalive_interval, Some(30));
        assert_eq!(opts.keepalive_count_max, Some(3));
        assert!(!opts.compression);
        assert_eq!(opts.connect_timeout, Some(10));
        assert!(opts.extra_args.is_empty());
    }

    #[test]
    fn test_connection_serialization_roundtrip() {
        let conn = Connection::new("prod-web", "192.168.1.1", "admin");
        let json = serde_json::to_string(&conn).unwrap();
        let deserialized: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, conn.name);
        assert_eq!(deserialized.host, conn.host);
        assert_eq!(deserialized.port, conn.port);
    }

    #[test]
    fn test_auth_method_variants_serialize() {
        let methods = vec![
            AuthMethod::Password("secret".into()),
            AuthMethod::PrivateKey {
                key_data: vec![1, 2, 3],
                passphrase: None,
            },
            AuthMethod::KeyFile {
                path: PathBuf::from("/home/user/.ssh/id_ed25519"),
                passphrase: Some("pass".into()),
            },
            AuthMethod::Agent,
            AuthMethod::Certificate {
                cert_path: PathBuf::from("/cert"),
                key_path: PathBuf::from("/key"),
            },
            AuthMethod::Interactive,
        ];

        for method in &methods {
            let json = serde_json::to_string(method).unwrap();
            let _: AuthMethod = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_group_new() {
        let group = Group::new("production");
        assert_eq!(group.name, "production");
        assert!(group.description.is_none());
        assert!(group.color.is_none());
    }
}
