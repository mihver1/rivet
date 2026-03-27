# Credentials Manager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add reusable credential profiles that connections can reference, so changing auth in one place updates all linked connections.

**Architecture:** New `Credential` entity stored in vault alongside connections. `Connection.auth` changes from `AuthMethod` to `AuthSource` enum (Inline or Profile reference). Daemon resolves profiles at connect time. Full CRUD via RPC, CLI, and SwiftUI.

**Tech Stack:** Rust (serde, uuid, chrono), SwiftUI, JSON-RPC

**Spec:** `docs/superpowers/specs/2026-03-27-credentials-manager-design.md`

---

### Task 1: Credential struct and AuthSource enum in rivet-core

**Files:**
- Create: `crates/rivet-core/src/credential.rs`
- Modify: `crates/rivet-core/src/lib.rs`
- Modify: `crates/rivet-core/src/connection.rs`

- [ ] **Step 1: Create `credential.rs` with Credential struct and AuthSource enum**

```rust
// crates/rivet-core/src/credential.rs

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::AuthMethod;

/// A reusable authentication profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub id: Uuid,
    pub name: String,
    pub auth: AuthMethod,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Credential {
    pub fn new(name: impl Into<String>, auth: AuthMethod) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            auth,
            description: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// How a connection resolves its authentication.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum AuthSource {
    /// Auth configured directly on the connection.
    Inline(AuthMethod),
    /// Reference to a credential profile.
    Profile { credential_id: Uuid },
}
```

- [ ] **Step 2: Add custom Deserialize for AuthSource (backward compat)**

Legacy connections store a bare `AuthMethod` (e.g. `{"type":"Agent","data":{...}}`). New connections store `{"type":"Inline","data":{...}}` or `{"type":"Profile","data":{"credential_id":"..."}}`. The deserializer must handle both.

Add to `credential.rs`:

```rust
impl<'de> serde::Deserialize<'de> for AuthSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let value = serde_json::Value::deserialize(deserializer)?;
        let obj = value.as_object().ok_or_else(|| D::Error::custom("expected object"))?;

        let type_str = obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| D::Error::custom("missing 'type' field"))?;

        match type_str {
            "Inline" => {
                // New format: {"type":"Inline","data":<AuthMethod>}
                let data = obj
                    .get("data")
                    .ok_or_else(|| D::Error::custom("missing 'data' for Inline"))?;
                let auth: AuthMethod =
                    serde_json::from_value(data.clone()).map_err(D::Error::custom)?;
                Ok(AuthSource::Inline(auth))
            }
            "Profile" => {
                // {"type":"Profile","data":{"credential_id":"<uuid>"}}
                let data = obj
                    .get("data")
                    .and_then(|d| d.as_object())
                    .ok_or_else(|| D::Error::custom("missing 'data' for Profile"))?;
                let id_str = data
                    .get("credential_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| D::Error::custom("missing 'credential_id'"))?;
                let credential_id =
                    Uuid::parse_str(id_str).map_err(|e| D::Error::custom(format!("invalid UUID: {e}")))?;
                Ok(AuthSource::Profile { credential_id })
            }
            _ => {
                // Legacy format: bare AuthMethod (e.g. {"type":"Agent","data":{...}})
                let auth: AuthMethod =
                    serde_json::from_value(serde_json::Value::Object(obj.clone()))
                        .map_err(D::Error::custom)?;
                Ok(AuthSource::Inline(auth))
            }
        }
    }
}
```

- [ ] **Step 3: Add tests to `credential.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::AuthMethod;

    #[test]
    fn test_credential_new() {
        let cred = Credential::new("deploy-key", AuthMethod::Agent { socket_path: None });
        assert_eq!(cred.name, "deploy-key");
        assert!(matches!(cred.auth, AuthMethod::Agent { .. }));
        assert!(cred.description.is_none());
    }

    #[test]
    fn test_credential_serialization_roundtrip() {
        let cred = Credential::new("test", AuthMethod::Password("secret".into()));
        let json = serde_json::to_string(&cred).unwrap();
        let deserialized: Credential = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, cred.name);
        assert_eq!(deserialized.id, cred.id);
    }

    #[test]
    fn test_auth_source_inline_roundtrip() {
        let source = AuthSource::Inline(AuthMethod::Agent { socket_path: None });
        let json = serde_json::to_string(&source).unwrap();
        let deserialized: AuthSource = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, AuthSource::Inline(AuthMethod::Agent { .. })));
    }

    #[test]
    fn test_auth_source_profile_roundtrip() {
        let id = Uuid::new_v4();
        let source = AuthSource::Profile { credential_id: id };
        let json = serde_json::to_string(&source).unwrap();
        let deserialized: AuthSource = serde_json::from_str(&json).unwrap();
        match deserialized {
            AuthSource::Profile { credential_id } => assert_eq!(credential_id, id),
            _ => panic!("expected Profile"),
        }
    }

    #[test]
    fn test_auth_source_legacy_deserialization() {
        // Legacy format: bare AuthMethod, no Inline wrapper
        let json = r#"{"type":"Agent","data":{"socket_path":null}}"#;
        let source: AuthSource = serde_json::from_str(json).unwrap();
        assert!(matches!(source, AuthSource::Inline(AuthMethod::Agent { .. })));
    }

    #[test]
    fn test_auth_source_legacy_agent_no_data() {
        // Very old format: Agent with no data field at all
        let json = r#"{"type":"Agent"}"#;
        let source: AuthSource = serde_json::from_str(json).unwrap();
        assert!(matches!(source, AuthSource::Inline(AuthMethod::Agent { socket_path: None })));
    }

    #[test]
    fn test_auth_source_legacy_password() {
        let json = r#"{"type":"Password","data":"secret"}"#;
        let source: AuthSource = serde_json::from_str(json).unwrap();
        match source {
            AuthSource::Inline(AuthMethod::Password(pw)) => assert_eq!(pw, "secret"),
            _ => panic!("expected Inline(Password)"),
        }
    }
}
```

- [ ] **Step 4: Register module in `lib.rs`**

In `crates/rivet-core/src/lib.rs`, add:

```rust
pub mod credential;
```

- [ ] **Step 5: Change `Connection.auth` from `AuthMethod` to `AuthSource`**

In `crates/rivet-core/src/connection.rs`:

Change the import and field:
```rust
use crate::credential::AuthSource;

pub struct Connection {
    // ...
    pub auth: AuthSource,   // was: AuthMethod
    // ...
}
```

Update `Connection::new()`:
```rust
auth: AuthSource::Inline(AuthMethod::Agent { socket_path: None }),
```

- [ ] **Step 6: Update all match sites on `Connection.auth`**

Every file that matches on `conn.auth` as `AuthMethod` must now match on `AuthSource`. The following files need updates:

- `crates/rivet-core/src/connection.rs` — tests: `matches!(conn.auth, AuthSource::Inline(AuthMethod::Agent { .. }))`
- `crates/rivet-core/src/protocol.rs` — `ConnCreateParams`, `ConnUpdateParams`, test
- `crates/rivet-vault/src/import.rs` — `into_connection()` wraps in `AuthSource::Inline()`
- `crates/rivet-cli/src/commands/conn.rs` — list display and add command
- `crates/rivet-daemon/src/handlers.rs` — `handle_ssh_connect_info` and all SSH/SCP handlers
- `crates/rivet-ssh/src/auth.rs` — `authenticate()` still receives `&AuthMethod` (no change needed here, resolution happens in daemon)

- [ ] **Step 7: Run tests to verify compilation and backward compat**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 8: Commit**

```bash
git add crates/rivet-core/src/credential.rs crates/rivet-core/src/lib.rs crates/rivet-core/src/connection.rs crates/rivet-core/src/protocol.rs crates/rivet-vault/src/import.rs crates/rivet-cli/src/commands/conn.rs crates/rivet-daemon/src/handlers.rs
git commit -m "feat: add Credential struct and AuthSource enum, migrate Connection.auth"
```

---

### Task 2: Vault storage for credentials

**Files:**
- Modify: `crates/rivet-vault/src/store.rs`
- Modify: `crates/rivet-core/src/error.rs`

- [ ] **Step 1: Add credential error variants to `error.rs`**

In `crates/rivet-core/src/error.rs`, add after `DuplicateGroupName`:

```rust
    #[error("credential not found: {0}")]
    CredentialNotFound(String),

    #[error("duplicate credential name: {0}")]
    DuplicateCredentialName(String),

    #[error("credential in use by connections: {0}")]
    CredentialInUse(String),
```

Add to `rpc_error_code()`:
```rust
    Self::CredentialNotFound(_) => -32019,
    Self::DuplicateCredentialName(_) => -32020,
    Self::CredentialInUse(_) => -32021,
```

- [ ] **Step 2: Add credential convenience methods to `UnlockedVault`**

In `crates/rivet-vault/src/store.rs`, add after the workflow convenience methods:

```rust
    // --- Credential convenience ---

    pub fn save_credential(&self, cred: &Credential) -> Result<()> {
        self.save_entity("credentials", &cred.id, cred)
    }

    pub fn load_credential(&self, id: &Uuid) -> Result<Credential> {
        self.load_entity("credentials", id)
    }

    pub fn list_credentials(&self) -> Result<Vec<Credential>> {
        self.list_entities("credentials")
    }

    pub fn delete_credential(&self, id: &Uuid) -> Result<()> {
        self.delete_entity("credentials", id)
    }

    pub fn find_credential_by_name(&self, name: &str) -> Result<Credential> {
        let credentials = self.list_credentials()?;
        credentials
            .into_iter()
            .find(|c| c.name == name)
            .ok_or_else(|| RivetError::CredentialNotFound(name.into()))
    }

    /// Resolve a connection's AuthSource to a concrete AuthMethod.
    pub fn resolve_auth(&self, conn: &Connection) -> Result<AuthMethod> {
        match &conn.auth {
            AuthSource::Inline(method) => Ok(method.clone()),
            AuthSource::Profile { credential_id } => {
                let cred = self.load_credential(credential_id)?;
                Ok(cred.auth)
            }
        }
    }
```

Add the necessary imports at the top of `store.rs`:
```rust
use rivet_core::credential::{AuthSource, Credential};
use rivet_core::connection::AuthMethod;
```

- [ ] **Step 3: Create `credentials/` directory in vault init**

In `VaultStore::init()`, add after the `workflows` directory creation:
```rust
fs::create_dir_all(self.vault_dir.join("credentials"))?;
```

- [ ] **Step 4: Add credential tests to `store.rs`**

```rust
    #[test]
    fn test_credential_crud() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir);
        init_fast(&store, "pass").unwrap();
        let vault = store.unlock("pass").unwrap();

        let cred = Credential::new("deploy-key", AuthMethod::Agent { socket_path: None });
        let cred_id = cred.id;
        vault.save_credential(&cred).unwrap();

        let creds = vault.list_credentials().unwrap();
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].name, "deploy-key");

        let loaded = vault.load_credential(&cred_id).unwrap();
        assert_eq!(loaded.name, "deploy-key");

        let found = vault.find_credential_by_name("deploy-key").unwrap();
        assert_eq!(found.id, cred_id);

        vault.delete_credential(&cred_id).unwrap();
        assert!(vault.list_credentials().unwrap().is_empty());
    }

    #[test]
    fn test_resolve_auth_inline() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir);
        init_fast(&store, "pass").unwrap();
        let vault = store.unlock("pass").unwrap();

        let conn = Connection::new("test", "host", "user");
        // Connection::new defaults to AuthSource::Inline(Agent)
        let resolved = vault.resolve_auth(&conn).unwrap();
        assert!(matches!(resolved, AuthMethod::Agent { .. }));
    }

    #[test]
    fn test_resolve_auth_profile() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir);
        init_fast(&store, "pass").unwrap();
        let vault = store.unlock("pass").unwrap();

        let cred = Credential::new("my-key", AuthMethod::Password("secret".into()));
        vault.save_credential(&cred).unwrap();

        let mut conn = Connection::new("test", "host", "user");
        conn.auth = AuthSource::Profile { credential_id: cred.id };
        vault.save_connection(&conn).unwrap();

        let resolved = vault.resolve_auth(&conn).unwrap();
        match resolved {
            AuthMethod::Password(pw) => assert_eq!(pw, "secret"),
            _ => panic!("expected Password"),
        }
    }

    #[test]
    fn test_resolve_auth_missing_credential() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir);
        init_fast(&store, "pass").unwrap();
        let vault = store.unlock("pass").unwrap();

        let mut conn = Connection::new("test", "host", "user");
        conn.auth = AuthSource::Profile { credential_id: Uuid::new_v4() };

        let result = vault.resolve_auth(&conn);
        assert!(result.is_err());
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/rivet-core/src/error.rs crates/rivet-vault/src/store.rs
git commit -m "feat: add credential vault storage and auth resolution"
```

---

### Task 3: RPC protocol types for credentials

**Files:**
- Modify: `crates/rivet-core/src/protocol.rs`

- [ ] **Step 1: Add credential RPC types**

Add after the existing workflow types in `protocol.rs`:

```rust
// --- Credentials ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredCreateParams {
    pub name: String,
    pub auth: AuthMethod,
    pub description: Option<String>,
}

impl CredCreateParams {
    pub fn into_credential(self) -> Credential {
        let mut cred = Credential::new(self.name, self.auth);
        cred.description = self.description;
        cred
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredListParams {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredGetParams {
    pub id: Option<Uuid>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredUpdateParams {
    pub id: Uuid,
    pub name: Option<String>,
    pub auth: Option<AuthMethod>,
    pub description: Option<Option<String>>,
}

impl CredUpdateParams {
    pub fn apply_to(self, cred: &mut Credential) {
        if let Some(name) = self.name {
            cred.name = name;
        }
        if let Some(auth) = self.auth {
            cred.auth = auth;
        }
        if let Some(desc) = self.description {
            cred.description = desc;
        }
        cred.updated_at = chrono::Utc::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredDeleteParams {
    pub id: Option<Uuid>,
    pub name: Option<String>,
    pub force: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredUsageParams {
    pub id: Option<Uuid>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredUsageResult {
    pub connections: Vec<CredUsageConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredUsageConnection {
    pub id: Uuid,
    pub name: String,
}
```

Add the necessary imports at the top:
```rust
use crate::credential::Credential;
```

- [ ] **Step 2: Run tests**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/rivet-core/src/protocol.rs
git commit -m "feat: add credential RPC protocol types"
```

---

### Task 4: Daemon handlers for credential CRUD

**Files:**
- Modify: `crates/rivet-daemon/src/handlers.rs`

- [ ] **Step 1: Add `cred.*` dispatch entries**

In the `dispatch()` match block, add after the group operations:

```rust
        // Credential operations
        "cred.create" => handle_cred_create(state, params).await,
        "cred.list" => handle_cred_list(state).await,
        "cred.get" => handle_cred_get(state, params).await,
        "cred.update" => handle_cred_update(state, params).await,
        "cred.delete" => handle_cred_delete(state, params).await,
        "cred.usage" => handle_cred_usage(state, params).await,
```

- [ ] **Step 2: Implement credential handlers**

Add to the bottom of `handlers.rs` (before any tests):

```rust
// --- Credentials ---

async fn handle_cred_create(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: CredCreateParams = parse_params(params)?;
    let mut state = state.write().await;

    let vault = state.require_vault().map_err(to_rpc_error)?;

    let existing = vault.list_credentials().map_err(to_rpc_error)?;
    if existing.iter().any(|c| c.name == p.name) {
        return Err(to_rpc_error(RivetError::DuplicateCredentialName(p.name.clone())));
    }

    let cred = p.into_credential();
    let id = cred.id;
    vault.save_credential(&cred).map_err(to_rpc_error)?;

    info!(id = %id, name = %cred.name, "credential created");
    to_value(IdResult { id })
}

async fn handle_cred_list(
    state: &SharedState,
) -> Result<Value, JsonRpcError> {
    let state = state.read().await;
    let vault = state.vault.as_ref().ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;
    let creds = vault.list_credentials().map_err(to_rpc_error)?;
    to_value(creds)
}

async fn handle_cred_get(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: CredGetParams = parse_params(params)?;
    let state = state.read().await;
    let vault = state.vault.as_ref().ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

    let cred = if let Some(id) = p.id {
        vault.load_credential(&id).map_err(to_rpc_error)?
    } else if let Some(ref name) = p.name {
        vault.find_credential_by_name(name).map_err(to_rpc_error)?
    } else {
        return Err(JsonRpcError {
            code: -32602,
            message: "must provide 'id' or 'name'".into(),
            data: None,
        });
    };

    to_value(cred)
}

async fn handle_cred_update(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: CredUpdateParams = parse_params(params)?;
    let mut state = state.write().await;
    let vault = state.require_vault().map_err(to_rpc_error)?;

    let mut cred = vault.load_credential(&p.id).map_err(to_rpc_error)?;

    if let Some(ref new_name) = p.name {
        if *new_name != cred.name {
            let existing = vault.list_credentials().map_err(to_rpc_error)?;
            if existing.iter().any(|c| c.name == *new_name && c.id != p.id) {
                return Err(to_rpc_error(RivetError::DuplicateCredentialName(new_name.clone())));
            }
        }
    }

    p.apply_to(&mut cred);
    vault.save_credential(&cred).map_err(to_rpc_error)?;

    info!(id = %cred.id, name = %cred.name, "credential updated");
    to_value(OkResult { ok: true })
}

async fn handle_cred_delete(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: CredDeleteParams = parse_params(params)?;
    let mut state = state.write().await;
    let vault = state.require_vault().map_err(to_rpc_error)?;

    let cred = if let Some(id) = p.id {
        vault.load_credential(&id).map_err(to_rpc_error)?
    } else if let Some(ref name) = p.name {
        vault.find_credential_by_name(name).map_err(to_rpc_error)?
    } else {
        return Err(JsonRpcError {
            code: -32602,
            message: "must provide 'id' or 'name'".into(),
            data: None,
        });
    };

    // Check if any connections reference this credential
    if !p.force.unwrap_or(false) {
        let connections = vault.list_connections().map_err(to_rpc_error)?;
        let using: Vec<String> = connections
            .iter()
            .filter(|c| matches!(&c.auth, AuthSource::Profile { credential_id } if *credential_id == cred.id))
            .map(|c| c.name.clone())
            .collect();

        if !using.is_empty() {
            return Err(to_rpc_error(RivetError::CredentialInUse(using.join(", "))));
        }
    }

    vault.delete_credential(&cred.id).map_err(to_rpc_error)?;

    info!(id = %cred.id, name = %cred.name, "credential deleted");
    to_value(OkResult { ok: true })
}

async fn handle_cred_usage(
    state: &SharedState,
    params: Option<Value>,
) -> Result<Value, JsonRpcError> {
    let p: CredUsageParams = parse_params(params)?;
    let state = state.read().await;
    let vault = state.vault.as_ref().ok_or_else(|| to_rpc_error(RivetError::VaultLocked))?;

    let cred = if let Some(id) = p.id {
        vault.load_credential(&id).map_err(to_rpc_error)?
    } else if let Some(ref name) = p.name {
        vault.find_credential_by_name(name).map_err(to_rpc_error)?
    } else {
        return Err(JsonRpcError {
            code: -32602,
            message: "must provide 'id' or 'name'".into(),
            data: None,
        });
    };

    let connections = vault.list_connections().map_err(to_rpc_error)?;
    let using: Vec<CredUsageConnection> = connections
        .into_iter()
        .filter(|c| matches!(&c.auth, AuthSource::Profile { credential_id } if *credential_id == cred.id))
        .map(|c| CredUsageConnection { id: c.id, name: c.name })
        .collect();

    to_value(CredUsageResult { connections: using })
}
```

Add the necessary imports at the top of `handlers.rs`:
```rust
use rivet_core::credential::AuthSource;
```

- [ ] **Step 3: Update existing SSH/SCP handlers to use `vault.resolve_auth()`**

In each handler that currently reads `conn.auth` directly (e.g., `handle_ssh_exec`, `handle_scp_upload`, `handle_scp_download`, `handle_tunnel_create`, `handle_ssh_connect_info`), the daemon must resolve the auth. Since `SshSession::connect()` takes a `&Connection`, and `Connection.auth` is now `AuthSource`, the resolution should happen before passing to SSH. The cleanest approach: resolve auth and create a temporary connection with inline auth for the SSH layer.

For `handle_ssh_connect_info`, update to resolve auth:

```rust
// Replace direct conn.auth match with resolution
let resolved_auth = vault.resolve_auth(&conn).map_err(to_rpc_error)?;

let (key_path, agent_socket_path) = match &resolved_auth {
    rivet_core::connection::AuthMethod::KeyFile { path, .. } => {
        (Some(path.to_string_lossy().into_owned()), None)
    }
    rivet_core::connection::AuthMethod::Agent { socket_path } => {
        (None, socket_path.as_ref().map(|p| p.to_string_lossy().into_owned()))
    }
    _ => (None, None),
};
```

For SSH exec and SCP handlers, update the session creation to resolve auth before connecting. In `SshSession::connect()` and `SshSession::connect_stream()`, change to accept `&AuthMethod` separately, or resolve in the handler and create a patched connection. The simplest approach: add a `resolve_auth` step before `SshSession::connect()` is called, and modify `auth::authenticate()` to receive the resolved `AuthMethod`.

Since `SshSession::connect()` reads `conn.auth` internally, the cleanest fix is to add a method to resolve auth on the Connection before passing it:

In `handlers.rs`, add a helper:
```rust
fn resolve_connection_auth(vault: &rivet_vault::store::UnlockedVault, conn: &mut Connection) -> Result<(), JsonRpcError> {
    let resolved = vault.resolve_auth(conn).map_err(to_rpc_error)?;
    conn.auth = AuthSource::Inline(resolved);
    Ok(())
}
```

Call this before every `SshSession::connect(&conn)` and `SshSession::connect_stream(&conn, ...)`.

- [ ] **Step 4: Update `auth::authenticate()` to handle `AuthSource`**

In `crates/rivet-ssh/src/auth.rs`, change `authenticate()` to accept `AuthSource` and resolve:

```rust
pub async fn authenticate(
    handle: &mut Handle<RivetHandler>,
    username: &str,
    auth_source: &AuthSource,
) -> Result<AuthOutcome, SshError> {
    let auth_method = match auth_source {
        AuthSource::Inline(method) => method,
        AuthSource::Profile { .. } => {
            return Err(SshError::Agent("unresolved credential profile — must be resolved before authentication".into()));
        }
    };
    match auth_method {
        // ... existing match arms unchanged
    }
}
```

This ensures that if a Profile reaches the SSH layer without being resolved, it fails with a clear error rather than silently.

- [ ] **Step 5: Run tests**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add crates/rivet-daemon/src/handlers.rs crates/rivet-ssh/src/auth.rs
git commit -m "feat: add credential CRUD daemon handlers and auth resolution"
```

---

### Task 5: CLI commands for credentials

**Files:**
- Create: `crates/rivet-cli/src/commands/cred.rs`
- Modify: `crates/rivet-cli/src/commands/mod.rs`
- Modify: `crates/rivet-cli/src/commands/conn.rs`
- Modify: `crates/rivet-cli/src/prefix.rs`
- Modify: `crates/rivet-cli/src/main.rs`

- [ ] **Step 1: Create `cred.rs` with CRUD commands**

```rust
// crates/rivet-cli/src/commands/cred.rs

use comfy_table::{presets, Table};
use rivet_core::credential::Credential;
use rivet_core::protocol::*;

use super::{CliError, get_client};

pub async fn list() -> Result<(), CliError> {
    let mut client = get_client().await?;
    let result = client
        .call("cred.list", Some(serde_json::json!({})))
        .await
        .map_err(CliError::Client)?;

    let creds: Vec<Credential> =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    if creds.is_empty() {
        println!("No credential profiles. Add one with: rivet cred add");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(presets::UTF8_FULL_CONDENSED);
    table.set_header(vec!["Name", "Auth Type", "Description"]);

    for cred in &creds {
        let auth_type = match &cred.auth {
            rivet_core::connection::AuthMethod::Password(_) => "password",
            rivet_core::connection::AuthMethod::PrivateKey { .. } => "key",
            rivet_core::connection::AuthMethod::KeyFile { .. } => "keyfile",
            rivet_core::connection::AuthMethod::Agent { .. } => "agent",
            rivet_core::connection::AuthMethod::Certificate { .. } => "cert",
            rivet_core::connection::AuthMethod::Interactive => "interactive",
        };
        table.add_row(vec![
            &cred.name,
            auth_type,
            cred.description.as_deref().unwrap_or(""),
        ]);
    }

    println!("{table}");
    Ok(())
}

pub async fn show(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("credential name".into()))?;

    let mut client = get_client().await?;
    let params = CredGetParams {
        id: None,
        name: Some(name.clone()),
    };
    let result = client
        .call("cred.get", Some(serde_json::to_value(&params).unwrap()))
        .await
        .map_err(CliError::Client)?;

    let cred: Credential =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!("Name:     {}", cred.name);
    println!("ID:       {}", cred.id);
    println!("Auth:     {:?}", cred.auth);
    if let Some(ref desc) = cred.description {
        println!("Desc:     {desc}");
    }
    println!("Created:  {}", cred.created_at);
    println!("Updated:  {}", cred.updated_at);

    // Show usage
    let usage_params = CredUsageParams {
        id: Some(cred.id),
        name: None,
    };
    let usage_result = client
        .call("cred.usage", Some(serde_json::to_value(&usage_params).unwrap()))
        .await
        .map_err(CliError::Client)?;

    let usage: CredUsageResult =
        serde_json::from_value(usage_result).map_err(|e| CliError::Other(e.to_string()))?;

    if !usage.connections.is_empty() {
        println!("Used by:  {}", usage.connections.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", "));
    }

    Ok(())
}

pub async fn add() -> Result<(), CliError> {
    let name = prompt("Name: ")?;
    let description_str = prompt("Description (optional): ")?;
    let description = if description_str.is_empty() {
        None
    } else {
        Some(description_str)
    };

    println!("Auth method:");
    println!("  1) SSH Agent (default)");
    println!("  2) Password");
    println!("  3) Key file");
    let auth_choice = prompt("Choice [1]: ")?;

    let auth = match auth_choice.as_str() {
        "2" => {
            let password = rpassword::prompt_password("Password: ")
                .map_err(|e| CliError::Other(e.to_string()))?;
            rivet_core::connection::AuthMethod::Password(password)
        }
        "3" => {
            let path = prompt("Key file path: ")?;
            let passphrase_str = rpassword::prompt_password("Key passphrase (empty for none): ")
                .map_err(|e| CliError::Other(e.to_string()))?;
            let passphrase = if passphrase_str.is_empty() {
                None
            } else {
                Some(passphrase_str)
            };
            rivet_core::connection::AuthMethod::KeyFile {
                path: path.into(),
                passphrase,
            }
        }
        _ => {
            let socket_str = prompt("Agent socket path (empty for default SSH_AUTH_SOCK): ")?;
            let socket_path = if socket_str.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(socket_str))
            };
            rivet_core::connection::AuthMethod::Agent { socket_path }
        }
    };

    let params = CredCreateParams {
        name,
        auth,
        description,
    };

    let mut client = get_client().await?;
    let result = client
        .call("cred.create", Some(serde_json::to_value(&params).unwrap()))
        .await
        .map_err(CliError::Client)?;

    let id_result: IdResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;
    println!("Credential created: {}", id_result.id);
    Ok(())
}

pub async fn edit(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("credential name".into()))?;

    let mut client = get_client().await?;
    let get_params = CredGetParams {
        id: None,
        name: Some(name.clone()),
    };
    let result = client
        .call("cred.get", Some(serde_json::to_value(&get_params).unwrap()))
        .await
        .map_err(CliError::Client)?;

    let cred: Credential =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!("Editing '{}' (press Enter to keep current value)", cred.name);

    let new_name = prompt_default("Name", &cred.name)?;
    let new_desc = prompt_default("Description", cred.description.as_deref().unwrap_or(""))?;

    let update = CredUpdateParams {
        id: cred.id,
        name: if new_name != cred.name {
            Some(new_name)
        } else {
            None
        },
        auth: None,
        description: if new_desc != cred.description.as_deref().unwrap_or("") {
            Some(if new_desc.is_empty() { None } else { Some(new_desc) })
        } else {
            None
        },
    };

    client
        .call("cred.update", Some(serde_json::to_value(&update).unwrap()))
        .await
        .map_err(CliError::Client)?;

    println!("Credential updated.");
    Ok(())
}

pub async fn rm(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("credential name".into()))?;

    let mut client = get_client().await?;
    let params = CredDeleteParams {
        id: None,
        name: Some(name.clone()),
        force: None,
    };
    client
        .call("cred.delete", Some(serde_json::to_value(&params).unwrap()))
        .await
        .map_err(CliError::Client)?;

    println!("Credential '{}' deleted.", name);
    Ok(())
}

fn prompt(msg: &str) -> Result<String, CliError> {
    use std::io::Write;
    print!("{msg}");
    std::io::stdout()
        .flush()
        .map_err(|e| CliError::Other(e.to_string()))?;
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| CliError::Other(e.to_string()))?;
    Ok(input.trim().to_string())
}

fn prompt_default(label: &str, default: &str) -> Result<String, CliError> {
    let input = prompt(&format!("{label} [{default}]: "))?;
    if input.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(input)
    }
}
```

- [ ] **Step 2: Register in `mod.rs`**

Add to `crates/rivet-cli/src/commands/mod.rs`:

```rust
pub mod cred;
```

Add dispatch entries:
```rust
        ["cred", "list"] => cred::list().await,
        ["cred", "show"] => cred::show(extra_args).await,
        ["cred", "add"] => cred::add().await,
        ["cred", "edit"] => cred::edit(extra_args).await,
        ["cred", "rm"] => cred::rm(extra_args).await,
```

- [ ] **Step 3: Register in command tree (`prefix.rs`)**

Add after the group block in `build_command_tree()`:

```rust
    // cred
    {
        let cred = root.add_child(CommandNode::new("cred"));
        cred.add_child(CommandNode::leaf("list"));
        cred.add_child(CommandNode::leaf("show"));
        cred.add_child(CommandNode::leaf("add"));
        cred.add_child(CommandNode::leaf("edit"));
        cred.add_child(CommandNode::leaf("rm"));
    }
```

- [ ] **Step 4: Update `conn add` to offer profile selection**

In `crates/rivet-cli/src/commands/conn.rs`, update the `add()` function's auth section:

```rust
    println!("Auth method:");
    println!("  1) Use credential profile");
    println!("  2) SSH Agent (inline, default)");
    println!("  3) Password (inline)");
    println!("  4) Key file (inline)");
    let auth_choice = prompt("Choice [2]: ")?;

    let auth = match auth_choice.as_str() {
        "1" => {
            // List available credential profiles
            let creds_result = client
                .call("cred.list", Some(serde_json::json!({})))
                .await
                .map_err(CliError::Client)?;
            let creds: Vec<rivet_core::credential::Credential> =
                serde_json::from_value(creds_result).map_err(|e| CliError::Other(e.to_string()))?;

            if creds.is_empty() {
                println!("No credential profiles. Create one first with: rivet cred add");
                return Err(CliError::Other("no credential profiles available".into()));
            }

            for (i, cred) in creds.iter().enumerate() {
                println!("  {}) {} ({:?})", i + 1, cred.name, cred.auth);
            }
            let choice = prompt("Select profile: ")?;
            let idx: usize = choice
                .parse::<usize>()
                .map_err(|_| CliError::Other("invalid choice".into()))?
                .checked_sub(1)
                .ok_or_else(|| CliError::Other("invalid choice".into()))?;
            let selected = creds
                .get(idx)
                .ok_or_else(|| CliError::Other("invalid choice".into()))?;

            rivet_core::credential::AuthSource::Profile { credential_id: selected.id }
        }
        "3" => {
            let password = rpassword::prompt_password("Password: ")
                .map_err(|e| CliError::Other(e.to_string()))?;
            rivet_core::credential::AuthSource::Inline(
                rivet_core::connection::AuthMethod::Password(password)
            )
        }
        "4" => {
            let path = prompt("Key file path: ")?;
            let passphrase_str = rpassword::prompt_password("Key passphrase (empty for none): ")
                .map_err(|e| CliError::Other(e.to_string()))?;
            let passphrase = if passphrase_str.is_empty() {
                None
            } else {
                Some(passphrase_str)
            };
            rivet_core::credential::AuthSource::Inline(
                rivet_core::connection::AuthMethod::KeyFile { path: path.into(), passphrase }
            )
        }
        _ => {
            let socket_str = prompt("Agent socket path (empty for default SSH_AUTH_SOCK): ")?;
            let socket_path = if socket_str.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(socket_str))
            };
            rivet_core::credential::AuthSource::Inline(
                rivet_core::connection::AuthMethod::Agent { socket_path }
            )
        }
    };
```

Note: `ConnCreateParams.auth` must also change from `AuthMethod` to `AuthSource`. This was done in Task 1 Step 6.

Update `list()` auth type display to handle `AuthSource`:
```rust
        let auth_type = match &conn.auth {
            rivet_core::credential::AuthSource::Profile { .. } => "profile",
            rivet_core::credential::AuthSource::Inline(method) => match method {
                rivet_core::connection::AuthMethod::Password(_) => "password",
                rivet_core::connection::AuthMethod::PrivateKey { .. } => "key",
                rivet_core::connection::AuthMethod::KeyFile { .. } => "keyfile",
                rivet_core::connection::AuthMethod::Agent { .. } => "agent",
                rivet_core::connection::AuthMethod::Certificate { .. } => "cert",
                rivet_core::connection::AuthMethod::Interactive => "interactive",
            },
        };
```

- [ ] **Step 5: Update help text in `main.rs`**

Add to the help output:
```rust
    println!("  cred   list|show|add|edit|rm  Manage credential profiles");
```

- [ ] **Step 6: Run tests**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/rivet-cli/src/commands/cred.rs crates/rivet-cli/src/commands/mod.rs crates/rivet-cli/src/commands/conn.rs crates/rivet-cli/src/prefix.rs crates/rivet-cli/src/main.rs
git commit -m "feat: add CLI commands for credential management"
```

---

### Task 6: Smoke tests for credential operations

**Files:**
- Modify: `tests/smoke_test.rs`

- [ ] **Step 1: Add credential smoke tests**

Add to `tests/smoke_test.rs`:

```rust
#[tokio::test]
async fn smoke_credential_create_and_list() {
    let ctx = TestContext::new().await;
    ctx.init_and_unlock().await;

    // Create credential
    let params = json!({
        "name": "deploy-key",
        "auth": {"type": "Agent", "data": {"socket_path": null}},
        "description": "Deploy SSH agent"
    });
    let result = ctx.call("cred.create", params).await;
    assert!(result.get("id").is_some());

    // List
    let creds = ctx.call("cred.list", json!({})).await;
    let creds = creds.as_array().unwrap();
    assert_eq!(creds.len(), 1);
    assert_eq!(creds[0]["name"], "deploy-key");
}

#[tokio::test]
async fn smoke_credential_get_and_update() {
    let ctx = TestContext::new().await;
    ctx.init_and_unlock().await;

    let result = ctx.call("cred.create", json!({
        "name": "test-cred",
        "auth": {"type": "Password", "data": "secret"}
    })).await;
    let id = result["id"].as_str().unwrap();

    // Get by name
    let cred = ctx.call("cred.get", json!({"name": "test-cred"})).await;
    assert_eq!(cred["name"], "test-cred");

    // Update
    ctx.call("cred.update", json!({"id": id, "name": "renamed-cred"})).await;
    let cred = ctx.call("cred.get", json!({"id": id})).await;
    assert_eq!(cred["name"], "renamed-cred");
}

#[tokio::test]
async fn smoke_credential_delete() {
    let ctx = TestContext::new().await;
    ctx.init_and_unlock().await;

    let result = ctx.call("cred.create", json!({
        "name": "to-delete",
        "auth": {"type": "Agent", "data": {"socket_path": null}}
    })).await;

    ctx.call("cred.delete", json!({"name": "to-delete"})).await;

    let creds = ctx.call("cred.list", json!({})).await;
    assert_eq!(creds.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn smoke_credential_duplicate_name() {
    let ctx = TestContext::new().await;
    ctx.init_and_unlock().await;

    ctx.call("cred.create", json!({
        "name": "dup",
        "auth": {"type": "Agent", "data": {"socket_path": null}}
    })).await;

    let result = ctx.call_raw("cred.create", json!({
        "name": "dup",
        "auth": {"type": "Agent", "data": {"socket_path": null}}
    })).await;
    assert!(result.error.is_some());
}

#[tokio::test]
async fn smoke_credential_usage() {
    let ctx = TestContext::new().await;
    ctx.init_and_unlock().await;

    // Create credential
    let cred_result = ctx.call("cred.create", json!({
        "name": "shared-key",
        "auth": {"type": "Agent", "data": {"socket_path": null}}
    })).await;
    let cred_id = cred_result["id"].as_str().unwrap();

    // Create connection using this credential
    ctx.call("conn.create", json!({
        "name": "server1",
        "host": "10.0.0.1",
        "username": "admin",
        "auth": {"type": "Profile", "data": {"credential_id": cred_id}}
    })).await;

    // Check usage
    let usage = ctx.call("cred.usage", json!({"name": "shared-key"})).await;
    let connections = usage["connections"].as_array().unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0]["name"], "server1");
}

#[tokio::test]
async fn smoke_credential_delete_blocked_when_in_use() {
    let ctx = TestContext::new().await;
    ctx.init_and_unlock().await;

    let cred_result = ctx.call("cred.create", json!({
        "name": "in-use",
        "auth": {"type": "Agent", "data": {"socket_path": null}}
    })).await;
    let cred_id = cred_result["id"].as_str().unwrap();

    ctx.call("conn.create", json!({
        "name": "linked-server",
        "host": "10.0.0.1",
        "username": "admin",
        "auth": {"type": "Profile", "data": {"credential_id": cred_id}}
    })).await;

    // Delete should fail without force
    let result = ctx.call_raw("cred.delete", json!({"name": "in-use"})).await;
    assert!(result.error.is_some());

    // Delete with force should succeed
    ctx.call("cred.delete", json!({"name": "in-use", "force": true})).await;
}
```

Note: The test helper methods (`TestContext`, `call`, `call_raw`) should match the existing test patterns in `smoke_test.rs`. Review the existing helper to ensure `call_raw` returns the full response with error field.

- [ ] **Step 2: Run tests**

Run: `cargo test --workspace`
Expected: All tests pass including new smoke tests

- [ ] **Step 3: Commit**

```bash
git add tests/smoke_test.rs
git commit -m "test: add credential smoke tests"
```

---

### Task 7: Swift model and SwiftUI views

**Files:**
- Create: `RivetApp/Sources/Models/Credential.swift`
- Modify: `RivetApp/Sources/Models/Connection.swift`
- Create: `RivetApp/Sources/Views/CredentialListView.swift`
- Create: `RivetApp/Sources/Views/AddCredentialView.swift`
- Modify: `RivetApp/Sources/Views/AddConnectionView.swift`

- [ ] **Step 1: Create Swift Credential model**

```swift
// RivetApp/Sources/Models/Credential.swift

import Foundation

struct RivetCredential: Codable, Identifiable, Hashable {
    let id: UUID
    var name: String
    var auth: AuthMethod
    var description: String?
    var createdAt: String
    var updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, name, auth, description
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    static func == (lhs: RivetCredential, rhs: RivetCredential) -> Bool {
        lhs.id == rhs.id
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }
}

/// How a connection resolves its authentication.
enum AuthSource: Codable {
    case inline(AuthMethod)
    case profile(credentialId: UUID)

    enum CodingKeys: String, CodingKey {
        case type_ = "type"
        case data
    }

    struct ProfileData: Codable {
        let credentialId: UUID

        enum CodingKeys: String, CodingKey {
            case credentialId = "credential_id"
        }
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type_ = try container.decode(String.self, forKey: .type_)

        switch type_ {
        case "Inline":
            let auth = try container.decode(AuthMethod.self, forKey: .data)
            self = .inline(auth)
        case "Profile":
            let data = try container.decode(ProfileData.self, forKey: .data)
            self = .profile(credentialId: data.credentialId)
        default:
            // Legacy: bare AuthMethod
            let auth = try AuthMethod(from: decoder)
            self = .inline(auth)
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .inline(let auth):
            try container.encode("Inline", forKey: .type_)
            try container.encode(auth, forKey: .data)
        case .profile(let credentialId):
            try container.encode("Profile", forKey: .type_)
            try container.encode(ProfileData(credentialId: credentialId), forKey: .data)
        }
    }

    var displayName: String {
        switch self {
        case .inline(let auth): return auth.displayName
        case .profile: return "Credential Profile"
        }
    }
}
```

- [ ] **Step 2: Update Connection.swift to use AuthSource**

Change `auth` property from `AuthMethod` to `AuthSource` in `RivetConnection`.

- [ ] **Step 3: Create CredentialListView**

```swift
// RivetApp/Sources/Views/CredentialListView.swift

import SwiftUI

struct CredentialListView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @State private var showAddSheet = false

    var body: some View {
        VStack {
            if viewModel.credentials.isEmpty {
                ContentUnavailableView(
                    "No Credential Profiles",
                    systemImage: "key",
                    description: Text("Add a credential profile to share auth across connections.")
                )
            } else {
                List(viewModel.credentials) { cred in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(cred.name)
                            .font(.headline)
                        Text(cred.auth.displayName)
                            .font(.subheadline)
                            .foregroundColor(.secondary)
                        if let desc = cred.description {
                            Text(desc)
                                .font(.caption)
                                .foregroundColor(.secondary)
                        }
                    }
                    .padding(.vertical, 2)
                }
            }
        }
        .toolbar {
            Button(action: { showAddSheet = true }) {
                Image(systemName: "plus")
            }
        }
        .sheet(isPresented: $showAddSheet) {
            AddCredentialView()
                .environmentObject(viewModel)
        }
    }
}
```

- [ ] **Step 4: Create AddCredentialView**

```swift
// RivetApp/Sources/Views/AddCredentialView.swift

import SwiftUI

struct AddCredentialView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @Environment(\.dismiss) var dismiss

    @State private var name = ""
    @State private var authType = "agent"
    @State private var password = ""
    @State private var keyPath = ""
    @State private var keyPassphrase = ""
    @State private var agentSocketPath = ""
    @State private var description = ""
    @State private var isSubmitting = false

    var body: some View {
        VStack(spacing: 0) {
            Text("Add Credential Profile")
                .font(.title2)
                .fontWeight(.semibold)
                .padding()

            Form {
                Section("Profile") {
                    TextField("Name", text: $name)
                    TextField("Description (optional)", text: $description)
                }

                Section("Authentication") {
                    Picker("Method", selection: $authType) {
                        Text("SSH Agent").tag("agent")
                        Text("Password").tag("password")
                        Text("Key File").tag("keyfile")
                    }

                    switch authType {
                    case "password":
                        SecureField("Password", text: $password)
                    case "keyfile":
                        TextField("Key File Path", text: $keyPath)
                        SecureField("Key Passphrase (optional)", text: $keyPassphrase)
                    default:
                        TextField("Agent Socket Path (optional)", text: $agentSocketPath)
                    }
                }
            }
            .formStyle(.grouped)

            HStack {
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button("Add") { addCredential() }
                    .keyboardShortcut(.defaultAction)
                    .buttonStyle(.borderedProminent)
                    .disabled(name.isEmpty || isSubmitting)
            }
            .padding()
        }
        .frame(width: 450, height: 400)
    }

    private func addCredential() {
        isSubmitting = true

        let authMethod: AuthMethod
        switch authType {
        case "password":
            authMethod = .password(password)
        case "keyfile":
            authMethod = .keyFile(
                path: keyPath,
                passphrase: keyPassphrase.isEmpty ? nil : keyPassphrase
            )
        default:
            authMethod = .agent(socketPath: agentSocketPath.isEmpty ? nil : agentSocketPath)
        }

        struct CreateParams: Encodable {
            let name: String
            let auth: AuthMethod
            let description: String?
        }

        let params = CreateParams(
            name: name,
            auth: authMethod,
            description: description.isEmpty ? nil : description
        )

        Task {
            let client = DaemonClient()
            do {
                try await client.connect()
                let _: IdResult = try await client.call(method: "cred.create", params: params)
                await viewModel.loadCredentials()
                dismiss()
            } catch {
                isSubmitting = false
            }
        }
    }
}
```

- [ ] **Step 5: Update AddConnectionView to support profile selection**

Add state for profile selection and update the auth section picker to include a "Credential Profile" option. When selected, show a picker from `viewModel.credentials`.

- [ ] **Step 6: Add `credentials` to AppViewModel**

Add `@Published var credentials: [RivetCredential] = []` and a `loadCredentials()` method.

- [ ] **Step 7: Commit**

```bash
git add RivetApp/Sources/Models/Credential.swift RivetApp/Sources/Models/Connection.swift RivetApp/Sources/Views/CredentialListView.swift RivetApp/Sources/Views/AddCredentialView.swift RivetApp/Sources/Views/AddConnectionView.swift
git commit -m "feat: add Swift model and SwiftUI views for credential management"
```

---

### Task 8: MCP tools for credentials

**Files:**
- Modify: `crates/rivet-mcp/src/tools.rs`

- [ ] **Step 1: Add credential tools**

Add tool definitions for `list_credentials` and `show_credential` following the existing pattern (e.g., `list_connections` and `show_connection`).

Register in the tools list and add handlers that call `cred.list` and `cred.get` RPC methods on the daemon.

- [ ] **Step 2: Run tests**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/rivet-mcp/src/tools.rs
git commit -m "feat: add MCP tools for credential management"
```

---

### Task 9: Final integration test and verification

**Files:**
- Modify: `tests/integration_test.rs`

- [ ] **Step 1: Add integration test for credential lifecycle**

```rust
#[tokio::test]
async fn test_credential_lifecycle() {
    let ctx = IntegrationContext::new().await;

    // Create credential
    let cred_id = ctx.create_credential("shared-agent", json!({
        "type": "Agent",
        "data": {"socket_path": null}
    })).await;

    // Create connection referencing it
    ctx.create_connection_with_profile("server1", "10.0.0.1", "admin", &cred_id).await;

    // Verify usage
    let usage = ctx.get_credential_usage(&cred_id).await;
    assert_eq!(usage.len(), 1);

    // Update credential
    ctx.update_credential(&cred_id, json!({"name": "updated-agent"})).await;

    // Connection still references it
    let usage = ctx.get_credential_usage(&cred_id).await;
    assert_eq!(usage.len(), 1);

    // Delete connection, then credential
    ctx.delete_connection("server1").await;
    ctx.delete_credential("updated-agent").await;
}
```

- [ ] **Step 2: Run full test suite**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 3: Run `cargo build` to verify clean build**

Run: `cargo build`
Expected: Compiles with no errors or warnings

- [ ] **Step 4: Final commit**

```bash
git add tests/integration_test.rs
git commit -m "test: add credential lifecycle integration test"
```
