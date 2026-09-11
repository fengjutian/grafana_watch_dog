import type { AppSettings } from "../report/types";

export const defaultSettings: AppSettings = {
  grafanaUrl: "http://localhost:3000",
  grafanaToken: "",
  mcpCommand: "mcp-grafana",
  mcpArgs: "--transport stdio --disable-write --enabled-tools search,datasource,prometheus,loki,alerting,dashboard",
  aiProvider: "MiniMax（国内）",
  aiBaseUrl: "https://api.minimaxi.com/v1",
  aiModel: "MiniMax-M2.7",
  aiKey: "",
  scheduleEnabled: false,
  scheduleTime: "08:00",
  monitorEnabled: false,
  monitorIntervalMinutes: 5,
  selectedDatasourceUids: [],
  selectedDashboardUids: [],
  alertCooldownMinutes: 30,
  mcpRetryAttempts: 3,
  alertRules: [
    { id: "cpu", name: "CPU 使用率", expr: '100 - (avg by(instance) (rate(node_cpu_seconds_total{mode="idle"}[5m])) * 100)', operator: "greater_than", threshold: 85, forChecks: 2, severity: "critical", unit: "%" },
    { id: "memory", name: "内存使用率", expr: "(1 - node_memory_MemAvailable_bytes / node_memory_MemTotal_bytes) * 100", operator: "greater_than", threshold: 90, forChecks: 1, severity: "critical", unit: "%" },
    { id: "disk", name: "磁盘剩余空间", expr: 'node_filesystem_avail_bytes{fstype!~"tmpfs|overlay"} / node_filesystem_size_bytes * 100', operator: "less_than", threshold: 10, forChecks: 1, severity: "critical", unit: "%" },
    { id: "server_up", name: "服务器在线状态", expr: 'up{job=~"node.*"}', operator: "less_than", threshold: 1, forChecks: 1, severity: "critical", unit: "" },
    { id: "database", name: "数据库在线状态", expr: 'max by(instance, job) ({__name__=~"mysql_up|pg_up|mongodb_up|redis_up"})', operator: "less_than", threshold: 1, forChecks: 1, severity: "critical", unit: "" },
  ],
};
