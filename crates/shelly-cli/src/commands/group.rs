use comfy_table::{presets, Table};
use shelly_core::connection::{Connection, Group};
use shelly_core::protocol::*;

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
        println!("No groups. Create one with: shelly group add");
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
