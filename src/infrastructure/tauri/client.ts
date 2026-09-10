import { invoke } from "@tauri-apps/api/core";
import { currentReport, defaultSettings, reportHistory } from "../demo/reportFixtures";
import type { AppSettings, McpTool, Report } from "../../domain/report/types";

const isTauri = () => "__TAURI_INTERNALS__" in window;
const pause = (ms = 450) => new Promise((resolve) => setTimeout(resolve, ms));

export async function listReports(): Promise<Report[]> {
  if (isTauri()) return invoke("list_reports");
  return reportHistory;
}
export async function generateReport(): Promise<Report> {
  if (isTauri()) return invoke("generate_report");
  await pause(1100); return { ...currentReport, generatedAt: new Date().toLocaleString("zh-CN") };
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
  if (isTauri()) return invoke("test_mcp_connection", { settings });
  await pause(); return settings.grafanaUrl ? "Grafana MCP 连接成功（演示模式）" : Promise.reject(new Error("请填写 Grafana 地址"));
}

export async function listMcpTools(settings: AppSettings): Promise<McpTool[]> {
  if (isTauri()) return invoke("list_mcp_tools", { settings });
  return [
    { name: "query_prometheus", description: "执行只读 PromQL 查询" },
    { name: "query_loki_logs", description: "执行只读 LogQL 查询" },
    { name: "list_alert_rules", description: "读取 Grafana 告警规则" },
  ];
}

export async function callMcpTool<T = unknown>(settings: AppSettings, name: string, args: Record<string, unknown>): Promise<T> {
  if (!isTauri()) throw new Error("MCP tool calls require the Tauri desktop runtime");
  return invoke<T>("call_mcp_tool", { settings, name, arguments: args });
}
