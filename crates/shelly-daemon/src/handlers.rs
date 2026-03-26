use std::path::PathBuf;

use serde_json::Value;
use shelly_core::error::ShellyError;
use shelly_core::protocol::*;
use shelly_vault::store::VaultStore;
use tracing::{debug, info};
use uuid::Uuid;

use crate::state::SharedState;

/// Dispatch a JSON-RPC method call to the appropriate handler.
///
/// Returns `Ok(Value)` on success, or a JSON-RPC error.
pub async fn dispatch(
    state: &SharedState,
    method: &str,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    match method {
        // Always available
        "daemon.status" => handle_daemon_status(state).await,
        "vault.status" => handle_vault_status(state).await,
        "vault.init" => handle_vault_init(state, params).await,
        "vault.unlock" => handle_vault_unlock(state, params).await,
        "vault.lock" => handle_vault_lock(state).await,

        // Require unlocked vault
        "vault.change_password" => handle_vault_change_password(state, params).await,
        "conn.list" => handle_conn_list(state, params).await,
        "conn.get" => handle_conn_get(state, params).await,
        "conn.create" => handle_conn_create(state, params).await,
        "conn.update" => handle_conn_update(state, params).await,
        "conn.delete" => handle_conn_delete(state, params).await,
        "conn.import" => handle_conn_import(state, params).await,

        // SSH operations
        "ssh.exec" => handle_ssh_exec(state, params).await,
        "ssh.connect_info" => handle_ssh_connect_info(state, params).await,

        // SCP operations
        "scp.upload" => handle_scp_upload(state, params).await,
        "scp.download" => handle_scp_download(state, params).await,

        _ => Err(JsonRpcError {
            code: -32601,
            message: format!("method not found: {method}"),
            data: None,
        }),
    }
}

// --- Helper ---

fn parse_params<T: serde::de::DeserializeOwned>(params: Option<Value>) -> Result<T, JsonRpcError> {
    let value = params.unwrap_or(Value::Object(serde_json::Map::new()));
    serde_json::from_value(value).map_err(|e| JsonRpcError {
        code: -32602,
        message: format!("invalid params: {e}"),
        data: None,
    })
}

fn to_rpc_error(e: ShellyError) -> JsonRpcError {
    JsonRpcError {
        code: e.rpc_error_code(),
        message: e.to_string(),
        data: None,
    }
}

fn to_value<T: serde::Serialize>(v: T) -> Result<Value, JsonRpcError> {
    serde_json::to_value(v).map_err(|e| JsonRpcError {
        code: -32603,
        message: format!("serialization error: {e}"),
        data: None,
    })
}

// --- Daemon ---

async fn handle_daemon_status(state: &SharedState) -> Result<Value, JsonRpcError> {
    let state = state.read().await;
    to_value(DaemonStatusResult {
        uptime_secs: state.uptime_secs(),
        active_sessions: state.active_session_count(),
        vault_locked: !state.is_vault_unlocked(),
    })
}

// --- Vault ---

async fn handle_vault_status(state: &SharedState) -> Result<Value, JsonRpcError> {
    let state = state.read().await;
    to_value(VaultStatusResult {
        initialized: state.is_vault_initialized(),
        locked: !state.is_vault_unlocked(),
    })
}

async fn handle_vault_init(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: VaultInitParams = parse_params(params)?;
    let mut state = state.write().await;

    let vault_dir = vault_dir();
    info!(path = %vault_dir.display(), "initializing vault");

    let store = VaultStore::new(vault_dir);
    store.init(&p.password).map_err(to_rpc_error)?;

    // Unlock immediately after init
    let vault = store.unlock(&p.password).map_err(to_rpc_error)?;
    state.vault = Some(vault);
    state.vault_store = None; // vault is unlocked, store is consumed

    to_value(OkResult { ok: true })
}

async fn handle_vault_unlock(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: VaultUnlockParams = parse_params(params)?;
    let mut state = state.write().await;

    if state.is_vault_unlocked() {
        return to_value(OkResult { ok: true });
    }

    let vault_dir = vault_dir();
    debug!(path = %vault_dir.display(), "unlocking vault");

    let store = VaultStore::new(vault_dir);
    let vault = store.unlock(&p.password).map_err(to_rpc_error)?;
    state.vault = Some(vault);
    state.vault_store = None;

    info!("vault unlocked");
    to_value(OkResult { ok: true })
}

async fn handle_vault_lock(state: &SharedState) -> Result<Value, JsonRpcError> {
    let mut state = state.write().await;

    if let Some(vault) = state.vault.take() {
        // Close all SSH sessions
        for (_id, session) in state.sessions.drain() {
            let _ = session.disconnect().await;
        }
        // Lock returns the VaultStore
        let store = vault.lock();
        state.vault_store = Some(store);
        info!("vault locked");
    }

    to_value(OkResult { ok: true })
}

async fn handle_vault_change_password(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: VaultChangePasswordParams = parse_params(params)?;
    let mut state = state.write().await;

    let vault = state.require_vault().map_err(to_rpc_error)?;
    vault
        .change_password(&p.old_password, &p.new_password)
        .map_err(to_rpc_error)?;

    info!("vault password changed");
    to_value(OkResult { ok: true })
}

// --- Connections ---

async fn handle_conn_list(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: ConnListParams = parse_params(params)?;
    let state = state.read().await;

    let vault = state
        .vault
        .as_ref()
        .ok_or_else(|| to_rpc_error(ShellyError::VaultLocked))?;

    let mut connections = vault.list_connections().map_err(to_rpc_error)?;

    // Filter by tag
    if let Some(ref tag) = p.tag {
        connections.retain(|c| c.tags.contains(tag));
    }

    // Filter by group
    if let Some(ref group_id) = p.group_id {
        connections.retain(|c| c.group_ids.contains(group_id));
    }

    to_value(connections)
}

async fn handle_conn_get(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: ConnGetParams = parse_params(params)?;
    let state = state.read().await;

    let vault = state
        .vault
        .as_ref()
        .ok_or_else(|| to_rpc_error(ShellyError::VaultLocked))?;

    let conn = if let Some(id) = p.id {
        vault
            .load_connection(&id)
            .map_err(to_rpc_error)?
    } else if let Some(ref name) = p.name {
        find_connection_by_name(vault, name)?
    } else {
        return Err(JsonRpcError {
            code: -32602,
            message: "either 'id' or 'name' is required".into(),
            data: None,
        });
    };

    to_value(conn)
}

async fn handle_conn_create(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: ConnCreateParams = parse_params(params)?;
    let mut state = state.write().await;

    let vault = state.require_vault().map_err(to_rpc_error)?;

    // Check for duplicate name
    let existing = vault.list_connections().map_err(to_rpc_error)?;
    if existing.iter().any(|c| c.name == p.name) {
        return Err(to_rpc_error(ShellyError::DuplicateConnectionName(
            p.name.clone(),
        )));
    }

    let conn = p.into_connection();
    let id = conn.id;
    vault.save_connection(&conn).map_err(to_rpc_error)?;

    info!(id = %id, name = %conn.name, "connection created");
    to_value(IdResult { id })
}

async fn handle_conn_update(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: ConnUpdateParams = parse_params(params)?;
    let mut state = state.write().await;

    let vault = state.require_vault().map_err(to_rpc_error)?;

    let mut conn = vault
        .load_connection(&p.id)
        .map_err(to_rpc_error)?;

    // Check for duplicate name if name is being changed
    if let Some(ref new_name) = p.name {
        if *new_name != conn.name {
            let existing = vault.list_connections().map_err(to_rpc_error)?;
            if existing.iter().any(|c| c.name == *new_name && c.id != p.id) {
                return Err(to_rpc_error(ShellyError::DuplicateConnectionName(
                    new_name.clone(),
                )));
            }
        }
    }

    p.apply_to(&mut conn);
    vault.save_connection(&conn).map_err(to_rpc_error)?;

    info!(id = %conn.id, name = %conn.name, "connection updated");
    to_value(OkResult { ok: true })
}

async fn handle_conn_delete(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: ConnDeleteParams = parse_params(params)?;
    let mut state = state.write().await;

    let vault = state.require_vault().map_err(to_rpc_error)?;

    let id = if let Some(id) = p.id {
        id
    } else if let Some(ref name) = p.name {
        let conn = find_connection_by_name(vault, name)?;
        conn.id
    } else {
        return Err(JsonRpcError {
            code: -32602,
            message: "either 'id' or 'name' is required".into(),
            data: None,
        });
    };

    vault.delete_connection(&id).map_err(to_rpc_error)?;

    // Remove any active session for this connection
    if let Some(session) = state.sessions.remove(&id) {
        let _ = session.disconnect().await;
    }

    info!(id = %id, "connection deleted");
    to_value(OkResult { ok: true })
}

async fn handle_conn_import(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: ConnImportParams = parse_params(params)?;
    let mut state = state.write().await;

    let vault = state.require_vault().map_err(to_rpc_error)?;

    let path = p.path.unwrap_or_else(|| {
        dirs::home_dir()
            .expect("home directory not found")
            .join(".ssh")
            .join("config")
    });

    let imported = shelly_vault::import::import_ssh_config_to_vault(vault, &path)
        .map_err(to_rpc_error)?;

    info!(count = imported, path = %path.display(), "imported SSH config");
    to_value(ConnImportResult {
        imported: imported as u32,
    })
}

// --- SSH ---

async fn handle_ssh_exec(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: SshExecParams = parse_params(params)?;

    // Load connection from vault
    let conn = {
        let state = state.read().await;
        let vault = state
            .vault
            .as_ref()
            .ok_or_else(|| to_rpc_error(ShellyError::VaultLocked))?;
        vault
            .load_connection(&p.connection_id)
            .map_err(to_rpc_error)?
    };

    // Get or create SSH session
    let result = {
        let mut state = state.write().await;

        // Check existing session
        let needs_new_session = match state.sessions.get(&p.connection_id) {
            Some(session) if !session.is_closed() => false,
            _ => true,
        };

        if needs_new_session {
            debug!(id = %p.connection_id, "creating new SSH session");
            let session = shelly_ssh::SshSession::connect(&conn)
                .await
                .map_err(|e| to_rpc_error(ShellyError::from(e)))?;
            state.sessions.insert(p.connection_id, session);
        }

        let session = state.sessions.get(&p.connection_id).unwrap();
        session.exec(&p.command).await
    };

    let exec_result = result.map_err(|e| to_rpc_error(ShellyError::from(e)))?;

    to_value(SshExecResult {
        exit_code: exec_result.exit_code as i32,
        stdout: exec_result.stdout_str(),
        stderr: exec_result.stderr_str(),
    })
}

async fn handle_ssh_connect_info(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: SshConnectInfoParams = parse_params(params)?;
    let state = state.read().await;

    let vault = state
        .vault
        .as_ref()
        .ok_or_else(|| to_rpc_error(ShellyError::VaultLocked))?;

    let conn = vault
        .load_connection(&p.connection_id)
        .map_err(to_rpc_error)?;

    let key_path = match &conn.auth {
        shelly_core::connection::AuthMethod::KeyFile { path, .. } => {
            Some(path.to_string_lossy().into_owned())
        }
        _ => None,
    };

    to_value(SshConnectInfoResult {
        host: conn.host,
        port: conn.port,
        username: conn.username,
        key_path,
        extra_args: conn.options.extra_args,
    })
}

// --- SCP ---

async fn handle_scp_upload(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: ScpUploadParams = parse_params(params)?;

    let conn = {
        let state = state.read().await;
        let vault = state
            .vault
            .as_ref()
            .ok_or_else(|| to_rpc_error(ShellyError::VaultLocked))?;
        vault
            .load_connection(&p.connection_id)
            .map_err(to_rpc_error)?
    };

    let bytes = {
        let mut state = state.write().await;
        ensure_session(&mut state, &p.connection_id, &conn).await?;
        let session = state.sessions.get(&p.connection_id).unwrap();

        session
            .upload_file(&PathBuf::from(&p.local_path), &p.remote_path)
            .await
            .map_err(|e| to_rpc_error(ShellyError::ScpTransferFailed(e.to_string())))?
    };

    to_value(ScpResult {
        bytes_transferred: bytes,
    })
}

async fn handle_scp_download(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: ScpDownloadParams = parse_params(params)?;

    let conn = {
        let state = state.read().await;
        let vault = state
            .vault
            .as_ref()
            .ok_or_else(|| to_rpc_error(ShellyError::VaultLocked))?;
        vault
            .load_connection(&p.connection_id)
            .map_err(to_rpc_error)?
    };

    let bytes = {
        let mut state = state.write().await;
        ensure_session(&mut state, &p.connection_id, &conn).await?;
        let session = state.sessions.get(&p.connection_id).unwrap();

        session
            .download_file(&p.remote_path, &PathBuf::from(&p.local_path))
            .await
            .map_err(|e| to_rpc_error(ShellyError::ScpTransferFailed(e.to_string())))?
    };

    to_value(ScpResult {
        bytes_transferred: bytes,
    })
}

// --- Helpers ---

/// Ensure an SSH session exists for the given connection ID.
async fn ensure_session(
    state: &mut crate::state::DaemonState,
    connection_id: &Uuid,
    conn: &shelly_core::connection::Connection,
) -> Result<(), JsonRpcError> {
    let needs_new = match state.sessions.get(connection_id) {
        Some(session) if !session.is_closed() => false,
        _ => true,
    };

    if needs_new {
        debug!(id = %connection_id, "creating SSH session");
        let session = shelly_ssh::SshSession::connect(conn)
            .await
            .map_err(|e| to_rpc_error(ShellyError::from(e)))?;
        state.sessions.insert(*connection_id, session);
    }

    Ok(())
}

/// Find a connection by name in the vault.
fn find_connection_by_name(
    vault: &shelly_vault::store::UnlockedVault,
    name: &str,
) -> Result<shelly_core::connection::Connection, JsonRpcError> {
    let connections = vault.list_connections().map_err(to_rpc_error)?;
    connections
        .into_iter()
        .find(|c| c.name == name)
        .ok_or_else(|| to_rpc_error(ShellyError::ConnectionNotFound(name.to_string())))
}
