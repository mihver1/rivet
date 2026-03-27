# Credentials Manager

## Problem

Each `Connection` embeds its own `AuthMethod`. When the same key or agent config is shared across many servers, changing it means editing every connection individually. There is no way to define a reusable authentication profile.

## Solution

Introduce **Credential** as a new first-class entity in the vault. A credential is a named, reusable authentication profile that connections can reference. Connections can either reference a credential profile or continue using inline auth.

## Data Model

### Credential entity

```rust
// crates/rivet-core/src/credential.rs

pub struct Credential {
    pub id: Uuid,
    pub name: String,               // e.g. "deploy-key", "1password-agent"
    pub auth: AuthMethod,           // reuses existing AuthMethod enum
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

Stored in vault at `~/.rivet/vault/credentials/{uuid}.enc`, following the existing entity-per-file pattern used by Connection, Group, and Workflow.

### AuthSource — replaces Connection.auth

The `Connection.auth` field type changes from `AuthMethod` to `AuthSource`:

```rust
// crates/rivet-core/src/credential.rs

pub enum AuthSource {
    /// Auth configured directly on the connection (current behavior).
    Inline(AuthMethod),
    /// Reference to a credential profile.
    Profile { credential_id: Uuid },
}
```

Serialization format:
- `Inline` — wraps the existing `AuthMethod` JSON. Backward compatible: any legacy `Connection` with a bare `AuthMethod` deserializes as `Inline(AuthMethod)`.
- `Profile` — `{"type":"Profile","data":{"credential_id":"<uuid>"}}`

### Backward compatibility

Legacy connections store `auth` as a bare `AuthMethod` (e.g. `{"type":"Agent","data":{...}}`). The custom `Deserialize` for `AuthSource` must:
1. Try to parse as `AuthSource` (with `Inline`/`Profile` tags)
2. On failure, fall back to parsing as `AuthMethod` and wrapping in `Inline()`

No vault migration needed. Old data works as-is.

### Connection struct change

```rust
pub struct Connection {
    // ... all existing fields ...
    pub auth: AuthSource,   // was: AuthMethod
    // ...
}
```

## Vault Storage

New methods on `UnlockedVault`:
- `save_credential(&self, cred: &Credential) -> Result<()>`
- `load_credential(&self, id: &Uuid) -> Result<Credential>`
- `list_credentials(&self) -> Result<Vec<Credential>>`
- `delete_credential(&self, id: &Uuid) -> Result<()>`
- `find_credential_by_name(&self, name: &str) -> Result<Credential>`

These follow the existing `save_entity`/`load_entity`/`list_entity_ids` pattern in `store.rs`.

### Credential resolution

New helper method for resolving a connection's auth at use time:

```rust
impl UnlockedVault {
    pub fn resolve_auth(&self, conn: &Connection) -> Result<AuthMethod> {
        match &conn.auth {
            AuthSource::Inline(method) => Ok(method.clone()),
            AuthSource::Profile { credential_id } => {
                let cred = self.load_credential(credential_id)?;
                Ok(cred.auth)
            }
        }
    }
}
```

## RPC Protocol

### New methods

| Method | Params | Result |
|--------|--------|--------|
| `cred.create` | `CredCreateParams { name, auth, description? }` | `IdResult { id }` |
| `cred.list` | `CredListParams {}` | `Vec<Credential>` |
| `cred.get` | `CredGetParams { id?, name? }` | `Credential` |
| `cred.update` | `CredUpdateParams { id, name?, auth?, description? }` | `OkResult` |
| `cred.delete` | `CredDeleteParams { id?, name? }` | `OkResult` |
| `cred.usage` | `CredUsageParams { id?, name? }` | `CredUsageResult { connections: Vec<{id, name}> }` |

### Changes to existing methods

`conn.create` and `conn.update` — the `auth` field in `ConnCreateParams` / `ConnUpdateParams` changes from `AuthMethod` to `AuthSource`.

`ssh.exec`, `ssh.connect_info`, `scp.upload`, `scp.download` — daemon resolves `AuthSource` to `AuthMethod` via `vault.resolve_auth()` before authenticating.

## CLI

### New commands

```
rivet cred add          Interactive creation of credential profile
rivet cred list         Table: Name, Auth Type, Description, Used By (count)
rivet cred show <name>  Show credential details + list of connections using it
rivet cred edit <name>  Edit credential fields
rivet cred rm <name>    Delete (warn if connections reference it)
```

### Changes to existing commands

`rivet conn add`:
```
Auth method:
  1) Use credential profile
  2) SSH Agent (inline)
  3) Password (inline)
  4) Key file (inline)
Choice [2]:
```

If choice 1, list available profiles and let user pick by name/number.

`rivet conn list` — auth column shows profile name when Profile, or method type when Inline.

`rivet conn show <name>` — displays resolved auth details.

## Daemon Handlers

### New handlers

Standard CRUD handlers for `cred.*` methods, following the pattern of `conn.*` handlers in `handlers.rs`.

`cred.delete` handler checks if any connections reference the credential. If so, returns an error with the list of connection names unless a `force: true` flag is passed.

### Changes to existing handlers

Every handler that currently reads `conn.auth` as `AuthMethod` must resolve through `vault.resolve_auth(&conn)`:
- `handle_ssh_exec`
- `handle_ssh_connect_info`
- `handle_scp_upload`
- `handle_scp_download`
- `handle_tunnel_create`
- `handle_workflow_run`

## SwiftUI

### New views

- `CredentialListView` — list of credential profiles, accessible from sidebar
- `AddCredentialView` — form to create a profile (name, auth method picker, description)
- `CredentialDetailView` — show/edit credential

### Changes to existing views

`AddConnectionView` — auth section gets a toggle/picker:
- "Use credential profile" → picker from existing profiles
- "Configure inline" → current auth method UI

`ConnectionDetailView` — shows resolved auth, with link to profile if using one.

### Swift model

```swift
struct RivetCredential: Codable, Identifiable, Hashable {
    let id: UUID
    var name: String
    var auth: AuthMethod
    var description: String?
    var createdAt: String
    var updatedAt: String
}

enum AuthSource: Codable {
    case inline(AuthMethod)
    case profile(credentialId: UUID)
}
```

## MCP Tools

New tools:
- `list_credentials` — list all credential profiles
- `show_credential` — show details of a credential by name

No changes needed to existing tools — resolution happens in the daemon.

## Testing

### Unit tests (rivet-core)
- `Credential` serialization roundtrip
- `AuthSource::Inline` serialization and backward compat (legacy `AuthMethod` format)
- `AuthSource::Profile` serialization roundtrip

### Vault tests (rivet-vault)
- Credential CRUD: create, load, list, delete
- `resolve_auth` with Inline → returns AuthMethod directly
- `resolve_auth` with Profile → loads credential and returns its AuthMethod
- `resolve_auth` with Profile pointing to deleted credential → returns error
- Find credential by name

### Integration/smoke tests
- `cred.create` / `cred.list` / `cred.get` / `cred.update` / `cred.delete` RPC flow
- Create credential, create connection with Profile ref, verify `ssh.connect_info` resolves correctly
- `cred.usage` returns correct connection list
- `cred.delete` blocked when referenced by connections
- Legacy connection (inline auth) still works after model change

## File inventory

| File | Action |
|------|--------|
| `crates/rivet-core/src/credential.rs` | **New** — `Credential`, `AuthSource` structs |
| `crates/rivet-core/src/lib.rs` | Add `pub mod credential` |
| `crates/rivet-core/src/connection.rs` | Change `auth: AuthMethod` → `auth: AuthSource`, update custom Deserialize |
| `crates/rivet-core/src/protocol.rs` | Add `Cred*Params`/`Cred*Result` types, update `ConnCreateParams`/`ConnUpdateParams` |
| `crates/rivet-vault/src/store.rs` | Add credential CRUD methods, `resolve_auth()` |
| `crates/rivet-vault/src/import.rs` | Update SSH config import to use `AuthSource::Inline` |
| `crates/rivet-daemon/src/handlers.rs` | Add `cred.*` handlers, update SSH/SCP/tunnel handlers to resolve auth |
| `crates/rivet-daemon/src/server.rs` | Register `cred.*` method dispatch |
| `crates/rivet-ssh/src/auth.rs` | No change — still receives `&AuthMethod` after resolution |
| `crates/rivet-ssh/src/session.rs` | Accept `&AuthMethod` instead of reading from `Connection.auth` directly |
| `crates/rivet-cli/src/commands/cred.rs` | **New** — CLI commands for credential management |
| `crates/rivet-cli/src/commands/mod.rs` | Add `pub mod cred` |
| `crates/rivet-cli/src/commands/conn.rs` | Update add/list/show to handle AuthSource |
| `crates/rivet-cli/src/commands/exec.rs` | No change — uses daemon-resolved connect info |
| `crates/rivet-cli/src/main.rs` | Register `cred` subcommand |
| `crates/rivet-mcp/src/tools.rs` | Add `list_credentials`, `show_credential` tools |
| `RivetApp/Sources/Models/Credential.swift` | **New** — Swift mirror of Credential, AuthSource |
| `RivetApp/Sources/Models/Connection.swift` | Update `auth` from AuthMethod to AuthSource |
| `RivetApp/Sources/Views/CredentialListView.swift` | **New** |
| `RivetApp/Sources/Views/AddCredentialView.swift` | **New** |
| `RivetApp/Sources/Views/AddConnectionView.swift` | Add profile picker in auth section |
| `tests/smoke_test.rs` | Add credential smoke tests |
| `tests/integration_test.rs` | Add credential integration tests |
