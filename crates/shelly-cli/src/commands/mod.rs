pub mod conn;
pub mod daemon;
pub mod exec;
pub mod group;
pub mod scp;
pub mod tunnel;
pub mod vault;

use crate::client::{ClientError, DaemonClient};

/// Dispatch a resolved command to the appropriate handler.
pub async fn dispatch(command: &[String], extra_args: &[String]) -> Result<(), CliError> {
    let cmd: Vec<&str> = command.iter().map(|s| s.as_str()).collect();
    match cmd.as_slice() {
        ["daemon", "start"] => daemon::start().await,
        ["daemon", "stop"] => daemon::stop().await,
        ["daemon", "status"] => daemon::status().await,
        ["vault", "init"] => vault::init().await,
        ["vault", "unlock"] => vault::unlock().await,
        ["vault", "lock"] => vault::lock().await,
        ["vault", "status"] => vault::status().await,
        ["vault", "change-password"] => vault::change_password().await,
        ["conn", "list"] => conn::list().await,
        ["conn", "show"] => conn::show(extra_args).await,
        ["conn", "add"] => conn::add().await,
        ["conn", "edit"] => conn::edit(extra_args).await,
        ["conn", "rm"] => conn::rm(extra_args).await,
        ["conn", "import"] => conn::import(extra_args).await,
        ["group", "list"] => group::list().await,
        ["group", "show"] => group::show(extra_args).await,
        ["group", "add"] => group::add().await,
        ["group", "edit"] => group::edit(extra_args).await,
        ["group", "rm"] => group::rm(extra_args).await,
        ["group", "exec"] => group::exec(extra_args).await,
        ["group", "upload"] => group::upload(extra_args).await,
        ["tunnel", "create"] => tunnel::create(extra_args).await,
        ["tunnel", "list"] => tunnel::list().await,
        ["tunnel", "close"] => tunnel::close(extra_args).await,
        ["ssh"] => crate::commands::exec::ssh_interactive(extra_args).await,
        ["exec"] => exec::exec(extra_args).await,
        ["scp", "upload"] => scp::upload(extra_args).await,
        ["scp", "download"] => scp::download(extra_args).await,
        _ => Err(CliError::UnknownCommand(command.join(" "))),
    }
}

/// Get a connected client, ensuring daemon is running.
pub async fn get_client() -> Result<DaemonClient, CliError> {
    crate::client::ensure_daemon_running()
        .await
        .map_err(CliError::Client)?;
    DaemonClient::connect().await.map_err(CliError::Client)
}

#[derive(Debug)]
pub enum CliError {
    Client(ClientError),
    UnknownCommand(String),
    MissingArgument(String),
    Other(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Client(e) => write!(f, "{e}"),
            CliError::UnknownCommand(cmd) => write!(f, "unknown command: {cmd}"),
            CliError::MissingArgument(arg) => write!(f, "missing argument: {arg}"),
            CliError::Other(msg) => write!(f, "{msg}"),
        }
    }
}
