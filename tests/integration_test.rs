//! Integration tests for the daemon JSON-RPC protocol.
//!
//! These tests start a real daemon on a temp socket and exercise
//! the full vault + connection lifecycle through the JSON-RPC API.

use serde_json::Value;

use shelly_core::protocol::*;

#[tokio::test]
async fn test_daemon_status_and_vault_lifecycle() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault_path = dir.path().join("vault");

    // Vault store lifecycle
    let store = shelly_vault::store::VaultStore::new(vault_path.clone());
    assert!(!store.is_initialized());

    store.init("test-password-123").unwrap();
    assert!(store.is_initialized());

    // Unlock
    let store = shelly_vault::store::VaultStore::new(vault_path.clone());
    let vault = store.unlock("test-password-123").unwrap();

    // Create connection
    let conn = shelly_core::connection::Connection::new("test-server", "10.0.0.1", "admin");
    let conn_id = conn.id;
    vault.save_connection(&conn).unwrap();

    // List connections
    let connections = vault.list_connections().unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].name, "test-server");

    // Load connection
    let loaded = vault.load_connection(&conn_id).unwrap();
    assert_eq!(loaded.host, "10.0.0.1");

    // Update connection
    let mut conn = loaded;
    conn.host = "10.0.0.2".into();
    conn.port = 2222;
    vault.save_connection(&conn).unwrap();

    let updated = vault.load_connection(&conn_id).unwrap();
    assert_eq!(updated.host, "10.0.0.2");
    assert_eq!(updated.port, 2222);

    // Delete connection
    vault.delete_connection(&conn_id).unwrap();
    let connections = vault.list_connections().unwrap();
    assert!(connections.is_empty());

    // Lock vault
    let store = vault.lock();
    assert!(store.is_initialized());

    // Unlock again with correct password
    let vault = store.unlock("test-password-123").unwrap();

    // Wrong password should fail
    let store2 = shelly_vault::store::VaultStore::new(vault_path.clone());
    assert!(store2.unlock("wrong-password").is_err());

    // Change password
    vault
        .change_password("test-password-123", "new-password-456")
        .unwrap();

    // Lock and re-unlock with new password
    let store = vault.lock();
    let _vault = store.unlock("new-password-456").unwrap();
}

#[tokio::test]
async fn test_protocol_types_roundtrip() {
    // Verify all RPC param types serialize/deserialize correctly
    let test_cases: Vec<(&str, Value)> = vec![
        (
            "vault.unlock",
            serde_json::to_value(VaultUnlockParams {
                password: "test".into(),
            })
            .unwrap(),
        ),
        (
            "vault.init",
            serde_json::to_value(VaultInitParams {
                password: "test".into(),
            })
            .unwrap(),
        ),
        (
            "conn.list",
            serde_json::to_value(ConnListParams {
                tag: Some("prod".into()),
                group_id: None,
            })
            .unwrap(),
        ),
        (
            "conn.get",
            serde_json::to_value(ConnGetParams {
                id: None,
                name: Some("test".into()),
            })
            .unwrap(),
        ),
        (
            "ssh.exec",
            serde_json::to_value(SshExecParams {
                connection_id: uuid::Uuid::new_v4(),
                command: "uptime".into(),
            })
            .unwrap(),
        ),
    ];

    for (method, params) in test_cases {
        let req = JsonRpcRequest::new(method, Some(params.clone()), 1);
        let json = serde_json::to_string(&req).unwrap();
        let parsed: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.method, method);
        assert_eq!(parsed.params.unwrap(), params);
    }
}

#[tokio::test]
async fn test_connection_crud_full_lifecycle() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault_path = dir.path().join("vault");

    let store = shelly_vault::store::VaultStore::new(vault_path);
    store.init("pwd").unwrap();
    let store = shelly_vault::store::VaultStore::new(dir.path().join("vault"));
    let vault = store.unlock("pwd").unwrap();

    // Create multiple connections
    let names = ["prod-web", "prod-db", "staging-web", "dev-box"];
    let mut ids = Vec::new();

    for name in &names {
        let mut conn = shelly_core::connection::Connection::new(*name, "10.0.0.1", "deploy");
        conn.tags = vec!["prod".into()];
        ids.push(conn.id);
        vault.save_connection(&conn).unwrap();
    }

    // List all
    let all = vault.list_connections().unwrap();
    assert_eq!(all.len(), 4);

    // Find by name
    let found = vault.find_connection_by_name("prod-web").unwrap();
    assert_eq!(found.name, "prod-web");

    // Find nonexistent
    assert!(vault.find_connection_by_name("nonexistent").is_err());

    // Delete one
    vault.delete_connection(&ids[0]).unwrap();
    assert_eq!(vault.list_connections().unwrap().len(), 3);

    // Group CRUD
    let group = shelly_core::connection::Group::new("production");
    let group_id = group.id;
    vault.save_group(&group).unwrap();

    let groups = vault.list_groups().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "production");

    vault.delete_group(&group_id).unwrap();
    assert!(vault.list_groups().unwrap().is_empty());
}

#[tokio::test]
async fn test_ssh_config_import() {
    let dir = tempfile::TempDir::new().unwrap();
    let vault_path = dir.path().join("vault");
    let config_path = dir.path().join("ssh_config");

    // Write sample SSH config
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

    let store = shelly_vault::store::VaultStore::new(vault_path);
    store.init("pwd").unwrap();
    let store = shelly_vault::store::VaultStore::new(dir.path().join("vault"));
    let vault = store.unlock("pwd").unwrap();

    let imported =
        shelly_vault::import::import_ssh_config_to_vault(&vault, &config_path).unwrap();
    assert_eq!(imported, 2); // wildcard host skipped

    let connections = vault.list_connections().unwrap();
    assert_eq!(connections.len(), 2);

    let prod = connections.iter().find(|c| c.name == "prod-web").unwrap();
    assert_eq!(prod.host, "192.168.1.10");
    assert_eq!(prod.port, 2222);
    assert_eq!(prod.username, "deploy");

    // Second import should not duplicate
    let imported2 =
        shelly_vault::import::import_ssh_config_to_vault(&vault, &config_path).unwrap();
    assert_eq!(imported2, 0);
    assert_eq!(vault.list_connections().unwrap().len(), 2);
}
