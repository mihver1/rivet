use std::sync::Arc;

use russh::client::Handle;
use russh::keys::{self, agent, PrivateKeyWithHashAlg};
use rivet_core::connection::AuthMethod;
use tracing::{debug, warn};

use crate::error::SshError;
use crate::handler::RivetHandler;

/// Result of authentication attempt.
#[derive(Debug)]
pub enum AuthOutcome {
    /// Authentication succeeded.
    Success,
    /// Authentication failed — no more methods to try.
    Failed,
}

/// Authenticate an SSH session using the given auth method.
///
/// Dispatches to the appropriate russh authentication mechanism
/// based on the `AuthMethod` variant from the connection config.
pub async fn authenticate(
    handle: &mut Handle<RivetHandler>,
    username: &str,
    auth_method: &AuthMethod,
) -> Result<AuthOutcome, SshError> {
    match auth_method {
        AuthMethod::Password(password) => {
            auth_password(handle, username, password).await
        }
        AuthMethod::KeyFile { path, passphrase } => {
            auth_key_file(handle, username, path, passphrase.as_deref()).await
        }
        AuthMethod::PrivateKey {
            key_data,
            passphrase,
        } => auth_private_key(handle, username, key_data, passphrase.as_deref()).await,
        AuthMethod::Agent => auth_agent(handle, username).await,
        AuthMethod::Certificate { .. } => {
            warn!("certificate authentication is not yet implemented");
            Err(SshError::UnsupportedAuthMethod("Certificate"))
        }
        AuthMethod::Interactive => {
            warn!("keyboard-interactive authentication is not yet implemented");
            Err(SshError::UnsupportedAuthMethod("Interactive"))
        }
    }
}

/// Password authentication.
async fn auth_password(
    handle: &mut Handle<RivetHandler>,
    username: &str,
    password: &str,
) -> Result<AuthOutcome, SshError> {
    debug!("authenticating with password");
    let result = handle
        .authenticate_password(username, password)
        .await
        .map_err(SshError::Protocol)?;

    match result {
        russh::client::AuthResult::Success => {
            debug!("password authentication succeeded");
            Ok(AuthOutcome::Success)
        }
        russh::client::AuthResult::Failure { .. } => {
            debug!("password authentication failed");
            Ok(AuthOutcome::Failed)
        }
    }
}

/// Authenticate using a key file on disk.
async fn auth_key_file(
    handle: &mut Handle<RivetHandler>,
    username: &str,
    path: &std::path::Path,
    passphrase: Option<&str>,
) -> Result<AuthOutcome, SshError> {
    debug!(path = %path.display(), "loading key file");

    let key = keys::load_secret_key(path, passphrase)
        .map_err(|e| SshError::KeyLoad(format!("{}: {}", path.display(), e)))?;

    auth_with_private_key(handle, username, key).await
}

/// Authenticate using an in-memory private key (PEM/OpenSSH format bytes).
async fn auth_private_key(
    handle: &mut Handle<RivetHandler>,
    username: &str,
    key_data: &[u8],
    passphrase: Option<&str>,
) -> Result<AuthOutcome, SshError> {
    debug!("decoding in-memory private key");

    let key_str =
        std::str::from_utf8(key_data).map_err(|e| SshError::KeyLoad(format!("invalid UTF-8 key data: {e}")))?;

    let key = keys::decode_secret_key(key_str, passphrase)
        .map_err(|e| SshError::KeyLoad(e.to_string()))?;

    auth_with_private_key(handle, username, key).await
}

/// Common logic for public key authentication after key is loaded.
async fn auth_with_private_key(
    handle: &mut Handle<RivetHandler>,
    username: &str,
    key: russh::keys::PrivateKey,
) -> Result<AuthOutcome, SshError> {
    // Determine best hash algorithm for RSA keys
    let hash_alg = handle
        .best_supported_rsa_hash()
        .await
        .map_err(SshError::Protocol)?;

    let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg.flatten());

    let result = handle
        .authenticate_publickey(username, key_with_hash)
        .await
        .map_err(SshError::Protocol)?;

    match result {
        russh::client::AuthResult::Success => {
            debug!("public key authentication succeeded");
            Ok(AuthOutcome::Success)
        }
        russh::client::AuthResult::Failure { .. } => {
            debug!("public key authentication failed");
            Ok(AuthOutcome::Failed)
        }
    }
}

/// SSH agent authentication.
///
/// Connects to the SSH agent (via SSH_AUTH_SOCK), lists available keys,
/// and tries each one until authentication succeeds or all are exhausted.
async fn auth_agent(
    handle: &mut Handle<RivetHandler>,
    username: &str,
) -> Result<AuthOutcome, SshError> {
    debug!("connecting to SSH agent");

    let mut agent = agent::client::AgentClient::connect_env()
        .await
        .map_err(|e| SshError::Agent(format!("failed to connect to agent: {e}")))?;

    let identities = agent
        .request_identities()
        .await
        .map_err(|e| SshError::Agent(format!("failed to list agent identities: {e}")))?;

    if identities.is_empty() {
        debug!("SSH agent has no identities");
        return Ok(AuthOutcome::Failed);
    }

    debug!(count = identities.len(), "found agent identities");

    for (i, key) in identities.iter().enumerate() {
        debug!(
            index = i,
            algorithm = %key.algorithm(),
            "trying agent key"
        );

        // Determine best hash algorithm for RSA keys
        let hash_alg = handle
            .best_supported_rsa_hash()
            .await
            .map_err(SshError::Protocol)?;

        let result = handle
            .authenticate_publickey_with(username, key.clone(), hash_alg.flatten(), &mut agent)
            .await
            .map_err(|e| SshError::Agent(format!("agent auth error: {e}")))?;

        match result {
            russh::client::AuthResult::Success => {
                debug!(index = i, "agent key authentication succeeded");
                return Ok(AuthOutcome::Success);
            }
            russh::client::AuthResult::Failure { .. } => {
                debug!(index = i, "agent key rejected, trying next");
                continue;
            }
        }
    }

    debug!("all agent keys exhausted");
    Ok(AuthOutcome::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_outcome_debug() {
        let success = AuthOutcome::Success;
        let failed = AuthOutcome::Failed;
        assert_eq!(format!("{success:?}"), "Success");
        assert_eq!(format!("{failed:?}"), "Failed");
    }
}
