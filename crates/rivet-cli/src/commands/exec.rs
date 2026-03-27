use rivet_core::protocol::*;

use super::{CliError, get_client};

/// Open an interactive SSH session using the system ssh binary.
pub async fn ssh_interactive(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("connection name".into()))?;

    let mut client = get_client().await?;

    // Get connection info first
    let get_params = ConnGetParams {
        id: None,
        name: Some(name.clone()),
    };
    let result = client
        .call("conn.get", Some(serde_json::to_value(&get_params).unwrap()))
        .await
        .map_err(CliError::Client)?;

    let conn: rivet_core::connection::Connection =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    let connect_params = SshConnectInfoParams {
        connection_id: conn.id,
    };
    let result = client
        .call(
            "ssh.connect_info",
            Some(serde_json::to_value(&connect_params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let info: SshConnectInfoResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    // Build ssh command
    let mut cmd = std::process::Command::new("ssh");

    if info.port != 22 {
        cmd.arg("-p").arg(info.port.to_string());
    }

    if let Some(ref key_path) = info.key_path {
        cmd.arg("-i").arg(key_path);
    }

    for arg in &info.extra_args {
        cmd.arg(arg);
    }

    // Add any additional args from command line (skip connection name)
    for arg in args.iter().skip(1) {
        cmd.arg(arg);
    }

    cmd.arg(format!("{}@{}", info.username, info.host));

    // Replace the current process with ssh
    let status = cmd
        .status()
        .map_err(|e| CliError::Other(format!("failed to exec ssh: {e}")))?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

/// Execute a command on a remote host via the daemon.
pub async fn exec(args: &[String]) -> Result<(), CliError> {
    if args.len() < 2 {
        return Err(CliError::MissingArgument(
            "usage: rivet exec <connection> <command...>".into(),
        ));
    }

    let name = &args[0];
    let command = args[1..].join(" ");

    let mut client = get_client().await?;

    // Resolve connection name to ID
    let get_params = ConnGetParams {
        id: None,
        name: Some(name.clone()),
    };
    let result = client
        .call("conn.get", Some(serde_json::to_value(&get_params).unwrap()))
        .await
        .map_err(CliError::Client)?;

    let conn: rivet_core::connection::Connection =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    let exec_params = SshExecParams {
        connection_id: conn.id,
        command,
    };

    let result = client
        .call(
            "ssh.exec",
            Some(serde_json::to_value(&exec_params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let exec_result: SshExecResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    if !exec_result.stdout.is_empty() {
        print!("{}", exec_result.stdout);
    }
    if !exec_result.stderr.is_empty() {
        eprint!("{}", exec_result.stderr);
    }

    if exec_result.exit_code != 0 {
        std::process::exit(exec_result.exit_code);
    }

    Ok(())
}
