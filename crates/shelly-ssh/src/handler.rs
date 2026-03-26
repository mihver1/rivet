use russh::client;
use russh::keys::PublicKey;
use tracing::debug;

/// SSH client handler that implements the russh callback interface.
///
/// For MVP, this handler accepts all server host keys.
/// TODO: Implement known_hosts checking.
#[derive(Debug)]
pub struct ShellyHandler;

impl client::Handler for ShellyHandler {
    type Error = ShellyHandlerError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        // TODO: check against known_hosts file
        debug!(
            key_type = %server_public_key.algorithm(),
            "accepting server host key (known_hosts check not yet implemented)"
        );
        Ok(true)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellyHandlerError {
    #[error("SSH protocol error: {0}")]
    Russh(#[from] russh::Error),
}
