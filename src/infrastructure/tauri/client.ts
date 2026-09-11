import { invoke } from "@tauri-apps/api/core";
import { defaultSettings } from "../../domain/settings/defaults";
import type { AlertEvent, AppSettings, ConnectionDiagnostic, GrafanaDiscovery, McpInstallResult, McpTool, MetricSeriesPoint, MonitorRunResult, Report } from "../../domain/report/types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

export async function listReports(): Promise<Report[]> {
  if (isTauri()) return invoke("list_reports");
  return [];
}
export async function generateReport(): Promise<Report> {
  if (isTauri()) return invoke("generate_report");
  throw new Error("日报生成仅支持 Tauri 桌面运行环境");
}
export async function loadSettings(): Promise<AppSettings> {
  if (isTauri()) return invoke("load_settings");
  const saved = localStorage.getItem("ai-ops-settings");
  return saved ? { ...defaultSettings, ...JSON.parse(saved) } : defaultSettings;
}
export async function saveSettings(settings: AppSettings): Promise<void> {
  if (isTauri()) return invoke("save_settings", { settings });
  localStorage.setItem("ai-ops-settings", JSON.stringify({ ...settings, grafanaToken: "", aiKey: "" }));
}
export async function testConnection(settings: AppSettings): Promise<string> {
  if (isTauri()) return invoke("test_connection", { settings });
  throw new Error("MCP 连接测试仅支持 Tauri 桌面运行环境");
}

export async function diagnoseConnection(settings: AppSettings): Promise<ConnectionDiagnostic> {
  if (!isTauri()) throw new Error("连接诊断仅支持 Tauri 桌面运行环境");
  return invoke("diagnose_connection", { settings });
}

export async function discoverGrafana(settings: AppSettings): Promise<GrafanaDiscovery> {
  if (!isTauri()) throw new Error("Grafana 资源发现仅支持 Tauri 桌面运行环境");
  return invoke("discover_grafana", { settings });
}

export async function listMcpTools(settings: AppSettings): Promise<McpTool[]> {
  if (isTauri()) return invoke("list_mcp_tools", { settings });
  throw new Error("MCP 工具发现仅支持 Tauri 桌面运行环境");
}

export async function callMcpTool<T = unknown>(settings: AppSettings, name: string, args: Record<string, unknown>): Promise<T> {
  if (!isTauri()) throw new Error("MCP tool calls require the Tauri desktop runtime");
  return invoke<T>("call_mcp_tool", { settings, name, arguments: args });
}

export async function installMcpGrafana(): Promise<McpInstallResult> {
  if (!isTauri()) throw new Error("一键安装仅在 Tauri 桌面应用中可用");
  return invoke<McpInstallResult>("install_mcp_grafana");
}

export async function runMonitorNow(settings: AppSettings): Promise<MonitorRunResult> {
  if (!isTauri()) throw new Error("MCP 监控仅支持 Tauri 桌面运行环境");
  return invoke("run_monitor_now", { settings });
}

export async function listAlertEvents(): Promise<AlertEvent[]> {
  if (!isTauri()) return [];
  return invoke("list_alert_events");
}

export async function listMetricSeries(hours = 24): Promise<MetricSeriesPoint[]> {
  if (!isTauri()) return [];
  return invoke("list_metric_series", { hours });
}

export async function analyzeAlerts(settings: AppSettings, question: string): Promise<string> {
  if (!isTauri()) throw new Error("AI 分析仅支持 Tauri 桌面运行环境");
  return invoke("analyze_alerts", { settings, question });
}
