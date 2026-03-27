use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
use rivet_core::protocol::{JsonRpcRequest, JsonRpcResponse, pid_file_path, socket_path};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// JSON-RPC client for communicating with rivetd.
pub struct DaemonClient {
    stream: BufReader<UnixStream>,
    next_id: AtomicU64,
}

impl DaemonClient {
    /// Connect to the daemon via Unix socket.
    pub async fn connect() -> Result<Self, McpClientError> {
        let path = socket_path();
        Self::connect_to(&path).await
    }

    /// Connect to a specific socket path.
    pub async fn connect_to(path: &Path) -> Result<Self, McpClientError> {
        let stream = UnixStream::connect(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::ConnectionRefused
                || e.kind() == std::io::ErrorKind::NotFound
            {
                McpClientError::DaemonNotRunning
            } else {
                McpClientError::Io(e.to_string())
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
    ) -> Result<Value, McpClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = JsonRpcRequest::new(method, params, id);

        let mut json = serde_json::to_string(&request)
            .map_err(|e| McpClientError::Protocol(format!("serialize error: {e}")))?;
        json.push('\n');

        self.stream
            .get_mut()
            .write_all(json.as_bytes())
            .await
            .map_err(|e| McpClientError::Io(e.to_string()))?;

        self.stream
            .get_mut()
            .flush()
            .await
            .map_err(|e| McpClientError::Io(e.to_string()))?;

        let mut line = String::new();
        let n = self
            .stream
            .read_line(&mut line)
            .await
            .map_err(|e| McpClientError::Io(e.to_string()))?;

        if n == 0 {
            return Err(McpClientError::Protocol("daemon closed connection".into()));
        }

        let response: JsonRpcResponse = serde_json::from_str(line.trim())
            .map_err(|e| McpClientError::Protocol(format!("invalid response: {e}")))?;

        if let Some(error) = response.error {
            return Err(McpClientError::Rpc(error.message));
        }

        response
            .result
            .ok_or_else(|| McpClientError::Protocol("missing result in response".into()))
    }
}

/// Ensure the daemon is running, starting it if needed.
pub async fn ensure_daemon_running() -> Result<(), McpClientError> {
    let sock = socket_path();
    if sock.exists() {
        if UnixStream::connect(&sock).await.is_ok() {
            return Ok(());
        }
        let _ = std::fs::remove_file(&sock);
    }

    let pid_path = pid_file_path();
    if pid_path.exists() {
        let _ = std::fs::remove_file(&pid_path);
    }

    let rivetd = which_rivetd()?;
    std::process::Command::new(&rivetd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| McpClientError::Protocol(format!("failed to start rivetd: {e}")))?;

    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if sock.exists() {
            if UnixStream::connect(&sock).await.is_ok() {
                return Ok(());
            }
        }
    }

    Err(McpClientError::Protocol(
        "rivetd started but socket not available after 5s".into(),
    ))
}

fn which_rivetd() -> Result<std::path::PathBuf, McpClientError> {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent().unwrap_or(Path::new("."));
        let candidate = dir.join("rivetd");
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    if let Ok(output) = std::process::Command::new("which")
        .arg("rivetd")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path.into());
            }
        }
    }

    Err(McpClientError::Protocol(
        "rivetd not found — install it or set PATH".into(),
    ))
}

#[derive(Debug)]
pub enum McpClientError {
    DaemonNotRunning,
    Io(String),
    Protocol(String),
    Rpc(String),
}

impl std::fmt::Display for McpClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpClientError::DaemonNotRunning => {
                write!(f, "Rivet daemon is not running. Start it with: rivet daemon start")
            }
            McpClientError::Io(e) => write!(f, "I/O error: {e}"),
            McpClientError::Protocol(msg) => write!(f, "protocol error: {msg}"),
            McpClientError::Rpc(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for McpClientError {}
