use shelly_core::error::ShellyError;

#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("SSH protocol error: {0}")]
    Protocol(russh::Error),

    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("key loading error: {0}")]
    KeyLoad(String),

    #[error("SSH agent error: {0}")]
    Agent(String),

    #[error("authentication failed")]
    AuthFailed,

    #[error("unsupported auth method: {0}")]
    UnsupportedAuthMethod(&'static str),

    #[error("channel error: {0}")]
    Channel(String),

    #[error("session is closed")]
    SessionClosed,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<SshError> for ShellyError {
    fn from(err: SshError) -> Self {
        match err {
            SshError::AuthFailed | SshError::UnsupportedAuthMethod(_) => {
                ShellyError::SshAuthFailed(err.to_string())
            }
            SshError::ConnectionFailed(_) | SshError::Protocol(_) => {
                ShellyError::SshConnectionFailed(err.to_string())
            }
            SshError::Io(e) => ShellyError::IoError(e),
            other => ShellyError::SshConnectionFailed(other.to_string()),
        }
    }
}
