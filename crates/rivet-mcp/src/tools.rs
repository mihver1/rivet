use serde::Serialize;
use serde_json::{Value, json};

use crate::client::DaemonClient;

/// Tool definition for MCP tools/list response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Build all tool definitions.
pub fn all_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "list_connections".into(),
            description: "List all SSH connections managed by Rivet. Returns connection names, hosts, ports, and group memberships. Optionally filter by group name.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "group": {
                        "type": "string",
                        "description": "Optional group name to filter connections"
                    }
                },
                "required": []
            }),
        },
        ToolDefinition {
            name: "show_connection".into(),
            description: "Show detailed information about a specific SSH connection, including host, port, username, authentication method, and SSH options.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Connection name"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolDefinition {
            name: "exec_command".into(),
            description: "Execute a shell command on a remote SSH connection. Returns stdout, stderr, and exit code. Use this to run commands on remote servers.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection": {
                        "type": "string",
                        "description": "Connection name to execute on"
                    },
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute"
                    }
                },
                "required": ["connection", "command"]
            }),
        },
        ToolDefinition {
            name: "group_exec".into(),
            description: "Execute a shell command on all SSH connections in a group simultaneously. Returns per-host results with stdout, stderr, and exit codes.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "group": {
                        "type": "string",
                        "description": "Group name"
                    },
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute on all group members"
                    }
                },
                "required": ["group", "command"]
            }),
        },
        ToolDefinition {
            name: "upload_file".into(),
            description: "Upload a local file to a remote SSH connection via SFTP. Returns the number of bytes transferred.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection": {
                        "type": "string",
                        "description": "Connection name"
                    },
                    "local_path": {
                        "type": "string",
                        "description": "Path to the local file"
                    },
                    "remote_path": {
                        "type": "string",
                        "description": "Destination path on the remote server"
                    }
                },
                "required": ["connection", "local_path", "remote_path"]
            }),
        },
        ToolDefinition {
            name: "download_file".into(),
            description: "Download a file from a remote SSH connection via SFTP. Returns the number of bytes transferred.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection": {
                        "type": "string",
                        "description": "Connection name"
                    },
                    "remote_path": {
                        "type": "string",
                        "description": "Path on the remote server"
                    },
                    "local_path": {
                        "type": "string",
                        "description": "Destination path on the local machine"
                    }
                },
                "required": ["connection", "remote_path", "local_path"]
            }),
        },
        ToolDefinition {
            name: "list_tunnels".into(),
            description: "List all active SSH tunnels. Shows tunnel type (local/remote/dynamic), ports, and associated connection.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "create_tunnel".into(),
            description: "Create an SSH tunnel. Supports local (-L), remote (-R), and dynamic SOCKS5 (-D) tunnels.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "connection": {
                        "type": "string",
                        "description": "Connection name to tunnel through"
                    },
                    "tunnel_type": {
                        "type": "string",
                        "enum": ["local", "remote", "dynamic"],
                        "description": "Type of tunnel: local (forward local port to remote), remote (forward remote port to local), dynamic (SOCKS5 proxy)"
                    },
                    "local_port": {
                        "type": "integer",
                        "description": "Local port to bind (required for all tunnel types)"
                    },
                    "remote_host": {
                        "type": "string",
                        "description": "Remote host to connect to (required for local and remote tunnels, default: localhost)"
                    },
                    "remote_port": {
                        "type": "integer",
                        "description": "Remote port to connect to (required for local and remote tunnels)"
                    }
                },
                "required": ["connection", "tunnel_type", "local_port"]
            }),
        },
        ToolDefinition {
            name: "list_workflows".into(),
            description: "List all saved workflows. Workflows are YAML-defined automation sequences (exec, upload, download steps) that can be run on connections or groups.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "run_workflow".into(),
            description: "Run a saved workflow on a connection or group. Workflows execute a sequence of steps (exec commands, upload/download files) with variable substitution and error handling.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "workflow": {
                        "type": "string",
                        "description": "Workflow name"
                    },
                    "connection": {
                        "type": "string",
                        "description": "Connection name (use either connection or group)"
                    },
                    "group": {
                        "type": "string",
                        "description": "Group name to run on all members (use either connection or group)"
                    },
                    "variables": {
                        "type": "object",
                        "description": "Variable overrides (key-value pairs to override workflow defaults)",
                        "additionalProperties": { "type": "string" }
                    }
                },
                "required": ["workflow"]
            }),
        },
        ToolDefinition {
            name: "list_credentials".into(),
            description: "List all saved credential profiles. Credentials are reusable authentication configurations (SSH keys, passwords, agents) that can be shared across multiple connections.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        },
        ToolDefinition {
            name: "show_credential".into(),
            description: "Show detailed information about a specific credential profile, including its authentication type, description, and which connections use it.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Credential profile name"
                    }
                },
                "required": ["name"]
            }),
        },
    ]
}

/// Dispatch a tool call to the appropriate handler.
pub async fn call_tool(
    name: &str,
    arguments: &Value,
) -> Result<Value, String> {
    // Ensure daemon is running
    crate::client::ensure_daemon_running()
        .await
        .map_err(|e| e.to_string())?;

    let mut client = DaemonClient::connect()
        .await
        .map_err(|e| e.to_string())?;

    match name {
        "list_connections" => handle_list_connections(&mut client, arguments).await,
        "show_connection" => handle_show_connection(&mut client, arguments).await,
        "exec_command" => handle_exec_command(&mut client, arguments).await,
        "group_exec" => handle_group_exec(&mut client, arguments).await,
        "upload_file" => handle_upload_file(&mut client, arguments).await,
        "download_file" => handle_download_file(&mut client, arguments).await,
        "list_tunnels" => handle_list_tunnels(&mut client, arguments).await,
        "create_tunnel" => handle_create_tunnel(&mut client, arguments).await,
        "list_workflows" => handle_list_workflows(&mut client, arguments).await,
        "run_workflow" => handle_run_workflow(&mut client, arguments).await,
        "list_credentials" => handle_list_credentials(&mut client, arguments).await,
        "show_credential" => handle_show_credential(&mut client, arguments).await,
        _ => Err(format!("Unknown tool: {name}")),
    }
}

async fn handle_list_connections(
    client: &mut DaemonClient,
    args: &Value,
) -> Result<Value, String> {
    let group_name = args.get("group").and_then(|v| v.as_str());

    // If group filter specified, resolve group ID first
    let group_id = if let Some(name) = group_name {
        let params = json!({ "name": name });
        let group = client
            .call("group.get", Some(params))
            .await
            .map_err(|e| e.to_string())?;
        group.get("id").and_then(|v| v.as_str()).map(String::from)
    } else {
        None
    };

    let params = json!({
        "tag": null,
        "group_id": group_id
    });
    let result = client
        .call("conn.list", Some(params))
        .await
        .map_err(|e| e.to_string())?;

    // Format connections for LLM readability
    let connections = result
        .as_array()
        .ok_or("unexpected response format")?;

    let mut lines = Vec::new();
    if connections.is_empty() {
        lines.push("No connections found.".to_string());
    } else {
        lines.push(format!("Found {} connection(s):", connections.len()));
        lines.push(String::new());
        for conn in connections {
            let name = conn.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let host = conn.get("host").and_then(|v| v.as_str()).unwrap_or("?");
            let port = conn.get("port").and_then(|v| v.as_u64()).unwrap_or(22);
            let user = conn.get("username").and_then(|v| v.as_str()).unwrap_or("?");
            lines.push(format!("- {name} ({user}@{host}:{port})"));
        }
    }

    Ok(json!({ "text": lines.join("\n"), "connections": result }))
}

async fn handle_show_connection(
    client: &mut DaemonClient,
    args: &Value,
) -> Result<Value, String> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: name")?;

    let params = json!({ "name": name });
    let conn = client
        .call("conn.get", Some(params))
        .await
        .map_err(|e| e.to_string())?;

    let host = conn.get("host").and_then(|v| v.as_str()).unwrap_or("?");
    let port = conn.get("port").and_then(|v| v.as_u64()).unwrap_or(22);
    let user = conn.get("username").and_then(|v| v.as_str()).unwrap_or("?");
    let auth = conn.get("auth_method").unwrap_or(&json!(null));

    let auth_type = auth.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");

    let mut lines = vec![
        format!("Connection: {name}"),
        format!("Host: {host}"),
        format!("Port: {port}"),
        format!("Username: {user}"),
        format!("Auth: {auth_type}"),
    ];

    if let Some(tags) = conn.get("tags").and_then(|v| v.as_array()) {
        if !tags.is_empty() {
            let tag_list: Vec<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
            lines.push(format!("Tags: {}", tag_list.join(", ")));
        }
    }

    Ok(json!({ "text": lines.join("\n"), "connection": conn }))
}

async fn handle_exec_command(
    client: &mut DaemonClient,
    args: &Value,
) -> Result<Value, String> {
    let connection = args
        .get("connection")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: connection")?;

    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: command")?;

    // Resolve connection name to ID
    let conn_params = json!({ "name": connection });
    let conn = client
        .call("conn.get", Some(conn_params))
        .await
        .map_err(|e| e.to_string())?;

    let conn_id = conn
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("could not resolve connection ID")?;

    let params = json!({
        "connection_id": conn_id,
        "command": command
    });

    let result = client
        .call("ssh.exec", Some(params))
        .await
        .map_err(|e| e.to_string())?;

    let stdout = result.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
    let stderr = result.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
    let exit_code = result.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-1);

    let mut text = String::new();
    if !stdout.is_empty() {
        text.push_str(stdout);
    }
    if !stderr.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&format!("[stderr] {stderr}"));
    }
    text.push_str(&format!("\n[exit code: {exit_code}]"));

    Ok(json!({
        "text": text.trim(),
        "stdout": stdout,
        "stderr": stderr,
        "exit_code": exit_code
    }))
}

async fn handle_group_exec(
    client: &mut DaemonClient,
    args: &Value,
) -> Result<Value, String> {
    let group = args
        .get("group")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: group")?;

    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: command")?;

    let params = json!({
        "group_name": group,
        "command": command
    });

    let result = client
        .call("group.exec", Some(params))
        .await
        .map_err(|e| e.to_string())?;

    let results = result
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or("unexpected response format")?;

    let mut lines = Vec::new();
    let mut ok = 0u32;
    let mut fail = 0u32;

    for r in results {
        let host = r.get("connection_name").and_then(|v| v.as_str()).unwrap_or("?");

        if let Some(err) = r.get("error").and_then(|v| v.as_str()) {
            lines.push(format!("[{host}] ERROR: {err}"));
            fail += 1;
        } else {
            let stdout = r.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
            let stderr = r.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
            let exit_code = r.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-1);

            if !stdout.is_empty() {
                for line in stdout.lines() {
                    lines.push(format!("[{host}] {line}"));
                }
            }
            if !stderr.is_empty() {
                for line in stderr.lines() {
                    lines.push(format!("[{host}] stderr: {line}"));
                }
            }

            if exit_code != 0 {
                lines.push(format!("[{host}] exit code: {exit_code}"));
                fail += 1;
            } else {
                ok += 1;
            }
        }
    }

    let total = results.len();
    lines.push(format!("---\n{ok} succeeded, {fail} failed ({total} total)"));

    Ok(json!({
        "text": lines.join("\n"),
        "results": result.get("results")
    }))
}

async fn handle_upload_file(
    client: &mut DaemonClient,
    args: &Value,
) -> Result<Value, String> {
    let connection = args
        .get("connection")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: connection")?;

    let local_path = args
        .get("local_path")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: local_path")?;

    let remote_path = args
        .get("remote_path")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: remote_path")?;

    // Resolve connection ID
    let conn_params = json!({ "name": connection });
    let conn = client
        .call("conn.get", Some(conn_params))
        .await
        .map_err(|e| e.to_string())?;

    let conn_id = conn
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("could not resolve connection ID")?;

    let params = json!({
        "connection_id": conn_id,
        "local_path": local_path,
        "remote_path": remote_path
    });

    let result = client
        .call("scp.upload", Some(params))
        .await
        .map_err(|e| e.to_string())?;

    let bytes = result
        .get("bytes_transferred")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    Ok(json!({
        "text": format!("Uploaded {local_path} to {connection}:{remote_path} ({bytes} bytes)"),
        "bytes_transferred": bytes
    }))
}

async fn handle_download_file(
    client: &mut DaemonClient,
    args: &Value,
) -> Result<Value, String> {
    let connection = args
        .get("connection")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: connection")?;

    let remote_path = args
        .get("remote_path")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: remote_path")?;

    let local_path = args
        .get("local_path")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: local_path")?;

    // Resolve connection ID
    let conn_params = json!({ "name": connection });
    let conn = client
        .call("conn.get", Some(conn_params))
        .await
        .map_err(|e| e.to_string())?;

    let conn_id = conn
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("could not resolve connection ID")?;

    let params = json!({
        "connection_id": conn_id,
        "remote_path": remote_path,
        "local_path": local_path
    });

    let result = client
        .call("scp.download", Some(params))
        .await
        .map_err(|e| e.to_string())?;

    let bytes = result
        .get("bytes_transferred")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    Ok(json!({
        "text": format!("Downloaded {connection}:{remote_path} to {local_path} ({bytes} bytes)"),
        "bytes_transferred": bytes
    }))
}

async fn handle_list_tunnels(
    client: &mut DaemonClient,
    _args: &Value,
) -> Result<Value, String> {
    let result = client
        .call("tunnel.list", None)
        .await
        .map_err(|e| e.to_string())?;

    let tunnels = result
        .as_array()
        .ok_or("unexpected response format")?;

    let mut lines = Vec::new();
    if tunnels.is_empty() {
        lines.push("No active tunnels.".to_string());
    } else {
        lines.push(format!("{} active tunnel(s):", tunnels.len()));
        lines.push(String::new());
        for t in tunnels {
            let id = t.get("id").and_then(|v| v.as_str()).unwrap_or("?");
            let short_id = &id[..8.min(id.len())];
            let conn = t.get("connection_name").and_then(|v| v.as_str()).unwrap_or("?");
            let spec = t.get("spec").and_then(|v| v.as_str()).unwrap_or("?");
            lines.push(format!("- [{short_id}] {conn}: {spec}"));
        }
    }

    Ok(json!({ "text": lines.join("\n"), "tunnels": result }))
}

async fn handle_create_tunnel(
    client: &mut DaemonClient,
    args: &Value,
) -> Result<Value, String> {
    let connection = args
        .get("connection")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: connection")?;

    let tunnel_type = args
        .get("tunnel_type")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: tunnel_type")?;

    let local_port = args
        .get("local_port")
        .and_then(|v| v.as_u64())
        .ok_or("missing required parameter: local_port")? as u16;

    // Resolve connection ID
    let conn_params = json!({ "name": connection });
    let conn = client
        .call("conn.get", Some(conn_params))
        .await
        .map_err(|e| e.to_string())?;

    let conn_id = conn
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("could not resolve connection ID")?;

    // Build tunnel spec based on type
    let spec = match tunnel_type {
        "local" => {
            let remote_host = args
                .get("remote_host")
                .and_then(|v| v.as_str())
                .unwrap_or("localhost");
            let remote_port = args
                .get("remote_port")
                .and_then(|v| v.as_u64())
                .ok_or("local tunnel requires remote_port")? as u16;
            json!({
                "Local": {
                    "local_port": local_port,
                    "remote_host": remote_host,
                    "remote_port": remote_port
                }
            })
        }
        "remote" => {
            let remote_host = args
                .get("remote_host")
                .and_then(|v| v.as_str())
                .unwrap_or("localhost");
            let remote_port = args
                .get("remote_port")
                .and_then(|v| v.as_u64())
                .ok_or("remote tunnel requires remote_port")? as u16;
            json!({
                "Remote": {
                    "remote_port": remote_port,
                    "local_host": remote_host,
                    "local_port": local_port
                }
            })
        }
        "dynamic" => {
            json!({
                "Dynamic": {
                    "local_port": local_port
                }
            })
        }
        other => return Err(format!("invalid tunnel_type: {other} (expected: local, remote, dynamic)")),
    };

    let params = json!({
        "connection_id": conn_id,
        "spec": spec
    });

    let result = client
        .call("tunnel.create", Some(params))
        .await
        .map_err(|e| e.to_string())?;

    let tunnel_id = result
        .get("tunnel_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    let desc = match tunnel_type {
        "local" => {
            let rh = args.get("remote_host").and_then(|v| v.as_str()).unwrap_or("localhost");
            let rp = args.get("remote_port").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("-L {local_port}:{rh}:{rp}")
        }
        "remote" => {
            let rp = args.get("remote_port").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("-R {rp}:localhost:{local_port}")
        }
        "dynamic" => format!("-D {local_port}"),
        _ => "?".to_string(),
    };

    Ok(json!({
        "text": format!("Tunnel created: {desc} via {connection} (id: {tunnel_id})"),
        "tunnel_id": tunnel_id
    }))
}

async fn handle_list_workflows(
    client: &mut DaemonClient,
    _args: &Value,
) -> Result<Value, String> {
    let result = client
        .call("workflow.list", None)
        .await
        .map_err(|e| e.to_string())?;

    let workflows = result
        .as_array()
        .ok_or("unexpected response format")?;

    let mut lines = Vec::new();
    if workflows.is_empty() {
        lines.push("No workflows saved.".to_string());
    } else {
        lines.push(format!("{} workflow(s):", workflows.len()));
        lines.push(String::new());
        for wf in workflows {
            let name = wf.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let desc = wf.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let steps = wf.get("steps").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            lines.push(format!("- {name} ({steps} steps){}", if desc.is_empty() { String::new() } else { format!(" — {desc}") }));
        }
    }

    Ok(json!({ "text": lines.join("\n"), "workflows": result }))
}

async fn handle_run_workflow(
    client: &mut DaemonClient,
    args: &Value,
) -> Result<Value, String> {
    let workflow = args
        .get("workflow")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: workflow")?;

    let connection = args.get("connection").and_then(|v| v.as_str());
    let group = args.get("group").and_then(|v| v.as_str());

    if connection.is_none() && group.is_none() {
        return Err("either 'connection' or 'group' must be specified".into());
    }

    let variables: std::collections::HashMap<String, String> = args
        .get("variables")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let params = json!({
        "workflow_name": workflow,
        "connection_name": connection,
        "group_name": group,
        "variables": variables
    });

    let result = client
        .call("workflow.run", Some(params))
        .await
        .map_err(|e| e.to_string())?;

    let results = result
        .as_array()
        .ok_or("unexpected response format")?;

    let mut lines = Vec::new();
    let mut all_success = true;

    for wf_result in results {
        let conn_name = wf_result.get("connection_name").and_then(|v| v.as_str()).unwrap_or("?");
        let wf_name = wf_result.get("workflow_name").and_then(|v| v.as_str()).unwrap_or("?");
        let success = wf_result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        let completed = wf_result.get("completed_steps").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = wf_result.get("total_steps").and_then(|v| v.as_u64()).unwrap_or(0);
        let failed = wf_result.get("failed_steps").and_then(|v| v.as_u64()).unwrap_or(0);

        if !success {
            all_success = false;
        }

        lines.push(format!("=== {wf_name} on {conn_name} ==="));

        if let Some(steps) = wf_result.get("steps").and_then(|v| v.as_array()) {
            for step in steps {
                let name = step.get("step_name").and_then(|v| v.as_str()).unwrap_or("?");
                let ok = step.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
                let skipped = step.get("skipped").and_then(|v| v.as_bool()).unwrap_or(false);

                let status = if skipped { "SKIP" } else if ok { "OK" } else { "FAIL" };
                let mut line = format!("[{status}] {name}");

                if let Some(stdout) = step.get("stdout").and_then(|v| v.as_str()) {
                    if !stdout.is_empty() {
                        line.push_str(&format!("\n  {}", stdout.trim()));
                    }
                }

                if let Some(err) = step.get("error").and_then(|v| v.as_str()) {
                    line.push_str(&format!("\n  error: {err}"));
                }

                lines.push(line);
            }
        }

        let status_label = if success { "SUCCESS" } else { "FAILED" };
        lines.push(format!("--- {completed}/{total} completed, {failed} failed [{status_label}]\n"));
    }

    Ok(json!({
        "text": lines.join("\n"),
        "success": all_success,
        "results": result
    }))
}

async fn handle_list_credentials(
    client: &mut DaemonClient,
    _args: &Value,
) -> Result<Value, String> {
    let result = client
        .call("cred.list", None)
        .await
        .map_err(|e| e.to_string())?;

    let credentials = result
        .as_array()
        .ok_or("unexpected response format")?;

    let mut lines = Vec::new();
    if credentials.is_empty() {
        lines.push("No credential profiles saved.".to_string());
    } else {
        lines.push(format!("{} credential(s):", credentials.len()));
        lines.push(String::new());
        for cred in credentials {
            let name = cred.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let auth = cred.get("auth").unwrap_or(&json!(null));
            let auth_type = auth.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");
            let desc = cred.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let suffix = if desc.is_empty() { String::new() } else { format!(" — {desc}") };
            lines.push(format!("- {name} ({auth_type}){suffix}"));
        }
    }

    Ok(json!({ "text": lines.join("\n"), "credentials": result }))
}

async fn handle_show_credential(
    client: &mut DaemonClient,
    args: &Value,
) -> Result<Value, String> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("missing required parameter: name")?;

    let params = json!({ "name": name });
    let cred = client
        .call("cred.get", Some(params))
        .await
        .map_err(|e| e.to_string())?;

    let id = cred.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let auth = cred.get("auth").unwrap_or(&json!(null));
    let auth_type = auth.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");
    let created = cred.get("created_at").and_then(|v| v.as_str()).unwrap_or("?");
    let updated = cred.get("updated_at").and_then(|v| v.as_str()).unwrap_or("?");

    let mut lines = vec![
        format!("Credential: {name}"),
        format!("ID: {id}"),
        format!("Auth: {auth_type}"),
    ];

    if let Some(desc) = cred.get("description").and_then(|v| v.as_str()) {
        lines.push(format!("Description: {desc}"));
    }

    lines.push(format!("Created: {created}"));
    lines.push(format!("Updated: {updated}"));

    Ok(json!({ "text": lines.join("\n"), "credential": cred }))
}
