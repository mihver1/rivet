/// Shelly MCP Server — exposes SSH management tools to Claude and other LLMs
/// via the Model Context Protocol (stdio transport).
///
/// Usage: shelly-mcp (reads JSON-RPC from stdin, writes to stdout)
mod client;
mod tools;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::debug;

const SERVER_NAME: &str = "shelly";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const PROTOCOL_VERSION: &str = "2024-11-05";

/// JSON-RPC 2.0 request (MCP uses this over stdio).
#[derive(Debug, Deserialize)]
struct McpRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Serialize)]
struct McpResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<McpError>,
}

#[derive(Debug, Serialize)]
struct McpError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl McpResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(McpError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

#[tokio::main]
async fn main() {
    // Set up tracing to stderr (stdout is for MCP protocol)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("shelly_mcp=debug".parse().unwrap()),
        )
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();

    debug!("shelly-mcp starting");

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                debug!("stdin closed, shutting down");
                break;
            }
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                debug!("received: {}", line);

                let request: McpRequest = match serde_json::from_str(line) {
                    Ok(req) => req,
                    Err(e) => {
                        let resp = McpResponse::error(
                            None,
                            -32700,
                            format!("parse error: {e}"),
                        );
                        write_response(&mut stdout, &resp).await;
                        continue;
                    }
                };

                // Notifications (no id) don't get a response
                let is_notification = request.id.is_none();

                let response = handle_request(&request).await;

                if !is_notification {
                    if let Some(resp) = response {
                        write_response(&mut stdout, &resp).await;
                    }
                }
            }
            Err(e) => {
                debug!("stdin error: {e}");
                break;
            }
        }
    }
}

async fn write_response(stdout: &mut tokio::io::Stdout, response: &McpResponse) {
    let mut json = serde_json::to_string(response).unwrap();
    json.push('\n');
    debug!("sending: {}", json.trim());
    let _ = stdout.write_all(json.as_bytes()).await;
    let _ = stdout.flush().await;
}

async fn handle_request(request: &McpRequest) -> Option<McpResponse> {
    let id = request.id.clone();

    match request.method.as_str() {
        "initialize" => Some(handle_initialize(id)),
        "notifications/initialized" => {
            debug!("client initialized");
            None
        }
        "tools/list" => Some(handle_tools_list(id)),
        "tools/call" => Some(handle_tools_call(id, &request.params).await),
        "ping" => Some(McpResponse::success(id, json!({}))),
        other => {
            debug!("unknown method: {other}");
            Some(McpResponse::error(
                id,
                -32601,
                format!("method not found: {other}"),
            ))
        }
    }
}

fn handle_initialize(id: Option<Value>) -> McpResponse {
    debug!("handling initialize");
    McpResponse::success(
        id,
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION
            }
        }),
    )
}

fn handle_tools_list(id: Option<Value>) -> McpResponse {
    let tools: Vec<Value> = tools::all_tools()
        .into_iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema
            })
        })
        .collect();

    McpResponse::success(id, json!({ "tools": tools }))
}

async fn handle_tools_call(id: Option<Value>, params: &Option<Value>) -> McpResponse {
    let params = match params {
        Some(p) => p,
        None => {
            return McpResponse::error(id, -32602, "missing params");
        }
    };

    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(name) => name,
        None => {
            return McpResponse::error(id, -32602, "missing tool name");
        }
    };

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    debug!("calling tool: {tool_name}");

    match tools::call_tool(tool_name, &arguments).await {
        Ok(result) => {
            // Extract text for MCP content block
            let text = result
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("OK")
                .to_string();

            McpResponse::success(
                id,
                json!({
                    "content": [
                        {
                            "type": "text",
                            "text": text
                        }
                    ]
                }),
            )
        }
        Err(e) => McpResponse::success(
            id,
            json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Error: {e}")
                    }
                ],
                "isError": true
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_response() {
        let resp = handle_initialize(Some(json!(1)));
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], "shelly");
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[test]
    fn test_tools_list() {
        let resp = handle_tools_list(Some(json!(2)));
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 10);

        let names: Vec<&str> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"list_connections"));
        assert!(names.contains(&"exec_command"));
        assert!(names.contains(&"upload_file"));
        assert!(names.contains(&"download_file"));
        assert!(names.contains(&"group_exec"));
        assert!(names.contains(&"show_connection"));
        assert!(names.contains(&"list_tunnels"));
        assert!(names.contains(&"create_tunnel"));
        assert!(names.contains(&"list_workflows"));
        assert!(names.contains(&"run_workflow"));
    }

    #[test]
    fn test_tools_have_input_schema() {
        let tools = tools::all_tools();
        for tool in &tools {
            assert!(
                tool.input_schema.get("type").is_some(),
                "tool {} missing type in input_schema",
                tool.name
            );
            assert_eq!(
                tool.input_schema["type"], "object",
                "tool {} input_schema type should be object",
                tool.name
            );
        }
    }

    #[test]
    fn test_error_response_format() {
        let resp = McpResponse::error(Some(json!(99)), -32601, "method not found");
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "method not found");
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let req = McpRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "foo/bar".into(),
            params: None,
        };

        let resp = handle_request(&req).await.unwrap();
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn test_ping() {
        let req = McpRequest {
            jsonrpc: "2.0".into(),
            id: Some(json!(1)),
            method: "ping".into(),
            params: None,
        };

        let resp = handle_request(&req).await.unwrap();
        assert!(resp.result.is_some());
    }

    #[tokio::test]
    async fn test_notification_no_response() {
        let req = McpRequest {
            jsonrpc: "2.0".into(),
            id: None,
            method: "notifications/initialized".into(),
            params: None,
        };

        let resp = handle_request(&req).await;
        assert!(resp.is_none());
    }

    #[tokio::test]
    async fn test_tools_call_missing_name() {
        let resp = handle_tools_call(Some(json!(1)), &Some(json!({}))).await;
        assert!(resp.error.is_some());
    }
}
