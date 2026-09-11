import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Button, Tooltip } from "@mantine/core";
import { IconAdjustments, IconBell, IconBrain, IconDownload, IconFileAnalytics, IconLayoutDashboard, IconRefresh, IconServerCog, IconSparkles } from "@tabler/icons-react";
import { listen } from "@tauri-apps/api/event";
import ReactECharts from "echarts-for-react";
import { analyzeAlerts, diagnoseConnection, discoverGrafana, generateReport, installMcpGrafana, listAlertEvents, listMcpTools, listReports, loadSettings, runMonitorNow, saveSettings } from "../../infrastructure/tauri/client";
import { defaultSettings } from "../../domain/settings/defaults";
import type { AlertEvent, AppSettings, ConnectionDiagnostic, GrafanaDiscovery, Issue, McpTool, Report, Status } from "../../domain/report/types";

type Page = "dashboard" | "reports" | "alerts" | "analysis" | "mcp" | "settings";

const icons = { dashboard: IconLayoutDashboard, reports: IconFileAnalytics, alerts: IconBell, analysis: IconBrain, mcp: IconServerCog, settings: IconAdjustments };
const nav: { id: Page; label: string }[] = [
  { id: "dashboard", label: "运行总览" }, { id: "reports", label: "日报历史" }, { id: "alerts", label: "告警历史" }, { id: "analysis", label: "AI 分析" },
  { id: "mcp", label: "MCP 服务" }, { id: "settings", label: "系统设置" },
];

const aiProviders: Record<string, { baseUrl: string; model: string }> = {
  "MiniMax（国内）": { baseUrl: "https://api.minimaxi.com/v1", model: "MiniMax-M2.7" },
  DeepSeek: { baseUrl: "https://api.deepseek.com", model: "deepseek-chat" },
  Qwen: { baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1", model: "qwen-plus" },
  OpenAI: { baseUrl: "https://api.openai.com/v1", model: "gpt-5-mini" },
};

function WatchDogMark({ size = 38 }: { size?: number }) {
  return <img className="watchdog-mark" src="/chinese-rural-dog.png" width={size} height={size} alt="" />;
}

function errorMessage(error: unknown, fallback: string) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") return error.message;
  return fallback;
}

function statusLabel(status: Status) { return ({ critical: "严重", high: "高风险", warning: "需关注", healthy: "健康" })[status]; }
function scoreTone(score: number) { return score >= 90 ? "green" : score >= 75 ? "amber" : score >= 60 ? "orange" : "red"; }

function IssueCard({ issue }: { issue: Issue }) {
  return <article className={`issue ${issue.severity}`}>
    <div className="issue-icon">{issue.severity === "critical" ? "!" : "↑"}</div>
    <div className="issue-body"><div className="issue-heading"><div><h3>{issue.title}</h3><small>{issue.source}</small></div><span className="change">{issue.change}</span></div><p>{issue.reason}</p><div className="recommendations">{issue.recommendations.map((r) => <span key={r}>{r}</span>)}</div></div>
  </article>;
}

const metricMeta = {
  cpu: { label: "CPU 使用率", icon: "CPU" }, memory: { label: "内存使用率", icon: "MEM" },
  disk: { label: "磁盘剩余空间", icon: "DSK" }, database: { label: "数据库状态", icon: "DB" },
} as const;

function serverKey(instance = "") { return instance.replace(/^https?:\/\//, "").split(":")[0] || "未知服务器"; }

function ServerOverview({ services }: { services: Report["services"] }) {
  const grouped = new Map<string, typeof services>();
  services.forEach(service => {
    const key = serverKey(service.instance ?? service.name.split(" · ").at(-1));
    grouped.set(key, [...(grouped.get(key) ?? []), service]);
  });
  return <div className="server-list">{[...grouped.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([server, metrics]) => {
    const available = metrics.find(metric => metric.category === "availability");
    return <article className="card server-card" key={server}>
      <div className="server-head"><div><span className="server-avatar">{server.slice(0, 2).toUpperCase()}</span><span><h3>{server}</h3><small>{metrics[0]?.datasourceUid ? `Prometheus · ${metrics[0].datasourceUid}` : "Prometheus 监控实例"}</small></span></div><i className={`status-pill ${available?.breached ? "critical" : "healthy"}`}>● {available?.breached ? "离线" : "在线"}</i></div>
      <div className="server-metrics">{(["cpu", "memory", "disk", "database"] as const).map(category => {
        const metric = metrics.find(item => item.category === category); const meta = metricMeta[category];
        return <div className={`server-metric ${metric?.breached ? "bad" : ""}`} key={category}><div><b>{meta.icon}</b><span>{meta.label}</span></div>{metric ? <><strong>{metric.value?.toLocaleString(undefined, { maximumFractionDigits: 2 })}{metric.unit}</strong><small>{category === "database" ? (metric.value === 1 ? "运行正常" : "连接异常") : `日均 ${metric.average?.toFixed(2)}${metric.unit ?? ""} · 范围 ${metric.minimum?.toFixed(2)}–${metric.maximum?.toFixed(2)} · ${metric.sampleCount} 次`}</small></> : <><strong className="muted-value">—</strong><small>未采集到该指标</small></>}</div>;
      })}</div>
    </article>;
  })}</div>;
}

function MetricPanel({ category, services, report }: { category: "cpu" | "memory" | "disk"; services: Report["services"]; report: Report }) {
  const metrics = services.filter(service => service.category === category);
  const values = metrics.flatMap(metric => [metric.minimum, metric.average, metric.maximum].filter((value): value is number => typeof value === "number"));
  const current = metrics.length ? metrics.reduce((sum, metric) => sum + (metric.average ?? metric.value ?? 0), 0) / metrics.length : 0;
  const trends = report.trends.filter(trend => metrics.some(metric => trend.label === metric.name && (trend.datasourceUid ?? metric.datasourceUid) === metric.datasourceUid));
  const maxPoints = Math.max(0, ...trends.map(trend => trend.history.length));
  const start = report.windowStart ? new Date(report.windowStart.replace(" ", "T")).getTime() : Date.now() - 86_400_000;
  const end = report.windowEnd ? new Date(report.windowEnd.replace(" ", "T")).getTime() : Date.now();
  const labels = Array.from({ length: maxPoints }, (_, index) => new Date(start + ((end - start) * index / Math.max(1, maxPoints - 1))).toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" }));
  const colors = ["#5794f2", "#73bf69", "#f2cc0c", "#ff9830", "#b877d9", "#e02f44"];
  const option = {
    animation: false, color: colors,
    tooltip: { trigger: "axis", backgroundColor: "rgba(24,27,31,.96)", borderWidth: 0, textStyle: { color: "#fff", fontSize: 11 }, valueFormatter: (value: number) => `${Number(value).toFixed(2)}%` },
    legend: { type: "scroll", bottom: 0, left: 8, right: 8, itemWidth: 12, itemHeight: 3, textStyle: { fontSize: 9, color: "#59616c" } },
    grid: { top: 16, left: 42, right: 16, bottom: 44 },
    xAxis: { type: "category", boundaryGap: false, data: labels, axisLine: { lineStyle: { color: "#cfd4da" } }, axisLabel: { color: "#7b828c", fontSize: 8, hideOverlap: true }, splitLine: { show: true, lineStyle: { color: "#eef0f3" } } },
    yAxis: { type: "value", min: 0, max: 100, axisLabel: { formatter: "{value}%", color: "#7b828c", fontSize: 8 }, splitLine: { lineStyle: { color: "#e8ebef" } } },
    dataZoom: [{ type: "inside", zoomOnMouseWheel: true, moveOnMouseMove: true }],
    series: trends.map((trend, index) => ({ name: trend.instance ?? trend.label, type: "line", showSymbol: false, smooth: .18, sampling: "lttb", connectNulls: false, lineStyle: { width: 1.8 }, areaStyle: index === 0 ? { opacity: .06 } : undefined, data: trend.history, markLine: index === 0 && metrics[0]?.threshold !== undefined ? { silent: true, symbol: "none", label: { formatter: `阈值 ${metrics[0].threshold}%`, fontSize: 8 }, lineStyle: { color: "#e02f44", type: "dashed", width: 1 }, data: [{ yAxis: metrics[0].threshold }] } : undefined })),
    graphic: trends.length ? undefined : [{ type: "text", left: "center", top: "middle", style: { text: "暂无该数据源的时序样本", fill: "#9299a3", fontSize: 11 } }],
  };
  return <article className="card grafana-panel"><div className="panel-title"><b>{metricMeta[category].label}</b><small>{metrics.length} 个实例</small></div><ReactECharts option={option} notMerge lazyUpdate className="echarts-timeseries" /><div className="panel-legend panel-stats"><span>平均 <b>{current.toFixed(2)}%</b></span><span>最低 <b>{values.length ? Math.min(...values).toFixed(2) : "—"}</b></span><span>最高 <b>{values.length ? Math.max(...values).toFixed(2) : "—"}</b></span></div></article>;
}

function Dashboard({ report, onGenerate, generating }: { report: Report; onGenerate: () => void; generating: boolean }) {
  const datasourceOptions = [...new Set(report.services.map(service => service.datasourceUid ?? "legacy"))].sort();
  const [datasource, setDatasource] = useState(datasourceOptions[0] ?? "legacy");
  const [job, setJob] = useState("all"); const [instance, setInstance] = useState("all");
  useEffect(() => { if (!datasourceOptions.includes(datasource)) setDatasource(datasourceOptions[0] ?? "legacy"); }, [report.id]);
  const sourceServices = report.services.filter(service => (service.datasourceUid ?? "legacy") === datasource);
  const jobs = [...new Set(sourceServices.map(service => service.job).filter(Boolean) as string[])].sort();
  const instances = [...new Set(sourceServices.map(service => service.instance).filter(Boolean) as string[])].sort();
  const filtered = sourceServices.filter(service => (job === "all" || service.job === job) && (instance === "all" || service.instance === instance));
  const filteredIssues = report.issues.filter(issue => issue.source.includes(datasource) || datasource === "legacy");
  const sourceScore = filtered.length ? Math.round(filtered.reduce((sum, service) => sum + service.score, 0) / filtered.length) : 0;
  const sourceStatus: Status = filtered.some(service => service.breached && service.score < 50) ? "critical" : filtered.some(service => service.breached) ? "warning" : "healthy";
  return <>
    <div className="grafana-heading"><div><p className="eyebrow">INFRASTRUCTURE / DAILY OVERVIEW</p><h1>服务器运行总览</h1><p>{report.windowStart ? `${report.windowStart} — ${report.windowEnd} · ${report.sampleCount ?? 0} 条采样 · 第 ${report.analysisNumber ?? 1} 次分析` : "历史快照报告"}</p></div><div><span className={`status-pill ${sourceStatus}`}>● 当前数据源健康度 {sourceScore}</span><Button leftSection={generating ? <IconRefresh size={16} className="spin" /> : <IconSparkles size={16} />} onClick={onGenerate} loading={generating}>生成今日日报</Button></div></div>
    <div className="dashboard-filters"><label><span>数据源</span><select value={datasource} onChange={event => { setDatasource(event.target.value); setJob("all"); setInstance("all"); }}>{datasourceOptions.map(uid => <option key={uid}>{uid}</option>)}</select></label><label><span>Job</span><select value={job} onChange={event => setJob(event.target.value)}><option value="all">全部</option>{jobs.map(value => <option key={value}>{value}</option>)}</select></label><label><span>实例</span><select value={instance} onChange={event => setInstance(event.target.value)}><option value="all">全部</option>{instances.map(value => <option key={value}>{value}</option>)}</select></label><label><span>分析窗口</span><select disabled><option>过去 24 小时</option></select></label><div className="source-lock">● 当前仅展示数据源 <b>{datasource}</b></div></div>
    <section className="overview-panels"><MetricPanel category="cpu" services={filtered} report={report} /><MetricPanel category="memory" services={filtered} report={report} /><MetricPanel category="disk" services={filtered} report={report} /></section>
    <section><div className="resource-bar"><b>Resource Details · {datasource}</b><span>{new Set(filtered.map(service => service.instance)).size} 个实例</span></div>{filtered.length ? <ServerOverview services={filtered} /> : <EmptyState title="当前筛选无数据" detail="请切换数据源、Job 或实例。" />}</section>
    <section className="source-summary card"><div><IconSparkles size={18} /><span><b>当前数据源分析</b><small>{datasource}</small></span></div><p>当前筛选共 {filtered.length} 项指标，{filtered.filter(s => s.breached).length ? `其中 ${filtered.filter(s => s.breached).length} 项超过阈值。` : "暂未发现超过阈值的指标。"}</p><div><span><b className="red-text">{filtered.filter(s => s.breached).length}</b>异常</span><span><b>{filtered.length}</b>指标</span></div></section>
    <section><div className="section-title"><div><p className="eyebrow">PRIORITY QUEUE</p><h2>优先处理 · {datasource}</h2></div><span>{filteredIssues.length} 项分析结果</span></div><div className="issues">{filteredIssues.map((i) => <IssueCard issue={i} key={i.id} />)}</div></section>
  </>;
}

function Reports({ reports, onSelect }: { reports: Report[]; onSelect: (r: Report) => void }) {
  return <><div className="page-title"><div><p className="eyebrow">REPORT ARCHIVE</p><h1>日报历史</h1><p>同一天的多次分析会独立保存，可对比状态变化。</p></div></div>{reports.length === 0 ? <EmptyState title="暂无真实日报" detail="SQLite 中还没有采集生成的日报记录。" /> : <div className="card report-table"><div className="table-head"><span>日期/分析</span><span>健康度</span><span>状态</span><span>关键摘要</span><span /></div>{reports.map((r) => <button className="report-row" key={r.id} onClick={() => onSelect(r)}><span><b>{r.date}{r.analysisNumber ? ` · #${r.analysisNumber}` : ""}</b><small>{r.generatedAt}{r.sampleCount ? ` · ${r.sampleCount} 条` : ""}</small></span><span className={`score ${scoreTone(r.score)}`}>{r.score}</span><span><i className={`status-pill ${r.status}`}>● {statusLabel(r.status)}</i></span><span>{r.summary}</span><span>→</span></button>)}</div>}</>;
}

function EmptyState({ title, detail, action }: { title: string; detail: string; action?: ReactNode }) {
  return <div className="card empty-state"><span>◇</span><h2>{title}</h2><p>{detail}</p>{action}</div>;
}

function Alerts({ events, loading, onRefresh }: { events: AlertEvent[]; loading: boolean; onRefresh: () => void }) {
  return <><div className="page-title"><div><p className="eyebrow">ALERT TIMELINE</p><h1>告警历史</h1><p>查看真实触发与恢复记录，最多保留最近 100 条。</p></div><Button variant="default" leftSection={<IconRefresh size={15} />} loading={loading} onClick={onRefresh}>刷新</Button></div>{events.length === 0 ? <EmptyState title="暂无告警记录" detail="监控规则触发或恢复后，事件会显示在这里。" /> : <div className="alert-history">{events.map(event => <article className={`card alert-event ${event.kind} ${event.severity}`} key={event.id}><i>{event.kind === "resolved" ? "✓" : "!"}</i><div><div><b>{event.ruleName}</b><span className={`status-pill ${event.kind === "resolved" ? "healthy" : event.severity}`}>{event.kind === "resolved" ? "已恢复" : "告警中"}</span></div><p>{event.message}</p><small>{new Date(event.createdAt).toLocaleString("zh-CN")}</small></div><strong>{event.value.toFixed(2)}{event.unit}</strong></article>)}</div>}</>;
}

function Analysis({ settings }: { settings: AppSettings }) {
  const [query, setQuery] = useState(""); const [answer, setAnswer] = useState(""); const [busy, setBusy] = useState(false); const [error, setError] = useState("");
  const ask = async () => { if (!query.trim()) return; setBusy(true); setAnswer(""); setError(""); try { setAnswer(await analyzeAlerts(settings, query)); } catch (cause) { setError(errorMessage(cause, "AI 分析失败")); } finally { setBusy(false); } };
  return <><div className="page-title"><div><p className="eyebrow">AI INVESTIGATION</p><h1>AI 异常分析</h1><p>基于最近 30 条真实告警事件生成有证据的分析。</p></div></div><div className="analysis-layout"><div className="card chat"><div className="chat-empty"><span className="ai-orb">✦</span><h2>需要分析什么异常？</h2><p>模型只会收到本地保存的告警事件，不会获得写操作能力。</p></div>{answer && <div className="answer"><span>✦</span><p>{answer}</p></div>}{error && <div className="load-error">{error}</div>}<div className="composer"><textarea value={query} onChange={event => setQuery(event.target.value)} placeholder="例如：最近告警可能由什么原因引起？" /><button className="primary" disabled={busy || !query.trim()} onClick={ask}>{busy ? "分析中…" : "发送 ↑"}</button></div></div><aside className="card evidence"><h3>分析边界</h3>{["最近 30 条告警", "触发与恢复状态", "指标值与阈值"].map((item, index) => <div key={item}><i>{index + 1}</i><span>{item}<small>真实 SQLite 记录</small></span><b>✓</b></div>)}<p>AI 输出仅供诊断参考，不会自动执行修复。</p></aside></div></>;
}

function SettingsPage({ initial, section, onSaved }: { initial: AppSettings; section: "mcp" | "settings"; onSaved: (settings: AppSettings) => void }) {
  const [form, setForm] = useState(initial); const [message, setMessage] = useState(""); const [testing, setTesting] = useState(false);
  const [tools, setTools] = useState<McpTool[]>([]);
  const [installing, setInstalling] = useState(false);
  const [checking, setChecking] = useState(false);
  const [diagnostic, setDiagnostic] = useState<ConnectionDiagnostic | null>(null);
  const [discovery, setDiscovery] = useState<GrafanaDiscovery | null>(null);
  const [discovering, setDiscovering] = useState(false);
  useEffect(() => setForm(initial), [initial]);
  const field = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => setForm({ ...form, [key]: value });
  const selectProvider = (provider: string) => {
    const preset = aiProviders[provider];
    setForm(current => preset ? { ...current, aiProvider: provider, aiBaseUrl: preset.baseUrl, aiModel: preset.model } : { ...current, aiProvider: provider });
  };
  const ruleField = (index: number, key: "threshold" | "forChecks", value: number) => setForm(current => ({ ...current, alertRules: current.alertRules.map((rule, i) => i === index ? { ...rule, [key]: value } : rule) }));
  const toggleResource = (key: "selectedDatasourceUids" | "selectedDashboardUids", uid: string) => setForm(current => ({ ...current, [key]: current[key].includes(uid) ? current[key].filter(item => item !== uid) : [...current[key], uid] }));
  const discover = async () => { setDiscovering(true); setMessage(""); try { const result = await discoverGrafana(form); setDiscovery(result); setMessage(`已发现 ${result.datasources.length} 个数据源、${result.dashboards.length} 个 Dashboard`); } catch (error) { setMessage(errorMessage(error, "Grafana 资源发现失败")); } finally { setDiscovering(false); } };
  const test = async () => { setTesting(true); setMessage(""); setDiagnostic(null); try { const result = await diagnoseConnection(form); setDiagnostic(result); if (result.success) { setTools(await listMcpTools(form)); setDiscovery(await discoverGrafana(form)); } else setTools([]); setMessage(result.success ? "连接、Grafana 鉴权及资源发现均正常" : "诊断发现连接问题，请查看各阶段详情"); } catch (e) { setTools([]); setMessage(errorMessage(e, "连接诊断失败")); } finally { setTesting(false); } };
  const save = async () => { try { await saveSettings(form); onSaved(form); setMessage("设置已保存；Token 和 API Key 已写入系统 Keychain。"); } catch (error) { setMessage(errorMessage(error, "保存设置失败")); } };
  const checkNow = async () => { setChecking(true); setMessage(""); try { const result = await runMonitorNow(form); setMessage(`检查 ${result.checked} 条规则，产生 ${result.events.length} 条通知${result.errors.length ? `；${result.errors.join("；")}` : ""}`); } catch (error) { setMessage(errorMessage(error, "检查失败")); } finally { setChecking(false); } };
  const install = async () => {
    setInstalling(true); setMessage("正在下载并准备官方 mcp-grafana…");
    try {
      const result = await installMcpGrafana();
      const cleanArgs = form.mcpArgs.replace(/^mcp-grafana\s+/, "");
      const next = { ...form, mcpCommand: result.command, mcpArgs: [...result.argsPrefix, cleanArgs].filter(Boolean).join(" ") };
      setForm(next); await saveSettings(next); onSaved(next);
      if (next.grafanaToken && next.grafanaUrl) {
        setTools(await listMcpTools(next)); setMessage(`${result.message}，并已连接 Grafana。`);
      } else { setMessage(`${result.message}。请填写 Grafana Token 后测试连接。`); }
    } catch (error) { setMessage(errorMessage(error, "安装失败")); }
    finally { setInstalling(false); }
  };
  const isMcp = section === "mcp";
  const configured = isMcp ? Boolean(form.grafanaUrl && form.grafanaToken && form.mcpCommand) : Boolean(form.aiBaseUrl && form.aiModel && form.aiKey);
  return <><div className="page-title"><div><p className="eyebrow">{isMcp ? "CONNECTIONS" : "PREFERENCES"}</p><h1>{isMcp ? "MCP 服务" : "系统设置"}</h1><p>{isMcp ? "连接 Grafana 的只读数据入口。" : "配置模型与日报生成计划。"}</p></div></div><div className="settings-grid"><div className="card form-card"><div className="section-head"><div><span className="section-kicker">{isMcp ? "GRAFANA PRODUCTION" : "AI PROVIDER"}</span><h2>{isMcp ? "Grafana MCP" : "分析模型"}</h2></div><span className={`status-pill ${configured ? "healthy" : "warning"}`}>● {configured ? "已填写" : "待配置"}</span></div>{isMcp ? <>
    <label>Grafana 地址<input value={form.grafanaUrl} onChange={e => field("grafanaUrl", e.target.value)} /></label><label>Service Account Token<input type="password" value={form.grafanaToken} onChange={e => field("grafanaToken", e.target.value)} placeholder="保存到系统 Keychain" /></label><details className="advanced"><summary>高级 MCP 设置</summary><div className="form-pair"><label>MCP 命令<input value={form.mcpCommand} onChange={e => field("mcpCommand", e.target.value)} /></label><label>启动参数<input value={form.mcpArgs} onChange={e => field("mcpArgs", e.target.value)} /></label></div><label>失败重试次数<input type="number" min="1" max="5" value={form.mcpRetryAttempts} onChange={e => field("mcpRetryAttempts", Math.min(5, Math.max(1, Number(e.target.value))))} /></label></details><div className="resource-head"><div><b>监控范围</b><small>自动读取 Grafana 数据源、Dashboard 与 Panel 查询</small></div><Button variant="default" loading={discovering} onClick={discover}>刷新 Grafana 资源</Button></div>{discovery && <div className="resource-groups"><div><b>数据源</b>{discovery.datasources.length ? discovery.datasources.map(source => <label className="resource-option" key={source.uid}><input type="checkbox" checked={form.selectedDatasourceUids.includes(source.uid)} onChange={() => toggleResource("selectedDatasourceUids", source.uid)} /><span>{source.name}<small>{source.kind}</small></span></label>) : <small>未发现数据源</small>}</div><div><b>Dashboard</b>{discovery.dashboards.length ? discovery.dashboards.map(dashboard => <label className="resource-option" key={dashboard.uid}><input type="checkbox" checked={form.selectedDashboardUids.includes(dashboard.uid)} onChange={() => toggleResource("selectedDashboardUids", dashboard.uid)} /><span>{dashboard.title}<small>{dashboard.uid}</small></span></label>) : <small>未发现 Dashboard</small>}</div></div>}
  </> : <><div className="form-pair"><label>Provider<select value={form.aiProvider} onChange={e => selectProvider(e.target.value)}><option>MiniMax（国内）</option><option>DeepSeek</option><option>Qwen</option><option>OpenAI</option><option>Custom OpenAI Compatible</option></select></label><label>模型<input value={form.aiModel} onChange={e => field("aiModel", e.target.value)} /></label></div><label>API Base URL<input value={form.aiBaseUrl} onChange={e => field("aiBaseUrl", e.target.value)} /></label><label>API Key<input type="password" value={form.aiKey} onChange={e => field("aiKey", e.target.value)} placeholder="保存到系统 Keychain" /></label><div className="schedule"><div><b>自动生成日报</b><small>每天到点采集 Grafana 并保存运行总览</small></div><input type="time" value={form.scheduleTime} onChange={e => field("scheduleTime", e.target.value)} /><button className={`toggle ${form.scheduleEnabled ? "on" : ""}`} onClick={() => field("scheduleEnabled", !form.scheduleEnabled)}><i /></button></div><div className="monitor-settings"><div className="schedule"><div><b>MCP 定时监控</b><small>按周期拉取已选择的数据源、Dashboard、Panel 查询与 Grafana 告警</small></div><button className={`toggle ${form.monitorEnabled ? "on" : ""}`} onClick={() => field("monitorEnabled", !form.monitorEnabled)}><i /></button></div><div className="selection-summary">已选择 {form.selectedDatasourceUids.length} 个数据源、{form.selectedDashboardUids.length} 个 Dashboard；请在“MCP 服务”中刷新并选择。</div><div className="form-pair"><label>检查间隔（分钟）<input type="number" min="1" value={form.monitorIntervalMinutes} onChange={e => field("monitorIntervalMinutes", Math.max(1, Number(e.target.value)))} /></label><label>重复提醒冷却（分钟）<input type="number" min="0" value={form.alertCooldownMinutes} onChange={e => field("alertCooldownMinutes", Math.max(0, Number(e.target.value)))} /></label></div><div className="rule-list"><b>告警规则兼日报指标</b>{form.alertRules.map((rule, index) => <div className="rule-row" key={rule.id}><span><b>{rule.name}</b><code>{rule.expr}</code></span><label>阈值<input type="number" value={rule.threshold} onChange={e => ruleField(index, "threshold", Number(e.target.value))} /></label><label>连续次数<input type="number" min="1" value={rule.forChecks} onChange={e => ruleField(index, "forChecks", Math.max(1, Number(e.target.value)))} /></label></div>)}</div></div></>}
  {isMcp && diagnostic && <div className="diagnostic-list"><b>连接诊断</b>{diagnostic.steps.map(step => <div className={step.success ? "ok" : "failed"} key={step.name}><i>{step.success ? "✓" : "!"}</i><span><b>{step.name}</b><small>{step.detail}</small></span><time>{step.durationMs} ms</time></div>)}</div>}{isMcp && tools.length > 0 && <div className="tool-list"><b>已发现工具</b>{tools.slice(0, 8).map(tool => <span key={tool.name}><code>{tool.name}</code><small>{tool.description}</small></span>)}</div>}<div className="form-actions">{isMcp && <Button leftSection={<IconDownload size={16} />} onClick={install} loading={installing}>安装 mcp-grafana</Button>}{isMcp && <Button variant="default" onClick={test} loading={testing}>连接诊断</Button>}{!isMcp && <Button variant="default" onClick={checkNow} loading={checking}>立即检查</Button>}<Button variant={isMcp ? "light" : "filled"} onClick={save}>保存设置</Button><span className="form-message">{message}</span></div></div><aside className="card safety"><span>♢</span><h3>默认安全策略</h3><ul><li>mcp-grafana 使用 --disable-write</li><li>建议使用 Viewer 最小权限账号</li><li>Token 与 API Key 保存到系统 Keychain</li><li>告警具备连续触发、冷却和恢复通知</li></ul></aside></div></>;
}

export default function WatchDogWorkspace() {
  const [page, setPage] = useState<Page>("dashboard"); const [reports, setReports] = useState<Report[]>([]); const [events, setEvents] = useState<AlertEvent[]>([]); const [eventsLoading, setEventsLoading] = useState(false); const [selected, setSelected] = useState<Report | null>(null); const [settings, setSettings] = useState(defaultSettings); const [generating, setGenerating] = useState(false); const [generationError, setGenerationError] = useState(""); const [toast, setToast] = useState(""); const [loadError, setLoadError] = useState("");
  const refreshEvents = async () => { setEventsLoading(true); try { setEvents(await listAlertEvents()); } catch (error) { setLoadError(errorMessage(error, "读取告警历史失败")); } finally { setEventsLoading(false); } };
  useEffect(() => { listReports().then(setReports).catch(error => setLoadError(errorMessage(error, "读取日报失败"))); loadSettings().then(setSettings).catch(error => setLoadError(errorMessage(error, "读取设置失败"))); refreshEvents(); }, []);
  useEffect(() => {
    if (!settings.monitorEnabled) return;
    const refresh = () => {
      listReports().then(setReports).catch(error => setLoadError(errorMessage(error, "刷新日报失败")));
      listAlertEvents().then(setEvents).catch(error => setLoadError(errorMessage(error, "刷新告警失败")));
    };
    const timer = window.setInterval(refresh, Math.max(1, settings.monitorIntervalMinutes) * 60_000);
    return () => window.clearInterval(timer);
  }, [settings.monitorEnabled, settings.monitorIntervalMinutes]);
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let disposed = false; const cleanups: (() => void)[] = [];
    const notify = (event: AlertEvent) => {
      setEvents(current => [event, ...current.filter(item => item.id !== event.id)].slice(0, 100));
      setToast(event.message); setTimeout(() => setToast(""), 6000);
      if ("Notification" in window) {
        const show = () => new Notification(`Grafana Watch Dog · ${event.kind === "resolved" ? "恢复" : "告警"}`, { body: event.message });
        if (Notification.permission === "granted") show();
        else if (Notification.permission === "default") Notification.requestPermission().then(permission => permission === "granted" && show());
      }
    };
    Promise.all([
      listen<AlertEvent>("monitor-alert", e => notify(e.payload)),
      listen<string>("monitor-error", e => { setToast(`监控失败：${e.payload}`); setTimeout(() => setToast(""), 6000); }),
      listen<Report>("report-generated", e => { setReports(current => [e.payload, ...current.filter(report => report.id !== e.payload.id)]); setToast("定时日报已生成"); setTimeout(() => setToast(""), 6000); }),
      listen<string>("report-error", e => { setToast(`定时日报失败：${e.payload}`); setTimeout(() => setToast(""), 6000); }),
    ]).then(unlisteners => disposed ? unlisteners.forEach(fn => fn()) : cleanups.push(...unlisteners));
    return () => { disposed = true; cleanups.forEach(fn => fn()); };
  }, []);
  const report = selected ?? reports[0];
  const title = useMemo(() => nav.find(n => n.id === page)?.label, [page]);
  const run = async () => { setGenerating(true); setGenerationError(""); try { const next = await generateReport(); setReports(old => [next, ...old.filter(r => r.id !== next.id)]); setSelected(next); setToast("今日日报生成完成"); setTimeout(() => setToast(""), 6000); } catch (error) { setGenerationError(errorMessage(error, "日报生成失败")); } finally { setGenerating(false); } };
  const navigate = (p: Page) => { setPage(p); if (p !== "reports") setSelected(null); };
  return <div className="app"><aside className="sidebar"><div className="brand"><WatchDogMark /><div><b>Grafana Watch Dog</b><small>AIOPS CONTROL</small></div></div><nav>{nav.map(n => { const NavIcon = icons[n.id]; return <button key={n.id} className={page === n.id ? "active" : ""} onClick={() => navigate(n.id)}><i><NavIcon size={17} /></i>{n.label}</button>; })}</nav><div className="sidebar-bottom"><div className="connection"><i /><span><b>{settings.monitorEnabled ? "监控运行中" : "监控未启用"}</b><small>{settings.monitorEnabled ? `每 ${settings.monitorIntervalMinutes} 分钟检查` : "等待 Grafana 配置"}</small></span></div><button className="profile"><span>CF</span><div><b>Ops Admin</b><small>本地工作区</small></div><i>•••</i></button></div></aside><main><header><button className="mobile-brand"><WatchDogMark size={25} /></button><span>{title}</span><div><span className="readonly"><i />只读模式</span><Tooltip label="刷新"><button className="icon-btn" aria-label="刷新"><IconRefresh size={15} /></button></Tooltip></div></header><div className="content">{loadError && <div className="load-error">{loadError}</div>}{generationError && <div className="load-error">日报生成失败：{generationError}</div>}{page === "dashboard" && (report ? <Dashboard report={report} onGenerate={run} generating={generating} /> : <><div className="page-title"><div><p className="eyebrow">DAILY OVERVIEW</p><h1>运行总览</h1><p>仅展示实际采集并持久化的运行数据。</p></div></div><EmptyState title="暂无真实运行数据" detail="请先配置 Grafana MCP 和 Prometheus 数据源 UID，然后启用自动日报或执行立即生成。" action={<Button loading={generating} onClick={run}>立即采集并生成</Button>} /></>)}{page === "reports" && !selected && <Reports reports={reports} onSelect={setSelected} />}{page === "reports" && selected && <><button className="back" onClick={() => setSelected(null)}>← 返回日报历史</button><Dashboard report={selected} onGenerate={run} generating={generating} /></>}{page === "alerts" && <Alerts events={events} loading={eventsLoading} onRefresh={refreshEvents} />}{page === "analysis" && <Analysis settings={settings} />}{page === "mcp" && <SettingsPage initial={settings} section="mcp" onSaved={setSettings} />}{page === "settings" && <SettingsPage initial={settings} section="settings" onSaved={setSettings} />}</div></main>{toast && <div className="toast">{toast}</div>}</div>;
}
