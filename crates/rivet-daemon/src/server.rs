use std::path::Path;

use rivet_core::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{debug, error, info, warn};

use crate::handlers;
use crate::state::SharedState;

/// Start the JSON-RPC server on a Unix domain socket.
///
/// Listens for connections, spawning a task per client.
/// Each client sends newline-delimited JSON-RPC requests.
pub async fn run_server(socket_path: &Path, state: SharedState) -> std::io::Result<()> {
    // Remove stale socket if it exists
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    // Create parent directory if needed
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    info!(path = %socket_path.display(), "daemon listening");

    // Set socket permissions to owner-only (0600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
    }

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                debug!("client connected");
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, state).await {
                        warn!(error = %e, "client handler error");
                    }
                    debug!("client disconnected");
                });
            }
            Err(e) => {
                error!(error = %e, "accept failed");
            }
        }
    }
}

/// Handle a single client connection.
///
/// Reads newline-delimited JSON-RPC requests and writes responses.
async fn handle_client(stream: UnixStream, state: SharedState) -> std::io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            // Client disconnected
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = process_request(trimmed, &state).await;

        let mut response_json = serde_json::to_string(&response)
            .unwrap_or_else(|e| {
                serde_json::to_string(&JsonRpcResponse::error(
                    serde_json::Value::Null,
                    JsonRpcError {
                        code: -32603,
                        message: format!("serialization error: {e}"),
                        data: None,
                    },
                ))
                .unwrap()
            });

        response_json.push('\n');
        writer.write_all(response_json.as_bytes()).await?;
        writer.flush().await?;
    }

    Ok(())
}

/// Parse and dispatch a JSON-RPC request.
async fn process_request(raw: &str, state: &SharedState) -> JsonRpcResponse {
    // Parse JSON
    let request: JsonRpcRequest = match serde_json::from_str(raw) {
        Ok(req) => req,
        Err(e) => {
            return JsonRpcResponse::error(
                serde_json::Value::Null,
                JsonRpcError {
                    code: -32700,
                    message: format!("parse error: {e}"),
                    data: None,
                },
            );
        }
    };

    // Validate JSON-RPC version
    if request.jsonrpc != "2.0" {
        return JsonRpcResponse::error(
            request.id,
            JsonRpcError {
                code: -32600,
                message: "invalid JSON-RPC version (expected 2.0)".into(),
                data: None,
            },
        );
    }

    debug!(method = %request.method, "dispatching RPC");

    // Dispatch to handler
    let result = handlers::dispatch(state, &request.method, request.params.clone()).await;

    match result {
        Ok(value) => JsonRpcResponse::success(request.id, value),
        Err(rpc_error) => JsonRpcResponse::error(request.id, rpc_error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::DaemonState;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn make_state() -> SharedState {
        Arc::new(RwLock::new(DaemonState::new()))
    }

    #[tokio::test]
    async fn test_process_invalid_json() {
        let state = make_state();
        let resp = process_request("not json", &state).await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32700);
    }

    #[tokio::test]
    async fn test_process_wrong_version() {
        let state = make_state();
        let resp = process_request(
            r#"{"jsonrpc":"1.0","method":"test","id":1}"#,
            &state,
        )
        .await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32600);
    }

    #[tokio::test]
    async fn test_process_unknown_method() {
        let state = make_state();
        let resp = process_request(
            r#"{"jsonrpc":"2.0","method":"nonexistent.method","id":1}"#,
            &state,
        )
        .await;
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn test_process_daemon_status() {
        let state = make_state();
        let resp = process_request(
            r#"{"jsonrpc":"2.0","method":"daemon.status","id":1}"#,
            &state,
        )
        .await;
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }
}
