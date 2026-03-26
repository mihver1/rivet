use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use shelly_ssh::SshSession;
use shelly_vault::store::{UnlockedVault, VaultStore};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Shared daemon state, behind `Arc<RwLock<..>>` for concurrent access.
pub struct DaemonState {
    /// Vault store (always available for init/status checks).
    pub vault_store: Option<VaultStore>,

    /// Unlocked vault — only present when vault is unlocked.
    pub vault: Option<UnlockedVault>,

    /// Active SSH sessions, keyed by connection ID.
    pub sessions: HashMap<Uuid, SshSession>,

    /// Daemon startup time for uptime calculation.
    pub started_at: Instant,
}

/// Thread-safe reference to daemon state.
pub type SharedState = Arc<RwLock<DaemonState>>;

impl DaemonState {
    /// Create a new daemon state.
    pub fn new() -> Self {
        Self {
            vault_store: None,
            vault: None,
            sessions: HashMap::new(),
            started_at: Instant::now(),
        }
    }

    /// Whether the vault is currently unlocked.
    pub fn is_vault_unlocked(&self) -> bool {
        self.vault.is_some()
    }

    /// Whether the vault is initialized (directory exists with vault.toml).
    pub fn is_vault_initialized(&self) -> bool {
        self.vault_store.is_some() || self.vault.is_some()
    }

    /// Get uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Get the count of active SSH sessions.
    pub fn active_session_count(&self) -> u32 {
        self.sessions
            .values()
            .filter(|s| !s.is_closed())
            .count() as u32
    }

    /// Get a mutable reference to the unlocked vault, or error.
    pub fn require_vault(&mut self) -> Result<&mut UnlockedVault, shelly_core::error::ShellyError> {
        self.vault
            .as_mut()
            .ok_or(shelly_core::error::ShellyError::VaultLocked)
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_state() {
        let state = DaemonState::new();
        assert!(!state.is_vault_unlocked());
        assert!(!state.is_vault_initialized());
        assert_eq!(state.active_session_count(), 0);
    }

    #[test]
    fn test_uptime() {
        let state = DaemonState::new();
        // Just verify it doesn't panic
        let uptime = state.uptime_secs();
        assert!(uptime < 5); // should be nearly 0
    }
}
