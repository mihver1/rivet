use std::path::Path;

use russh::client::Handle;
use russh_sftp::client::SftpSession;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info};

use crate::error::SshError;
use crate::handler::ShellyHandler;

/// Upload a local file to the remote host via SFTP.
///
/// Returns the number of bytes transferred.
pub async fn upload_file(
    handle: &Handle<ShellyHandler>,
    local_path: &Path,
    remote_path: &str,
) -> Result<u64, SshError> {
    info!(
        local = %local_path.display(),
        remote = remote_path,
        "uploading file"
    );

    let data = tokio::fs::read(local_path)
        .await
        .map_err(SshError::Io)?;

    let bytes_len = data.len() as u64;

    let sftp = open_sftp(handle).await?;

    // Create and write to remote file
    let mut remote_file = sftp
        .create(remote_path)
        .await
        .map_err(|e| SshError::Channel(format!("SFTP create failed: {e}")))?;

    remote_file
        .write_all(&data)
        .await
        .map_err(|e| SshError::Channel(format!("SFTP write failed: {e}")))?;

    remote_file
        .shutdown()
        .await
        .map_err(|e| SshError::Channel(format!("SFTP flush failed: {e}")))?;

    sftp.close()
        .await
        .map_err(|e| SshError::Channel(format!("SFTP close failed: {e}")))?;

    info!(bytes = bytes_len, "upload complete");
    Ok(bytes_len)
}

/// Download a file from the remote host via SFTP.
///
/// Returns the number of bytes transferred.
pub async fn download_file(
    handle: &Handle<ShellyHandler>,
    remote_path: &str,
    local_path: &Path,
) -> Result<u64, SshError> {
    info!(
        remote = remote_path,
        local = %local_path.display(),
        "downloading file"
    );

    let sftp = open_sftp(handle).await?;

    let mut remote_file = sftp
        .open(remote_path)
        .await
        .map_err(|e| SshError::Channel(format!("SFTP open failed: {e}")))?;

    let mut data = Vec::new();
    remote_file
        .read_to_end(&mut data)
        .await
        .map_err(|e| SshError::Channel(format!("SFTP read failed: {e}")))?;

    let bytes_len = data.len() as u64;

    sftp.close()
        .await
        .map_err(|e| SshError::Channel(format!("SFTP close failed: {e}")))?;

    // Write to local file, creating parent dirs if needed
    if let Some(parent) = local_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(SshError::Io)?;
    }

    tokio::fs::write(local_path, &data)
        .await
        .map_err(SshError::Io)?;

    info!(bytes = bytes_len, "download complete");
    Ok(bytes_len)
}

/// Upload raw bytes to a remote path via SFTP.
///
/// Returns the number of bytes transferred.
pub async fn upload_bytes(
    handle: &Handle<ShellyHandler>,
    data: &[u8],
    remote_path: &str,
) -> Result<u64, SshError> {
    debug!(
        remote = remote_path,
        bytes = data.len(),
        "uploading bytes"
    );

    let sftp = open_sftp(handle).await?;

    sftp.write(remote_path, data)
        .await
        .map_err(|e| SshError::Channel(format!("SFTP write failed: {e}")))?;

    sftp.close()
        .await
        .map_err(|e| SshError::Channel(format!("SFTP close failed: {e}")))?;

    Ok(data.len() as u64)
}

/// Download a remote file and return its contents as bytes.
pub async fn download_bytes(
    handle: &Handle<ShellyHandler>,
    remote_path: &str,
) -> Result<Vec<u8>, SshError> {
    debug!(remote = remote_path, "downloading bytes");

    let sftp = open_sftp(handle).await?;

    let data = sftp
        .read(remote_path)
        .await
        .map_err(|e| SshError::Channel(format!("SFTP read failed: {e}")))?;

    sftp.close()
        .await
        .map_err(|e| SshError::Channel(format!("SFTP close failed: {e}")))?;

    debug!(bytes = data.len(), "download complete");
    Ok(data)
}

/// Open an SFTP session on the given SSH handle.
async fn open_sftp(handle: &Handle<ShellyHandler>) -> Result<SftpSession, SshError> {
    debug!("opening SFTP subsystem");

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| SshError::Channel(format!("failed to open session channel: {e}")))?;

    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| SshError::Channel(format!("SFTP subsystem request failed: {e}")))?;

    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| SshError::Channel(format!("SFTP session init failed: {e}")))?;

    debug!("SFTP session established");
    Ok(sftp)
}
