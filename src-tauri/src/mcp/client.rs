use super::protocol::{JsonRpcRequest, JsonRpcResponse, PROTOCOL_VERSION};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

#[derive(Debug, Clone)]
pub struct GrafanaMcpConfig {
    pub command: String,
    pub args: Vec<String>,
    pub grafana_url: String,
    pub service_account_token: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSummary {
    pub name: String,
    pub description: String,
}

pub struct GrafanaMcpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl GrafanaMcpClient {
    pub fn connect(config: GrafanaMcpConfig) -> Result<Self, String> {
        if config.command.trim().is_empty() {
            return Err("MCP command must not be empty".into());
        }
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .env("GRAFANA_URL", &config.grafana_url)
            .env(
                "GRAFANA_SERVICE_ACCOUNT_TOKEN",
                &config.service_account_token,
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("无法启动官方 mcp-grafana：{error}"))?;
        let stdin = child.stdin.take().ok_or("无法连接 MCP stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("无法连接 MCP stdout")?);
        let mut client = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        client.initialize()?;
        Ok(client)
    }

    fn initialize(&mut self) -> Result<(), String> {
        self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "grafana_watch_dog", "version": env!("CARGO_PKG_VERSION") }
            }),
        )?;
        self.notify("notifications/initialized", None)
    }

    pub fn list_tools(&mut self) -> Result<Vec<ToolSummary>, String> {
        let result = self.request("tools/list", json!({}))?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .ok_or("MCP 响应缺少 tools")?;
        Ok(tools
            .iter()
            .filter_map(|tool| {
                Some(ToolSummary {
                    name: tool.get("name")?.as_str()?.to_owned(),
                    description: tool
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                })
            })
            .collect())
    }

    pub fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value, String> {
        let result = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )?;
        if result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let detail = result
                .get("content")
                .and_then(Value::as_array)
                .and_then(|items| {
                    items
                        .iter()
                        .find_map(|item| item.get("text").and_then(Value::as_str))
                })
                .unwrap_or("MCP 工具返回错误");
            return Err(detail.to_owned());
        }
        Ok(result)
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(JsonRpcRequest {
            jsonrpc: "2.0",
            id: Some(id),
            method,
            params: Some(params),
        })?;
        loop {
            let mut line = String::new();
            let bytes = self
                .stdout
                .read_line(&mut line)
                .map_err(|e| format!("读取 MCP 响应失败：{e}"))?;
            if bytes == 0 {
                return Err("mcp-grafana 在响应前退出".into());
            }
            let response: JsonRpcResponse = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if response.id != Some(id) {
                continue;
            }
            if let Some(error) = response.error {
                return Err(format!("MCP error {}: {}", error.code, error.message));
            }
            return response.result.ok_or("MCP 响应缺少 result".into());
        }
    }

    fn notify(&mut self, method: &str, params: Option<Value>) -> Result<(), String> {
        self.send(JsonRpcRequest {
            jsonrpc: "2.0",
            id: None,
            method,
            params,
        })
    }

    fn send(&mut self, request: JsonRpcRequest<'_>) -> Result<(), String> {
        serde_json::to_writer(&mut self.stdin, &request).map_err(|e| e.to_string())?;
        self.stdin.write_all(b"\n").map_err(|e| e.to_string())?;
        self.stdin.flush().map_err(|e| e.to_string())
    }
}

impl Drop for GrafanaMcpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
