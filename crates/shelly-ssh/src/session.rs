use std::sync::Arc;
use std::time::Duration;

use russh::client::{self, Handle};
use russh::{ChannelMsg, Disconnect};
use shelly_core::connection::Connection;
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{debug, info};

use crate::auth::{self, AuthOutcome};
use crate::error::SshError;
use crate::handler::ShellyHandler;

/// An authenticated SSH session wrapping a russh Handle.
///
/// Used for programmatic operations (exec, file transfer).
/// Interactive sessions go through the system `ssh` binary instead.
pub struct SshSession {
    handle: Handle<ShellyHandler>,
    host: String,
    port: u16,
    username: String,
}

impl SshSession {
    /// Connect and authenticate to an SSH server using connection parameters.
    ///
    /// Returns an authenticated session ready for command execution.
    pub async fn connect(conn: &Connection) -> Result<Self, SshError> {
        let addr = format!("{}:{}", conn.host, conn.port);
        info!(host = %conn.host, port = conn.port, user = %conn.username, "connecting");

        // Build russh client config from connection options
        let config = build_config(conn);

        let mut handle = client::connect(Arc::new(config), &*addr, ShellyHandler)
            .await
            .map_err(|e| SshError::ConnectionFailed(format!("{addr}: {e}")))?;

        debug!("TCP connection established, authenticating");

        let outcome = auth::authenticate(&mut handle, &conn.username, &conn.auth).await?;

        match outcome {
            AuthOutcome::Success => {
                info!(host = %conn.host, "SSH session established");
                Ok(Self {
                    handle,
                    host: conn.host.clone(),
                    port: conn.port,
                    username: conn.username.clone(),
                })
            }
            AuthOutcome::Failed => Err(SshError::AuthFailed),
        }
    }

    /// Connect to an SSH server over an existing stream (e.g. for jump hosts).
    pub async fn connect_stream<S>(
        conn: &Connection,
        stream: S,
    ) -> Result<Self, SshError>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        info!(
            host = %conn.host,
            port = conn.port,
            user = %conn.username,
            "connecting over forwarded stream"
        );

        let config = build_config(conn);

        let mut handle = client::connect_stream(Arc::new(config), stream, ShellyHandler)
            .await
            .map_err(|e| SshError::ConnectionFailed(format!("stream to {}:{}: {e}", conn.host, conn.port)))?;

        debug!("stream connection established, authenticating");

        let outcome = auth::authenticate(&mut handle, &conn.username, &conn.auth).await?;

        match outcome {
            AuthOutcome::Success => {
                info!(host = %conn.host, "SSH session established over stream");
                Ok(Self {
                    handle,
                    host: conn.host.clone(),
                    port: conn.port,
                    username: conn.username.clone(),
                })
            }
            AuthOutcome::Failed => Err(SshError::AuthFailed),
        }
    }

    /// Execute a command on the remote host and return its output.
    ///
    /// Returns (exit_code, stdout, stderr).
    pub async fn exec(&self, command: &str) -> Result<ExecResult, SshError> {
        debug!(command, host = %self.host, "executing command");

        let mut channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| SshError::Channel(format!("failed to open session channel: {e}")))?;

        channel
            .exec(true, command)
            .await
            .map_err(|e| SshError::Channel(format!("failed to exec: {e}")))?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code: Option<u32> = None;

        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => {
                    stdout.extend_from_slice(&data);
                }
                ChannelMsg::ExtendedData { data, ext: 1 } => {
                    stderr.extend_from_slice(&data);
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    exit_code = Some(exit_status);
                }
                ChannelMsg::Eof | ChannelMsg::Close => break,
                _ => {}
            }
        }

        let result = ExecResult {
            exit_code: exit_code.unwrap_or(0),
            stdout,
            stderr,
        };

        debug!(
            exit_code = result.exit_code,
            stdout_len = result.stdout.len(),
            stderr_len = result.stderr.len(),
            "command finished"
        );

        Ok(result)
    }

    /// Open a direct-tcpip channel for port forwarding.
    pub async fn open_direct_tcpip(
        &self,
        host: &str,
        port: u32,
        originator: &str,
        originator_port: u32,
    ) -> Result<russh::Channel<client::Msg>, SshError> {
        self.handle
            .channel_open_direct_tcpip(host, port, originator, originator_port)
            .await
            .map_err(|e| SshError::Channel(format!("direct-tcpip failed: {e}")))
    }

    /// Check if the session is still alive.
    pub fn is_closed(&self) -> bool {
        self.handle.is_closed()
    }

    /// Disconnect the session gracefully.
    pub async fn disconnect(&self) -> Result<(), SshError> {
        info!(host = %self.host, "disconnecting SSH session");
        self.handle
            .disconnect(Disconnect::ByApplication, "shelly disconnect", "en")
            .await
            .map_err(|e| SshError::Channel(format!("disconnect failed: {e}")))?;
        Ok(())
    }

    /// Get the remote host address.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Get the remote port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get the username.
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Get a reference to the underlying russh handle.
    ///
    /// Useful for advanced operations not directly exposed by SshSession.
    pub fn handle(&self) -> &Handle<ShellyHandler> {
        &self.handle
    }
}

/// Result of a remote command execution.
#[derive(Debug)]
pub struct ExecResult {
    /// Process exit code (0 = success).
    pub exit_code: u32,
    /// Standard output bytes.
    pub stdout: Vec<u8>,
    /// Standard error bytes.
    pub stderr: Vec<u8>,
}

impl ExecResult {
    /// Get stdout as a UTF-8 string (lossy conversion).
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// Get stderr as a UTF-8 string (lossy conversion).
    pub fn stderr_str(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }

    /// Whether the command exited successfully (code 0).
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Build a russh client::Config from connection options.
fn build_config(conn: &Connection) -> client::Config {
    let mut config = client::Config::default();

    if let Some(timeout) = conn.options.connect_timeout {
        config.inactivity_timeout = Some(Duration::from_secs(timeout as u64));
    }

    if let Some(interval) = conn.options.keepalive_interval {
        config.keepalive_interval = Some(Duration::from_secs(interval as u64));
    }

    if let Some(max) = conn.options.keepalive_count_max {
        config.keepalive_max = max as usize;
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_result_helpers() {
        let result = ExecResult {
            exit_code: 0,
            stdout: b"hello world\n".to_vec(),
            stderr: Vec::new(),
        };
        assert!(result.success());
        assert_eq!(result.stdout_str(), "hello world\n");
        assert_eq!(result.stderr_str(), "");
    }

    #[test]
    fn test_exec_result_failure() {
        let result = ExecResult {
            exit_code: 127,
            stdout: Vec::new(),
            stderr: b"command not found\n".to_vec(),
        };
        assert!(!result.success());
        assert_eq!(result.stderr_str(), "command not found\n");
    }

    #[test]
    fn test_build_config_defaults() {
        let conn = Connection::new("test", "localhost", "user");
        let config = build_config(&conn);
        // Default keepalive from SshOptions: 30s
        assert_eq!(config.keepalive_interval, Some(Duration::from_secs(30)));
        assert_eq!(config.keepalive_max, 3);
        assert_eq!(config.inactivity_timeout, Some(Duration::from_secs(10)));
    }

    #[test]
    fn test_build_config_custom() {
        let mut conn = Connection::new("test", "localhost", "user");
        conn.options.keepalive_interval = Some(60);
        conn.options.keepalive_count_max = Some(5);
        conn.options.connect_timeout = Some(30);
        let config = build_config(&conn);
        assert_eq!(config.keepalive_interval, Some(Duration::from_secs(60)));
        assert_eq!(config.keepalive_max, 5);
        assert_eq!(config.inactivity_timeout, Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_build_config_none_options() {
        let mut conn = Connection::new("test", "localhost", "user");
        conn.options.keepalive_interval = None;
        conn.options.keepalive_count_max = None;
        conn.options.connect_timeout = None;
        let config = build_config(&conn);
        // When None, keep russh defaults
        assert!(config.keepalive_interval.is_none());
        assert!(config.inactivity_timeout.is_none());
    }
}
