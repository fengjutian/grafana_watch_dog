export type Severity = "critical" | "warning" | "info";
export type Status = "critical" | "high" | "warning" | "healthy";

export interface Issue { id: string; severity: Severity; title: string; source: string; change: string; reason: string; recommendations: string[] }
export interface Trend { label: string; value: number; unit: string; change: number; history: number[]; datasourceUid?: string; instance?: string; category?: string }
export interface ServiceHealth { name: string; kind: string; score: number; metrics: string[]; instance?: string; category?: "cpu" | "memory" | "disk" | "database" | "availability"; value?: number; unit?: string; threshold?: number; datasourceUid?: string; job?: string; breached?: boolean; average?: number; minimum?: number; maximum?: number; sampleCount?: number }
export interface Report {
  id: string; date: string; score: number; status: Status; summary: string; generatedAt: string;
  analysisNumber?: number; windowStart?: string; windowEnd?: string; sampleCount?: number;
  stats: { critical: number; warning: number; healthy: number; alerts: number };
  services: ServiceHealth[]; trends: Trend[]; issues: Issue[];
}

export interface AppSettings {
  grafanaUrl: string; grafanaToken: string; mcpCommand: string; mcpArgs: string;
  aiProvider: string; aiBaseUrl: string; aiModel: string; aiKey: string;
  scheduleEnabled: boolean; scheduleTime: string;
  monitorEnabled: boolean; monitorIntervalMinutes: number;
  selectedDatasourceUids: string[]; selectedDashboardUids: string[];
  alertCooldownMinutes: number; alertRules: AlertRule[];
  mcpRetryAttempts: number;
}

export type AlertOperator = "greater_than" | "greater_or_equal" | "less_than" | "less_or_equal" | "equal";
export interface AlertRule { id: string; name: string; expr: string; operator: AlertOperator; threshold: number; forChecks: number; severity: Severity; unit: string }
export interface AlertEvent { id: string; ruleId: string; ruleName: string; severity: Severity; kind: "firing" | "resolved"; value: number; threshold: number; unit: string; message: string; createdAt: string }
export interface MonitorRunResult { checked: number; events: AlertEvent[]; errors: string[]; completedAt: string }
export interface DiagnosticStep { name: string; success: boolean; detail: string; durationMs: number }
export interface ConnectionDiagnostic { success: boolean; attempts: number; steps: DiagnosticStep[] }
export interface GrafanaDatasource { uid: string; name: string; kind: string }
export interface GrafanaDashboard { uid: string; title: string }
export interface GrafanaDiscovery { datasources: GrafanaDatasource[]; dashboards: GrafanaDashboard[] }

export interface McpTool { name: string; description: string }
export interface McpInstallResult { command: string; argsPrefix: string[]; method: "existing" | "uv-tool" | "go"; message: string }
