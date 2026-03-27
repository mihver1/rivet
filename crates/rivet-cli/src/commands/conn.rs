use comfy_table::{presets, Table};
use rivet_core::connection::Connection;
use rivet_core::protocol::*;

use super::{CliError, get_client};

pub async fn list() -> Result<(), CliError> {
    let mut client = get_client().await?;
    let params = ConnListParams {
        tag: None,
        group_id: None,
    };
    let result = client
        .call("conn.list", Some(serde_json::to_value(&params).unwrap()))
        .await
        .map_err(CliError::Client)?;

    let connections: Vec<Connection> =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    if connections.is_empty() {
        println!("No connections. Add one with: rivet conn add");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(presets::UTF8_FULL_CONDENSED);
    table.set_header(vec!["Name", "Host", "Port", "User", "Auth", "Tags"]);

    for conn in &connections {
        let auth_type = match &conn.auth {
            rivet_core::credential::AuthSource::Inline(method) => match method {
                rivet_core::connection::AuthMethod::Password(_) => "password",
                rivet_core::connection::AuthMethod::PrivateKey { .. } => "key",
                rivet_core::connection::AuthMethod::KeyFile { .. } => "keyfile",
                rivet_core::connection::AuthMethod::Agent { .. } => "agent",
                rivet_core::connection::AuthMethod::Certificate { .. } => "cert",
                rivet_core::connection::AuthMethod::Interactive => "interactive",
            },
            rivet_core::credential::AuthSource::Profile { .. } => "profile",
        };

        table.add_row(vec![
            &conn.name,
            &conn.host,
            &conn.port.to_string(),
            &conn.username,
            auth_type,
            &conn.tags.join(", "),
        ]);
    }

    println!("{table}");
    Ok(())
}

pub async fn show(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("connection name".into()))?;

    let mut client = get_client().await?;
    let params = ConnGetParams {
        id: None,
        name: Some(name.clone()),
    };
    let result = client
        .call("conn.get", Some(serde_json::to_value(&params).unwrap()))
        .await
        .map_err(CliError::Client)?;

    let conn: Connection =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!("Name:     {}", conn.name);
    println!("ID:       {}", conn.id);
    println!("Host:     {}", conn.host);
    println!("Port:     {}", conn.port);
    println!("User:     {}", conn.username);
    println!("Auth:     {:?}", conn.auth);
    if !conn.tags.is_empty() {
        println!("Tags:     {}", conn.tags.join(", "));
    }
    if let Some(ref notes) = conn.notes {
        println!("Notes:    {notes}");
    }
    println!("Created:  {}", conn.created_at);
    println!("Updated:  {}", conn.updated_at);

    Ok(())
}

pub async fn add() -> Result<(), CliError> {
    // Interactive connection creation
    let name = prompt("Name: ")?;
    let host = prompt("Host: ")?;
    let username = prompt("Username: ")?;
    let port_str = prompt("Port [22]: ")?;
    let port = if port_str.is_empty() {
        None
    } else {
        Some(
            port_str
                .parse::<u16>()
                .map_err(|_| CliError::Other("invalid port number".into()))?,
        )
    };

    println!("Auth method:");
    println!("  1) Use credential profile");
    println!("  2) SSH Agent (inline, default)");
    println!("  3) Password (inline)");
    println!("  4) Key file (inline)");
    let auth_choice = prompt("Choice [2]: ")?;

    let auth = match auth_choice.as_str() {
        "1" => {
            // List credential profiles and let user select
            let mut cred_client = get_client().await?;
            let cred_list_params = CredListParams {};
            let cred_result = cred_client
                .call(
                    "cred.list",
                    Some(serde_json::to_value(&cred_list_params).unwrap()),
                )
                .await
                .map_err(CliError::Client)?;

            let credentials: Vec<rivet_core::credential::Credential> =
                serde_json::from_value(cred_result)
                    .map_err(|e| CliError::Other(e.to_string()))?;

            if credentials.is_empty() {
                return Err(CliError::Other(
                    "No credential profiles found. Create one with: rivet cred add".into(),
                ));
            }

            println!("Available credential profiles:");
            for (i, cred) in credentials.iter().enumerate() {
                println!("  {}) {}", i + 1, cred.name);
            }

            let selection_str = prompt("Select profile: ")?;
            let selection: usize = selection_str
                .parse()
                .map_err(|_| CliError::Other("invalid selection".into()))?;

            if selection == 0 || selection > credentials.len() {
                return Err(CliError::Other("selection out of range".into()));
            }

            let selected = &credentials[selection - 1];
            rivet_core::credential::AuthSource::Profile {
                credential_id: selected.id,
            }
        }
        "3" => {
            let password = rpassword::prompt_password("Password: ")
                .map_err(|e| CliError::Other(e.to_string()))?;
            rivet_core::credential::AuthSource::Inline(
                rivet_core::connection::AuthMethod::Password(password),
            )
        }
        "4" => {
            let path = prompt("Key file path: ")?;
            let passphrase_str = rpassword::prompt_password("Key passphrase (empty for none): ")
                .map_err(|e| CliError::Other(e.to_string()))?;
            let passphrase = if passphrase_str.is_empty() {
                None
            } else {
                Some(passphrase_str)
            };
            rivet_core::credential::AuthSource::Inline(
                rivet_core::connection::AuthMethod::KeyFile {
                    path: path.into(),
                    passphrase,
                },
            )
        }
        _ => {
            let socket_str = prompt("Agent socket path (empty for default SSH_AUTH_SOCK): ")?;
            let socket_path = if socket_str.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(socket_str))
            };
            rivet_core::credential::AuthSource::Inline(
                rivet_core::connection::AuthMethod::Agent { socket_path },
            )
        }
    };

    let tags_str = prompt("Tags (comma-separated, empty for none): ")?;
    let tags = if tags_str.is_empty() {
        None
    } else {
        Some(tags_str.split(',').map(|s| s.trim().to_string()).collect())
    };

    let params = ConnCreateParams {
        name,
        host,
        port,
        username,
        auth,
        tags,
        group_ids: None,
        jump_host: None,
        options: None,
        notes: None,
    };

    let mut client = get_client().await?;
    let result = client
        .call(
            "conn.create",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let id_result: IdResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;
    println!("Connection created: {}", id_result.id);
    Ok(())
}

pub async fn edit(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("connection name".into()))?;

    // Get existing connection
    let mut client = get_client().await?;
    let get_params = ConnGetParams {
        id: None,
        name: Some(name.clone()),
    };
    let result = client
        .call(
            "conn.get",
            Some(serde_json::to_value(&get_params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let conn: Connection =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!("Editing '{}' (press Enter to keep current value)", conn.name);

    let new_name = prompt_default("Name", &conn.name)?;
    let new_host = prompt_default("Host", &conn.host)?;
    let new_port = prompt_default("Port", &conn.port.to_string())?;
    let new_username = prompt_default("Username", &conn.username)?;

    let update = ConnUpdateParams {
        id: conn.id,
        name: if new_name != conn.name {
            Some(new_name)
        } else {
            None
        },
        host: if new_host != conn.host {
            Some(new_host)
        } else {
            None
        },
        port: {
            let p = new_port
                .parse::<u16>()
                .map_err(|_| CliError::Other("invalid port".into()))?;
            if p != conn.port {
                Some(p)
            } else {
                None
            }
        },
        username: if new_username != conn.username {
            Some(new_username)
        } else {
            None
        },
        auth: None,
        tags: None,
        group_ids: None,
        jump_host: None,
        options: None,
        notes: None,
    };

    client
        .call(
            "conn.update",
            Some(serde_json::to_value(&update).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    println!("Connection updated.");
    Ok(())
}

pub async fn rm(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("connection name".into()))?;

    let mut client = get_client().await?;
    let params = ConnDeleteParams {
        id: None,
        name: Some(name.clone()),
    };
    client
        .call(
            "conn.delete",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    println!("Connection '{}' deleted.", name);
    Ok(())
}

pub async fn import(args: &[String]) -> Result<(), CliError> {
    let path = args.first().map(|p| std::path::PathBuf::from(p));

    let mut client = get_client().await?;
    let params = ConnImportParams { path };
    let result = client
        .call(
            "conn.import",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let import_result: ConnImportResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!("Imported {} connections.", import_result.imported);
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
