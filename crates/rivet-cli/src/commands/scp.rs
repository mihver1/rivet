use rivet_core::protocol::*;

use super::{CliError, get_client};

pub async fn upload(args: &[String]) -> Result<(), CliError> {
    if args.len() < 3 {
        return Err(CliError::MissingArgument(
            "usage: rivet scp upload <connection> <local_path> <remote_path>".into(),
        ));
    }

    let name = &args[0];
    let local_path = &args[1];
    let remote_path = &args[2];

    let mut client = get_client().await?;

    // Resolve connection name
    let conn = resolve_connection(&mut client, name).await?;

    let params = ScpUploadParams {
        connection_id: conn.id,
        local_path: local_path.clone(),
        remote_path: remote_path.clone(),
    };

    let result = client
        .call(
            "scp.upload",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let scp_result: ScpResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!(
        "Uploaded {} bytes: {} → {}:{}",
        scp_result.bytes_transferred, local_path, name, remote_path
    );
    Ok(())
}

pub async fn download(args: &[String]) -> Result<(), CliError> {
    if args.len() < 3 {
        return Err(CliError::MissingArgument(
            "usage: rivet scp download <connection> <remote_path> <local_path>".into(),
        ));
    }

    let name = &args[0];
    let remote_path = &args[1];
    let local_path = &args[2];

    let mut client = get_client().await?;

    let conn = resolve_connection(&mut client, name).await?;

    let params = ScpDownloadParams {
        connection_id: conn.id,
        remote_path: remote_path.clone(),
        local_path: local_path.clone(),
    };

    let result = client
        .call(
            "scp.download",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let scp_result: ScpResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!(
        "Downloaded {} bytes: {}:{} → {}",
        scp_result.bytes_transferred, name, remote_path, local_path
    );
    Ok(())
}

async fn resolve_connection(
    client: &mut crate::client::DaemonClient,
    name: &str,
) -> Result<rivet_core::connection::Connection, CliError> {
    let params = ConnGetParams {
        id: None,
        name: Some(name.to_string()),
    };
    let result = client
        .call("conn.get", Some(serde_json::to_value(&params).unwrap()))
        .await
        .map_err(CliError::Client)?;
    serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))
}
