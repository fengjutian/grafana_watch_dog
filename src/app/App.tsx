import { useEffect, useMemo, useState } from "react";
import { Button, Tooltip } from "@mantine/core";
import { IconActivityHeartbeat, IconAdjustments, IconBrain, IconFileAnalytics, IconLayoutDashboard, IconRefresh, IconServerCog, IconSparkles } from "@tabler/icons-react";
import { generateReport, listMcpTools, listReports, loadSettings, saveSettings, testConnection } from "../infrastructure/tauri/client";
import { defaultSettings } from "../infrastructure/demo/reportFixtures";
import type { AppSettings, Issue, McpTool, Report, Status } from "../domain/report/types";

type Page = "dashboard" | "reports" | "analysis" | "mcp" | "settings";

const icons = { dashboard: IconLayoutDashboard, reports: IconFileAnalytics, analysis: IconBrain, mcp: IconServerCog, settings: IconAdjustments };
const nav: { id: Page; label: string }[] = [
  { id: "dashboard", label: "运行总览" }, { id: "reports", label: "日报历史" }, { id: "analysis", label: "AI 分析" },
  { id: "mcp", label: "MCP 服务" }, { id: "settings", label: "系统设置" },
];

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
      <div className="card health-card"><div className="section-head"><div><span className="section-kicker">SYSTEM HEALTH</span><h2>系统健康度</h2></div><span className={`status-pill ${report.status}`}>● {statusLabel(report.status)}</span></div><HealthGauge score={report.score} /><p className="comparison"><b>↓ 5</b> 较昨日 · 主要受数据库性能影响</p></div>
      <div className="card conclusion"><div className="conclusion-top"><span className="ai-mark"><IconSparkles size={20} /></span><div><span className="section-kicker">AI CONCLUSION</span><h2>今日结论</h2></div></div><blockquote>{report.summary}</blockquote><div className="stat-row"><div><b className="red-text">{report.stats.critical}</b><span>严重问题</span></div><div><b className="amber-text">{report.stats.warning}</b><span>需要关注</span></div><div><b className="green-text">{report.stats.healthy}</b><span>正常指标</span></div><div><b>{report.stats.alerts}</b><span>昨日告警</span></div></div></div>
    </section>
    <section><div className="section-title"><div><p className="eyebrow">SERVICE PULSE</p><h2>服务状态</h2></div><span>数据更新于 {report.generatedAt.split(" ").at(-1)}</span></div><div className="service-grid">{report.services.map((s) => <div className="card service-card" key={s.name}><div className="service-top"><div className={`service-icon ${scoreTone(s.score)}`}>{s.kind.slice(0, 1)}</div><div><h3>{s.name}</h3><small>{s.kind}</small></div><b className={scoreTone(s.score)}>{s.score}</b></div><div className="meter"><i style={{ width: `${s.score}%` }} className={scoreTone(s.score)} /></div><div className="metric-chips">{s.metrics.map((m) => <span key={m}>{m}</span>)}</div></div>)}</div></section>
    <section><div className="section-title"><div><p className="eyebrow">7-DAY SIGNAL</p><h2>关键趋势</h2></div><span>对比过去 7 天均值</span></div><div className="trend-grid">{report.trends.map((t) => <div className="card trend-card" key={t.label}><div><span>{t.label}</span><strong>{t.value.toLocaleString()}{t.unit}</strong><small className={t.change > 20 ? "red-text" : "amber-text"}>↑ {t.change}%</small></div><Sparkline data={t.history} /></div>)}</div></section>
    <section><div className="section-title"><div><p className="eyebrow">PRIORITY QUEUE</p><h2>优先处理</h2></div><span>{report.issues.length} 项分析结果</span></div><div className="issues">{report.issues.map((i) => <IssueCard issue={i} key={i.id} />)}</div></section>
  </>;
}

function Reports({ reports, onSelect }: { reports: Report[]; onSelect: (r: Report) => void }) {
  return <><div className="page-title"><div><p className="eyebrow">REPORT ARCHIVE</p><h1>日报历史</h1><p>回看系统健康度变化，快速定位状态转折点。</p></div></div><div className="card report-table"><div className="table-head"><span>日期</span><span>健康度</span><span>状态</span><span>关键摘要</span><span /></div>{reports.map((r, i) => <button className="report-row" key={r.id} onClick={() => onSelect(r)}><span><b>{r.date}</b><small>{i === 0 ? "今天" : "08:00 生成"}</small></span><span className={`score ${scoreTone(r.score)}`}>{r.score}</span><span><i className={`status-pill ${r.status}`}>● {statusLabel(r.status)}</i></span><span>{r.summary}</span><span>→</span></button>)}</div></>;
}

function Analysis({ report }: { report: Report }) {
  const [query, setQuery] = useState(""); const [answer, setAnswer] = useState(""); const [busy, setBusy] = useState(false);
  const ask = () => { if (!query.trim()) return; setBusy(true); setAnswer(""); setTimeout(() => { setAnswer(`根据 Prometheus、Loki 与告警时间线，${report.summary} 当前最强关联信号是慢查询 +188%、API P95 +24% 与 14:32 的 OOM 事件。建议先核对 Top Slow SQL，再关联检查该时间窗口的发布记录。`); setBusy(false); }, 900); };
  return <><div className="page-title"><div><p className="eyebrow">AI INVESTIGATION</p><h1>向运行数据提问</h1><p>AI 会编排只读查询，并给出带证据的分析结论。</p></div></div><div className="analysis-layout"><div className="card chat"><div className="chat-empty"><span className="ai-orb">✦</span><h2>今天想调查什么？</h2><p>试试询问异常原因、变化趋势或下一步建议。</p><div className="suggestions">{["昨天 API 为什么变慢？", "MySQL 风险来自哪里？", "列出最需要关注的服务"].map(q => <button onClick={() => setQuery(q)} key={q}>{q}</button>)}</div></div>{(busy || answer) && <div className="answer"><span>✦</span><p>{busy ? "正在查询 Prometheus、Loki 与 Alerting…" : answer}</p></div>}<div className="composer"><textarea value={query} onChange={e => setQuery(e.target.value)} placeholder="询问过去 24 小时的运行情况…" /><button className="primary" onClick={ask}>发送 ↑</button></div></div><aside className="card evidence"><h3>查询范围</h3>{["Prometheus 指标", "Loki 日志", "Grafana Alerting", "Dashboard 元数据"].map((x, i) => <div key={x}><i>{i + 1}</i><span>{x}<small>只读访问</small></span><b>✓</b></div>)}<p>所有查询都通过 mcp-grafana 执行，MVP 默认禁用写操作。</p></aside></div></>;
}

function SettingsPage({ initial, section }: { initial: AppSettings; section: "mcp" | "settings" }) {
  const [form, setForm] = useState(initial); const [message, setMessage] = useState(""); const [testing, setTesting] = useState(false);
  const [tools, setTools] = useState<McpTool[]>([]);
  useEffect(() => setForm(initial), [initial]);
  const field = (key: keyof AppSettings, value: string | boolean) => setForm({ ...form, [key]: value });
  const test = async () => { setTesting(true); setMessage(""); try { const result = await testConnection(form); setTools(await listMcpTools(form)); setMessage(result); } catch (e) { setTools([]); setMessage(e instanceof Error ? e.message : "连接失败"); } finally { setTesting(false); } };
  const save = async () => { await saveSettings(form); setMessage("设置已保存；敏感凭据不会写入浏览器存储。"); };
  const isMcp = section === "mcp";
  return <><div className="page-title"><div><p className="eyebrow">{isMcp ? "CONNECTIONS" : "PREFERENCES"}</p><h1>{isMcp ? "MCP 服务" : "系统设置"}</h1><p>{isMcp ? "连接 Grafana 的只读数据入口。" : "配置模型与日报生成计划。"}</p></div></div><div className="settings-grid"><div className="card form-card"><div className="section-head"><div><span className="section-kicker">{isMcp ? "GRAFANA PRODUCTION" : "AI PROVIDER"}</span><h2>{isMcp ? "Grafana MCP" : "分析模型"}</h2></div><span className="status-pill healthy">● {isMcp ? "演示模式" : "待配置"}</span></div>{isMcp ? <>
    <label>Grafana 地址<input value={form.grafanaUrl} onChange={e => field("grafanaUrl", e.target.value)} /></label><label>Service Account Token<input type="password" value={form.grafanaToken} onChange={e => field("grafanaToken", e.target.value)} placeholder="保存到系统安全存储" /></label><div className="form-pair"><label>MCP 命令<input value={form.mcpCommand} onChange={e => field("mcpCommand", e.target.value)} /></label><label>启动参数<input value={form.mcpArgs} onChange={e => field("mcpArgs", e.target.value)} /></label></div>
  </> : <><div className="form-pair"><label>Provider<select value={form.aiProvider} onChange={e => field("aiProvider", e.target.value)}><option>DeepSeek</option><option>Qwen</option><option>OpenAI</option><option>Custom OpenAI Compatible</option></select></label><label>模型<input value={form.aiModel} onChange={e => field("aiModel", e.target.value)} /></label></div><label>API Base URL<input value={form.aiBaseUrl} onChange={e => field("aiBaseUrl", e.target.value)} /></label><label>API Key<input type="password" value={form.aiKey} onChange={e => field("aiKey", e.target.value)} placeholder="保存到系统安全存储" /></label><div className="schedule"><div><b>自动生成日报</b><small>每天在指定时间运行分析</small></div><input type="time" value={form.scheduleTime} onChange={e => field("scheduleTime", e.target.value)} /><button className={`toggle ${form.scheduleEnabled ? "on" : ""}`} onClick={() => field("scheduleEnabled", !form.scheduleEnabled)}><i /></button></div></>}
  {isMcp && tools.length > 0 && <div className="tool-list"><b>已发现工具</b>{tools.slice(0, 8).map(tool => <span key={tool.name}><code>{tool.name}</code><small>{tool.description}</small></span>)}</div>}<div className="form-actions">{isMcp && <Button variant="default" onClick={test} loading={testing}>测试连接</Button>}<Button onClick={save}>保存设置</Button><span className="form-message">{message}</span></div></div><aside className="card safety"><span>♢</span><h3>默认安全策略</h3><ul><li>mcp-grafana 使用 --disable-write</li><li>建议使用 Viewer 最小权限账号</li><li>Token 与 API Key 不写入 SQLite</li><li>AI 仅能分析并提供建议</li></ul></aside></div></>;
}

export default function App() {
  const [page, setPage] = useState<Page>("dashboard"); const [reports, setReports] = useState<Report[]>([]); const [selected, setSelected] = useState<Report | null>(null); const [settings, setSettings] = useState(defaultSettings); const [generating, setGenerating] = useState(false); const [toast, setToast] = useState("");
  useEffect(() => { listReports().then(setReports); loadSettings().then(setSettings); }, []);
  const report = selected ?? reports[0];
  const title = useMemo(() => nav.find(n => n.id === page)?.label, [page]);
  const run = async () => { setGenerating(true); try { const next = await generateReport(); setReports(old => [next, ...old.filter(r => r.id !== next.id)]); setSelected(next); setToast("今日日报生成完成"); setTimeout(() => setToast(""), 2600); } finally { setGenerating(false); } };
  const navigate = (p: Page) => { setPage(p); if (p !== "reports") setSelected(null); };
  return <div className="app"><aside className="sidebar"><div className="brand"><span><IconActivityHeartbeat size={21} /></span><div><b>Grafana Watch Dog</b><small>AIOPS DAILY</small></div></div><nav>{nav.map(n => { const NavIcon = icons[n.id]; return <button key={n.id} className={page === n.id ? "active" : ""} onClick={() => navigate(n.id)}><i><NavIcon size={18} /></i>{n.label}</button>; })}</nav><div className="sidebar-bottom"><div className="connection"><i /><span><b>演示数据</b><small>等待 Grafana 配置</small></span></div><button className="profile"><span>CF</span><div><b>Ops Admin</b><small>本地工作区</small></div><i>•••</i></button></div></aside><main><header><button className="mobile-brand"><IconActivityHeartbeat size={18} /></button><span>{title}</span><div><span className="readonly">◉ 只读模式</span><Tooltip label="刷新"><button className="icon-btn" aria-label="刷新"><IconRefresh size={16} /></button></Tooltip></div></header><div className="content">{report && page === "dashboard" && <Dashboard report={report} onGenerate={run} generating={generating} />}{page === "reports" && !selected && <Reports reports={reports} onSelect={setSelected} />}{page === "reports" && selected && <><button className="back" onClick={() => setSelected(null)}>← 返回日报历史</button><Dashboard report={selected} onGenerate={run} generating={generating} /></>}{report && page === "analysis" && <Analysis report={report} />}{page === "mcp" && <SettingsPage initial={settings} section="mcp" />}{page === "settings" && <SettingsPage initial={settings} section="settings" />}</div></main>{toast && <div className="toast">✓ {toast}</div>}</div>;
}
