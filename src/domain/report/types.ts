export type Severity = "critical" | "warning" | "info";
export type Status = "critical" | "high" | "warning" | "healthy";

export interface Issue { id: string; severity: Severity; title: string; source: string; change: string; reason: string; recommendations: string[] }
export interface Trend { label: string; value: number; unit: string; change: number; history: number[] }
export interface ServiceHealth { name: string; kind: string; score: number; metrics: string[] }
export interface Report {
  id: string; date: string; score: number; status: Status; summary: string; generatedAt: string;
  stats: { critical: number; warning: number; healthy: number; alerts: number };
  services: ServiceHealth[]; trends: Trend[]; issues: Issue[];
}

export interface AppSettings {
  grafanaUrl: string; grafanaToken: string; mcpCommand: string; mcpArgs: string;
  aiProvider: string; aiBaseUrl: string; aiModel: string; aiKey: string;
  scheduleEnabled: boolean; scheduleTime: string;
  monitorEnabled: boolean; monitorIntervalMinutes: number; prometheusDatasourceUid: string;
  alertCooldownMinutes: number; alertRules: AlertRule[];
}

export type AlertOperator = "greater_than" | "greater_or_equal" | "less_than" | "less_or_equal" | "equal";
export interface AlertRule { id: string; name: string; expr: string; operator: AlertOperator; threshold: number; forChecks: number; severity: Severity; unit: string }
export interface AlertEvent { id: string; ruleId: string; ruleName: string; severity: Severity; kind: "firing" | "resolved"; value: number; threshold: number; unit: string; message: string; createdAt: string }
export interface MonitorRunResult { checked: number; events: AlertEvent[]; errors: string[]; completedAt: string }

export interface McpTool { name: string; description: string }
export interface McpInstallResult { command: string; argsPrefix: string[]; method: "existing" | "uvx" | "go"; message: string }
