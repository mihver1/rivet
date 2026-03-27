use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::{AuthMethod, Connection, Group, SshOptions, TunnelSpec};
use crate::credential::{AuthSource, Credential};

// --- Paths ---

pub fn rivet_dir() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".rivet")
}

pub fn socket_path() -> PathBuf {
    rivet_dir().join("rivet.sock")
}

pub fn pid_file_path() -> PathBuf {
    rivet_dir().join("rivetd.pid")
}

pub fn vault_dir() -> PathBuf {
    rivet_dir().join("vault")
}

pub fn config_path() -> PathBuf {
    rivet_dir().join("config.toml")
}

pub fn log_dir() -> PathBuf {
    rivet_dir().join("logs")
}

// --- JSON-RPC 2.0 ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    pub id: serde_json::Value,
}

impl JsonRpcRequest {
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>, id: u64) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
            id: serde_json::Value::Number(id.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: serde_json::Value,
}

impl JsonRpcResponse {
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(id: serde_json::Value, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(error),
            id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// --- Vault ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultUnlockParams {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultInitParams {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultChangePasswordParams {
    pub old_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStatusResult {
    pub locked: bool,
    pub initialized: bool,
}

// --- Connection ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnListParams {
    pub tag: Option<String>,
    pub group_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnGetParams {
    pub id: Option<Uuid>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnCreateParams {
    pub name: String,
    pub host: String,
    pub port: Option<u16>,
    pub username: String,
    pub auth: AuthSource,
    pub tags: Option<Vec<String>>,
    pub group_ids: Option<Vec<Uuid>>,
    pub jump_host: Option<Uuid>,
    pub options: Option<SshOptions>,
    pub notes: Option<String>,
}

impl ConnCreateParams {
    pub fn into_connection(self) -> Connection {
        let mut conn = Connection::new(self.name, self.host, self.username);
        if let Some(port) = self.port {
            conn.port = port;
        }
        conn.auth = self.auth;
        conn.tags = self.tags.unwrap_or_default();
        conn.group_ids = self.group_ids.unwrap_or_default();
        conn.jump_host = self.jump_host;
        if let Some(options) = self.options {
            conn.options = options;
        }
        conn.notes = self.notes;
        conn
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnUpdateParams {
    pub id: Uuid,
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub auth: Option<AuthSource>,
    pub tags: Option<Vec<String>>,
    pub group_ids: Option<Vec<Uuid>>,
    pub jump_host: Option<Option<Uuid>>,
    pub options: Option<SshOptions>,
    pub notes: Option<Option<String>>,
}

impl ConnUpdateParams {
    pub fn apply_to(self, conn: &mut Connection) {
        if let Some(name) = self.name {
            conn.name = name;
        }
        if let Some(host) = self.host {
            conn.host = host;
        }
        if let Some(port) = self.port {
            conn.port = port;
        }
        if let Some(username) = self.username {
            conn.username = username;
        }
        if let Some(auth) = self.auth {
            conn.auth = auth;
        }
        if let Some(tags) = self.tags {
            conn.tags = tags;
        }
        if let Some(group_ids) = self.group_ids {
            conn.group_ids = group_ids;
        }
        if let Some(jump_host) = self.jump_host {
            conn.jump_host = jump_host;
        }
        if let Some(options) = self.options {
            conn.options = options;
        }
        if let Some(notes) = self.notes {
            conn.notes = notes;
        }
        conn.updated_at = chrono::Utc::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnDeleteParams {
    pub id: Option<Uuid>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnImportParams {
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnImportResult {
    pub imported: u32,
}

// --- Group ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupListParams {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupGetParams {
    pub id: Option<Uuid>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupCreateParams {
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
}

impl GroupCreateParams {
    pub fn into_group(self) -> Group {
        let mut g = Group::new(self.name);
        g.description = self.description;
        g.color = self.color;
        g
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupUpdateParams {
    pub id: Uuid,
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub color: Option<Option<String>>,
}

impl GroupUpdateParams {
    pub fn apply_to(self, group: &mut Group) {
        if let Some(name) = self.name {
            group.name = name;
        }
        if let Some(description) = self.description {
            group.description = description;
        }
        if let Some(color) = self.color {
            group.color = color;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupDeleteParams {
    pub id: Option<Uuid>,
    pub name: Option<String>,
}

// --- Group Operations ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupExecParams {
    pub group_id: Option<Uuid>,
    pub group_name: Option<String>,
    pub command: String,
    /// Max parallel connections (None = all at once).
    pub concurrency: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupExecResult {
    pub results: Vec<GroupExecHostResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupExecHostResult {
    pub connection_id: Uuid,
    pub connection_name: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    /// Set if connection/exec failed entirely.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupUploadParams {
    pub group_id: Option<Uuid>,
    pub group_name: Option<String>,
    pub local_path: String,
    pub remote_path: String,
    /// Max parallel uploads (None = all at once).
    pub concurrency: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupUploadResult {
    pub results: Vec<GroupUploadHostResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupUploadHostResult {
    pub connection_id: Uuid,
    pub connection_name: String,
    pub bytes_transferred: u64,
    /// Set if connection/upload failed entirely.
    pub error: Option<String>,
}

// --- SSH ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshExecParams {
    pub connection_id: Uuid,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConnectInfoParams {
    pub connection_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConnectInfoResult {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub key_path: Option<String>,
    pub agent_socket_path: Option<String>,
    pub extra_args: Vec<String>,
}

// --- SCP ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScpUploadParams {
    pub connection_id: Uuid,
    pub local_path: String,
    pub remote_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScpDownloadParams {
    pub connection_id: Uuid,
    pub remote_path: String,
    pub local_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScpResult {
    pub bytes_transferred: u64,
}

// --- Tunnel ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelCreateParams {
    /// Connection name or ID to tunnel through
    pub connection_id: Option<Uuid>,
    pub connection_name: Option<String>,
    pub spec: TunnelSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelCreateResult {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelInfo {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub connection_name: String,
    pub spec: TunnelSpec,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelCloseParams {
    pub id: Uuid,
}

// --- Workflow ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowGetParams {
    pub id: Option<Uuid>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDeleteParams {
    pub id: Option<Uuid>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunParams {
    /// Workflow to run — by ID or name.
    pub workflow_id: Option<Uuid>,
    pub workflow_name: Option<String>,

    /// Target: either a single connection or a group.
    pub connection_id: Option<Uuid>,
    pub connection_name: Option<String>,
    pub group_id: Option<Uuid>,
    pub group_name: Option<String>,

    /// Variable overrides (merge with workflow defaults).
    #[serde(default)]
    pub variables: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowImportParams {
    /// Raw YAML content to import.
    pub yaml: String,
}

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

// --- Daemon ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatusResult {
    pub uptime_secs: u64,
    pub active_sessions: u32,
    pub active_tunnels: u32,
    pub vault_locked: bool,
}

// --- Generic ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkResult {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdResult {
    pub id: Uuid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_roundtrip() {
        let req = JsonRpcRequest::new(
            "vault.unlock",
            Some(serde_json::to_value(VaultUnlockParams { password: "test".into() }).unwrap()),
            1,
        );
        let json = serde_json::to_string(&req).unwrap();
        let deserialized: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.method, "vault.unlock");
        assert_eq!(deserialized.jsonrpc, "2.0");
    }

    #[test]
    fn test_json_rpc_response_success() {
        let resp = JsonRpcResponse::success(
            serde_json::Value::Number(1.into()),
            serde_json::to_value(OkResult { ok: true }).unwrap(),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_json_rpc_response_error() {
        let resp = JsonRpcResponse::error(
            serde_json::Value::Number(1.into()),
            JsonRpcError {
                code: -32001,
                message: "vault locked".into(),
                data: None,
            },
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("\"result\""));
        assert!(json.contains("\"error\""));
        assert!(json.contains("-32001"));
    }

    #[test]
    fn test_vault_params_roundtrip() {
        let params = VaultUnlockParams { password: "secret".into() };
        let json = serde_json::to_string(&params).unwrap();
        let deserialized: VaultUnlockParams = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.password, "secret");
    }

    #[test]
    fn test_conn_create_params_to_connection() {
        let params = ConnCreateParams {
            name: "test".into(),
            host: "example.com".into(),
            port: Some(2222),
            username: "admin".into(),
            auth: AuthSource::Inline(AuthMethod::Agent { socket_path: None }),
            tags: Some(vec!["prod".into()]),
            group_ids: None,
            jump_host: None,
            options: None,
            notes: Some("test note".into()),
        };
        let conn = params.into_connection();
        assert_eq!(conn.name, "test");
        assert_eq!(conn.port, 2222);
        assert_eq!(conn.tags, vec!["prod"]);
        assert_eq!(conn.notes, Some("test note".into()));
    }

    #[test]
    fn test_ssh_exec_result_roundtrip() {
        let result = SshExecResult {
            exit_code: 0,
            stdout: "up 42 days".into(),
            stderr: String::new(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: SshExecResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.exit_code, 0);
        assert_eq!(deserialized.stdout, "up 42 days");
    }

    #[test]
    fn test_group_create_params_to_group() {
        let params = GroupCreateParams {
            name: "production".into(),
            description: Some("Production servers".into()),
            color: Some("#ff0000".into()),
        };
        let group = params.into_group();
        assert_eq!(group.name, "production");
        assert_eq!(group.description, Some("Production servers".into()));
        assert_eq!(group.color, Some("#ff0000".into()));
    }

    #[test]
    fn test_group_update_apply() {
        let mut group = Group::new("old-name");
        let update = GroupUpdateParams {
            id: group.id,
            name: Some("new-name".into()),
            description: Some(Some("new desc".into())),
            color: None,
        };
        update.apply_to(&mut group);
        assert_eq!(group.name, "new-name");
        assert_eq!(group.description, Some("new desc".into()));
        assert!(group.color.is_none()); // unchanged
    }

    #[test]
    fn test_group_update_clear_description() {
        let mut group = Group::new("test");
        group.description = Some("old desc".into());
        let update = GroupUpdateParams {
            id: group.id,
            name: None,
            description: Some(None), // explicitly clear
            color: None,
        };
        update.apply_to(&mut group);
        assert!(group.description.is_none());
    }

    #[test]
    fn test_group_params_roundtrip() {
        let params = GroupCreateParams {
            name: "staging".into(),
            description: None,
            color: Some("blue".into()),
        };
        let json = serde_json::to_string(&params).unwrap();
        let deserialized: GroupCreateParams = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "staging");
        assert_eq!(deserialized.color, Some("blue".into()));
    }

    #[test]
    fn test_conn_update_apply() {
        let mut conn = Connection::new("old-name", "old-host", "old-user");
        let update = ConnUpdateParams {
            id: conn.id,
            name: Some("new-name".into()),
            host: None,
            port: Some(2222),
            username: None,
            auth: None,
            tags: None,
            group_ids: None,
            jump_host: None,
            options: None,
            notes: None,
        };
        update.apply_to(&mut conn);
        assert_eq!(conn.name, "new-name");
        assert_eq!(conn.host, "old-host");
        assert_eq!(conn.port, 2222);
        assert_eq!(conn.username, "old-user");
    }
}
