use serde::Serialize;
use std::{fs, path::Path, process::Command};

const GO_PACKAGE: &str = "github.com/grafana/mcp-grafana/cmd/mcp-grafana@latest";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub command: String,
    pub args_prefix: Vec<String>,
    pub method: String,
    pub message: String,
}

fn command_works(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn install_official_server(tools_dir: &Path) -> Result<InstallResult, String> {
    if command_works("mcp-grafana", &["--help"]) {
        return Ok(InstallResult {
            command: "mcp-grafana".into(),
            args_prefix: vec![],
            method: "existing".into(),
            message: "mcp-grafana 已安装，无需重复安装".into(),
        });
    }

    if command_works("uv", &["--version"]) {
        let output = Command::new("uv")
            .args(["tool", "install", "mcp-grafana"])
            .output()
            .map_err(|e| format!("无法启动 uv tool install：{e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("持久安装 mcp-grafana 失败：{}", stderr.trim()));
        }
        if !command_works("mcp-grafana", &["--help"]) {
            return Err("uv 已完成安装，但 mcp-grafana 仍不在 PATH 中；请重启应用后再试".into());
        }
        return Ok(InstallResult {
            command: "mcp-grafana".into(),
            args_prefix: vec![],
            method: "uv-tool".into(),
            message: "已通过 uv tool 持久安装 mcp-grafana".into(),
        });
    }

    if command_works("go", &["version"]) {
        fs::create_dir_all(tools_dir).map_err(|e| format!("无法创建工具目录：{e}"))?;
        let output = Command::new("go")
            .args(["install", GO_PACKAGE])
            .env("GOBIN", tools_dir)
            .output()
            .map_err(|e| format!("无法启动 go install：{e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("安装 mcp-grafana 失败：{}", stderr.trim()));
        }
        let binary = tools_dir.join(if cfg!(windows) {
            "mcp-grafana.exe"
        } else {
            "mcp-grafana"
        });
        if !binary.exists() {
            return Err("go install 已完成，但没有找到 mcp-grafana 二进制文件".into());
        }
        return Ok(InstallResult {
            command: binary.to_string_lossy().into_owned(),
            args_prefix: vec![],
            method: "go".into(),
            message: "已将 mcp-grafana 持久安装到应用工具目录".into(),
        });
    }

    Err("未找到 uv 或 Go，无法持久安装 mcp-grafana。".into())
}
