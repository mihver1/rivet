use std::fs;
use std::path::{Path, PathBuf};

use shelly_core::connection::{AuthMethod, Connection, SshOptions};
use shelly_core::error::Result;

use crate::store::UnlockedVault;

/// Parse an SSH config file and extract Connection entries.
/// Skips wildcard hosts (e.g., `Host *`).
pub fn parse_ssh_config(path: &Path) -> Result<Vec<Connection>> {
    let content = fs::read_to_string(path)?;
    parse_ssh_config_str(&content)
}

fn parse_ssh_config_str(content: &str) -> Result<Vec<Connection>> {
    let mut connections = Vec::new();
    let mut current: Option<SshConfigEntry> = None;

    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Split into key and value
        let (key, value) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((k, v)) => (k.trim().to_lowercase(), v.trim().to_string()),
            None => continue,
        };

        if key == "host" {
            // Finalize previous entry
            if let Some(entry) = current.take() {
                if let Some(conn) = entry.into_connection() {
                    connections.push(conn);
                }
            }

            // Skip wildcard patterns
            if value.contains('*') || value.contains('?') {
                current = None;
            } else {
                current = Some(SshConfigEntry {
                    name: value,
                    ..Default::default()
                });
            }
        } else if let Some(entry) = current.as_mut() {
            match key.as_str() {
                "hostname" => entry.hostname = Some(value),
                "user" => entry.user = Some(value),
                "port" => entry.port = value.parse().ok(),
                "identityfile" => entry.identity_file = Some(expand_tilde(&value)),
                "serveraliveinterval" => entry.keepalive_interval = value.parse().ok(),
                "serveralivecountmax" => entry.keepalive_count_max = value.parse().ok(),
                "compression" => entry.compression = value.eq_ignore_ascii_case("yes"),
                "connecttimeout" => entry.connect_timeout = value.parse().ok(),
                "proxyjump" => entry.proxy_jump = Some(value),
                _ => {} // Ignore unknown directives
            }
        }
    }

    // Finalize last entry
    if let Some(entry) = current {
        if let Some(conn) = entry.into_connection() {
            connections.push(conn);
        }
    }

    Ok(connections)
}

/// Import SSH config into vault, skipping connections that already exist by name.
/// Returns the number of newly imported connections.
pub fn import_ssh_config_to_vault(
    vault: &UnlockedVault,
    path: &Path,
) -> Result<u32> {
    let parsed = parse_ssh_config(path)?;
    let existing = vault.list_connections()?;
    let existing_names: std::collections::HashSet<&str> =
        existing.iter().map(|c| c.name.as_str()).collect();

    let mut imported = 0u32;
    for conn in parsed {
        if !existing_names.contains(conn.name.as_str()) {
            vault.save_connection(&conn)?;
            imported += 1;
        }
    }

    Ok(imported)
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

#[derive(Default)]
struct SshConfigEntry {
    name: String,
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
    identity_file: Option<PathBuf>,
    keepalive_interval: Option<u32>,
    keepalive_count_max: Option<u32>,
    compression: bool,
    connect_timeout: Option<u32>,
    proxy_jump: Option<String>,
}

impl SshConfigEntry {
    fn into_connection(self) -> Option<Connection> {
        // Host without HostName uses the Host value itself
        let host = self.hostname.unwrap_or_else(|| self.name.clone());

        // Default user to current OS user
        let username = self.user.unwrap_or_else(|| {
            std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| "root".into())
        });

        let auth = if let Some(path) = self.identity_file {
            AuthMethod::KeyFile {
                path,
                passphrase: None,
            }
        } else {
            AuthMethod::Agent
        };

        let mut conn = Connection::new(self.name, host, username);
        conn.port = self.port.unwrap_or(22);
        conn.auth = auth;
        conn.options = SshOptions {
            keepalive_interval: self.keepalive_interval.or(Some(30)),
            keepalive_count_max: self.keepalive_count_max.or(Some(3)),
            compression: self.compression,
            connect_timeout: self.connect_timeout.or(Some(10)),
            extra_args: Vec::new(),
        };

        // Store ProxyJump info in notes (resolving to UUID requires saved connections)
        if let Some(proxy) = self.proxy_jump {
            conn.notes = Some(format!("ProxyJump: {proxy}"));
        }

        Some(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_basic_ssh_config() {
        let config = r#"
Host production-web
    HostName 10.0.1.5
    User deploy
    Port 2222
    IdentityFile ~/.ssh/id_ed25519

Host staging
    HostName staging.example.com
    User admin
"#;
        let connections = parse_ssh_config_str(config).unwrap();
        assert_eq!(connections.len(), 2);

        assert_eq!(connections[0].name, "production-web");
        assert_eq!(connections[0].host, "10.0.1.5");
        assert_eq!(connections[0].username, "deploy");
        assert_eq!(connections[0].port, 2222);
        match &connections[0].auth {
            AuthMethod::KeyFile { path, .. } => {
                assert!(path.to_string_lossy().contains("id_ed25519"));
            }
            _ => panic!("expected KeyFile auth"),
        }

        assert_eq!(connections[1].name, "staging");
        assert_eq!(connections[1].host, "staging.example.com");
        assert_eq!(connections[1].username, "admin");
        assert_eq!(connections[1].port, 22);
        assert!(matches!(connections[1].auth, AuthMethod::Agent));
    }

    #[test]
    fn test_skip_wildcard_hosts() {
        let config = r#"
Host *
    ServerAliveInterval 60

Host actual-server
    HostName 10.0.1.1
    User root
"#;
        let connections = parse_ssh_config_str(config).unwrap();
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].name, "actual-server");
    }

    #[test]
    fn test_proxy_jump() {
        let config = r#"
Host behind-bastion
    HostName 10.0.2.5
    User admin
    ProxyJump bastion.example.com
"#;
        let connections = parse_ssh_config_str(config).unwrap();
        assert_eq!(connections.len(), 1);
        assert_eq!(
            connections[0].notes,
            Some("ProxyJump: bastion.example.com".into())
        );
    }

    #[test]
    fn test_ssh_options_parsing() {
        let config = r#"
Host custom-opts
    HostName host.example.com
    User user
    ServerAliveInterval 120
    ServerAliveCountMax 5
    Compression yes
    ConnectTimeout 30
"#;
        let connections = parse_ssh_config_str(config).unwrap();
        assert_eq!(connections.len(), 1);
        let opts = &connections[0].options;
        assert_eq!(opts.keepalive_interval, Some(120));
        assert_eq!(opts.keepalive_count_max, Some(5));
        assert!(opts.compression);
        assert_eq!(opts.connect_timeout, Some(30));
    }

    #[test]
    fn test_import_deduplication() {
        let dir2 = TempDir::new().unwrap();
        let vault_dir = dir2.path().join("vault");
        let store = crate::store::VaultStore::new(vault_dir);
        store.init("pass").unwrap();
        let vault = store.unlock("pass").unwrap();

        // Pre-existing connection
        let existing = Connection::new("staging", "old-host", "old-user");
        vault.save_connection(&existing).unwrap();

        // Config with both existing and new
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config");
        fs::write(
            &config_path,
            r#"
Host staging
    HostName new-host
    User new-user

Host new-server
    HostName 10.0.1.1
    User admin
"#,
        )
        .unwrap();

        let imported = import_ssh_config_to_vault(&vault, &config_path).unwrap();
        assert_eq!(imported, 1); // only new-server imported

        let all = vault.list_connections().unwrap();
        assert_eq!(all.len(), 2);

        // Verify the existing "staging" was NOT overwritten
        let staging = vault.find_connection_by_name("staging").unwrap();
        assert_eq!(staging.host, "old-host");
    }

    #[test]
    fn test_host_without_hostname() {
        let config = r#"
Host myserver.example.com
    User deploy
"#;
        let connections = parse_ssh_config_str(config).unwrap();
        assert_eq!(connections.len(), 1);
        // When no HostName, the Host value is used as the host
        assert_eq!(connections[0].host, "myserver.example.com");
        assert_eq!(connections[0].name, "myserver.example.com");
    }
}
