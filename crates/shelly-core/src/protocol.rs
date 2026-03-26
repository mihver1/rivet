use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connection::{AuthMethod, Connection, SshOptions};

// --- Paths ---

pub fn shelly_dir() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".shelly")
}

pub fn socket_path() -> PathBuf {
    shelly_dir().join("shelly.sock")
}

pub fn pid_file_path() -> PathBuf {
    shelly_dir().join("shellyd.pid")
}

pub fn vault_dir() -> PathBuf {
    shelly_dir().join("vault")
}

pub fn config_path() -> PathBuf {
    shelly_dir().join("config.toml")
}

pub fn log_dir() -> PathBuf {
    shelly_dir().join("logs")
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
    pub auth: AuthMethod,
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
    pub auth: Option<AuthMethod>,
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

// --- Daemon ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatusResult {
    pub uptime_secs: u64,
    pub active_sessions: u32,
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
            auth: AuthMethod::Agent,
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
