use rivet_core::error::RivetError;

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

    #[error("tunnel error: {0}")]
    Tunnel(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<SshError> for RivetError {
    fn from(err: SshError) -> Self {
        match err {
            SshError::AuthFailed | SshError::UnsupportedAuthMethod(_) => {
                RivetError::SshAuthFailed(err.to_string())
            }
            SshError::ConnectionFailed(_) | SshError::Protocol(_) => {
                RivetError::SshConnectionFailed(err.to_string())
            }
            SshError::Tunnel(msg) => RivetError::TunnelError(msg),
            SshError::Io(e) => RivetError::IoError(e),
            other => RivetError::SshConnectionFailed(other.to_string()),
        }
    }
}
