use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use uuid::Uuid;
use zeroize::Zeroize;

use shelly_core::connection::{Connection, Group};
use shelly_core::workflow::Workflow;
use shelly_core::error::{Result, ShellyError};

use crate::crypto::{self, Argon2Params};
use crate::models::VaultConfig;

pub struct VaultStore {
    vault_dir: PathBuf,
}

pub struct UnlockedVault {
    store: VaultStore,
    dek: [u8; 32],
}

impl Drop for UnlockedVault {
    fn drop(&mut self) {
        self.dek.zeroize();
    }
}

impl VaultStore {
    pub fn new(vault_dir: PathBuf) -> Self {
        Self { vault_dir }
    }

    pub fn is_initialized(&self) -> bool {
        self.vault_dir.join("vault.toml").exists() && self.vault_dir.join("master.key").exists()
    }

    pub fn init(&self, password: &str) -> Result<()> {
        if self.is_initialized() {
            return Err(ShellyError::VaultAlreadyInitialized);
        }

        // Create directory structure
        fs::create_dir_all(self.vault_dir.join("connections"))?;
        fs::create_dir_all(self.vault_dir.join("groups"))?;
        fs::create_dir_all(self.vault_dir.join("keys"))?;
        fs::create_dir_all(self.vault_dir.join("workflows"))?;

        // Generate cryptographic materials
        let salt = crypto::generate_salt();
        let mut dek = crypto::generate_dek();

        // Create vault config
        let config = VaultConfig::new(&salt);
        let config_toml =
            toml::to_string_pretty(&config).map_err(|e| ShellyError::SerializationError(e.to_string()))?;
        fs::write(self.vault_dir.join("vault.toml"), config_toml)?;

        // Derive KEK and encrypt DEK
        let params = Argon2Params {
            memory_cost: config.argon2_memory_cost,
            time_cost: config.argon2_time_cost,
            parallelism: config.argon2_parallelism,
            salt,
        };

        let mut kek = crypto::derive_kek(password, &params)?;
        let encrypted_dek = crypto::encrypt_dek(&kek, &dek)?;
        fs::write(self.vault_dir.join("master.key"), &encrypted_dek)?;

        // Zeroize sensitive material
        kek.zeroize();
        dek.zeroize();

        Ok(())
    }

    pub fn unlock(self, password: &str) -> Result<UnlockedVault> {
        if !self.is_initialized() {
            return Err(ShellyError::VaultNotInitialized);
        }

        // Read vault config
        let config_str = fs::read_to_string(self.vault_dir.join("vault.toml"))?;
        let config: VaultConfig =
            toml::from_str(&config_str).map_err(|e| ShellyError::SerializationError(e.to_string()))?;

        // Derive KEK
        let params = Argon2Params {
            memory_cost: config.argon2_memory_cost,
            time_cost: config.argon2_time_cost,
            parallelism: config.argon2_parallelism,
            salt: config.salt_bytes(),
        };

        let mut kek = crypto::derive_kek(password, &params)?;

        // Decrypt DEK
        let encrypted_dek = fs::read(self.vault_dir.join("master.key"))?;
        let dek = crypto::decrypt_dek(&kek, &encrypted_dek)?;

        kek.zeroize();

        Ok(UnlockedVault { store: self, dek })
    }
}

impl UnlockedVault {
    pub fn vault_dir(&self) -> &Path {
        &self.store.vault_dir
    }

    // --- Generic entity CRUD ---

    pub fn save_entity<T: Serialize>(&self, subdir: &str, id: &Uuid, entity: &T) -> Result<()> {
        let json = serde_json::to_vec(entity)?;
        let encrypted = crypto::encrypt_aes_gcm(&self.dek, &json)?;

        let dir = self.store.vault_dir.join(subdir);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(format!("{id}.enc")), &encrypted)?;
        Ok(())
    }

    pub fn load_entity<T: DeserializeOwned>(&self, subdir: &str, id: &Uuid) -> Result<T> {
        let path = self.store.vault_dir.join(subdir).join(format!("{id}.enc"));
        if !path.exists() {
            return Err(ShellyError::ConnectionNotFound(id.to_string()));
        }
        let encrypted = fs::read(&path)?;
        let json = crypto::decrypt_aes_gcm(&self.dek, &encrypted)?;
        let entity = serde_json::from_slice(&json)?;
        Ok(entity)
    }

    pub fn delete_entity(&self, subdir: &str, id: &Uuid) -> Result<()> {
        let path = self.store.vault_dir.join(subdir).join(format!("{id}.enc"));
        if !path.exists() {
            return Err(ShellyError::ConnectionNotFound(id.to_string()));
        }
        fs::remove_file(&path)?;
        Ok(())
    }

    pub fn list_entity_ids(&self, subdir: &str) -> Result<Vec<Uuid>> {
        let dir = self.store.vault_dir.join(subdir);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(uuid_str) = name.strip_suffix(".enc") {
                if let Ok(uuid) = Uuid::parse_str(uuid_str) {
                    ids.push(uuid);
                }
            }
        }
        Ok(ids)
    }

    pub fn list_entities<T: DeserializeOwned>(&self, subdir: &str) -> Result<Vec<T>> {
        let ids = self.list_entity_ids(subdir)?;
        ids.iter().map(|id| self.load_entity(subdir, id)).collect()
    }

    // --- Connection convenience ---

    pub fn save_connection(&self, conn: &Connection) -> Result<()> {
        self.save_entity("connections", &conn.id, conn)
    }

    pub fn load_connection(&self, id: &Uuid) -> Result<Connection> {
        self.load_entity("connections", id)
    }

    pub fn list_connections(&self) -> Result<Vec<Connection>> {
        self.list_entities("connections")
    }

    pub fn delete_connection(&self, id: &Uuid) -> Result<()> {
        self.delete_entity("connections", id)
    }

    pub fn find_connection_by_name(&self, name: &str) -> Result<Connection> {
        let connections = self.list_connections()?;
        connections
            .into_iter()
            .find(|c| c.name == name)
            .ok_or_else(|| ShellyError::ConnectionNotFound(name.into()))
    }

    // --- Group convenience ---

    pub fn save_group(&self, group: &Group) -> Result<()> {
        self.save_entity("groups", &group.id, group)
    }

    pub fn load_group(&self, id: &Uuid) -> Result<Group> {
        self.load_entity("groups", id)
    }

    pub fn list_groups(&self) -> Result<Vec<Group>> {
        self.list_entities("groups")
    }

    pub fn delete_group(&self, id: &Uuid) -> Result<()> {
        self.delete_entity("groups", id)
    }

    pub fn find_group_by_name(&self, name: &str) -> Result<Group> {
        let groups = self.list_groups()?;
        groups
            .into_iter()
            .find(|g| g.name == name)
            .ok_or_else(|| ShellyError::GroupNotFound(name.into()))
    }

    // --- Workflow convenience ---

    pub fn save_workflow(&self, workflow: &Workflow) -> Result<()> {
        self.save_entity("workflows", &workflow.id, workflow)
    }

    pub fn load_workflow(&self, id: &Uuid) -> Result<Workflow> {
        self.load_entity("workflows", id)
    }

    pub fn list_workflows(&self) -> Result<Vec<Workflow>> {
        self.list_entities("workflows")
    }

    pub fn delete_workflow(&self, id: &Uuid) -> Result<()> {
        self.delete_entity("workflows", id)
    }

    pub fn find_workflow_by_name(&self, name: &str) -> Result<Workflow> {
        let workflows = self.list_workflows()?;
        workflows
            .into_iter()
            .find(|w| w.name == name)
            .ok_or_else(|| ShellyError::WorkflowNotFound(name.into()))
    }

    // --- Password change ---

    pub fn change_password(&self, old_password: &str, new_password: &str) -> Result<()> {
        // Verify old password by re-deriving KEK and decrypting DEK
        let config_str = fs::read_to_string(self.store.vault_dir.join("vault.toml"))?;
        let config: VaultConfig =
            toml::from_str(&config_str).map_err(|e| ShellyError::SerializationError(e.to_string()))?;

        let old_params = Argon2Params {
            memory_cost: config.argon2_memory_cost,
            time_cost: config.argon2_time_cost,
            parallelism: config.argon2_parallelism,
            salt: config.salt_bytes(),
        };

        let mut old_kek = crypto::derive_kek(old_password, &old_params)?;
        let encrypted_dek = fs::read(self.store.vault_dir.join("master.key"))?;
        let dek = crypto::decrypt_dek(&old_kek, &encrypted_dek)?;
        old_kek.zeroize();

        // Generate new salt, derive new KEK, re-encrypt DEK
        let new_salt = crypto::generate_salt();
        let new_config = VaultConfig {
            salt: crate::models::VaultConfig::new(&new_salt).salt,
            ..config
        };

        let new_params = Argon2Params {
            memory_cost: new_config.argon2_memory_cost,
            time_cost: new_config.argon2_time_cost,
            parallelism: new_config.argon2_parallelism,
            salt: new_salt,
        };

        let mut new_kek = crypto::derive_kek(new_password, &new_params)?;
        let new_encrypted_dek = crypto::encrypt_dek(&new_kek, &dek)?;
        new_kek.zeroize();

        // Write updated files
        let config_toml =
            toml::to_string_pretty(&new_config).map_err(|e| ShellyError::SerializationError(e.to_string()))?;
        fs::write(self.store.vault_dir.join("vault.toml"), config_toml)?;
        fs::write(self.store.vault_dir.join("master.key"), &new_encrypted_dek)?;

        Ok(())
    }

    /// Consume the vault, zeroizing the DEK. Returns the VaultStore for re-locking.
    pub fn lock(mut self) -> VaultStore {
        self.dek.zeroize();
        let vault_dir = std::mem::take(&mut self.store.vault_dir);
        std::mem::forget(self); // prevent double-zeroize in Drop
        VaultStore::new(vault_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shelly_core::connection::AuthMethod;
    use tempfile::TempDir;

    fn test_vault_dir() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let vault_dir = dir.path().join("vault");
        (dir, vault_dir)
    }

    // Use low-cost argon2 params for tests by setting env
    fn init_fast(store: &VaultStore, password: &str) -> Result<()> {
        // For testing, we init with the default (production) params
        // but the test params are fast because we set low values in VaultConfig
        // Actually, we just use store.init() which uses production params.
        // This is acceptable for tests as argon2 with 64MB is still fast on modern hardware.
        store.init(password)
    }

    #[test]
    fn test_vault_init_and_unlock() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir.clone());

        assert!(!store.is_initialized());
        init_fast(&store, "test-password").unwrap();
        assert!(store.is_initialized());

        // Unlock should succeed
        let vault = store.unlock("test-password").unwrap();
        assert!(vault.vault_dir().exists());
    }

    #[test]
    fn test_vault_wrong_password() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir.clone());

        init_fast(&store, "correct").unwrap();
        let result = store.unlock("wrong");
        assert!(result.is_err());
    }

    #[test]
    fn test_connection_crud() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir);
        init_fast(&store, "pass").unwrap();
        let vault = store.unlock("pass").unwrap();

        // Create
        let conn = Connection::new("test-server", "10.0.1.50", "deploy");
        let conn_id = conn.id;
        vault.save_connection(&conn).unwrap();

        // List
        let connections = vault.list_connections().unwrap();
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].name, "test-server");

        // Load
        let loaded = vault.load_connection(&conn_id).unwrap();
        assert_eq!(loaded.host, "10.0.1.50");

        // Find by name
        let found = vault.find_connection_by_name("test-server").unwrap();
        assert_eq!(found.id, conn_id);

        // Delete
        vault.delete_connection(&conn_id).unwrap();
        let connections = vault.list_connections().unwrap();
        assert!(connections.is_empty());
    }

    #[test]
    fn test_multiple_connections() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir);
        init_fast(&store, "pass").unwrap();
        let vault = store.unlock("pass").unwrap();

        for i in 0..5 {
            let conn = Connection::new(format!("server-{i}"), format!("10.0.1.{i}"), "admin");
            vault.save_connection(&conn).unwrap();
        }

        let connections = vault.list_connections().unwrap();
        assert_eq!(connections.len(), 5);
    }

    #[test]
    fn test_connection_with_auth_methods() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir);
        init_fast(&store, "pass").unwrap();
        let vault = store.unlock("pass").unwrap();

        let mut conn = Connection::new("pw-server", "host", "user");
        conn.auth = AuthMethod::Password("secret123".into());
        vault.save_connection(&conn).unwrap();

        let loaded = vault.load_connection(&conn.id).unwrap();
        match &loaded.auth {
            AuthMethod::Password(pw) => assert_eq!(pw, "secret123"),
            _ => panic!("wrong auth method"),
        }
    }

    #[test]
    fn test_change_password() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir.clone());
        init_fast(&store, "old-pass").unwrap();
        let vault = store.unlock("old-pass").unwrap();

        // Save a connection
        let conn = Connection::new("test", "host", "user");
        let conn_id = conn.id;
        vault.save_connection(&conn).unwrap();

        // Change password
        vault.change_password("old-pass", "new-pass").unwrap();

        // Lock
        let store = vault.lock();

        // Old password should fail
        let store2 = VaultStore::new(vault_dir.clone());
        assert!(store2.unlock("old-pass").is_err());

        // New password should work and data should be intact
        let vault = store.unlock("new-pass").unwrap();
        let loaded = vault.load_connection(&conn_id).unwrap();
        assert_eq!(loaded.name, "test");
    }

    #[test]
    fn test_group_crud() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir);
        init_fast(&store, "pass").unwrap();
        let vault = store.unlock("pass").unwrap();

        let group = Group::new("production");
        vault.save_group(&group).unwrap();

        let groups = vault.list_groups().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "production");

        vault.delete_group(&group.id).unwrap();
        assert!(vault.list_groups().unwrap().is_empty());
    }

    #[test]
    fn test_vault_not_initialized() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir);
        assert!(!store.is_initialized());

        let result = store.unlock("any");
        assert!(matches!(result, Err(ShellyError::VaultNotInitialized)));
    }

    #[test]
    fn test_vault_already_initialized() {
        let (_dir, vault_dir) = test_vault_dir();
        let store = VaultStore::new(vault_dir);
        init_fast(&store, "pass").unwrap();

        let result = init_fast(&store, "pass2");
        assert!(matches!(result, Err(ShellyError::VaultAlreadyInitialized)));
    }
}
