use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::de::DeserializeOwned;
use serde_json::Value;
use shelly_core::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, pid_file_path, socket_path};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// JSON-RPC client for communicating with shellyd.
pub struct DaemonClient {
    stream: BufReader<UnixStream>,
    next_id: AtomicU64,
}

impl DaemonClient {
    /// Connect to the daemon via Unix socket.
    pub async fn connect() -> Result<Self, ClientError> {
        let path = socket_path();
        Self::connect_to(&path).await
    }

    /// Connect to a specific socket path.
    pub async fn connect_to(path: &Path) -> Result<Self, ClientError> {
        let stream = UnixStream::connect(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::ConnectionRefused
                || e.kind() == std::io::ErrorKind::NotFound
            {
                ClientError::DaemonNotRunning
            } else {
                ClientError::Io(e)
            }
        })?;

        Ok(Self {
            stream: BufReader::new(stream),
            next_id: AtomicU64::new(1),
        })
    }

    /// Send a JSON-RPC request and return the raw result value.
    pub async fn call(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest::new(method, params, id);

        let mut json = serde_json::to_string(&request)
            .map_err(|e| ClientError::Protocol(format!("serialize error: {e}")))?;
        json.push('\n');

        self.stream
            .get_mut()
            .write_all(json.as_bytes())
            .await
            .map_err(ClientError::Io)?;

        self.stream
            .get_mut()
            .flush()
            .await
            .map_err(ClientError::Io)?;

        let mut line = String::new();
        let n = self
            .stream
            .read_line(&mut line)
            .await
            .map_err(ClientError::Io)?;

        if n == 0 {
            return Err(ClientError::Protocol("daemon closed connection".into()));
        }

        let response: JsonRpcResponse = serde_json::from_str(line.trim())
            .map_err(|e| ClientError::Protocol(format!("invalid response: {e}")))?;

        if let Some(error) = response.error {
            return Err(ClientError::Rpc(error));
        }

        response
            .result
            .ok_or_else(|| ClientError::Protocol("missing result in response".into()))
    }

    /// Send a JSON-RPC request and deserialize the result.
    #[allow(dead_code)]
    pub async fn call_typed<P: serde::Serialize, R: DeserializeOwned>(
        &mut self,
        method: &str,
        params: &P,
    ) -> Result<R, ClientError> {
        let params_value = serde_json::to_value(params)
            .map_err(|e| ClientError::Protocol(format!("serialize params: {e}")))?;
        let result = self.call(method, Some(params_value)).await?;
        serde_json::from_value(result)
            .map_err(|e| ClientError::Protocol(format!("deserialize result: {e}")))
    }
}

/// Ensure the daemon is running, starting it if needed.
pub async fn ensure_daemon_running() -> Result<(), ClientError> {
    let sock = socket_path();
    if sock.exists() {
        // Try connecting
        if UnixStream::connect(&sock).await.is_ok() {
            return Ok(());
        }
        // Stale socket — clean up
        let _ = std::fs::remove_file(&sock);
    }

    // Clean up stale PID file
    let pid_path = pid_file_path();
    if pid_path.exists() {
        let _ = std::fs::remove_file(&pid_path);
    }

    // Spawn shellyd in the background
    let shellyd = which_shellyd()?;
    std::process::Command::new(&shellyd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| ClientError::Protocol(format!("failed to start shellyd: {e}")))?;

    // Wait for socket to appear
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if sock.exists() {
            if UnixStream::connect(&sock).await.is_ok() {
                return Ok(());
            }
        }
    }

    Err(ClientError::Protocol(
        "shellyd started but socket not available after 5s".into(),
    ))
}

/// Find the shellyd binary.
fn which_shellyd() -> Result<std::path::PathBuf, ClientError> {
    // First, try next to the current executable
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or(Path::new("."));
        let candidate = dir.join("shellyd");
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    // Then try PATH
    if let Ok(output) = std::process::Command::new("which")
        .arg("shellyd")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path.into());
            }
        }
    }

    Err(ClientError::Protocol(
        "shellyd not found — install it or set PATH".into(),
    ))
}

#[derive(Debug)]
pub enum ClientError {
    DaemonNotRunning,
    Io(std::io::Error),
    Protocol(String),
    Rpc(JsonRpcError),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::DaemonNotRunning => {
                write!(f, "daemon is not running (start with: shelly daemon start)")
            }
            ClientError::Io(e) => write!(f, "I/O error: {e}"),
            ClientError::Protocol(msg) => write!(f, "protocol error: {msg}"),
            ClientError::Rpc(e) => write!(f, "{}", e.message),
        }
    }
}

impl std::error::Error for ClientError {}
