//! SSH tunnel support (Phase 2).
//!
//! This module will provide:
//! - Local port forwarding (`-L`)
//! - Remote port forwarding (`-R`)
//! - Dynamic SOCKS proxy (`-D`)
//! - Unix socket forwarding
//!
//! For MVP, tunneling is deferred — interactive SSH (via system `ssh` binary)
//! supports tunnels natively via `extra_args`.

use crate::error::SshError;

/// Tunnel configuration (Phase 2 placeholder).
#[derive(Debug, Clone)]
pub enum TunnelSpec {
    /// Local port forwarding: -L local_port:remote_host:remote_port
    Local {
        bind_addr: String,
        bind_port: u16,
        remote_host: String,
        remote_port: u16,
    },
    /// Remote port forwarding: -R remote_port:local_host:local_port
    Remote {
        bind_addr: String,
        bind_port: u16,
        local_host: String,
        local_port: u16,
    },
    /// Dynamic SOCKS proxy: -D bind_port
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
            } => {
                format!("-L {bind_addr}:{bind_port}:{remote_host}:{remote_port}")
            }
            TunnelSpec::Remote {
                bind_addr,
                bind_port,
                local_host,
                local_port,
            } => {
                format!("-R {bind_addr}:{bind_port}:{local_host}:{local_port}")
            }
            TunnelSpec::Dynamic {
                bind_addr,
                bind_port,
            } => {
                format!("-D {bind_addr}:{bind_port}")
            }
        }
    }
}

/// Start a tunnel (Phase 2 — not yet implemented).
pub fn start_tunnel(_spec: &TunnelSpec) -> Result<(), SshError> {
    Err(SshError::Channel(
        "SSH tunneling is not yet implemented (Phase 2)".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_tunnel_arg() {
        let spec = TunnelSpec::Local {
            bind_addr: "127.0.0.1".into(),
            bind_port: 8080,
            remote_host: "db.internal".into(),
            remote_port: 5432,
        };
        assert_eq!(spec.to_ssh_arg(), "-L 127.0.0.1:8080:db.internal:5432");
    }

    #[test]
    fn test_remote_tunnel_arg() {
        let spec = TunnelSpec::Remote {
            bind_addr: "0.0.0.0".into(),
            bind_port: 9090,
            local_host: "localhost".into(),
            local_port: 3000,
        };
        assert_eq!(spec.to_ssh_arg(), "-R 0.0.0.0:9090:localhost:3000");
    }

    #[test]
    fn test_dynamic_tunnel_arg() {
        let spec = TunnelSpec::Dynamic {
            bind_addr: "127.0.0.1".into(),
            bind_port: 1080,
        };
        assert_eq!(spec.to_ssh_arg(), "-D 127.0.0.1:1080");
    }

    #[test]
    fn test_start_tunnel_returns_error() {
        let spec = TunnelSpec::Local {
            bind_addr: "127.0.0.1".into(),
            bind_port: 8080,
            remote_host: "localhost".into(),
            remote_port: 80,
        };
        assert!(start_tunnel(&spec).is_err());
    }
}
