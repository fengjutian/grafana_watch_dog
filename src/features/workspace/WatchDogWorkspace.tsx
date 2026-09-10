import { useEffect, useMemo, useState } from "react";
import { Button, Tooltip } from "@mantine/core";
import { IconAdjustments, IconBell, IconBrain, IconDownload, IconFileAnalytics, IconLayoutDashboard, IconRefresh, IconServerCog, IconSparkles } from "@tabler/icons-react";
import { listen } from "@tauri-apps/api/event";
import { analyzeAlerts, diagnoseConnection, generateReport, installMcpGrafana, listAlertEvents, listMcpTools, listReports, loadSettings, runMonitorNow, saveSettings } from "../../infrastructure/tauri/client";
import { defaultSettings } from "../../domain/settings/defaults";
import type { AlertEvent, AppSettings, ConnectionDiagnostic, Issue, McpTool, Report, Status } from "../../domain/report/types";

type Page = "dashboard" | "reports" | "alerts" | "analysis" | "mcp" | "settings";

const icons = { dashboard: IconLayoutDashboard, reports: IconFileAnalytics, alerts: IconBell, analysis: IconBrain, mcp: IconServerCog, settings: IconAdjustments };
const nav: { id: Page; label: string }[] = [
  { id: "dashboard", label: "运行总览" }, { id: "reports", label: "日报历史" }, { id: "alerts", label: "告警历史" }, { id: "analysis", label: "AI 分析" },
  { id: "mcp", label: "MCP 服务" }, { id: "settings", label: "系统设置" },
];

function WatchDogMark({ size = 38 }: { size?: number }) {
  return <img className="watchdog-mark" src="/watchdog-mark.svg" width={size} height={size} alt="" />;
}

function errorMessage(error: unknown, fallback: string) {
  if (error instanceof Error) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") return error.message;
  return fallback;
}

function statusLabel(status: Status) { return ({ critical: "严重", high: "高风险", warning: "需关注", healthy: "健康" })[status]; }
function scoreTone(score: number) { return score >= 90 ? "green" : score >= 75 ? "amber" : score >= 60 ? "orange" : "red"; }

function Sparkline({ data }: { data: number[] }) {
  const min = Math.min(...data), max = Math.max(...data), range = max - min || 1;
  const points = data.map((v, i) => `${(i / (data.length - 1)) * 120},${38 - ((v - min) / range) * 30}`).join(" ");
  return <svg className="spark" viewBox="0 0 120 44" aria-label="过去 7 天趋势"><polyline points={points} fill="none" stroke="currentColor" strokeWidth="2.5" /><circle cx="120" cy={38 - ((data.at(-1)! - min) / range) * 30} r="3.5" fill="currentColor" /></svg>;
}

function HealthGauge({ score, compact = false }: { score: number; compact?: boolean }) {
  const r = compact ? 45 : 70, c = Math.PI * r, progress = c * score / 100;
  return <div className={`gauge ${compact ? "compact" : ""}`}>
    <svg viewBox="0 0 180 105"><path d={`M ${90-r} 90 A ${r} ${r} 0 0 1 ${90+r} 90`} className="gauge-track" pathLength={c} /><path d={`M ${90-r} 90 A ${r} ${r} 0 0 1 ${90+r} 90`} className={`gauge-fill ${scoreTone(score)}`} strokeDasharray={`${progress} ${c}`} pathLength={c} /></svg>
    <div className="gauge-value"><strong>{score}</strong><span>/ 100</span></div>
  </div>;
}

function IssueCard({ issue }: { issue: Issue }) {
  return <article className={`issue ${issue.severity}`}>
    <div className="issue-icon">{issue.severity === "critical" ? "!" : "↑"}</div>
    <div className="issue-body"><div className="issue-heading"><div><h3>{issue.title}</h3><small>{issue.source}</small></div><span className="change">{issue.change}</span></div><p>{issue.reason}</p><div className="recommendations">{issue.recommendations.map((r) => <span key={r}>{r}</span>)}</div></div>
  </article>;
}

function Dashboard({ report, onGenerate, generating }: { report: Report; onGenerate: () => void; generating: boolean }) {
  return <>
    <div className="page-title"><div><p className="eyebrow">DAILY OVERVIEW · {report.date}</p><h1>早上好，系统值得你关注一下。</h1><p>过去 24 小时的核心运行状态已经整理完毕。</p></div><Button leftSection={generating ? <IconRefresh size={16} className="spin" /> : <IconSparkles size={16} />} onClick={onGenerate} loading={generating}>生成今日日报</Button></div>
    <section className="hero-grid">
      <div className="card health-card"><div className="section-head"><div><span className="section-kicker">SYSTEM HEALTH</span><h2>系统健康度</h2></div><span className={`status-pill ${report.status}`}>● {statusLabel(report.status)}</span></div><HealthGauge score={report.score} /></div>
      <div className="card conclusion"><div className="conclusion-top"><span className="ai-mark"><IconSparkles size={20} /></span><div><span className="section-kicker">AI CONCLUSION</span><h2>今日结论</h2></div></div><blockquote>{report.summary}</blockquote><div className="stat-row"><div><b className="red-text">{report.stats.critical}</b><span>严重问题</span></div><div><b className="amber-text">{report.stats.warning}</b><span>需要关注</span></div><div><b className="green-text">{report.stats.healthy}</b><span>正常指标</span></div><div><b>{report.stats.alerts}</b><span>昨日告警</span></div></div></div>
    </section>
    <section><div className="section-title"><div><p className="eyebrow">SERVICE PULSE</p><h2>服务状态</h2></div><span>数据更新于 {report.generatedAt.split(" ").at(-1)}</span></div><div className="service-grid">{report.services.map((s) => <div className="card service-card" key={s.name}><div className="service-top"><div className={`service-icon ${scoreTone(s.score)}`}>{s.kind.slice(0, 1)}</div><div><h3>{s.name}</h3><small>{s.kind}</small></div><b className={scoreTone(s.score)}>{s.score}</b></div><div className="meter"><i style={{ width: `${s.score}%` }} className={scoreTone(s.score)} /></div><div className="metric-chips">{s.metrics.map((m) => <span key={m}>{m}</span>)}</div></div>)}</div></section>
    <section><div className="section-title"><div><p className="eyebrow">7-DAY SIGNAL</p><h2>关键趋势</h2></div><span>对比过去 7 天均值</span></div><div className="trend-grid">{report.trends.map((t) => <div className="card trend-card" key={t.label}><div><span>{t.label}</span><strong>{t.value.toLocaleString()}{t.unit}</strong><small className={t.change > 20 ? "red-text" : "amber-text"}>↑ {t.change}%</small></div><Sparkline data={t.history} /></div>)}</div></section>
    <section><div className="section-title"><div><p className="eyebrow">PRIORITY QUEUE</p><h2>优先处理</h2></div><span>{report.issues.length} 项分析结果</span></div><div className="issues">{report.issues.map((i) => <IssueCard issue={i} key={i.id} />)}</div></section>
  </>;
}

function Reports({ reports, onSelect }: { reports: Report[]; onSelect: (r: Report) => void }) {
  return <><div className="page-title"><div><p className="eyebrow">REPORT ARCHIVE</p><h1>日报历史</h1><p>回看系统健康度变化，快速定位状态转折点。</p></div></div>{reports.length === 0 ? <EmptyState title="暂无真实日报" detail="SQLite 中还没有采集生成的日报记录。" /> : <div className="card report-table"><div className="table-head"><span>日期</span><span>健康度</span><span>状态</span><span>关键摘要</span><span /></div>{reports.map((r) => <button className="report-row" key={r.id} onClick={() => onSelect(r)}><span><b>{r.date}</b><small>{r.generatedAt}</small></span><span className={`score ${scoreTone(r.score)}`}>{r.score}</span><span><i className={`status-pill ${r.status}`}>● {statusLabel(r.status)}</i></span><span>{r.summary}</span><span>→</span></button>)}</div>}</>;
}

function EmptyState({ title, detail }: { title: string; detail: string }) {
  return <div className="card empty-state"><span>◇</span><h2>{title}</h2><p>{detail}</p></div>;
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
  useEffect(() => setForm(initial), [initial]);
  const field = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => setForm({ ...form, [key]: value });
  const ruleField = (index: number, key: "threshold" | "forChecks", value: number) => setForm(current => ({ ...current, alertRules: current.alertRules.map((rule, i) => i === index ? { ...rule, [key]: value } : rule) }));
  const test = async () => { setTesting(true); setMessage(""); setDiagnostic(null); try { const result = await diagnoseConnection(form); setDiagnostic(result); if (result.success) setTools(await listMcpTools(form)); else setTools([]); setMessage(result.success ? "连接与 Grafana 鉴权均正常" : "诊断发现连接问题，请查看各阶段详情"); } catch (e) { setTools([]); setMessage(errorMessage(e, "连接诊断失败")); } finally { setTesting(false); } };
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
    <label>Grafana 地址<input value={form.grafanaUrl} onChange={e => field("grafanaUrl", e.target.value)} /></label><label>Service Account Token<input type="password" value={form.grafanaToken} onChange={e => field("grafanaToken", e.target.value)} placeholder="保存到系统 Keychain" /></label><div className="form-pair"><label>MCP 命令<input value={form.mcpCommand} onChange={e => field("mcpCommand", e.target.value)} /></label><label>启动参数<input value={form.mcpArgs} onChange={e => field("mcpArgs", e.target.value)} /></label></div><label>失败重试次数<input type="number" min="1" max="5" value={form.mcpRetryAttempts} onChange={e => field("mcpRetryAttempts", Math.min(5, Math.max(1, Number(e.target.value))))} /></label>
  </> : <><div className="form-pair"><label>Provider<select value={form.aiProvider} onChange={e => field("aiProvider", e.target.value)}><option>DeepSeek</option><option>Qwen</option><option>OpenAI</option><option>Custom OpenAI Compatible</option></select></label><label>模型<input value={form.aiModel} onChange={e => field("aiModel", e.target.value)} /></label></div><label>API Base URL<input value={form.aiBaseUrl} onChange={e => field("aiBaseUrl", e.target.value)} /></label><label>API Key<input type="password" value={form.aiKey} onChange={e => field("aiKey", e.target.value)} placeholder="保存到系统 Keychain" /></label><div className="schedule"><div><b>自动生成日报</b><small>每天在指定时间运行分析</small></div><input type="time" value={form.scheduleTime} onChange={e => field("scheduleTime", e.target.value)} /><button className={`toggle ${form.scheduleEnabled ? "on" : ""}`} onClick={() => field("scheduleEnabled", !form.scheduleEnabled)}><i /></button></div><div className="monitor-settings"><div className="schedule"><div><b>MCP 定时监控</b><small>应用运行时按周期查询 Prometheus</small></div><button className={`toggle ${form.monitorEnabled ? "on" : ""}`} onClick={() => field("monitorEnabled", !form.monitorEnabled)}><i /></button></div><div className="form-pair"><label>Prometheus 数据源 UID<input value={form.prometheusDatasourceUid} onChange={e => field("prometheusDatasourceUid", e.target.value)} placeholder="例如 prometheus-prod" /></label><label>检查间隔（分钟）<input type="number" min="1" value={form.monitorIntervalMinutes} onChange={e => field("monitorIntervalMinutes", Math.max(1, Number(e.target.value)))} /></label></div><label>重复提醒冷却（分钟）<input type="number" min="0" value={form.alertCooldownMinutes} onChange={e => field("alertCooldownMinutes", Math.max(0, Number(e.target.value)))} /></label><div className="rule-list"><b>告警规则</b>{form.alertRules.map((rule, index) => <div className="rule-row" key={rule.id}><span><b>{rule.name}</b><code>{rule.expr}</code></span><label>阈值<input type="number" value={rule.threshold} onChange={e => ruleField(index, "threshold", Number(e.target.value))} /></label><label>连续次数<input type="number" min="1" value={rule.forChecks} onChange={e => ruleField(index, "forChecks", Math.max(1, Number(e.target.value)))} /></label></div>)}</div></div></>}
  {isMcp && diagnostic && <div className="diagnostic-list"><b>连接诊断</b>{diagnostic.steps.map(step => <div className={step.success ? "ok" : "failed"} key={step.name}><i>{step.success ? "✓" : "!"}</i><span><b>{step.name}</b><small>{step.detail}</small></span><time>{step.durationMs} ms</time></div>)}</div>}{isMcp && tools.length > 0 && <div className="tool-list"><b>已发现工具</b>{tools.slice(0, 8).map(tool => <span key={tool.name}><code>{tool.name}</code><small>{tool.description}</small></span>)}</div>}<div className="form-actions">{isMcp && <Button leftSection={<IconDownload size={16} />} onClick={install} loading={installing}>安装 mcp-grafana</Button>}{isMcp && <Button variant="default" onClick={test} loading={testing}>连接诊断</Button>}{!isMcp && <Button variant="default" onClick={checkNow} loading={checking}>立即检查</Button>}<Button variant={isMcp ? "light" : "filled"} onClick={save}>保存设置</Button><span className="form-message">{message}</span></div></div><aside className="card safety"><span>♢</span><h3>默认安全策略</h3><ul><li>mcp-grafana 使用 --disable-write</li><li>建议使用 Viewer 最小权限账号</li><li>Token 与 API Key 保存到系统 Keychain</li><li>告警具备连续触发、冷却和恢复通知</li></ul></aside></div></>;
}

export default function WatchDogWorkspace() {
  const [page, setPage] = useState<Page>("dashboard"); const [reports, setReports] = useState<Report[]>([]); const [selected, setSelected] = useState<Report | null>(null); const [settings, setSettings] = useState(defaultSettings); const [generating, setGenerating] = useState(false); const [toast, setToast] = useState(""); const [loadError, setLoadError] = useState("");
  useEffect(() => { listReports().then(setReports).catch(error => setLoadError(errorMessage(error, "读取日报失败"))); loadSettings().then(setSettings).catch(error => setLoadError(errorMessage(error, "读取设置失败"))); }, []);
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let disposed = false; const cleanups: (() => void)[] = [];
    const notify = (event: AlertEvent) => {
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
    ]).then(unlisteners => disposed ? unlisteners.forEach(fn => fn()) : cleanups.push(...unlisteners));
    return () => { disposed = true; cleanups.forEach(fn => fn()); };
  }, []);
  const report = selected ?? reports[0];
  const title = useMemo(() => nav.find(n => n.id === page)?.label, [page]);
  const run = async () => { setGenerating(true); try { const next = await generateReport(); setReports(old => [next, ...old.filter(r => r.id !== next.id)]); setSelected(next); setToast("今日日报生成完成"); } catch (error) { setToast(errorMessage(error, "日报生成失败")); } finally { setGenerating(false); setTimeout(() => setToast(""), 6000); } };
  const navigate = (p: Page) => { setPage(p); if (p !== "reports") setSelected(null); };
  return <div className="app"><aside className="sidebar"><div className="brand"><WatchDogMark /><div><b>Grafana Watch Dog</b><small>AIOPS CONTROL</small></div></div><nav>{nav.map(n => { const NavIcon = icons[n.id]; return <button key={n.id} className={page === n.id ? "active" : ""} onClick={() => navigate(n.id)}><i><NavIcon size={17} /></i>{n.label}</button>; })}</nav><div className="sidebar-bottom"><div className="connection"><i /><span><b>{settings.monitorEnabled ? "监控运行中" : "监控未启用"}</b><small>{settings.monitorEnabled ? `每 ${settings.monitorIntervalMinutes} 分钟检查` : "等待 Grafana 配置"}</small></span></div><button className="profile"><span>CF</span><div><b>Ops Admin</b><small>本地工作区</small></div><i>•••</i></button></div></aside><main><header><button className="mobile-brand"><WatchDogMark size={25} /></button><span>{title}</span><div><span className="readonly"><i />只读模式</span><Tooltip label="刷新"><button className="icon-btn" aria-label="刷新"><IconRefresh size={15} /></button></Tooltip></div></header><div className="content">{loadError && <div className="load-error">{loadError}</div>}{page === "dashboard" && (report ? <Dashboard report={report} onGenerate={run} generating={generating} /> : <><div className="page-title"><div><p className="eyebrow">DAILY OVERVIEW</p><h1>运行总览</h1><p>仅展示实际采集并持久化的运行数据。</p></div></div><EmptyState title="暂无真实运行数据" detail="请先配置 Grafana MCP。真实日报采集器接入前不会生成占位报告。" /></>)}{page === "reports" && !selected && <Reports reports={reports} onSelect={setSelected} />}{page === "reports" && selected && <><button className="back" onClick={() => setSelected(null)}>← 返回日报历史</button><Dashboard report={selected} onGenerate={run} generating={generating} /></>}{page === "analysis" && <Analysis />}{page === "mcp" && <SettingsPage initial={settings} section="mcp" onSaved={setSettings} />}{page === "settings" && <SettingsPage initial={settings} section="settings" onSaved={setSettings} />}</div></main>{toast && <div className="toast">{toast}</div>}</div>;
}
