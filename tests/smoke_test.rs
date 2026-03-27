//! Smoke & integration tests for the Rivet daemon.
//!
//! Each test spins up a real daemon (in-process) on a temporary Unix socket,
//! connects a client, and exercises JSON-RPC methods end-to-end.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::RwLock;

use rivet_daemon::state::{DaemonState, SharedState};
use rivet_vault::store::VaultStore;

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

/// A running test daemon: holds the temp dir, socket path, and server task.
struct TestDaemon {
    socket_path: PathBuf,
    _dir: tempfile::TempDir, // dropped last → cleans up
    _server: tokio::task::JoinHandle<()>,
}

impl TestDaemon {
    /// Start a new daemon on a temp socket with an empty state.
    async fn start() -> Self {
        let dir = tempfile::TempDir::new().unwrap();
        let socket_path = dir.path().join("test.sock");

        let state: SharedState = Arc::new(RwLock::new(DaemonState::new()));

        let sock = socket_path.clone();
        let st = state.clone();
        let server = tokio::spawn(async move {
            rivet_daemon::server::run_server(&sock, st).await.ok();
        });

        // Wait for socket to appear
        for _ in 0..50 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(socket_path.exists(), "daemon socket did not appear");

        Self {
            socket_path,
            _dir: dir,
            _server: server,
        }
    }

    /// Connect a raw JSON-RPC client.
    async fn connect(&self) -> TestClient {
        TestClient::connect(&self.socket_path).await
    }
}

/// Minimal JSON-RPC client for tests.
struct TestClient {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
    next_id: u64,
}

impl TestClient {
    async fn connect(path: &Path) -> Self {
        let stream = UnixStream::connect(path).await.expect("connect to daemon");
        let (r, w) = stream.into_split();
        Self {
            reader: BufReader::new(r),
            writer: w,
            next_id: 1,
        }
    }

    /// Send an RPC call and return the parsed response.
    async fn rpc(&mut self, method: &str, params: Option<Value>) -> RpcResult {
        let id = self.next_id;
        self.next_id += 1;

        let req = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id,
        });

        let mut data = serde_json::to_string(&req).unwrap();
        data.push('\n');
        self.writer.write_all(data.as_bytes()).await.unwrap();
        self.writer.flush().await.unwrap();

        let mut line = String::new();
        self.reader.read_line(&mut line).await.unwrap();
        let resp: Value = serde_json::from_str(line.trim()).unwrap();

        if let Some(err) = resp.get("error") {
            RpcResult::Err {
                code: err["code"].as_i64().unwrap_or(-1),
                message: err["message"].as_str().unwrap_or("").to_string(),
            }
        } else {
            RpcResult::Ok(resp["result"].clone())
        }
    }

    /// Call and expect success, return the result value.
    async fn ok(&mut self, method: &str, params: Option<Value>) -> Value {
        match self.rpc(method, params).await {
            RpcResult::Ok(v) => v,
            RpcResult::Err { code, message } => {
                panic!("{method} failed: [{code}] {message}")
            }
        }
    }

    /// Call and expect an error, return the code.
    async fn err(&mut self, method: &str, params: Option<Value>) -> i64 {
        match self.rpc(method, params).await {
            RpcResult::Err { code, .. } => code,
            RpcResult::Ok(v) => panic!("{method} should have failed, got: {v}"),
        }
    }
}

#[derive(Debug)]
enum RpcResult {
    Ok(Value),
    Err { code: i64, message: String },
}

// ---------------------------------------------------------------------------
// Smoke tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smoke_daemon_status() {
    let daemon = TestDaemon::start().await;
    let mut c = daemon.connect().await;

    let status = c.ok("daemon.status", None).await;
    assert!(status["uptime_secs"].is_number());
    assert_eq!(status["active_sessions"], 0);
    assert_eq!(status["active_tunnels"], 0);
    assert_eq!(status["vault_locked"], true);
}

#[tokio::test]
async fn smoke_vault_status_before_init() {
    let daemon = TestDaemon::start().await;
    let mut c = daemon.connect().await;

    let status = c.ok("vault.status", None).await;
    assert_eq!(status["initialized"], false);
    assert_eq!(status["locked"], true);
}

#[tokio::test]
async fn smoke_unknown_method() {
    let daemon = TestDaemon::start().await;
    let mut c = daemon.connect().await;

    let code = c.err("nonexistent.method", None).await;
    assert_eq!(code, -32601);
}

#[tokio::test]
async fn smoke_invalid_json() {
    let daemon = TestDaemon::start().await;
    let stream = UnixStream::connect(&daemon.socket_path).await.unwrap();
    let (r, mut w) = stream.into_split();
    let mut reader = BufReader::new(r);

    w.write_all(b"this is not json\n").await.unwrap();
    w.flush().await.unwrap();

    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    let resp: Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(resp["error"]["code"], -32700);
}

#[tokio::test]
async fn smoke_wrong_jsonrpc_version() {
    let daemon = TestDaemon::start().await;
    let stream = UnixStream::connect(&daemon.socket_path).await.unwrap();
    let (r, mut w) = stream.into_split();
    let mut reader = BufReader::new(r);

    let req = r#"{"jsonrpc":"1.0","method":"daemon.status","id":1}"#;
    w.write_all(format!("{req}\n").as_bytes()).await.unwrap();
    w.flush().await.unwrap();

    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    let resp: Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(resp["error"]["code"], -32600);
}

#[tokio::test]
async fn smoke_vault_operations_require_init() {
    let daemon = TestDaemon::start().await;
    let mut c = daemon.connect().await;

    // Unlock without init should fail
    let code = c
        .err("vault.unlock", Some(json!({"password": "test"})))
        .await;
    assert!(code < 0, "expected error, got code {code}");
}

#[tokio::test]
async fn smoke_conn_operations_require_unlocked_vault() {
    let daemon = TestDaemon::start().await;
    let mut c = daemon.connect().await;

    // All conn operations should fail when vault is locked
    let code = c
        .err("conn.list", Some(json!({"tag": null, "group_id": null})))
        .await;
    // -32001 = VaultLocked
    assert_eq!(code, -32001);

    let code = c
        .err("conn.get", Some(json!({"id": null, "name": "test"})))
        .await;
    assert_eq!(code, -32001);
}

#[tokio::test]
async fn smoke_group_operations_require_unlocked_vault() {
    let daemon = TestDaemon::start().await;
    let mut c = daemon.connect().await;

    let code = c.err("group.list", None).await;
    assert_eq!(code, -32001);

    let code = c
        .err(
            "group.create",
            Some(json!({"name": "test", "description": null, "color": null})),
        )
        .await;
    assert_eq!(code, -32001);
}

#[tokio::test]
async fn smoke_tunnel_list_require_unlocked_vault() {
    let daemon = TestDaemon::start().await;
    let mut c = daemon.connect().await;

    let code = c.err("tunnel.list", None).await;
    assert_eq!(code, -32001);
}

#[tokio::test]
async fn smoke_workflow_list_require_unlocked_vault() {
    let daemon = TestDaemon::start().await;
    let mut c = daemon.connect().await;

    let code = c.err("workflow.list", None).await;
    assert_eq!(code, -32001);
}

// ---------------------------------------------------------------------------
// Full lifecycle tests (with vault init via state injection)
// ---------------------------------------------------------------------------

/// Helper: start daemon with unlocked vault, return daemon + client.
async fn setup() -> (TestDaemon, TestClient) {
    let dir = tempfile::TempDir::new().unwrap();
    let socket_path = dir.path().join("test.sock");
    let vault_dir = dir.path().join("vault");

    // Initialize vault on disk
    let store = VaultStore::new(vault_dir.clone());
    store.init("test-password").unwrap();

    // Create state with unlocked vault
    let store = VaultStore::new(vault_dir.clone());
    let vault = store.unlock("test-password").unwrap();

    let mut daemon_state = DaemonState::new();
    daemon_state.vault_store = Some(VaultStore::new(vault_dir.clone()));
    daemon_state.vault = Some(vault);
    let state: SharedState = Arc::new(RwLock::new(daemon_state));

    let sock = socket_path.clone();
    let st = state.clone();
    let server = tokio::spawn(async move {
        rivet_daemon::server::run_server(&sock, st).await.ok();
    });

    for _ in 0..50 {
        if socket_path.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(socket_path.exists(), "daemon socket did not appear");

    let client = TestClient::connect(&socket_path).await;

    let daemon = TestDaemon {
        socket_path,
        _dir: dir,
        _server: server,
    };

    (daemon, client)
}

// ---------------------------------------------------------------------------
// Connection CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smoke_connection_create_and_list() {
    let (_d, mut c) = setup().await;

    // Create a connection
    let result = c
        .ok(
            "conn.create",
            Some(json!({
                "name": "test-server",
                "host": "192.168.1.10",
                "port": 22,
                "username": "admin",
                "auth": {"type": "Agent"},
                "tags": ["prod"],
                "group_ids": [],
                "options": {
                    "compression": false,
                    "extra_args": []
                }
            })),
        )
        .await;

    let conn_id = result["id"].as_str().unwrap();
    assert!(!conn_id.is_empty());

    // List connections
    let list = c
        .ok("conn.list", Some(json!({"tag": null, "group_id": null})))
        .await;
    let conns = list.as_array().unwrap();
    assert_eq!(conns.len(), 1);
    assert_eq!(conns[0]["name"], "test-server");
    assert_eq!(conns[0]["host"], "192.168.1.10");
}

#[tokio::test]
async fn smoke_connection_get_by_name_and_id() {
    let (_d, mut c) = setup().await;

    let result = c
        .ok(
            "conn.create",
            Some(json!({
                "name": "my-box",
                "host": "10.0.0.1",
                "port": 22,
                "username": "root",
                "auth": {"type": "Agent"},
                "tags": [],
                "group_ids": [],
                "options": {"compression": false, "extra_args": []}
            })),
        )
        .await;
    let conn_id = result["id"].as_str().unwrap().to_string();

    // Get by name
    let conn = c
        .ok("conn.get", Some(json!({"id": null, "name": "my-box"})))
        .await;
    assert_eq!(conn["host"], "10.0.0.1");
    assert_eq!(conn["username"], "root");

    // Get by ID
    let conn = c
        .ok("conn.get", Some(json!({"id": conn_id, "name": null})))
        .await;
    assert_eq!(conn["name"], "my-box");
}

#[tokio::test]
async fn smoke_connection_update() {
    let (_d, mut c) = setup().await;

    let result = c
        .ok(
            "conn.create",
            Some(json!({
                "name": "orig",
                "host": "1.1.1.1",
                "port": 22,
                "username": "user",
                "auth": {"type": "Agent"},
                "tags": [],
                "group_ids": [],
                "options": {"compression": false, "extra_args": []}
            })),
        )
        .await;
    let conn_id = result["id"].as_str().unwrap().to_string();

    // Update
    c.ok(
        "conn.update",
        Some(json!({
            "id": conn_id,
            "name": null,
            "host": "2.2.2.2",
            "port": 2222,
            "username": "deploy",
            "auth": null,
            "tags": null,
            "group_ids": null,
            "jump_host": null,
            "options": null,
            "notes": "updated"
        })),
    )
    .await;

    let conn = c
        .ok("conn.get", Some(json!({"id": conn_id, "name": null})))
        .await;
    assert_eq!(conn["host"], "2.2.2.2");
    assert_eq!(conn["port"], 2222);
    assert_eq!(conn["username"], "deploy");
    assert_eq!(conn["notes"], "updated");
}

#[tokio::test]
async fn smoke_connection_delete() {
    let (_d, mut c) = setup().await;

    let result = c
        .ok(
            "conn.create",
            Some(json!({
                "name": "to-delete",
                "host": "1.1.1.1",
                "port": 22,
                "username": "user",
                "auth": {"type": "Agent"},
                "tags": [],
                "group_ids": [],
                "options": {"compression": false, "extra_args": []}
            })),
        )
        .await;
    let conn_id = result["id"].as_str().unwrap().to_string();

    // Delete by ID
    c.ok(
        "conn.delete",
        Some(json!({"id": conn_id, "name": null})),
    )
    .await;

    // Verify gone
    let list = c
        .ok("conn.list", Some(json!({"tag": null, "group_id": null})))
        .await;
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn smoke_connection_delete_by_name() {
    let (_d, mut c) = setup().await;

    c.ok(
        "conn.create",
        Some(json!({
            "name": "named-delete",
            "host": "1.1.1.1",
            "port": 22,
            "username": "user",
            "auth": {"type": "Agent"},
            "tags": [],
            "group_ids": [],
            "options": {"compression": false, "extra_args": []}
        })),
    )
    .await;

    // Delete by name
    c.ok(
        "conn.delete",
        Some(json!({"id": null, "name": "named-delete"})),
    )
    .await;

    let list = c
        .ok("conn.list", Some(json!({"tag": null, "group_id": null})))
        .await;
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn smoke_connection_not_found() {
    let (_d, mut c) = setup().await;

    let code = c
        .err(
            "conn.get",
            Some(json!({"id": null, "name": "nonexistent"})),
        )
        .await;
    // -32002 = ConnectionNotFound
    assert_eq!(code, -32002);
}

#[tokio::test]
async fn smoke_connection_duplicate_name() {
    let (_d, mut c) = setup().await;

    let params = json!({
        "name": "dup",
        "host": "1.1.1.1",
        "port": 22,
        "username": "user",
        "auth": {"type": "Agent"},
        "tags": [],
        "group_ids": [],
        "options": {"compression": false, "extra_args": []}
    });

    c.ok("conn.create", Some(params.clone())).await;
    let code = c.err("conn.create", Some(params)).await;
    // -32008 = DuplicateConnectionName
    assert_eq!(code, -32008);
}

#[tokio::test]
async fn smoke_connection_filter_by_tag() {
    let (_d, mut c) = setup().await;

    let base = json!({
        "host": "1.1.1.1", "port": 22, "username": "user",
        "auth": {"type": "Agent"}, "group_ids": [],
        "options": {"compression": false, "extra_args": []}
    });

    // Create with different tags
    let mut p1 = base.clone();
    p1["name"] = json!("prod-1");
    p1["tags"] = json!(["prod"]);
    c.ok("conn.create", Some(p1)).await;

    let mut p2 = base.clone();
    p2["name"] = json!("staging-1");
    p2["tags"] = json!(["staging"]);
    c.ok("conn.create", Some(p2)).await;

    // Filter by tag
    let list = c
        .ok("conn.list", Some(json!({"tag": "prod", "group_id": null})))
        .await;
    let conns = list.as_array().unwrap();
    assert_eq!(conns.len(), 1);
    assert_eq!(conns[0]["name"], "prod-1");
}

// ---------------------------------------------------------------------------
// Group CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smoke_group_create_and_list() {
    let (_d, mut c) = setup().await;

    let result = c
        .ok(
            "group.create",
            Some(json!({"name": "production", "description": "prod servers", "color": "#ff0000"})),
        )
        .await;
    let group_id = result["id"].as_str().unwrap().to_string();
    assert!(!group_id.is_empty());

    let list = c.ok("group.list", None).await;
    let groups = list.as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["name"], "production");
    assert_eq!(groups[0]["description"], "prod servers");
    assert_eq!(groups[0]["color"], "#ff0000");
}

#[tokio::test]
async fn smoke_group_get() {
    let (_d, mut c) = setup().await;

    let result = c
        .ok(
            "group.create",
            Some(json!({"name": "staging", "description": null, "color": null})),
        )
        .await;
    let group_id = result["id"].as_str().unwrap().to_string();

    // Get by ID
    let group = c
        .ok("group.get", Some(json!({"id": group_id, "name": null})))
        .await;
    assert_eq!(group["name"], "staging");

    // Get by name
    let group = c
        .ok("group.get", Some(json!({"id": null, "name": "staging"})))
        .await;
    assert_eq!(group["id"], group_id);
}

#[tokio::test]
async fn smoke_group_update() {
    let (_d, mut c) = setup().await;

    let result = c
        .ok(
            "group.create",
            Some(json!({"name": "old-name", "description": null, "color": null})),
        )
        .await;
    let group_id = result["id"].as_str().unwrap().to_string();

    c.ok(
        "group.update",
        Some(json!({
            "id": group_id,
            "name": null,
            "name": "new-name",
            "description": "updated desc",
            "color": "#00ff00"
        })),
    )
    .await;

    let group = c
        .ok("group.get", Some(json!({"id": group_id, "name": null})))
        .await;
    assert_eq!(group["name"], "new-name");
    assert_eq!(group["description"], "updated desc");
    assert_eq!(group["color"], "#00ff00");
}

#[tokio::test]
async fn smoke_group_delete() {
    let (_d, mut c) = setup().await;

    let result = c
        .ok(
            "group.create",
            Some(json!({"name": "to-delete", "description": null, "color": null})),
        )
        .await;
    let group_id = result["id"].as_str().unwrap().to_string();

    c.ok(
        "group.delete",
        Some(json!({"id": group_id, "name": null})),
    )
    .await;

    let list = c.ok("group.list", None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn smoke_group_duplicate_name() {
    let (_d, mut c) = setup().await;

    c.ok(
        "group.create",
        Some(json!({"name": "dup-group", "description": null, "color": null})),
    )
    .await;

    let code = c
        .err(
            "group.create",
            Some(json!({"name": "dup-group", "description": null, "color": null})),
        )
        .await;
    // -32014 = DuplicateGroupName
    assert_eq!(code, -32014);
}

#[tokio::test]
async fn smoke_group_not_found() {
    let (_d, mut c) = setup().await;

    let code = c
        .err(
            "group.get",
            Some(json!({"id": null, "name": "nonexistent"})),
        )
        .await;
    // -32013 = GroupNotFound
    assert_eq!(code, -32013);
}

// ---------------------------------------------------------------------------
// Group ↔ Connection membership
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smoke_connection_group_membership() {
    let (_d, mut c) = setup().await;

    // Create group
    let group = c
        .ok(
            "group.create",
            Some(json!({"name": "web-servers", "description": null, "color": null})),
        )
        .await;
    let group_id = group["id"].as_str().unwrap().to_string();

    // Create connection in that group
    c.ok(
            "conn.create",
            Some(json!({
                "name": "web-1",
                "host": "10.0.0.1",
                "port": 22,
                "username": "deploy",
                "auth": {"type": "Agent"},
                "tags": [],
                "group_ids": [group_id],
                "options": {"compression": false, "extra_args": []}
            })),
        )
        .await;

    // List connections filtered by group
    let list = c
        .ok(
            "conn.list",
            Some(json!({"tag": null, "group_id": group_id})),
        )
        .await;
    let conns = list.as_array().unwrap();
    assert_eq!(conns.len(), 1);
    assert_eq!(conns[0]["name"], "web-1");

    // Delete group — should remove group_id from connection
    c.ok(
        "group.delete",
        Some(json!({"id": group_id, "name": null})),
    )
    .await;

    // Connection should still exist but without the group
    let conn = c
        .ok("conn.get", Some(json!({"id": null, "name": "web-1"})))
        .await;
    let group_ids = conn["group_ids"].as_array().unwrap();
    assert!(group_ids.is_empty(), "group_id should be removed on group delete");
}

// ---------------------------------------------------------------------------
// Vault lifecycle via RPC
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smoke_vault_lock_and_unlock() {
    let (_d, mut c) = setup().await;

    // Vault is unlocked — operations work
    c.ok(
        "conn.create",
        Some(json!({
            "name": "before-lock",
            "host": "1.1.1.1",
            "port": 22,
            "username": "user",
            "auth": {"type": "Agent"},
            "tags": [],
            "group_ids": [],
            "options": {"compression": false, "extra_args": []}
        })),
    )
    .await;

    // Lock
    c.ok("vault.lock", None).await;

    // Operations should fail
    let code = c
        .err("conn.list", Some(json!({"tag": null, "group_id": null})))
        .await;
    assert_eq!(code, -32001);

    // Unlock
    c.ok("vault.unlock", Some(json!({"password": "test-password"})))
        .await;

    // Operations work again — data persists
    let list = c
        .ok("conn.list", Some(json!({"tag": null, "group_id": null})))
        .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["name"], "before-lock");
}

#[tokio::test]
async fn smoke_vault_wrong_password() {
    let (_d, mut c) = setup().await;

    c.ok("vault.lock", None).await;

    let code = c
        .err("vault.unlock", Some(json!({"password": "wrong"})))
        .await;
    assert!(code < 0, "should fail with wrong password");
}

#[tokio::test]
async fn smoke_vault_change_password() {
    let (_d, mut c) = setup().await;

    // Change password
    c.ok(
        "vault.change_password",
        Some(json!({
            "old_password": "test-password",
            "new_password": "new-password-123"
        })),
    )
    .await;

    // Lock and unlock with new password
    c.ok("vault.lock", None).await;
    c.ok(
        "vault.unlock",
        Some(json!({"password": "new-password-123"})),
    )
    .await;

    // Old password fails
    c.ok("vault.lock", None).await;
    let code = c
        .err("vault.unlock", Some(json!({"password": "test-password"})))
        .await;
    assert!(code < 0);
}

// ---------------------------------------------------------------------------
// SSH config import
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smoke_ssh_config_import() {
    let (_d, mut c) = setup().await;

    // Create a temp SSH config file
    let config_dir = tempfile::TempDir::new().unwrap();
    let config_path = config_dir.path().join("config");
    std::fs::write(
        &config_path,
        r#"
Host prod-web
    HostName 192.168.1.10
    User deploy
    Port 2222
    IdentityFile ~/.ssh/prod_key

Host staging
    HostName 10.0.0.50
    User admin

Host *
    ServerAliveInterval 30
"#,
    )
    .unwrap();

    let result = c
        .ok(
            "conn.import",
            Some(json!({"path": config_path.to_str().unwrap()})),
        )
        .await;
    assert_eq!(result["imported"], 2);

    // Verify connections created
    let list = c
        .ok("conn.list", Some(json!({"tag": null, "group_id": null})))
        .await;
    assert_eq!(list.as_array().unwrap().len(), 2);

    // Verify prod-web details
    let prod = c
        .ok("conn.get", Some(json!({"id": null, "name": "prod-web"})))
        .await;
    assert_eq!(prod["host"], "192.168.1.10");
    assert_eq!(prod["port"], 2222);
    assert_eq!(prod["username"], "deploy");

    // Re-import should not duplicate
    let result = c
        .ok(
            "conn.import",
            Some(json!({"path": config_path.to_str().unwrap()})),
        )
        .await;
    assert_eq!(result["imported"], 0);
    assert_eq!(
        c.ok("conn.list", Some(json!({"tag": null, "group_id": null})))
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

// ---------------------------------------------------------------------------
// Multiple connections CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smoke_multiple_connections() {
    let (_d, mut c) = setup().await;

    let names = ["alpha", "bravo", "charlie", "delta", "echo"];
    let mut ids = Vec::new();

    for (i, name) in names.iter().enumerate() {
        let result = c
            .ok(
                "conn.create",
                Some(json!({
                    "name": name,
                    "host": format!("10.0.0.{}", i + 1),
                    "port": 22,
                    "username": "user",
                    "auth": {"type": "Agent"},
                    "tags": [],
                    "group_ids": [],
                    "options": {"compression": false, "extra_args": []}
                })),
            )
            .await;
        ids.push(result["id"].as_str().unwrap().to_string());
    }

    // List all
    let list = c
        .ok("conn.list", Some(json!({"tag": null, "group_id": null})))
        .await;
    assert_eq!(list.as_array().unwrap().len(), 5);

    // Delete two
    c.ok("conn.delete", Some(json!({"id": &ids[0], "name": null})))
        .await;
    c.ok("conn.delete", Some(json!({"id": &ids[2], "name": null})))
        .await;

    let list = c
        .ok("conn.list", Some(json!({"tag": null, "group_id": null})))
        .await;
    assert_eq!(list.as_array().unwrap().len(), 3);

    // Remaining should be bravo, delta, echo
    let remaining: Vec<String> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    assert!(remaining.contains(&"bravo".to_string()));
    assert!(remaining.contains(&"delta".to_string()));
    assert!(remaining.contains(&"echo".to_string()));
}

// ---------------------------------------------------------------------------
// Tunnel list (empty, no SSH required)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smoke_tunnel_list_empty() {
    let (_d, mut c) = setup().await;

    let list = c.ok("tunnel.list", None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// Workflow operations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smoke_workflow_import_and_list() {
    let (_d, mut c) = setup().await;

    let workflow_yaml = r#"
name: health-check
description: Check server health
steps:
  - name: uptime
    exec:
      command: uptime
  - name: disk
    exec:
      command: df -h
"#;

    let result = c
        .ok(
            "workflow.import",
            Some(json!({"yaml": workflow_yaml})),
        )
        .await;
    let wf_id = result["id"].as_str().unwrap().to_string();
    assert!(!wf_id.is_empty());

    // List
    let list = c.ok("workflow.list", None).await;
    let workflows = list.as_array().unwrap();
    assert_eq!(workflows.len(), 1);
    assert_eq!(workflows[0]["name"], "health-check");
    assert_eq!(workflows[0]["description"], "Check server health");

    // Get
    let wf = c
        .ok("workflow.get", Some(json!({"id": null, "name": "health-check"})))
        .await;
    assert_eq!(wf["name"], "health-check");
    let steps = wf["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0]["name"], "uptime");
}

#[tokio::test]
async fn smoke_workflow_delete() {
    let (_d, mut c) = setup().await;

    let result = c
        .ok(
            "workflow.import",
            Some(json!({"yaml": "name: temp\nsteps:\n  - name: test\n    exec:\n      command: echo hi"})),
        )
        .await;
    let wf_id = result["id"].as_str().unwrap().to_string();

    c.ok(
        "workflow.delete",
        Some(json!({"id": wf_id, "name": null})),
    )
    .await;

    let list = c.ok("workflow.list", None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// Concurrent clients
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smoke_concurrent_clients() {
    let (daemon, _c) = setup().await;

    // Spawn 5 clients concurrently, each doing a status call
    let mut handles = Vec::new();
    for i in 0..5 {
        let path = daemon.socket_path.clone();
        handles.push(tokio::spawn(async move {
            let mut client = TestClient::connect(&path).await;

            // Each client creates a connection
            let result = client
                .ok(
                    "conn.create",
                    Some(json!({
                        "name": format!("concurrent-{i}"),
                        "host": "1.1.1.1",
                        "port": 22,
                        "username": "user",
                        "auth": {"type": "Agent"},
                        "tags": [],
                        "group_ids": [],
                        "options": {"compression": false, "extra_args": []}
                    })),
                )
                .await;
            assert!(result["id"].is_string());

            // Check status
            let status = client.ok("daemon.status", None).await;
            assert!(status["uptime_secs"].is_number());
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // Verify all 5 connections exist (from a fresh client)
    let mut c = daemon.connect().await;
    let list = c
        .ok("conn.list", Some(json!({"tag": null, "group_id": null})))
        .await;
    assert_eq!(list.as_array().unwrap().len(), 5);
}

// ---------------------------------------------------------------------------
// Auth method variants
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smoke_connection_auth_variants() {
    let (_d, mut c) = setup().await;

    let auths = vec![
        ("agent-conn", json!({"type": "Agent"})),
        ("password-conn", json!({"type": "Password", "data": "secret123"})),
        (
            "keyfile-conn",
            json!({"type": "KeyFile", "data": {"path": "/home/user/.ssh/id_rsa", "passphrase": null}}),
        ),
        ("interactive-conn", json!({"type": "Interactive"})),
    ];

    for (name, auth) in &auths {
        c.ok(
            "conn.create",
            Some(json!({
                "name": name,
                "host": "1.1.1.1",
                "port": 22,
                "username": "user",
                "auth": auth,
                "tags": [],
                "group_ids": [],
                "options": {"compression": false, "extra_args": []}
            })),
        )
        .await;
    }

    let list = c
        .ok("conn.list", Some(json!({"tag": null, "group_id": null})))
        .await;
    assert_eq!(list.as_array().unwrap().len(), 4);

    // Verify auth types roundtrip — auth is now wrapped in AuthSource::Inline
    let agent = c
        .ok("conn.get", Some(json!({"id": null, "name": "agent-conn"})))
        .await;
    assert_eq!(agent["auth"]["type"], "Inline");
    assert_eq!(agent["auth"]["data"]["type"], "Agent");

    let password = c
        .ok("conn.get", Some(json!({"id": null, "name": "password-conn"})))
        .await;
    assert_eq!(password["auth"]["type"], "Inline");
    assert_eq!(password["auth"]["data"]["type"], "Password");
    assert_eq!(password["auth"]["data"]["data"], "secret123");

    let keyfile = c
        .ok("conn.get", Some(json!({"id": null, "name": "keyfile-conn"})))
        .await;
    assert_eq!(keyfile["auth"]["type"], "Inline");
    assert_eq!(keyfile["auth"]["data"]["type"], "KeyFile");
    assert_eq!(keyfile["auth"]["data"]["data"]["path"], "/home/user/.ssh/id_rsa");
}

// ---------------------------------------------------------------------------
// Credential CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn smoke_credential_create_and_list() {
    let (_d, mut c) = setup().await;

    let result = c.ok("cred.create", Some(json!({
        "name": "deploy-key",
        "auth": {"type": "Agent", "data": {"socket_path": null}},
        "description": "Deploy SSH agent"
    }))).await;
    assert!(result.get("id").is_some());

    let creds = c.ok("cred.list", Some(json!({}))).await;
    let creds = creds.as_array().unwrap();
    assert_eq!(creds.len(), 1);
    assert_eq!(creds[0]["name"], "deploy-key");
}

#[tokio::test]
async fn smoke_credential_get_and_update() {
    let (_d, mut c) = setup().await;

    let result = c.ok("cred.create", Some(json!({
        "name": "test-cred",
        "auth": {"type": "Password", "data": "secret"}
    }))).await;
    let id = result["id"].as_str().unwrap();

    // Get by name
    let cred = c.ok("cred.get", Some(json!({"name": "test-cred"}))).await;
    assert_eq!(cred["name"], "test-cred");

    // Update name
    c.ok("cred.update", Some(json!({"id": id, "name": "renamed-cred"}))).await;
    let cred = c.ok("cred.get", Some(json!({"id": id}))).await;
    assert_eq!(cred["name"], "renamed-cred");
}

#[tokio::test]
async fn smoke_credential_delete() {
    let (_d, mut c) = setup().await;

    c.ok("cred.create", Some(json!({
        "name": "to-delete",
        "auth": {"type": "Agent", "data": {"socket_path": null}}
    }))).await;

    c.ok("cred.delete", Some(json!({"name": "to-delete"}))).await;

    let creds = c.ok("cred.list", Some(json!({}))).await;
    assert_eq!(creds.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn smoke_credential_duplicate_name() {
    let (_d, mut c) = setup().await;

    c.ok("cred.create", Some(json!({
        "name": "dup",
        "auth": {"type": "Agent", "data": {"socket_path": null}}
    }))).await;

    // Should fail with DuplicateCredentialName (-32020)
    let code = c.err("cred.create", Some(json!({
        "name": "dup",
        "auth": {"type": "Agent", "data": {"socket_path": null}}
    }))).await;
    assert_eq!(code, -32020);
}

#[tokio::test]
async fn smoke_credential_usage() {
    let (_d, mut c) = setup().await;

    let cred_result = c.ok("cred.create", Some(json!({
        "name": "shared-key",
        "auth": {"type": "Agent", "data": {"socket_path": null}}
    }))).await;
    let cred_id = cred_result["id"].as_str().unwrap();

    // Create connection using this credential
    c.ok("conn.create", Some(json!({
        "name": "server1",
        "host": "10.0.0.1",
        "username": "admin",
        "auth": {"type": "Profile", "data": {"credential_id": cred_id}}
    }))).await;

    // Check usage
    let usage = c.ok("cred.usage", Some(json!({"name": "shared-key"}))).await;
    let connections = usage["connections"].as_array().unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0]["name"], "server1");
}

#[tokio::test]
async fn smoke_credential_delete_blocked_when_in_use() {
    let (_d, mut c) = setup().await;

    let cred_result = c.ok("cred.create", Some(json!({
        "name": "in-use",
        "auth": {"type": "Agent", "data": {"socket_path": null}}
    }))).await;
    let cred_id = cred_result["id"].as_str().unwrap();

    c.ok("conn.create", Some(json!({
        "name": "linked-server",
        "host": "10.0.0.1",
        "username": "admin",
        "auth": {"type": "Profile", "data": {"credential_id": cred_id}}
    }))).await;

    // Delete should fail without force (-32021 = CredentialInUse)
    let code = c.err("cred.delete", Some(json!({"name": "in-use"}))).await;
    assert_eq!(code, -32021);

    // Delete with force should succeed
    c.ok("cred.delete", Some(json!({"name": "in-use", "force": true}))).await;
}
