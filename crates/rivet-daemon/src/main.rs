mod handlers;
mod server;
mod state;

use std::sync::Arc;

use rivet_core::protocol::{log_dir, pid_file_path, rivet_dir, socket_path};
use tokio::signal;
use tokio::sync::RwLock;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use state::DaemonState;

#[tokio::main]
async fn main() {
    // Setup tracing
    setup_tracing();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "rivetd starting"
    );

    // Ensure ~/.rivet directory exists
    let rivet = rivet_dir();
    if let Err(e) = std::fs::create_dir_all(&rivet) {
        error!(error = %e, "failed to create rivet directory");
        std::process::exit(1);
    }

    // Write PID file
    let pid_path = pid_file_path();
    if let Err(e) = std::fs::write(&pid_path, std::process::id().to_string()) {
        error!(error = %e, "failed to write PID file");
        std::process::exit(1);
    }

    // Initialize state
    let state: state::SharedState = Arc::new(RwLock::new(DaemonState::new()));

    // Try to pre-load vault store (if initialized)
    {
        let vault_dir = rivet_core::protocol::vault_dir();
        let store = rivet_vault::store::VaultStore::new(vault_dir);
        if store.is_initialized() {
            info!("vault found (locked)");
            state.write().await.vault_store = Some(store);
        } else {
            info!("no vault found — use vault.init to create one");
        }
    }

    let sock = socket_path();

    // Spawn the server
    let server_state = state.clone();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server::run_server(&sock, server_state).await {
            error!(error = %e, "server error");
        }
    });

    info!("rivetd ready");

    // Wait for shutdown signal
    shutdown_signal().await;

    info!("shutting down...");

    // Cleanup
    server_handle.abort();

    // Disconnect all SSH sessions
    {
        let mut state = state.write().await;
        for (_id, session) in state.sessions.drain() {
            let _ = session.disconnect().await;
        }
        // Lock vault if unlocked
        if let Some(vault) = state.vault.take() {
            let _store = vault.lock();
        }
    }

    // Remove PID file and socket
    let _ = std::fs::remove_file(&pid_path);
    let _ = std::fs::remove_file(socket_path());

    info!("rivetd stopped");
}

/// Wait for SIGTERM or SIGINT.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { info!("received SIGINT"); }
        _ = terminate => { info!("received SIGTERM"); }
    }
}

/// Setup tracing with stderr output.
fn setup_tracing() {
    let _log_dir = log_dir();
    let _ = std::fs::create_dir_all(&_log_dir);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,rivet_daemon=debug,rivet_ssh=debug"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}
