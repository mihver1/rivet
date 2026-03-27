//! SSH tunnel support.
//!
//! Provides:
//! - Local port forwarding (`-L`): binds a local TCP port, forwards via SSH channel
//! - Remote port forwarding (`-R`): requests remote server to forward back via SSH
//! - Dynamic SOCKS5 proxy (`-D`): local SOCKS5 proxy, each connection forwarded via SSH

use std::sync::Arc;

use russh::client;
use rivet_core::connection::TunnelSpec;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::error::SshError;
use crate::handler::RivetHandler;

/// A running tunnel managed by the daemon.
pub struct TunnelHandle {
    pub id: Uuid,
    pub spec: TunnelSpec,
    pub connection_id: Uuid,
    shutdown_tx: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl TunnelHandle {
    /// Signal this tunnel to shut down and wait for cleanup.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.task.await;
    }

    /// Whether the background task has finished (tunnel died).
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

/// Start a local port forwarding tunnel (-L).
///
/// Binds a local TCP listener on `bind_addr:bind_port`. For each incoming
/// TCP connection, opens an SSH direct-tcpip channel to `remote_host:remote_port`
/// and bridges data bidirectionally.
pub fn start_local_tunnel(
    handle: Arc<client::Handle<RivetHandler>>,
    connection_id: Uuid,
    spec: TunnelSpec,
) -> Result<TunnelHandle, SshError> {
    let (bind_addr, bind_port, remote_host, remote_port) = match &spec {
        TunnelSpec::Local {
            bind_addr,
            bind_port,
            remote_host,
            remote_port,
        } => (
            bind_addr.clone(),
            *bind_port,
            remote_host.clone(),
            *remote_port,
        ),
        _ => return Err(SshError::Tunnel("not a local tunnel spec".into())),
    };

    let id = Uuid::new_v4();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let task = tokio::spawn(async move {
        let addr = format!("{bind_addr}:{bind_port}");
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => {
                info!(addr = %addr, "local tunnel listening");
                l
            }
            Err(e) => {
                error!(addr = %addr, error = %e, "failed to bind local tunnel");
                return;
            }
        };

        let mut shutdown_rx = shutdown_rx;

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, peer)) => {
                            debug!(peer = %peer, "local tunnel: accepted connection");
                            let handle = handle.clone();
                            let rh = remote_host.clone();
                            let rp = remote_port;
                            tokio::spawn(async move {
                                if let Err(e) = bridge_local(handle, stream, &rh, rp).await {
                                    debug!(error = %e, "local tunnel bridge ended");
                                }
                            });
                        }
                        Err(e) => {
                            error!(error = %e, "local tunnel: accept failed");
                            break;
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("local tunnel shutting down");
                    break;
                }
            }
        }
    });

    Ok(TunnelHandle {
        id,
        spec,
        connection_id,
        shutdown_tx,
        task,
    })
}

/// Bridge a local TCP stream to a remote host via SSH direct-tcpip.
async fn bridge_local(
    handle: Arc<client::Handle<RivetHandler>>,
    mut tcp_stream: TcpStream,
    remote_host: &str,
    remote_port: u16,
) -> Result<(), SshError> {
    let channel = handle
        .channel_open_direct_tcpip(remote_host, remote_port as u32, "127.0.0.1", 0)
        .await
        .map_err(|e| SshError::Tunnel(format!("direct-tcpip open failed: {e}")))?;

    let mut channel_stream = channel.into_stream();

    match io::copy_bidirectional(&mut tcp_stream, &mut channel_stream).await {
        Ok((to_remote, to_local)) => {
            debug!(to_remote, to_local, "local tunnel bridge completed");
        }
        Err(e) => {
            // Connection reset is normal when either side closes
            if e.kind() != std::io::ErrorKind::ConnectionReset {
                debug!(error = %e, "local tunnel bridge error");
            }
        }
    }

    Ok(())
}

/// Start a dynamic SOCKS5 tunnel (-D).
///
/// Binds a local TCP listener as a SOCKS5 proxy. For each incoming connection,
/// performs a minimal SOCKS5 handshake to extract the target host:port,
/// then opens an SSH direct-tcpip channel and bridges data.
pub fn start_dynamic_tunnel(
    handle: Arc<client::Handle<RivetHandler>>,
    connection_id: Uuid,
    spec: TunnelSpec,
) -> Result<TunnelHandle, SshError> {
    let (bind_addr, bind_port) = match &spec {
        TunnelSpec::Dynamic {
            bind_addr,
            bind_port,
        } => (bind_addr.clone(), *bind_port),
        _ => return Err(SshError::Tunnel("not a dynamic tunnel spec".into())),
    };

    let id = Uuid::new_v4();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let task = tokio::spawn(async move {
        let addr = format!("{bind_addr}:{bind_port}");
        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => {
                info!(addr = %addr, "SOCKS5 tunnel listening");
                l
            }
            Err(e) => {
                error!(addr = %addr, error = %e, "failed to bind SOCKS5 tunnel");
                return;
            }
        };

        let mut shutdown_rx = shutdown_rx;

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, peer)) => {
                            debug!(peer = %peer, "SOCKS5: accepted connection");
                            let handle = handle.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_socks5(handle, stream).await {
                                    debug!(error = %e, "SOCKS5 session ended");
                                }
                            });
                        }
                        Err(e) => {
                            error!(error = %e, "SOCKS5: accept failed");
                            break;
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    info!("SOCKS5 tunnel shutting down");
                    break;
                }
            }
        }
    });

    Ok(TunnelHandle {
        id,
        spec,
        connection_id,
        shutdown_tx,
        task,
    })
}

/// Minimal SOCKS5 handshake + proxy via SSH channel.
async fn handle_socks5(
    handle: Arc<client::Handle<RivetHandler>>,
    mut stream: TcpStream,
) -> Result<(), SshError> {
    // --- SOCKS5 greeting ---
    // Client: VER(1) NMETHODS(1) METHODS(N)
    let mut buf = [0u8; 2];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| SshError::Tunnel(format!("socks5 greeting: {e}")))?;

    if buf[0] != 0x05 {
        return Err(SshError::Tunnel("not SOCKS5".into()));
    }

    let nmethods = buf[1] as usize;
    let mut methods = vec![0u8; nmethods];
    stream
        .read_exact(&mut methods)
        .await
        .map_err(|e| SshError::Tunnel(format!("socks5 methods: {e}")))?;

    // Reply: no auth required
    stream
        .write_all(&[0x05, 0x00])
        .await
        .map_err(|e| SshError::Tunnel(format!("socks5 reply: {e}")))?;

    // --- SOCKS5 CONNECT request ---
    // VER(1) CMD(1) RSV(1) ATYP(1) DST.ADDR(var) DST.PORT(2)
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .await
        .map_err(|e| SshError::Tunnel(format!("socks5 request: {e}")))?;

    if header[1] != 0x01 {
        // Only CONNECT supported
        stream.write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await.ok();
        return Err(SshError::Tunnel("only SOCKS5 CONNECT supported".into()));
    }

    let (target_host, target_port) = match header[3] {
        // IPv4
        0x01 => {
            let mut addr = [0u8; 4];
            stream.read_exact(&mut addr).await.map_err(|e| SshError::Tunnel(e.to_string()))?;
            let mut port_buf = [0u8; 2];
            stream.read_exact(&mut port_buf).await.map_err(|e| SshError::Tunnel(e.to_string()))?;
            let port = u16::from_be_bytes(port_buf);
            (format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3]), port)
        }
        // Domain name
        0x03 => {
            let mut len_buf = [0u8; 1];
            stream.read_exact(&mut len_buf).await.map_err(|e| SshError::Tunnel(e.to_string()))?;
            let mut domain = vec![0u8; len_buf[0] as usize];
            stream.read_exact(&mut domain).await.map_err(|e| SshError::Tunnel(e.to_string()))?;
            let mut port_buf = [0u8; 2];
            stream.read_exact(&mut port_buf).await.map_err(|e| SshError::Tunnel(e.to_string()))?;
            let port = u16::from_be_bytes(port_buf);
            (String::from_utf8_lossy(&domain).to_string(), port)
        }
        // IPv6
        0x04 => {
            let mut addr = [0u8; 16];
            stream.read_exact(&mut addr).await.map_err(|e| SshError::Tunnel(e.to_string()))?;
            let mut port_buf = [0u8; 2];
            stream.read_exact(&mut port_buf).await.map_err(|e| SshError::Tunnel(e.to_string()))?;
            let port = u16::from_be_bytes(port_buf);
            let ipv6 = std::net::Ipv6Addr::from(addr);
            (ipv6.to_string(), port)
        }
        atyp => {
            return Err(SshError::Tunnel(format!("unsupported SOCKS5 address type: {atyp}")));
        }
    };

    debug!(host = %target_host, port = target_port, "SOCKS5 CONNECT");

    // Open SSH channel to target
    let channel = match handle
        .channel_open_direct_tcpip(&target_host, target_port as u32, "127.0.0.1", 0)
        .await
    {
        Ok(ch) => ch,
        Err(e) => {
            // SOCKS5 reply: general failure
            stream.write_all(&[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await.ok();
            return Err(SshError::Tunnel(format!("direct-tcpip to {target_host}:{target_port}: {e}")));
        }
    };

    // SOCKS5 reply: success
    stream
        .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(|e| SshError::Tunnel(format!("socks5 success reply: {e}")))?;

    // Bridge
    let mut channel_stream = channel.into_stream();
    match io::copy_bidirectional(&mut stream, &mut channel_stream).await {
        Ok((to_remote, to_local)) => {
            debug!(to_remote, to_local, host = %target_host, "SOCKS5 bridge completed");
        }
        Err(e) => {
            if e.kind() != std::io::ErrorKind::ConnectionReset {
                debug!(error = %e, "SOCKS5 bridge error");
            }
        }
    }

    Ok(())
}

/// Start a remote port forwarding tunnel (-R).
///
/// Requests the SSH server to listen on `bind_addr:bind_port`.
/// Incoming connections on the remote side are forwarded to `local_host:local_port`.
///
/// Note: remote forwarding requires the RivetHandler to support
/// `server_channel_open_forwarded_tcpip` callbacks. For MVP, this is a
/// simplified implementation that relies on the handler forwarding channels.
pub fn start_remote_tunnel(
    handle: Arc<client::Handle<RivetHandler>>,
    connection_id: Uuid,
    spec: TunnelSpec,
    forwarded_rx: tokio::sync::mpsc::UnboundedReceiver<ForwardedChannel>,
) -> Result<TunnelHandle, SshError> {
    let (bind_addr, bind_port, local_host, local_port) = match &spec {
        TunnelSpec::Remote {
            bind_addr,
            bind_port,
            local_host,
            local_port,
        } => (
            bind_addr.clone(),
            *bind_port,
            local_host.clone(),
            *local_port,
        ),
        _ => return Err(SshError::Tunnel("not a remote tunnel spec".into())),
    };

    let id = Uuid::new_v4();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let task = tokio::spawn(async move {
        // Request the server to start forwarding
        match handle
            .tcpip_forward(&bind_addr, bind_port as u32)
            .await
        {
            Ok(port) => {
                let actual_port = if port > 0 { port } else { bind_port as u32 };
                info!(
                    addr = %bind_addr,
                    port = actual_port,
                    "remote tunnel: server listening"
                );
            }
            Err(e) => {
                error!(error = %e, "remote tunnel: tcpip_forward failed");
                return;
            }
        }

        let mut shutdown_rx = shutdown_rx;
        let mut forwarded_rx = forwarded_rx;

        loop {
            tokio::select! {
                Some(fwd) = forwarded_rx.recv() => {
                    debug!(
                        from = %fwd.originator_address,
                        from_port = fwd.originator_port,
                        "remote tunnel: incoming connection"
                    );
                    let lh = local_host.clone();
                    let lp = local_port;
                    tokio::spawn(async move {
                        if let Err(e) = bridge_remote(fwd.channel, &lh, lp).await {
                            debug!(error = %e, "remote tunnel bridge ended");
                        }
                    });
                }
                _ = shutdown_rx.changed() => {
                    info!("remote tunnel shutting down");
                    // Cancel forwarding on server
                    let _ = handle
                        .cancel_tcpip_forward(&bind_addr, bind_port as u32)
                        .await;
                    break;
                }
            }
        }
    });

    Ok(TunnelHandle {
        id,
        spec,
        connection_id,
        shutdown_tx,
        task,
    })
}

/// A forwarded channel from the server (for remote port forwarding).
pub struct ForwardedChannel {
    pub channel: russh::Channel<client::Msg>,
    pub connected_address: String,
    pub connected_port: u32,
    pub originator_address: String,
    pub originator_port: u32,
}

/// Bridge a remote-forwarded SSH channel to a local TCP target.
async fn bridge_remote(
    channel: russh::Channel<client::Msg>,
    local_host: &str,
    local_port: u16,
) -> Result<(), SshError> {
    let addr = format!("{local_host}:{local_port}");
    let mut tcp_stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| SshError::Tunnel(format!("connect to {addr}: {e}")))?;

    let mut channel_stream = channel.into_stream();

    match io::copy_bidirectional(&mut tcp_stream, &mut channel_stream).await {
        Ok((to_local, to_remote)) => {
            debug!(to_local, to_remote, "remote tunnel bridge completed");
        }
        Err(e) => {
            if e.kind() != std::io::ErrorKind::ConnectionReset {
                debug!(error = %e, "remote tunnel bridge error");
            }
        }
    }

    Ok(())
}

/// Start any tunnel type based on the spec.
pub fn start_tunnel(
    handle: Arc<client::Handle<RivetHandler>>,
    connection_id: Uuid,
    spec: TunnelSpec,
    forwarded_rx: Option<tokio::sync::mpsc::UnboundedReceiver<ForwardedChannel>>,
) -> Result<TunnelHandle, SshError> {
    match &spec {
        TunnelSpec::Local { .. } => start_local_tunnel(handle, connection_id, spec),
        TunnelSpec::Dynamic { .. } => start_dynamic_tunnel(handle, connection_id, spec),
        TunnelSpec::Remote { .. } => {
            let rx = forwarded_rx
                .ok_or_else(|| SshError::Tunnel("remote tunnel requires forwarded channel receiver".into()))?;
            start_remote_tunnel(handle, connection_id, spec, rx)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tunnel_handle_id() {
        // Just verify TunnelHandle fields are accessible
        let id = Uuid::new_v4();
        let spec = TunnelSpec::Local {
            bind_addr: "127.0.0.1".into(),
            bind_port: 8080,
            remote_host: "localhost".into(),
            remote_port: 80,
        };
        // Can't construct TunnelHandle without running tokio, but verify TunnelSpec works
        assert_eq!(spec.to_ssh_arg(), "-L 127.0.0.1:8080:localhost:80");
        assert!(!id.is_nil());
    }
}
