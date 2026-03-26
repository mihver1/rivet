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

/// SSH tunnel specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TunnelSpec {
    /// Local port forwarding: -L bind_addr:bind_port:remote_host:remote_port
    Local {
        bind_addr: String,
        bind_port: u16,
        remote_host: String,
        remote_port: u16,
    },
    /// Remote port forwarding: -R bind_addr:bind_port:local_host:local_port
    Remote {
        bind_addr: String,
        bind_port: u16,
        local_host: String,
        local_port: u16,
    },
    /// Dynamic SOCKS5 proxy: -D bind_addr:bind_port
    Dynamic {
        bind_addr: String,
        bind_port: u16,
    },
}

impl TunnelSpec {
    /// Format as SSH -L/-R/-D argument.
    pub fn to_ssh_arg(&self) -> String {
        match self {
            TunnelSpec::Local {
                bind_addr,
                bind_port,
                remote_host,
                remote_port,
            } => format!("-L {bind_addr}:{bind_port}:{remote_host}:{remote_port}"),
            TunnelSpec::Remote {
                bind_addr,
                bind_port,
                local_host,
                local_port,
            } => format!("-R {bind_addr}:{bind_port}:{local_host}:{local_port}"),
            TunnelSpec::Dynamic {
                bind_addr,
                bind_port,
            } => format!("-D {bind_addr}:{bind_port}"),
        }
    }

    /// Short display label for the tunnel type.
    pub fn type_label(&self) -> &'static str {
        match self {
            TunnelSpec::Local { .. } => "local",
            TunnelSpec::Remote { .. } => "remote",
            TunnelSpec::Dynamic { .. } => "dynamic",
        }
    }

    /// Parse a tunnel spec from CLI arguments.
    ///
    /// Formats:
    /// - `-L [bind_addr:]bind_port:remote_host:remote_port`
    /// - `-R [bind_addr:]bind_port:local_host:local_port`
    /// - `-D [bind_addr:]bind_port`
    pub fn parse(flag: &str, spec: &str) -> Result<Self, String> {
        let parts: Vec<&str> = spec.split(':').collect();
        match flag {
            "-L" | "-l" | "L" | "l" => match parts.len() {
                3 => Ok(TunnelSpec::Local {
                    bind_addr: "127.0.0.1".into(),
                    bind_port: parts[0].parse().map_err(|_| "invalid bind port")?,
                    remote_host: parts[1].into(),
                    remote_port: parts[2].parse().map_err(|_| "invalid remote port")?,
                }),
                4 => Ok(TunnelSpec::Local {
                    bind_addr: parts[0].into(),
                    bind_port: parts[1].parse().map_err(|_| "invalid bind port")?,
                    remote_host: parts[2].into(),
                    remote_port: parts[3].parse().map_err(|_| "invalid remote port")?,
                }),
                _ => Err("local tunnel format: [bind_addr:]port:host:port".into()),
            },
            "-R" | "-r" | "R" | "r" => match parts.len() {
                3 => Ok(TunnelSpec::Remote {
                    bind_addr: "0.0.0.0".into(),
                    bind_port: parts[0].parse().map_err(|_| "invalid bind port")?,
                    local_host: parts[1].into(),
                    local_port: parts[2].parse().map_err(|_| "invalid local port")?,
                }),
                4 => Ok(TunnelSpec::Remote {
                    bind_addr: parts[0].into(),
                    bind_port: parts[1].parse().map_err(|_| "invalid bind port")?,
                    local_host: parts[2].into(),
                    local_port: parts[3].parse().map_err(|_| "invalid local port")?,
                }),
                _ => Err("remote tunnel format: [bind_addr:]port:host:port".into()),
            },
            "-D" | "-d" | "D" | "d" => match parts.len() {
                1 => Ok(TunnelSpec::Dynamic {
                    bind_addr: "127.0.0.1".into(),
                    bind_port: parts[0].parse().map_err(|_| "invalid bind port")?,
                }),
                2 => Ok(TunnelSpec::Dynamic {
                    bind_addr: parts[0].into(),
                    bind_port: parts[1].parse().map_err(|_| "invalid bind port")?,
                }),
                _ => Err("dynamic tunnel format: [bind_addr:]port".into()),
            },
            _ => Err(format!("unknown tunnel flag: {flag}, use -L, -R, or -D")),
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

    #[test]
    fn test_tunnel_spec_local_ssh_arg() {
        let spec = TunnelSpec::Local {
            bind_addr: "127.0.0.1".into(),
            bind_port: 8080,
            remote_host: "db.internal".into(),
            remote_port: 5432,
        };
        assert_eq!(spec.to_ssh_arg(), "-L 127.0.0.1:8080:db.internal:5432");
        assert_eq!(spec.type_label(), "local");
    }

    #[test]
    fn test_tunnel_spec_remote_ssh_arg() {
        let spec = TunnelSpec::Remote {
            bind_addr: "0.0.0.0".into(),
            bind_port: 9090,
            local_host: "localhost".into(),
            local_port: 3000,
        };
        assert_eq!(spec.to_ssh_arg(), "-R 0.0.0.0:9090:localhost:3000");
        assert_eq!(spec.type_label(), "remote");
    }

    #[test]
    fn test_tunnel_spec_dynamic_ssh_arg() {
        let spec = TunnelSpec::Dynamic {
            bind_addr: "127.0.0.1".into(),
            bind_port: 1080,
        };
        assert_eq!(spec.to_ssh_arg(), "-D 127.0.0.1:1080");
        assert_eq!(spec.type_label(), "dynamic");
    }

    #[test]
    fn test_tunnel_spec_parse_local() {
        let spec = TunnelSpec::parse("-L", "8080:db.internal:5432").unwrap();
        match spec {
            TunnelSpec::Local { bind_addr, bind_port, remote_host, remote_port } => {
                assert_eq!(bind_addr, "127.0.0.1");
                assert_eq!(bind_port, 8080);
                assert_eq!(remote_host, "db.internal");
                assert_eq!(remote_port, 5432);
            }
            _ => panic!("expected Local"),
        }
    }

    #[test]
    fn test_tunnel_spec_parse_local_with_bind() {
        let spec = TunnelSpec::parse("-L", "0.0.0.0:8080:db.internal:5432").unwrap();
        match spec {
            TunnelSpec::Local { bind_addr, .. } => assert_eq!(bind_addr, "0.0.0.0"),
            _ => panic!("expected Local"),
        }
    }

    #[test]
    fn test_tunnel_spec_parse_remote() {
        let spec = TunnelSpec::parse("-R", "9090:localhost:3000").unwrap();
        match spec {
            TunnelSpec::Remote { bind_addr, bind_port, local_host, local_port } => {
                assert_eq!(bind_addr, "0.0.0.0");
                assert_eq!(bind_port, 9090);
                assert_eq!(local_host, "localhost");
                assert_eq!(local_port, 3000);
            }
            _ => panic!("expected Remote"),
        }
    }

    #[test]
    fn test_tunnel_spec_parse_dynamic() {
        let spec = TunnelSpec::parse("-D", "1080").unwrap();
        match spec {
            TunnelSpec::Dynamic { bind_addr, bind_port } => {
                assert_eq!(bind_addr, "127.0.0.1");
                assert_eq!(bind_port, 1080);
            }
            _ => panic!("expected Dynamic"),
        }
    }

    #[test]
    fn test_tunnel_spec_parse_dynamic_with_bind() {
        let spec = TunnelSpec::parse("-D", "0.0.0.0:1080").unwrap();
        match spec {
            TunnelSpec::Dynamic { bind_addr, bind_port } => {
                assert_eq!(bind_addr, "0.0.0.0");
                assert_eq!(bind_port, 1080);
            }
            _ => panic!("expected Dynamic"),
        }
    }

    #[test]
    fn test_tunnel_spec_serialization_roundtrip() {
        let specs = vec![
            TunnelSpec::Local {
                bind_addr: "127.0.0.1".into(),
                bind_port: 8080,
                remote_host: "db".into(),
                remote_port: 5432,
            },
            TunnelSpec::Remote {
                bind_addr: "0.0.0.0".into(),
                bind_port: 9090,
                local_host: "localhost".into(),
                local_port: 3000,
            },
            TunnelSpec::Dynamic {
                bind_addr: "127.0.0.1".into(),
                bind_port: 1080,
            },
        ];
        for spec in &specs {
            let json = serde_json::to_string(spec).unwrap();
            let deserialized: TunnelSpec = serde_json::from_str(&json).unwrap();
            assert_eq!(spec.to_ssh_arg(), deserialized.to_ssh_arg());
        }
    }
}
