use shelly_core::protocol::{DaemonStatusResult, pid_file_path, socket_path};

use super::CliError;
use crate::client::{ClientError, DaemonClient, ensure_daemon_running};

pub async fn start() -> Result<(), CliError> {
    match DaemonClient::connect().await {
        Ok(_) => {
            println!("daemon is already running");
            return Ok(());
        }
        Err(ClientError::DaemonNotRunning) => {}
        Err(e) => return Err(CliError::Client(e)),
    }

    ensure_daemon_running().await.map_err(CliError::Client)?;
    println!("daemon started");
    Ok(())
}

pub async fn stop() -> Result<(), CliError> {
    // Try graceful shutdown via PID
    let pid_path = pid_file_path();
    if pid_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                libc_kill(pid);
                // Wait for socket to disappear
                let sock = socket_path();
                for _ in 0..30 {
                    if !sock.exists() {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                println!("daemon stopped");
                return Ok(());
            }
        }
    }

    println!("daemon is not running");
    Ok(())
}

/// Send SIGTERM to process.
fn libc_kill(pid: i32) {
    #[cfg(unix)]
    {
        // SAFETY: sending SIGTERM to a process is safe
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

pub async fn status() -> Result<(), CliError> {
    match DaemonClient::connect().await {
        Ok(mut client) => {
            let result = client
                .call("daemon.status", None)
                .await
                .map_err(CliError::Client)?;

            let status: DaemonStatusResult =
                serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

            println!("Status:   running");
            println!("Uptime:   {}s", status.uptime_secs);
            println!("Sessions: {}", status.active_sessions);
            println!("Tunnels:  {}", status.active_tunnels);
            println!(
                "Vault:    {}",
                if status.vault_locked {
                    "locked"
                } else {
                    "unlocked"
                }
            );
            Ok(())
        }
        Err(ClientError::DaemonNotRunning) => {
            println!("Status: not running");
            Ok(())
        }
        Err(e) => Err(CliError::Client(e)),
    }
}
