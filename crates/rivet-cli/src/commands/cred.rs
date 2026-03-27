use comfy_table::{presets, Table};
use rivet_core::credential::Credential;
use rivet_core::protocol::*;

use super::{CliError, get_client};

pub async fn list() -> Result<(), CliError> {
    let mut client = get_client().await?;
    let params = CredListParams {};
    let result = client
        .call(
            "cred.list",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let credentials: Vec<Credential> =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    if credentials.is_empty() {
        println!("No credential profiles. Create one with: rivet cred add");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(presets::UTF8_FULL_CONDENSED);
    table.set_header(vec!["Name", "Auth Type", "Description"]);

    for cred in &credentials {
        let auth_type = match &cred.auth {
            rivet_core::connection::AuthMethod::Password(_) => "password",
            rivet_core::connection::AuthMethod::PrivateKey { .. } => "key",
            rivet_core::connection::AuthMethod::KeyFile { .. } => "keyfile",
            rivet_core::connection::AuthMethod::Agent { .. } => "agent",
            rivet_core::connection::AuthMethod::Certificate { .. } => "cert",
            rivet_core::connection::AuthMethod::Interactive => "interactive",
        };

        table.add_row(vec![
            &cred.name,
            auth_type,
            cred.description.as_deref().unwrap_or(""),
        ]);
    }

    println!("{table}");
    Ok(())
}

pub async fn show(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("credential name".into()))?;

    let mut client = get_client().await?;
    let params = CredGetParams {
        id: None,
        name: Some(name.clone()),
    };
    let result = client
        .call("cred.get", Some(serde_json::to_value(&params).unwrap()))
        .await
        .map_err(CliError::Client)?;

    let cred: Credential =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!("Name:        {}", cred.name);
    println!("ID:          {}", cred.id);
    println!("Auth:        {:?}", cred.auth);
    if let Some(ref desc) = cred.description {
        println!("Description: {desc}");
    }
    println!("Created:     {}", cred.created_at);
    println!("Updated:     {}", cred.updated_at);

    // Show which connections use this credential
    let usage_params = CredUsageParams {
        id: Some(cred.id),
        name: None,
    };
    let usage_result = client
        .call(
            "cred.usage",
            Some(serde_json::to_value(&usage_params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let usage: CredUsageResult =
        serde_json::from_value(usage_result).map_err(|e| CliError::Other(e.to_string()))?;

    if usage.connections.is_empty() {
        println!("Used by:     (none)");
    } else {
        let names: Vec<&str> = usage.connections.iter().map(|c| c.name.as_str()).collect();
        println!("Used by:     {}", names.join(", "));
    }

    Ok(())
}

pub async fn add() -> Result<(), CliError> {
    let name = prompt("Name: ")?;
    let description_str = prompt("Description (empty for none): ")?;
    let description = if description_str.is_empty() {
        None
    } else {
        Some(description_str)
    };

    println!("Auth method:");
    println!("  1) SSH Agent (default)");
    println!("  2) Password");
    println!("  3) Key file");
    let auth_choice = prompt("Choice [1]: ")?;

    let auth = match auth_choice.as_str() {
        "2" => {
            let password = rpassword::prompt_password("Password: ")
                .map_err(|e| CliError::Other(e.to_string()))?;
            rivet_core::connection::AuthMethod::Password(password)
        }
        "3" => {
            let path = prompt("Key file path: ")?;
            let passphrase_str = rpassword::prompt_password("Key passphrase (empty for none): ")
                .map_err(|e| CliError::Other(e.to_string()))?;
            let passphrase = if passphrase_str.is_empty() {
                None
            } else {
                Some(passphrase_str)
            };
            rivet_core::connection::AuthMethod::KeyFile {
                path: path.into(),
                passphrase,
            }
        }
        _ => {
            let socket_str = prompt("Agent socket path (empty for default SSH_AUTH_SOCK): ")?;
            let socket_path = if socket_str.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(socket_str))
            };
            rivet_core::connection::AuthMethod::Agent { socket_path }
        }
    };

    let params = CredCreateParams {
        name,
        auth,
        description,
    };

    let mut client = get_client().await?;
    let result = client
        .call(
            "cred.create",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let id_result: IdResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;
    println!("Credential created: {}", id_result.id);
    Ok(())
}

pub async fn edit(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("credential name".into()))?;

    let mut client = get_client().await?;
    let get_params = CredGetParams {
        id: None,
        name: Some(name.clone()),
    };
    let result = client
        .call(
            "cred.get",
            Some(serde_json::to_value(&get_params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let cred: Credential =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!(
        "Editing '{}' (press Enter to keep current value)",
        cred.name
    );

    let new_name = prompt_default("Name", &cred.name)?;
    let current_desc = cred.description.as_deref().unwrap_or("");
    let new_desc = prompt_default("Description", current_desc)?;

    let update = CredUpdateParams {
        id: cred.id,
        name: if new_name != cred.name {
            Some(new_name)
        } else {
            None
        },
        auth: None,
        description: if new_desc != current_desc {
            if new_desc.is_empty() {
                Some(None)
            } else {
                Some(Some(new_desc))
            }
        } else {
            None
        },
    };

    client
        .call(
            "cred.update",
            Some(serde_json::to_value(&update).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    println!("Credential updated.");
    Ok(())
}

pub async fn rm(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("credential name".into()))?;

    let mut client = get_client().await?;
    let params = CredDeleteParams {
        id: None,
        name: Some(name.clone()),
        force: None,
    };
    client
        .call(
            "cred.delete",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    println!("Credential '{}' deleted.", name);
    Ok(())
}

fn prompt(msg: &str) -> Result<String, CliError> {
    use std::io::Write;
    print!("{msg}");
    std::io::stdout()
        .flush()
        .map_err(|e| CliError::Other(e.to_string()))?;
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| CliError::Other(e.to_string()))?;
    Ok(input.trim().to_string())
}

fn prompt_default(label: &str, default: &str) -> Result<String, CliError> {
    let input = prompt(&format!("{label} [{default}]: "))?;
    if input.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(input)
    }
}
