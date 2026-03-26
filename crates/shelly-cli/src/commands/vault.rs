use shelly_core::protocol::{VaultChangePasswordParams, VaultInitParams, VaultStatusResult, VaultUnlockParams};

use super::{CliError, get_client};

pub async fn init() -> Result<(), CliError> {
    let password = prompt_new_password("New vault password")?;

    let mut client = get_client().await?;
    let params = VaultInitParams { password };
    client
        .call(
            "vault.init",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    println!("Vault initialized and unlocked.");
    Ok(())
}

pub async fn unlock() -> Result<(), CliError> {
    let password = prompt_password("Vault password")?;

    let mut client = get_client().await?;
    let params = VaultUnlockParams { password };
    client
        .call(
            "vault.unlock",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    println!("Vault unlocked.");
    Ok(())
}

pub async fn lock() -> Result<(), CliError> {
    let mut client = get_client().await?;
    client
        .call("vault.lock", None)
        .await
        .map_err(CliError::Client)?;

    println!("Vault locked.");
    Ok(())
}

pub async fn status() -> Result<(), CliError> {
    let mut client = get_client().await?;
    let result = client
        .call("vault.status", None)
        .await
        .map_err(CliError::Client)?;

    let status: VaultStatusResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!(
        "Initialized: {}",
        if status.initialized { "yes" } else { "no" }
    );
    println!(
        "Status:      {}",
        if status.locked { "locked" } else { "unlocked" }
    );
    Ok(())
}

pub async fn change_password() -> Result<(), CliError> {
    let old_password = prompt_password("Current password")?;
    let new_password = prompt_new_password("New password")?;

    let mut client = get_client().await?;
    let params = VaultChangePasswordParams {
        old_password,
        new_password,
    };
    client
        .call(
            "vault.change_password",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    println!("Password changed.");
    Ok(())
}

fn prompt_password(prompt: &str) -> Result<String, CliError> {
    rpassword::prompt_password(format!("{prompt}: "))
        .map_err(|e| CliError::Other(format!("failed to read password: {e}")))
}

fn prompt_new_password(prompt: &str) -> Result<String, CliError> {
    let p1 = prompt_password(prompt)?;
    let p2 = prompt_password("Confirm password")?;
    if p1 != p2 {
        return Err(CliError::Other("passwords do not match".into()));
    }
    if p1.is_empty() {
        return Err(CliError::Other("password cannot be empty".into()));
    }
    Ok(p1)
}
