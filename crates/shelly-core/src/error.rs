use thiserror::Error;

#[derive(Debug, Error)]
pub enum ShellyError {
    #[error("vault is locked")]
    VaultLocked,

    #[error("vault is not initialized")]
    VaultNotInitialized,

    #[error("vault is already initialized")]
    VaultAlreadyInitialized,

    #[error("connection not found: {0}")]
    ConnectionNotFound(String),

    #[error("duplicate connection name: {0}")]
    DuplicateConnectionName(String),

    #[error("SSH authentication failed: {0}")]
    SshAuthFailed(String),

    #[error("SSH connection failed: {0}")]
    SshConnectionFailed(String),

    #[error("SCP transfer failed: {0}")]
    ScpTransferFailed(String),

    #[error("invalid password")]
    InvalidPassword,

    #[error("crypto error: {0}")]
    CryptoError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    SerializationError(String),

    #[error("daemon is not running")]
    DaemonNotRunning,

    #[error("group not found: {0}")]
    GroupNotFound(String),

    #[error("duplicate group name: {0}")]
    DuplicateGroupName(String),

    #[error("internal error: {0}")]
    InternalError(String),
}

impl ShellyError {
    pub fn rpc_error_code(&self) -> i32 {
        match self {
            Self::VaultLocked => -32001,
            Self::ConnectionNotFound(_) => -32002,
            Self::SshAuthFailed(_) => -32003,
            Self::SshConnectionFailed(_) => -32004,
            Self::ScpTransferFailed(_) => -32005,
            Self::VaultNotInitialized => -32006,
            Self::VaultAlreadyInitialized => -32007,
            Self::DuplicateConnectionName(_) => -32008,
            Self::InvalidPassword => -32009,
            Self::CryptoError(_) => -32010,
            Self::SerializationError(_) => -32011,
            Self::DaemonNotRunning => -32012,
            Self::GroupNotFound(_) => -32013,
            Self::DuplicateGroupName(_) => -32014,
            Self::IoError(_) => -32603,
            Self::InternalError(_) => -32603,
        }
    }
}

impl From<serde_json::Error> for ShellyError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationError(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, ShellyError>;
