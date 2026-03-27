use std::path::PathBuf;

use serde_json::Value;
use rivet_core::error::RivetError;
use rivet_core::protocol::*;
use rivet_vault::store::VaultStore;
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

        // Group operations
        "group.list" => handle_group_list(state).await,
        "group.get" => handle_group_get(state, params).await,
        "group.create" => handle_group_create(state, params).await,
        "group.update" => handle_group_update(state, params).await,
        "group.delete" => handle_group_delete(state, params).await,

        // Group operations (bulk)
        "group.exec" => handle_group_exec(state, params).await,
        "group.upload" => handle_group_upload(state, params).await,

        // Tunnel operations
        "tunnel.create" => handle_tunnel_create(state, params).await,
        "tunnel.list" => handle_tunnel_list(state).await,
        "tunnel.close" => handle_tunnel_close(state, params).await,

        // Workflow operations
        "workflow.list" => handle_workflow_list(state).await,
        "workflow.get" => handle_workflow_get(state, params).await,
        "workflow.import" => handle_workflow_import(state, params).await,
        "workflow.delete" => handle_workflow_delete(state, params).await,
        "workflow.run" => handle_workflow_run(state, params).await,

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

fn to_rpc_error(e: RivetError) -> JsonRpcError {
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
        active_tunnels: state.active_tunnel_count(),
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

    // Use the vault_store from state (set by lock()), or fall back to default path
    let store = state.vault_store.take().unwrap_or_else(|| {
        let vault_dir = vault_dir();
        debug!(path = %vault_dir.display(), "unlocking vault");
        VaultStore::new(vault_dir)
    });

    let vault = store.unlock(&p.password).map_err(to_rpc_error)?;
    state.vault = Some(vault);

    info!("vault unlocked");
    to_value(OkResult { ok: true })
}

async fn handle_vault_lock(state: &SharedState) -> Result<Value, JsonRpcError> {
    let mut state = state.write().await;

    if let Some(vault) = state.vault.take() {
        // Close all tunnels
        for (_id, tunnel) in state.tunnels.drain() {
            tunnel.shutdown().await;
        }
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
        .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

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
        .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

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
        return Err(to_rpc_error(RivetError::DuplicateConnectionName(
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
                return Err(to_rpc_error(RivetError::DuplicateConnectionName(
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

    let imported = rivet_vault::import::import_ssh_config_to_vault(vault, &path)
        .map_err(to_rpc_error)?;

    info!(count = imported, path = %path.display(), "imported SSH config");
    to_value(ConnImportResult {
        imported: imported as u32,
    })
}

// --- Groups ---

async fn handle_group_list(state: &SharedState) -> Result<Value, JsonRpcError> {
    let state = state.read().await;

    let vault = state
        .vault
        .as_ref()
        .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

    let groups = vault.list_groups().map_err(to_rpc_error)?;
    to_value(groups)
}

async fn handle_group_get(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: GroupGetParams = parse_params(params)?;
    let state = state.read().await;

    let vault = state
        .vault
        .as_ref()
        .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

    let group = if let Some(id) = p.id {
        vault.load_group(&id).map_err(to_rpc_error)?
    } else if let Some(ref name) = p.name {
        find_group_by_name(vault, name)?
    } else {
        return Err(JsonRpcError {
            code: -32602,
            message: "either 'id' or 'name' is required".into(),
            data: None,
        });
    };

    to_value(group)
}

async fn handle_group_create(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: GroupCreateParams = parse_params(params)?;
    let mut state = state.write().await;

    let vault = state.require_vault().map_err(to_rpc_error)?;

    // Check for duplicate name
    let existing = vault.list_groups().map_err(to_rpc_error)?;
    if existing.iter().any(|g| g.name == p.name) {
        return Err(to_rpc_error(RivetError::DuplicateGroupName(
            p.name.clone(),
        )));
    }

    let group = p.into_group();
    let id = group.id;
    vault.save_group(&group).map_err(to_rpc_error)?;

    info!(id = %id, name = %group.name, "group created");
    to_value(IdResult { id })
}

async fn handle_group_update(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: GroupUpdateParams = parse_params(params)?;
    let mut state = state.write().await;

    let vault = state.require_vault().map_err(to_rpc_error)?;

    let mut group = vault.load_group(&p.id).map_err(to_rpc_error)?;

    // Check for duplicate name if name is being changed
    if let Some(ref new_name) = p.name {
        if *new_name != group.name {
            let existing = vault.list_groups().map_err(to_rpc_error)?;
            if existing.iter().any(|g| g.name == *new_name && g.id != p.id) {
                return Err(to_rpc_error(RivetError::DuplicateGroupName(
                    new_name.clone(),
                )));
            }
        }
    }

    p.apply_to(&mut group);
    vault.save_group(&group).map_err(to_rpc_error)?;

    info!(id = %group.id, name = %group.name, "group updated");
    to_value(OkResult { ok: true })
}

async fn handle_group_delete(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: GroupDeleteParams = parse_params(params)?;
    let mut state = state.write().await;

    let vault = state.require_vault().map_err(to_rpc_error)?;

    let id = if let Some(id) = p.id {
        id
    } else if let Some(ref name) = p.name {
        let group = find_group_by_name(vault, name)?;
        group.id
    } else {
        return Err(JsonRpcError {
            code: -32602,
            message: "either 'id' or 'name' is required".into(),
            data: None,
        });
    };

    // Remove group_id from all connections that reference it
    let connections = vault.list_connections().map_err(to_rpc_error)?;
    for mut conn in connections {
        if conn.group_ids.contains(&id) {
            conn.group_ids.retain(|gid| *gid != id);
            conn.updated_at = chrono::Utc::now();
            vault.save_connection(&conn).map_err(to_rpc_error)?;
        }
    }

    vault.delete_group(&id).map_err(to_rpc_error)?;

    info!(id = %id, "group deleted");
    to_value(OkResult { ok: true })
}

/// Find a group by name in the vault.
fn find_group_by_name(
    vault: &rivet_vault::store::UnlockedVault,
    name: &str,
) -> Result<rivet_core::connection::Group, JsonRpcError> {
    vault
        .find_group_by_name(name)
        .map_err(to_rpc_error)
}

// --- Group Operations ---

async fn handle_group_exec(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: GroupExecParams = parse_params(params)?;

    // Resolve group and get member connections
    let (connections, group_name) = {
        let state = state.read().await;
        let vault = state
            .vault
            .as_ref()
            .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

        let group = if let Some(id) = p.group_id {
            vault.load_group(&id).map_err(to_rpc_error)?
        } else if let Some(ref name) = p.group_name {
            find_group_by_name(vault, name)?
        } else {
            return Err(JsonRpcError {
                code: -32602,
                message: "either 'group_id' or 'group_name' is required".into(),
                data: None,
            });
        };

        let all_conns = vault.list_connections().map_err(to_rpc_error)?;
        let members: Vec<_> = all_conns
            .into_iter()
            .filter(|c| c.group_ids.contains(&group.id))
            .collect();

        if members.is_empty() {
            return Err(JsonRpcError {
                code: -32602,
                message: format!("group '{}' has no connections", group.name),
                data: None,
            });
        }

        (members, group.name)
    };

    let concurrency = p.concurrency.unwrap_or(connections.len());
    info!(
        group = %group_name,
        hosts = connections.len(),
        concurrency,
        command = %p.command,
        "group exec starting"
    );

    // Execute on all connections in parallel with concurrency limit
    let mut results = Vec::with_capacity(connections.len());

    // Use chunks for concurrency control
    for chunk in connections.chunks(concurrency) {
        let mut handles = Vec::new();

        for conn in chunk {
            let state = state.clone();
            let command = p.command.clone();
            let conn = conn.clone();

            handles.push(tokio::spawn(async move {
                exec_on_host(&state, &conn, &command).await
            }));
        }

        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(GroupExecHostResult {
                    connection_id: Uuid::nil(),
                    connection_name: "unknown".into(),
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: String::new(),
                    error: Some(format!("task panicked: {e}")),
                }),
            }
        }
    }

    info!(
        group = %group_name,
        total = results.len(),
        ok = results.iter().filter(|r| r.error.is_none() && r.exit_code == 0).count(),
        "group exec completed"
    );

    to_value(GroupExecResult { results })
}

/// Execute a command on a single host, returning a result entry.
async fn exec_on_host(
    state: &SharedState,
    conn: &rivet_core::connection::Connection,
    command: &str,
) -> GroupExecHostResult {
    // Ensure session
    {
        let mut state = state.write().await;
        let needs_new = match state.sessions.get(&conn.id) {
            Some(s) if !s.is_closed() => false,
            _ => true,
        };
        if needs_new {
            match rivet_ssh::SshSession::connect(conn).await {
                Ok(session) => {
                    state.sessions.insert(conn.id, session);
                }
                Err(e) => {
                    return GroupExecHostResult {
                        connection_id: conn.id,
                        connection_name: conn.name.clone(),
                        exit_code: -1,
                        stdout: String::new(),
                        stderr: String::new(),
                        error: Some(format!("connection failed: {e}")),
                    };
                }
            }
        }
    }

    // Execute
    let result = {
        let state = state.read().await;
        let session = state.sessions.get(&conn.id).unwrap();
        session.exec(command).await
    };

    match result {
        Ok(exec) => GroupExecHostResult {
            connection_id: conn.id,
            connection_name: conn.name.clone(),
            exit_code: exec.exit_code as i32,
            stdout: exec.stdout_str(),
            stderr: exec.stderr_str(),
            error: None,
        },
        Err(e) => GroupExecHostResult {
            connection_id: conn.id,
            connection_name: conn.name.clone(),
            exit_code: -1,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(format!("exec failed: {e}")),
        },
    }
}

async fn handle_group_upload(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: GroupUploadParams = parse_params(params)?;

    // Resolve group and get member connections
    let (connections, group_name) = {
        let state = state.read().await;
        let vault = state
            .vault
            .as_ref()
            .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

        let group = if let Some(id) = p.group_id {
            vault.load_group(&id).map_err(to_rpc_error)?
        } else if let Some(ref name) = p.group_name {
            find_group_by_name(vault, name)?
        } else {
            return Err(JsonRpcError {
                code: -32602,
                message: "either 'group_id' or 'group_name' is required".into(),
                data: None,
            });
        };

        let all_conns = vault.list_connections().map_err(to_rpc_error)?;
        let members: Vec<_> = all_conns
            .into_iter()
            .filter(|c| c.group_ids.contains(&group.id))
            .collect();

        if members.is_empty() {
            return Err(JsonRpcError {
                code: -32602,
                message: format!("group '{}' has no connections", group.name),
                data: None,
            });
        }

        (members, group.name)
    };

    let concurrency = p.concurrency.unwrap_or(connections.len());
    info!(
        group = %group_name,
        hosts = connections.len(),
        local = %p.local_path,
        remote = %p.remote_path,
        "group upload starting"
    );

    let mut results = Vec::with_capacity(connections.len());

    for chunk in connections.chunks(concurrency) {
        let mut handles = Vec::new();

        for conn in chunk {
            let state = state.clone();
            let local_path = p.local_path.clone();
            let remote_path = p.remote_path.clone();
            let conn = conn.clone();

            handles.push(tokio::spawn(async move {
                upload_to_host(&state, &conn, &local_path, &remote_path).await
            }));
        }

        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(GroupUploadHostResult {
                    connection_id: Uuid::nil(),
                    connection_name: "unknown".into(),
                    bytes_transferred: 0,
                    error: Some(format!("task panicked: {e}")),
                }),
            }
        }
    }

    info!(
        group = %group_name,
        total = results.len(),
        ok = results.iter().filter(|r| r.error.is_none()).count(),
        "group upload completed"
    );

    to_value(GroupUploadResult { results })
}

/// Upload a file to a single host, returning a result entry.
async fn upload_to_host(
    state: &SharedState,
    conn: &rivet_core::connection::Connection,
    local_path: &str,
    remote_path: &str,
) -> GroupUploadHostResult {
    // Ensure session
    {
        let mut state = state.write().await;
        let needs_new = match state.sessions.get(&conn.id) {
            Some(s) if !s.is_closed() => false,
            _ => true,
        };
        if needs_new {
            match rivet_ssh::SshSession::connect(conn).await {
                Ok(session) => {
                    state.sessions.insert(conn.id, session);
                }
                Err(e) => {
                    return GroupUploadHostResult {
                        connection_id: conn.id,
                        connection_name: conn.name.clone(),
                        bytes_transferred: 0,
                        error: Some(format!("connection failed: {e}")),
                    };
                }
            }
        }
    }

    // Upload
    let result = {
        let state = state.read().await;
        let session = state.sessions.get(&conn.id).unwrap();
        session
            .upload_file(&PathBuf::from(local_path), remote_path)
            .await
    };

    match result {
        Ok(bytes) => GroupUploadHostResult {
            connection_id: conn.id,
            connection_name: conn.name.clone(),
            bytes_transferred: bytes,
            error: None,
        },
        Err(e) => GroupUploadHostResult {
            connection_id: conn.id,
            connection_name: conn.name.clone(),
            bytes_transferred: 0,
            error: Some(format!("upload failed: {e}")),
        },
    }
}

// --- Tunnels ---

async fn handle_tunnel_create(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: TunnelCreateParams = parse_params(params)?;

    // Resolve connection
    let conn = {
        let state = state.read().await;
        let vault = state
            .vault
            .as_ref()
            .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

        if let Some(id) = p.connection_id {
            vault.load_connection(&id).map_err(to_rpc_error)?
        } else if let Some(ref name) = p.connection_name {
            find_connection_by_name(vault, name)?
        } else {
            return Err(JsonRpcError {
                code: -32602,
                message: "either 'connection_id' or 'connection_name' is required".into(),
                data: None,
            });
        }
    };

    // Ensure SSH session exists
    {
        let mut state = state.write().await;
        ensure_session(&mut state, &conn.id, &conn).await?;
    }

    // Start tunnel
    let tunnel_handle = {
        let state = state.read().await;
        let session = state.sessions.get(&conn.id).unwrap();
        let handle_arc = session.handle_arc();

        rivet_ssh::tunnel::start_tunnel(handle_arc, conn.id, p.spec, None)
            .map_err(|e| to_rpc_error(RivetError::TunnelError(e.to_string())))?
    };

    let tunnel_id = tunnel_handle.id;

    {
        let mut state = state.write().await;
        state.tunnels.insert(tunnel_id, tunnel_handle);
    }

    info!(id = %tunnel_id, connection = %conn.name, "tunnel created");
    to_value(TunnelCreateResult { id: tunnel_id })
}

async fn handle_tunnel_list(state: &SharedState) -> Result<Value, JsonRpcError> {
    let state = state.read().await;

    let vault = state
        .vault
        .as_ref()
        .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

    let mut tunnels = Vec::new();
    for (_, tunnel) in &state.tunnels {
        // Try to get connection name
        let conn_name = vault
            .load_connection(&tunnel.connection_id)
            .map(|c| c.name)
            .unwrap_or_else(|_| "unknown".into());

        tunnels.push(TunnelInfo {
            id: tunnel.id,
            connection_id: tunnel.connection_id,
            connection_name: conn_name,
            spec: tunnel.spec.clone(),
            active: !tunnel.is_finished(),
        });
    }

    to_value(tunnels)
}

async fn handle_tunnel_close(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: TunnelCloseParams = parse_params(params)?;

    let tunnel = {
        let mut state = state.write().await;
        state.tunnels.remove(&p.id)
    };

    if let Some(tunnel) = tunnel {
        info!(id = %p.id, "closing tunnel");
        tunnel.shutdown().await;
        to_value(OkResult { ok: true })
    } else {
        Err(JsonRpcError {
            code: -32015,
            message: format!("tunnel not found: {}", p.id),
            data: None,
        })
    }
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
            .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;
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
            let session = rivet_ssh::SshSession::connect(&conn)
                .await
                .map_err(|e| to_rpc_error(RivetError::from(e)))?;
            state.sessions.insert(p.connection_id, session);
        }

        let session = state.sessions.get(&p.connection_id).unwrap();
        session.exec(&p.command).await
    };

    let exec_result = result.map_err(|e| to_rpc_error(RivetError::from(e)))?;

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
        .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

    let conn = vault
        .load_connection(&p.connection_id)
        .map_err(to_rpc_error)?;

    let (key_path, agent_socket_path) = match &conn.auth {
        rivet_core::connection::AuthMethod::KeyFile { path, .. } => {
            (Some(path.to_string_lossy().into_owned()), None)
        }
        rivet_core::connection::AuthMethod::Agent { socket_path } => {
            (None, socket_path.as_ref().map(|p| p.to_string_lossy().into_owned()))
        }
        _ => (None, None),
    };

    to_value(SshConnectInfoResult {
        host: conn.host,
        port: conn.port,
        username: conn.username,
        key_path,
        agent_socket_path,
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
            .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;
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
            .map_err(|e| to_rpc_error(RivetError::ScpTransferFailed(e.to_string())))?
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
            .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;
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
            .map_err(|e| to_rpc_error(RivetError::ScpTransferFailed(e.to_string())))?
    };

    to_value(ScpResult {
        bytes_transferred: bytes,
    })
}

// --- Workflow ---

async fn handle_workflow_list(state: &SharedState) -> Result<Value, JsonRpcError> {
    let state = state.read().await;
    let vault = state.vault.as_ref().ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

    let workflows = vault.list_workflows().map_err(to_rpc_error)?;
    to_value(workflows)
}

async fn handle_workflow_get(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: WorkflowGetParams = parse_params(params)?;
    let state = state.read().await;
    let vault = state.vault.as_ref().ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

    let workflow = if let Some(id) = p.id {
        vault.load_workflow(&id).map_err(to_rpc_error)?
    } else if let Some(name) = p.name {
        vault.find_workflow_by_name(&name).map_err(to_rpc_error)?
    } else {
        return Err(JsonRpcError {
            code: -32602,
            message: "either id or name must be provided".into(),
            data: None,
        });
    };

    to_value(workflow)
}

async fn handle_workflow_import(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: WorkflowImportParams = parse_params(params)?;
    let state = state.read().await;
    let vault = state.vault.as_ref().ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

    // Parse YAML into workflow
    let mut workflow: rivet_core::workflow::Workflow =
        serde_yaml::from_str(&p.yaml).map_err(|e| JsonRpcError {
            code: -32602,
            message: format!("invalid workflow YAML: {e}"),
            data: None,
        })?;

    // Validate
    workflow.validate().map_err(|errs| JsonRpcError {
        code: -32602,
        message: format!("workflow validation failed: {}", errs.join("; ")),
        data: None,
    })?;

    // Check for duplicate name
    if let Ok(existing) = vault.find_workflow_by_name(&workflow.name) {
        return Err(to_rpc_error(RivetError::DuplicateWorkflowName(
            existing.name,
        )));
    }

    // Assign a new UUID
    workflow.id = Uuid::new_v4();
    let id = workflow.id;
    vault.save_workflow(&workflow).map_err(to_rpc_error)?;
    info!(name = %workflow.name, "workflow imported");

    to_value(IdResult { id })
}

async fn handle_workflow_delete(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: WorkflowDeleteParams = parse_params(params)?;
    let state = state.read().await;
    let vault = state.vault.as_ref().ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

    let workflow = if let Some(id) = p.id {
        vault.load_workflow(&id).map_err(to_rpc_error)?
    } else if let Some(name) = p.name {
        vault.find_workflow_by_name(&name).map_err(to_rpc_error)?
    } else {
        return Err(JsonRpcError {
            code: -32602,
            message: "either id or name must be provided".into(),
            data: None,
        });
    };

    vault.delete_workflow(&workflow.id).map_err(to_rpc_error)?;
    info!(name = %workflow.name, "workflow deleted");

    to_value(OkResult { ok: true })
}

async fn handle_workflow_run(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: WorkflowRunParams = parse_params(params)?;

    // Resolve workflow
    let workflow = {
        let state = state.read().await;
        let vault = state
            .vault
            .as_ref()
            .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

        if let Some(id) = p.workflow_id {
            vault.load_workflow(&id).map_err(to_rpc_error)?
        } else if let Some(name) = &p.workflow_name {
            vault.find_workflow_by_name(name).map_err(to_rpc_error)?
        } else {
            return Err(JsonRpcError {
                code: -32602,
                message: "either workflow_id or workflow_name must be provided".into(),
                data: None,
            });
        }
    };

    // Resolve target connections
    let connections = {
        let state = state.read().await;
        let vault = state
            .vault
            .as_ref()
            .ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

        if let Some(conn_id) = p.connection_id {
            vec![vault.load_connection(&conn_id).map_err(to_rpc_error)?]
        } else if let Some(conn_name) = &p.connection_name {
            vec![find_connection_by_name(vault, conn_name)?]
        } else if let Some(group_id) = p.group_id {
            let group = vault.load_group(&group_id).map_err(to_rpc_error)?;
            get_group_connections(vault, &group)?
        } else if let Some(group_name) = &p.group_name {
            let group = find_group_by_name(vault, group_name)?;
            get_group_connections(vault, &group)?
        } else {
            return Err(JsonRpcError {
                code: -32602,
                message: "target required: connection_id, connection_name, group_id, or group_name"
                    .into(),
                data: None,
            });
        }
    };

    // Merge variables
    let vars = workflow.merged_variables(&p.variables);

    // Run workflow on each connection
    let mut all_results = Vec::new();

    for conn in &connections {
        let result =
            execute_workflow_on_connection(state, &workflow, conn, &vars).await;
        all_results.push(result);
    }

    to_value(all_results)
}

/// Execute a workflow on a single connection.
async fn execute_workflow_on_connection(
    state: &SharedState,
    workflow: &rivet_core::workflow::Workflow,
    conn: &rivet_core::connection::Connection,
    vars: &std::collections::HashMap<String, String>,
) -> rivet_core::workflow::WorkflowResult {
    use rivet_core::workflow::*;

    let mut step_results = Vec::new();
    let mut completed = 0;
    let mut failed = 0;
    let mut aborted = false;

    for step in &workflow.steps {
        if aborted {
            step_results.push(StepResult {
                step_name: step.name.clone(),
                success: false,
                skipped: true,
                stdout: None,
                stderr: None,
                exit_code: None,
                bytes_transferred: None,
                error: Some("aborted due to previous step failure".into()),
            });
            continue;
        }

        let expanded = step.expand(vars);

        // Check condition
        if let Some(ref condition) = expanded.condition {
            let cond_result = execute_command_on_connection(
                state,
                &conn.id,
                conn,
                condition,
            )
            .await;

            match cond_result {
                Ok(exec_result) if exec_result.exit_code != 0 => {
                    step_results.push(StepResult {
                        step_name: expanded.name.clone(),
                        success: true,
                        skipped: true,
                        stdout: None,
                        stderr: None,
                        exit_code: None,
                        bytes_transferred: None,
                        error: None,
                    });
                    completed += 1;
                    continue;
                }
                Err(e) => {
                    step_results.push(StepResult {
                        step_name: expanded.name.clone(),
                        success: false,
                        skipped: true,
                        stdout: None,
                        stderr: None,
                        exit_code: None,
                        bytes_transferred: None,
                        error: Some(format!("condition check failed: {e}")),
                    });
                    failed += 1;
                    if expanded.on_failure == OnFailure::Abort {
                        aborted = true;
                    }
                    continue;
                }
                _ => {} // condition passed, proceed
            }
        }

        let step_result = match &expanded.action {
            StepAction::Exec(exec) => {
                execute_exec_step(state, &conn.id, conn, &expanded.name, &exec.command).await
            }
            StepAction::Upload(transfer) => {
                execute_upload_step(
                    state,
                    &conn.id,
                    conn,
                    &expanded.name,
                    &transfer.local_path,
                    &transfer.remote_path,
                )
                .await
            }
            StepAction::Download(transfer) => {
                execute_download_step(
                    state,
                    &conn.id,
                    conn,
                    &expanded.name,
                    &transfer.remote_path,
                    &transfer.local_path,
                )
                .await
            }
        };

        if step_result.success {
            completed += 1;
        } else {
            failed += 1;
            match expanded.on_failure {
                OnFailure::Abort => aborted = true,
                OnFailure::Continue | OnFailure::Skip => {}
            }
        }

        step_results.push(step_result);
    }

    WorkflowResult {
        workflow_name: workflow.name.clone(),
        connection_name: conn.name.clone(),
        steps: step_results,
        success: failed == 0,
        total_steps: workflow.steps.len(),
        completed_steps: completed,
        failed_steps: failed,
    }
}

/// Execute a single exec step.
async fn execute_exec_step(
    state: &SharedState,
    conn_id: &Uuid,
    conn: &rivet_core::connection::Connection,
    step_name: &str,
    command: &str,
) -> rivet_core::workflow::StepResult {
    match execute_command_on_connection(state, conn_id, conn, command).await {
        Ok(exec_result) => rivet_core::workflow::StepResult {
            step_name: step_name.into(),
            success: exec_result.exit_code == 0,
            skipped: false,
            stdout: Some(exec_result.stdout),
            stderr: Some(exec_result.stderr),
            exit_code: Some(exec_result.exit_code),
            bytes_transferred: None,
            error: None,
        },
        Err(e) => rivet_core::workflow::StepResult {
            step_name: step_name.into(),
            success: false,
            skipped: false,
            stdout: None,
            stderr: None,
            exit_code: None,
            bytes_transferred: None,
            error: Some(e),
        },
    }
}

/// Execute a command on a connection (shared helper for workflow engine).
async fn execute_command_on_connection(
    state: &SharedState,
    conn_id: &Uuid,
    conn: &rivet_core::connection::Connection,
    command: &str,
) -> Result<SshExecResult, String> {
    // Ensure session
    {
        let mut state = state.write().await;
        ensure_session(&mut state, conn_id, conn)
            .await
            .map_err(|e| e.message.clone())?;
    }

    // Execute
    let state = state.read().await;
    let session = state
        .sessions
        .get(conn_id)
        .ok_or_else(|| "session not found".to_string())?;

    let result = session
        .exec(command)
        .await
        .map_err(|e| e.to_string())?;

    Ok(SshExecResult {
        exit_code: result.exit_code as i32,
        stdout: result.stdout_str(),
        stderr: result.stderr_str(),
    })
}

/// Execute an upload step.
async fn execute_upload_step(
    state: &SharedState,
    conn_id: &Uuid,
    conn: &rivet_core::connection::Connection,
    step_name: &str,
    local_path: &str,
    remote_path: &str,
) -> rivet_core::workflow::StepResult {
    // Ensure session
    {
        let mut state = state.write().await;
        if let Err(e) = ensure_session(&mut state, conn_id, conn).await {
            return rivet_core::workflow::StepResult {
                step_name: step_name.into(),
                success: false,
                skipped: false,
                stdout: None,
                stderr: None,
                exit_code: None,
                bytes_transferred: None,
                error: Some(e.message),
            };
        }
    }

    let state = state.read().await;
    let session = match state.sessions.get(conn_id) {
        Some(s) => s,
        None => {
            return rivet_core::workflow::StepResult {
                step_name: step_name.into(),
                success: false,
                skipped: false,
                stdout: None,
                stderr: None,
                exit_code: None,
                bytes_transferred: None,
                error: Some("session not found".into()),
            }
        }
    };

    match session.upload_file(&PathBuf::from(local_path), remote_path).await {
        Ok(bytes) => rivet_core::workflow::StepResult {
            step_name: step_name.into(),
            success: true,
            skipped: false,
            stdout: None,
            stderr: None,
            exit_code: None,
            bytes_transferred: Some(bytes),
            error: None,
        },
        Err(e) => rivet_core::workflow::StepResult {
            step_name: step_name.into(),
            success: false,
            skipped: false,
            stdout: None,
            stderr: None,
            exit_code: None,
            bytes_transferred: None,
            error: Some(e.to_string()),
        },
    }
}

/// Execute a download step.
async fn execute_download_step(
    state: &SharedState,
    conn_id: &Uuid,
    conn: &rivet_core::connection::Connection,
    step_name: &str,
    remote_path: &str,
    local_path: &str,
) -> rivet_core::workflow::StepResult {
    // Ensure session
    {
        let mut state = state.write().await;
        if let Err(e) = ensure_session(&mut state, conn_id, conn).await {
            return rivet_core::workflow::StepResult {
                step_name: step_name.into(),
                success: false,
                skipped: false,
                stdout: None,
                stderr: None,
                exit_code: None,
                bytes_transferred: None,
                error: Some(e.message),
            };
        }
    }

    let state = state.read().await;
    let session = match state.sessions.get(conn_id) {
        Some(s) => s,
        None => {
            return rivet_core::workflow::StepResult {
                step_name: step_name.into(),
                success: false,
                skipped: false,
                stdout: None,
                stderr: None,
                exit_code: None,
                bytes_transferred: None,
                error: Some("session not found".into()),
            }
        }
    };

    match session.download_file(remote_path, &PathBuf::from(local_path)).await {
        Ok(bytes) => rivet_core::workflow::StepResult {
            step_name: step_name.into(),
            success: true,
            skipped: false,
            stdout: None,
            stderr: None,
            exit_code: None,
            bytes_transferred: Some(bytes),
            error: None,
        },
        Err(e) => rivet_core::workflow::StepResult {
            step_name: step_name.into(),
            success: false,
            skipped: false,
            stdout: None,
            stderr: None,
            exit_code: None,
            bytes_transferred: None,
            error: Some(e.to_string()),
        },
    }
}

/// Get all connections belonging to a group.
fn get_group_connections(
    vault: &rivet_vault::store::UnlockedVault,
    group: &rivet_core::connection::Group,
) -> Result<Vec<rivet_core::connection::Connection>, JsonRpcError> {
    let all_connections = vault.list_connections().map_err(to_rpc_error)?;
    Ok(all_connections
        .into_iter()
        .filter(|c| c.group_ids.contains(&group.id))
        .collect())
}

// --- Helpers ---

/// Ensure an SSH session exists for the given connection ID.
async fn ensure_session(
    state: &mut crate::state::DaemonState,
    connection_id: &Uuid,
    conn: &rivet_core::connection::Connection,
) -> Result<(), JsonRpcError> {
    let needs_new = match state.sessions.get(connection_id) {
        Some(session) if !session.is_closed() => false,
        _ => true,
    };

    if needs_new {
        debug!(id = %connection_id, "creating SSH session");
        let session = rivet_ssh::SshSession::connect(conn)
            .await
            .map_err(|e| to_rpc_error(RivetError::from(e)))?;
        state.sessions.insert(*connection_id, session);
    }

    Ok(())
}

/// Find a connection by name in the vault.
fn find_connection_by_name(
    vault: &rivet_vault::store::UnlockedVault,
    name: &str,
) -> Result<rivet_core::connection::Connection, JsonRpcError> {
    let connections = vault.list_connections().map_err(to_rpc_error)?;
    connections
        .into_iter()
        .find(|c| c.name == name)
        .ok_or_else(|| to_rpc_error(RivetError::ConnectionNotFound(name.to_string())))
}
