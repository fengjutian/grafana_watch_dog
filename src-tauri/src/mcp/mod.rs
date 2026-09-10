mod client;
mod installer;
mod protocol;

pub use client::{GrafanaMcpClient, GrafanaMcpConfig, ToolSummary};
pub use installer::{install_official_server, InstallResult};
