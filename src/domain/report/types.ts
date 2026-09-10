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
}
