use comfy_table::{presets, Table};
use rivet_core::connection::{Connection, Group};
use rivet_core::protocol::*;

use super::{CliError, get_client};

pub async fn list() -> Result<(), CliError> {
    let mut client = get_client().await?;

    let result = client
        .call("group.list", None)
        .await
        .map_err(CliError::Client)?;

    let groups: Vec<Group> =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    if groups.is_empty() {
        println!("No groups. Create one with: rivet group add");
        return Ok(());
    }

    // Get connections to count members per group
    let conn_result = client
        .call(
            "conn.list",
            Some(serde_json::to_value(&ConnListParams { tag: None, group_id: None }).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let connections: Vec<Connection> =
        serde_json::from_value(conn_result).map_err(|e| CliError::Other(e.to_string()))?;

    let mut table = Table::new();
    table.load_preset(presets::UTF8_FULL_CONDENSED);
    table.set_header(vec!["Name", "Description", "Color", "Connections"]);

    for group in &groups {
        let member_count = connections
            .iter()
            .filter(|c| c.group_ids.contains(&group.id))
            .count();

        table.add_row(vec![
            &group.name,
            group.description.as_deref().unwrap_or(""),
            group.color.as_deref().unwrap_or(""),
            &member_count.to_string(),
        ]);
    }

    println!("{table}");
    Ok(())
}

pub async fn show(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("group name".into()))?;

    let mut client = get_client().await?;

    let params = GroupGetParams {
        id: None,
        name: Some(name.clone()),
    };
    let result = client
        .call(
            "group.get",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let group: Group =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!("Name:        {}", group.name);
    println!("ID:          {}", group.id);
    if let Some(ref desc) = group.description {
        println!("Description: {desc}");
    }
    if let Some(ref color) = group.color {
        println!("Color:       {color}");
    }

    // List member connections
    let conn_result = client
        .call(
            "conn.list",
            Some(
                serde_json::to_value(&ConnListParams {
                    tag: None,
                    group_id: Some(group.id),
                })
                .unwrap(),
            ),
        )
        .await
        .map_err(CliError::Client)?;

    let connections: Vec<Connection> =
        serde_json::from_value(conn_result).map_err(|e| CliError::Other(e.to_string()))?;

    if connections.is_empty() {
        println!("Members:     (none)");
    } else {
        println!("Members:");
        for conn in &connections {
            println!("  - {} ({}@{}:{})", conn.name, conn.username, conn.host, conn.port);
        }
    }

    Ok(())
}

pub async fn add() -> Result<(), CliError> {
    let name = prompt("Name: ")?;
    let description_str = prompt("Description (empty for none): ")?;
    let color_str = prompt("Color (empty for none): ")?;

    let params = GroupCreateParams {
        name,
        description: if description_str.is_empty() {
            None
        } else {
            Some(description_str)
        },
        color: if color_str.is_empty() {
            None
        } else {
            Some(color_str)
        },
    };

    let mut client = get_client().await?;
    let result = client
        .call(
            "group.create",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let id_result: IdResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;
    println!("Group created: {}", id_result.id);
    Ok(())
}

pub async fn edit(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("group name".into()))?;

    // Get existing group
    let mut client = get_client().await?;
    let get_params = GroupGetParams {
        id: None,
        name: Some(name.clone()),
    };
    let result = client
        .call(
            "group.get",
            Some(serde_json::to_value(&get_params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let group: Group =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!("Editing '{}' (press Enter to keep current value)", group.name);

    let new_name = prompt_default("Name", &group.name)?;
    let new_desc = prompt_default(
        "Description",
        group.description.as_deref().unwrap_or(""),
    )?;
    let new_color = prompt_default(
        "Color",
        group.color.as_deref().unwrap_or(""),
    )?;

    let update = GroupUpdateParams {
        id: group.id,
        name: if new_name != group.name {
            Some(new_name)
        } else {
            None
        },
        description: if new_desc != group.description.as_deref().unwrap_or("") {
            Some(if new_desc.is_empty() {
                None
            } else {
                Some(new_desc)
            })
        } else {
            None
        },
        color: if new_color != group.color.as_deref().unwrap_or("") {
            Some(if new_color.is_empty() {
                None
            } else {
                Some(new_color)
            })
        } else {
            None
        },
    };

    client
        .call(
            "group.update",
            Some(serde_json::to_value(&update).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    println!("Group updated.");
    Ok(())
}

pub async fn rm(args: &[String]) -> Result<(), CliError> {
    let name = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("group name".into()))?;

    let mut client = get_client().await?;
    let params = GroupDeleteParams {
        id: None,
        name: Some(name.clone()),
    };
    client
        .call(
            "group.delete",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    println!("Group '{}' deleted.", name);
    Ok(())
}

pub async fn exec(args: &[String]) -> Result<(), CliError> {
    // Usage: rivet group exec <group-name> <command...>
    if args.is_empty() {
        return Err(CliError::MissingArgument("group name".into()));
    }

    let group_name = &args[0];
    let command = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        return Err(CliError::MissingArgument("command".into()));
    };

    let params = GroupExecParams {
        group_id: None,
        group_name: Some(group_name.clone()),
        command,
        concurrency: None,
    };

    let mut client = get_client().await?;
    let result = client
        .call(
            "group.exec",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let exec_result: GroupExecResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    let mut ok_count = 0;
    let mut fail_count = 0;

    for r in &exec_result.results {
        if let Some(ref err) = r.error {
            println!("[{}] ERROR: {}", r.connection_name, err);
            fail_count += 1;
        } else {
            if !r.stdout.is_empty() {
                for line in r.stdout.lines() {
                    println!("[{}] {}", r.connection_name, line);
                }
            }
            if !r.stderr.is_empty() {
                for line in r.stderr.lines() {
                    eprintln!("[{}] stderr: {}", r.connection_name, line);
                }
            }
            if r.exit_code != 0 {
                println!("[{}] exit code: {}", r.connection_name, r.exit_code);
                fail_count += 1;
            } else {
                ok_count += 1;
            }
        }
    }

    println!(
        "---\n{} succeeded, {} failed ({} total)",
        ok_count,
        fail_count,
        exec_result.results.len()
    );

    Ok(())
}

pub async fn upload(args: &[String]) -> Result<(), CliError> {
    // Usage: rivet group upload <group-name> <local-path> <remote-path>
    if args.len() < 3 {
        return Err(CliError::MissingArgument(
            "group name, local path, remote path".into(),
        ));
    }

    let group_name = &args[0];
    let local_path = &args[1];
    let remote_path = &args[2];

    let params = GroupUploadParams {
        group_id: None,
        group_name: Some(group_name.clone()),
        local_path: local_path.clone(),
        remote_path: remote_path.clone(),
        concurrency: None,
    };

    let mut client = get_client().await?;
    let result = client
        .call(
            "group.upload",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let upload_result: GroupUploadResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    let mut ok_count = 0;
    let mut fail_count = 0;

    for r in &upload_result.results {
        if let Some(ref err) = r.error {
            println!("[{}] ERROR: {}", r.connection_name, err);
            fail_count += 1;
        } else {
            println!(
                "[{}] {} bytes transferred",
                r.connection_name, r.bytes_transferred
            );
            ok_count += 1;
        }
    }

    println!(
        "---\n{} succeeded, {} failed ({} total)",
        ok_count,
        fail_count,
        upload_result.results.len()
    );

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
