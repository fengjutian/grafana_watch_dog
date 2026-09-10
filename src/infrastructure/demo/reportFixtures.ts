import type { AppSettings, Report } from "../../domain/report/types";

const trends = [
  { label: "CPU", value: 52, unit: "%", change: 12, history: [38, 41, 45, 43, 48, 49, 52] },
  { label: "Memory", value: 71, unit: "%", change: 18, history: [54, 58, 57, 62, 66, 68, 71] },
  { label: "QPS", value: 1240, unit: "", change: 31, history: [820, 910, 880, 1010, 1100, 1180, 1240] },
  { label: "API P95", value: 1.82, unit: "s", change: 24, history: [1.15, 1.22, 1.31, 1.28, 1.55, 1.69, 1.82] },
];

export const currentReport: Report = {
  id: "report-2026-09-10", date: "2026-09-10", score: 87, status: "warning", generatedAt: "2026-09-10 08:00",
  summary: "系统整体稳定，但数据库性能出现明显恶化趋势。慢查询增长显著高于业务流量增长，建议优先检查订单查询相关 SQL 与索引。",
  stats: { critical: 2, warning: 3, healthy: 18, alerts: 8 }, trends,
  services: [
    { name: "服务器", kind: "Server", score: 92, metrics: ["CPU 52%", "内存 71%", "磁盘 53%"] },
    { name: "数据库", kind: "MySQL", score: 81, metrics: ["QPS 1,240", "慢查询 1,823", "死锁 12"] },
    { name: "API", kind: "FastAPI", score: 94, metrics: ["P95 1.82s", "错误率 3.1%", "QPS 684"] },
    { name: "日志", kind: "Loki", score: 73, metrics: ["ERROR 128", "Timeout 42", "OOM 1"] },
  ],
  issues: [
    { id: "mysql-slow", severity: "critical", title: "MySQL 慢查询异常增长", source: "Prometheus · MySQL", change: "+188%", reason: "慢查询增长明显高于 QPS 的 39% 增长，疑似 SQL 性能退化，而非单纯业务流量增长。", recommendations: ["检查 Top Slow SQL", "检查 orders 表索引", "检查连接池与锁等待"] },
    { id: "oom", severity: "critical", title: "FastAPI 发生 OOM", source: "Loki · 14:32", change: "1 次", reason: "OOM 前 15 分钟内存与 Swap 持续上涨，并伴随 API P95 延迟升高。", recommendations: ["检查进程内存快照", "核对当时请求峰值", "检查最近发布变更"] },
    { id: "memory", severity: "warning", title: "orderslave 内存压力升高", source: "Prometheus · Node", change: "+18%", reason: "内存已达到 88%，过去 7 天持续上升。", recommendations: ["确认缓存占用", "检查异常进程"] },
  ]
};

export const reportHistory: Report[] = [
  currentReport,
  { ...currentReport, id: "report-2026-09-09", date: "2026-09-09", score: 92, status: "healthy", summary: "整体运行健康，未发现需要立即处置的问题。", stats: { critical: 0, warning: 1, healthy: 21, alerts: 3 } },
  { ...currentReport, id: "report-2026-09-08", date: "2026-09-08", score: 76, status: "warning", summary: "API 延迟和网关错误率短时升高，服务恢复后指标正常。", stats: { critical: 1, warning: 4, healthy: 16, alerts: 11 } },
  { ...currentReport, id: "report-2026-09-07", date: "2026-09-07", score: 95, status: "healthy", summary: "系统状态良好。", stats: { critical: 0, warning: 0, healthy: 23, alerts: 1 } },
  { ...currentReport, id: "report-2026-09-06", date: "2026-09-06", score: 91, status: "healthy", summary: "系统状态良好。", stats: { critical: 0, warning: 1, healthy: 22, alerts: 2 } },
];

export const defaultSettings: AppSettings = {
  grafanaUrl: "http://localhost:3000", grafanaToken: "", mcpCommand: "mcp-grafana", mcpArgs: "--disable-write",
  aiProvider: "DeepSeek", aiBaseUrl: "https://api.deepseek.com", aiModel: "deepseek-chat", aiKey: "",
  scheduleEnabled: true, scheduleTime: "08:00",
};
