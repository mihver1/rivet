use comfy_table::{presets, Table};
use shelly_core::connection::TunnelSpec;
use shelly_core::protocol::*;

use super::{CliError, get_client};

pub async fn create(args: &[String]) -> Result<(), CliError> {
    // Usage: shelly tunnel create <connection> -L|-R|-D <spec>
    if args.len() < 3 {
        eprintln!("Usage: shelly tunnel create <connection> -L|-R|-D <spec>");
        eprintln!();
        eprintln!("Examples:");
        eprintln!("  shelly tunnel create prod-db -L 5432:localhost:5432");
        eprintln!("  shelly tunnel create gateway -D 1080");
        eprintln!("  shelly tunnel create web -R 8080:localhost:3000");
        return Err(CliError::MissingArgument("connection, flag, spec".into()));
    }

    let connection_name = &args[0];
    let flag = &args[1];
    let spec_str = &args[2];

    let spec = TunnelSpec::parse(flag, spec_str)
        .map_err(|e| CliError::Other(format!("invalid tunnel spec: {e}")))?;

    let params = TunnelCreateParams {
        connection_id: None,
        connection_name: Some(connection_name.clone()),
        spec,
    };

    let mut client = get_client().await?;
    let result = client
        .call(
            "tunnel.create",
            Some(serde_json::to_value(&params).unwrap()),
        )
        .await
        .map_err(CliError::Client)?;

    let create_result: TunnelCreateResult =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    println!("Tunnel created: {}", create_result.id);
    Ok(())
}

pub async fn list() -> Result<(), CliError> {
    let mut client = get_client().await?;
    let result = client
        .call("tunnel.list", None)
        .await
        .map_err(CliError::Client)?;

    let tunnels: Vec<TunnelInfo> =
        serde_json::from_value(result).map_err(|e| CliError::Other(e.to_string()))?;

    if tunnels.is_empty() {
        println!("No active tunnels.");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(presets::UTF8_FULL_CONDENSED);
    table.set_header(vec!["ID", "Connection", "Type", "Spec", "Status"]);

    for t in &tunnels {
        // Show first 8 chars of UUID for readability
        let short_id = &t.id.to_string()[..8];
        table.add_row(vec![
            short_id,
            &t.connection_name,
            t.spec.type_label(),
            &t.spec.to_ssh_arg(),
            if t.active { "active" } else { "dead" },
        ]);
    }

    println!("{table}");
    Ok(())
}

pub async fn close(args: &[String]) -> Result<(), CliError> {
    let id_str = args
        .first()
        .ok_or_else(|| CliError::MissingArgument("tunnel ID".into()))?;

    // Support prefix matching on UUID
    let mut client = get_client().await?;

    // First list tunnels to find by prefix
    let list_result = client
        .call("tunnel.list", None)
        .await
        .map_err(CliError::Client)?;

    let tunnels: Vec<TunnelInfo> =
        serde_json::from_value(list_result).map_err(|e| CliError::Other(e.to_string()))?;

    let matches: Vec<&TunnelInfo> = tunnels
        .iter()
        .filter(|t| t.id.to_string().starts_with(id_str.as_str()))
        .collect();

    match matches.len() {
        0 => {
            return Err(CliError::Other(format!("no tunnel matching '{id_str}'")));
        }
        1 => {
            let tunnel = matches[0];
            let params = TunnelCloseParams { id: tunnel.id };
            client
                .call(
                    "tunnel.close",
                    Some(serde_json::to_value(&params).unwrap()),
                )
                .await
                .map_err(CliError::Client)?;
            println!("Tunnel {} closed.", &tunnel.id.to_string()[..8]);
        }
        n => {
            eprintln!("Ambiguous: {n} tunnels match '{id_str}':");
            for t in matches {
                eprintln!("  {} — {} {}", &t.id.to_string()[..8], t.connection_name, t.spec.to_ssh_arg());
            }
            return Err(CliError::Other("provide more characters to disambiguate".into()));
        }
    }

    Ok(())
}
