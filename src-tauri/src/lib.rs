use chrono::Local;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Mutex, thread, time::Duration};
use tauri::{Emitter, Manager, State};

mod mcp;
mod monitor;
use mcp::{
    install_official_server, GrafanaMcpClient, GrafanaMcpConfig, InstallResult, ToolSummary,
};
use monitor::{evaluate, extract_metric_values, AlertEvent, AlertRule, AlertState, Comparison};

struct Database(Mutex<Connection>);
struct RuntimeSettings(Mutex<AppSettings>);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    grafana_url: String,
    #[serde(default, skip_serializing)]
    grafana_token: String,
    mcp_command: String,
    mcp_args: String,
    ai_provider: String,
    ai_base_url: String,
    ai_model: String,
    #[serde(default, skip_serializing)]
    ai_key: String,
    schedule_enabled: bool,
    schedule_time: String,
    #[serde(default)]
    monitor_enabled: bool,
    #[serde(default = "default_monitor_interval")]
    monitor_interval_minutes: u64,
    #[serde(default)]
    prometheus_datasource_uid: String,
    #[serde(default = "default_cooldown")]
    alert_cooldown_minutes: i64,
    #[serde(default = "default_alert_rules")]
    alert_rules: Vec<AlertRule>,
}

fn default_monitor_interval() -> u64 { 5 }
fn default_cooldown() -> i64 { 30 }
fn default_alert_rules() -> Vec<AlertRule> {
    vec![
        AlertRule { id:"cpu".into(), name:"CPU 使用率".into(), expr:"100 - (avg by(instance) (rate(node_cpu_seconds_total{mode=\"idle\"}[5m])) * 100)".into(), operator:Comparison::GreaterThan, threshold:85.0, for_checks:2, severity:"critical".into(), unit:"%".into() },
        AlertRule { id:"memory".into(), name:"内存使用率".into(), expr:"(1 - node_memory_MemAvailable_bytes / node_memory_MemTotal_bytes) * 100".into(), operator:Comparison::GreaterThan, threshold:90.0, for_checks:1, severity:"critical".into(), unit:"%".into() },
        AlertRule { id:"disk".into(), name:"磁盘剩余空间".into(), expr:"node_filesystem_avail_bytes{fstype!~\"tmpfs|overlay\"} / node_filesystem_size_bytes * 100".into(), operator:Comparison::LessThan, threshold:10.0, for_checks:1, severity:"critical".into(), unit:"%".into() },
        AlertRule { id:"server_up".into(), name:"服务器在线状态".into(), expr:"up{job=~\"node.*\"}".into(), operator:Comparison::LessThan, threshold:1.0, for_checks:1, severity:"critical".into(), unit:"".into() },
    ]
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            grafana_url: "http://localhost:3000".into(),
            grafana_token: String::new(),
            mcp_command: "mcp-grafana".into(),
            mcp_args: "--transport stdio --disable-write --enabled-tools search,datasource,prometheus,loki,alerting,dashboard".into(),
            ai_provider: "DeepSeek".into(),
            ai_base_url: "https://api.deepseek.com".into(),
            ai_model: "deepseek-chat".into(),
            ai_key: String::new(),
            schedule_enabled: true,
            schedule_time: "08:00".into(),
            monitor_enabled: false,
            monitor_interval_minutes: default_monitor_interval(),
            prometheus_datasource_uid: String::new(),
            alert_cooldown_minutes: default_cooldown(),
            alert_rules: default_alert_rules(),
        }
    }
}

fn db_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("grafana_watch_dog.db"))
}

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS reports (
          id TEXT PRIMARY KEY, report_date TEXT NOT NULL, score INTEGER NOT NULL,
          status TEXT NOT NULL, summary TEXT NOT NULL, report_json TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_reports_date ON reports(report_date DESC);
        CREATE TABLE IF NOT EXISTS alert_states (
          rule_id TEXT PRIMARY KEY, state_json TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS alert_events (
          id TEXT PRIMARY KEY, rule_id TEXT NOT NULL, kind TEXT NOT NULL,
          severity TEXT NOT NULL, message TEXT NOT NULL, event_json TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_alert_events_created ON alert_events(created_at DESC);",
    )
}

fn demo_report() -> Value {
    let now = Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    json!({
      "id": format!("report-{date}"), "date": date, "score": 87, "status": "warning",
      "generatedAt": now.format("%Y-%m-%d %H:%M").to_string(),
      "summary": "系统整体稳定，但数据库性能出现明显恶化趋势。慢查询增长显著高于业务流量增长，建议优先检查订单查询相关 SQL 与索引。",
      "stats": { "critical": 2, "warning": 3, "healthy": 18, "alerts": 8 },
      "services": [
        { "name":"服务器", "kind":"Server", "score":92, "metrics":["CPU 52%","内存 71%","磁盘 53%"] },
        { "name":"数据库", "kind":"MySQL", "score":81, "metrics":["QPS 1,240","慢查询 1,823","死锁 12"] },
        { "name":"API", "kind":"FastAPI", "score":94, "metrics":["P95 1.82s","错误率 3.1%","QPS 684"] },
        { "name":"日志", "kind":"Loki", "score":73, "metrics":["ERROR 128","Timeout 42","OOM 1"] }
      ],
      "trends": [
        { "label":"CPU","value":52,"unit":"%","change":12,"history":[38,41,45,43,48,49,52] },
        { "label":"Memory","value":71,"unit":"%","change":18,"history":[54,58,57,62,66,68,71] },
        { "label":"QPS","value":1240,"unit":"","change":31,"history":[820,910,880,1010,1100,1180,1240] },
        { "label":"API P95","value":1.82,"unit":"s","change":24,"history":[1.15,1.22,1.31,1.28,1.55,1.69,1.82] }
      ],
      "issues": [
        { "id":"mysql-slow","severity":"critical","title":"MySQL 慢查询异常增长","source":"Prometheus · MySQL","change":"+188%","reason":"慢查询增长明显高于 QPS 的 39% 增长，疑似 SQL 性能退化，而非单纯业务流量增长。","recommendations":["检查 Top Slow SQL","检查 orders 表索引","检查连接池与锁等待"] },
        { "id":"oom","severity":"critical","title":"FastAPI 发生 OOM","source":"Loki · 14:32","change":"1 次","reason":"OOM 前 15 分钟内存与 Swap 持续上涨，并伴随 API P95 延迟升高。","recommendations":["检查进程内存快照","核对当时请求峰值","检查最近发布变更"] },
        { "id":"memory","severity":"warning","title":"orderslave 内存压力升高","source":"Prometheus · Node","change":"+18%","reason":"内存已达到 88%，过去 7 天持续上升。","recommendations":["确认缓存占用","检查异常进程"] }
      ]
    })
}

#[tauri::command]
fn list_reports(db: State<'_, Database>) -> Result<Vec<Value>, String> {
    let conn = db.0.lock().map_err(|_| "数据库锁异常".to_string())?;
    let mut stmt = conn
        .prepare("SELECT report_json FROM reports ORDER BY report_date DESC LIMIT 90")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let reports: Vec<Value> = rows
        .filter_map(Result::ok)
        .filter_map(|s| serde_json::from_str(&s).ok())
        .collect();
    if reports.is_empty() {
        Ok(vec![demo_report()])
    } else {
        Ok(reports)
    }
}

#[tauri::command]
fn generate_report(db: State<'_, Database>) -> Result<Value, String> {
    // This deterministic collector is the offline fallback. The Grafana MCP collector
    // can replace it without changing the report schema or UI contract.
    let report = demo_report();
    let conn = db.0.lock().map_err(|_| "数据库锁异常".to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO reports (id,report_date,score,status,summary,report_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![report["id"].as_str(), report["date"].as_str(), report["score"].as_i64(), report["status"].as_str(), report["summary"].as_str(), report.to_string(), report["generatedAt"].as_str()]
    ).map_err(|e| e.to_string())?;
    Ok(report)
}

#[tauri::command]
fn load_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let path = settings_path(&app)?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, runtime: State<'_, RuntimeSettings>, settings: AppSettings) -> Result<(), String> {
    *runtime.0.lock().map_err(|_| "运行时设置锁异常".to_string())? = settings.clone();
    let mut settings = settings;
    // Secrets are deliberately excluded until an OS-keychain adapter is configured.
    settings.grafana_token.clear();
    settings.ai_key.clear();
    let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(&app)?, raw).map_err(|e| e.to_string())
}

#[tauri::command]
fn test_connection(settings: AppSettings) -> Result<String, String> {
    if settings.grafana_url.trim().is_empty() {
        return Err("请填写 Grafana 地址".into());
    }
    if !settings
        .mcp_args
        .split_whitespace()
        .any(|arg| arg == "--disable-write")
    {
        return Err("安全检查失败：MVP 必须使用 --disable-write".into());
    }
    let mut client = GrafanaMcpClient::connect(mcp_config(&settings))?;
    let tools = client.list_tools()?;
    Ok(format!(
        "已连接官方 mcp-grafana，共发现 {} 个只读工具",
        tools.len()
    ))
}

fn mcp_config(settings: &AppSettings) -> GrafanaMcpConfig {
    GrafanaMcpConfig {
        command: settings.mcp_command.clone(),
        args: settings
            .mcp_args
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
        grafana_url: settings.grafana_url.clone(),
        service_account_token: settings.grafana_token.clone(),
    }
}

#[tauri::command]
fn list_mcp_tools(settings: AppSettings) -> Result<Vec<ToolSummary>, String> {
    if !settings
        .mcp_args
        .split_whitespace()
        .any(|arg| arg == "--disable-write")
    {
        return Err("安全检查失败：MVP 必须使用 --disable-write".into());
    }
    GrafanaMcpClient::connect(mcp_config(&settings))?.list_tools()
}

#[tauri::command]
fn call_mcp_tool(settings: AppSettings, name: String, arguments: Value) -> Result<Value, String> {
    if !settings
        .mcp_args
        .split_whitespace()
        .any(|arg| arg == "--disable-write")
    {
        return Err("安全检查失败：MVP 必须使用 --disable-write".into());
    }
    GrafanaMcpClient::connect(mcp_config(&settings))?.call_tool(&name, arguments)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MonitorRunResult {
    checked: usize,
    events: Vec<AlertEvent>,
    errors: Vec<String>,
    completed_at: String,
}

fn execute_monitor(app: &tauri::AppHandle, settings: &AppSettings) -> Result<MonitorRunResult, String> {
    if settings.prometheus_datasource_uid.trim().is_empty() {
        return Err("请先配置 Prometheus 数据源 UID".into());
    }
    if !settings.mcp_args.split_whitespace().any(|arg| arg == "--disable-write") {
        return Err("安全检查失败：监控必须使用 --disable-write".into());
    }
    let conn = Connection::open(db_path(app)?).map_err(|e| e.to_string())?;
    let mut client = GrafanaMcpClient::connect(mcp_config(settings))?;
    let now = Local::now();
    let mut events = Vec::new();
    let mut errors = Vec::new();

    for rule in &settings.alert_rules {
        let response = client.call_tool("query_prometheus", json!({
            "datasourceUid": settings.prometheus_datasource_uid,
            "expr": rule.expr,
            "queryType": "instant",
            "startTime": "now"
        }));
        let value = match response
            .and_then(|value| extract_metric_values(&value))
            .and_then(|values| rule.operator.aggregate(values.into_iter()).ok_or_else(|| "查询结果为空".into()))
        {
            Ok(value) => value,
            Err(error) => { errors.push(format!("{}：{}", rule.name, error)); continue; }
        };
        let previous = conn.query_row(
            "SELECT state_json FROM alert_states WHERE rule_id=?1", [&rule.id],
            |row| row.get::<_, String>(0),
        ).ok().and_then(|raw| serde_json::from_str::<AlertState>(&raw).ok());
        let (state, event) = evaluate(rule, value, previous, settings.alert_cooldown_minutes.max(0), now);
        conn.execute(
            "INSERT OR REPLACE INTO alert_states(rule_id,state_json) VALUES (?1,?2)",
            params![rule.id, serde_json::to_string(&state).map_err(|e| e.to_string())?],
        ).map_err(|e| e.to_string())?;
        if let Some(event) = event {
            let raw = serde_json::to_string(&event).map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO alert_events(id,rule_id,kind,severity,message,event_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![event.id, event.rule_id, event.kind, event.severity, event.message, raw, event.created_at],
            ).map_err(|e| e.to_string())?;
            let _ = app.emit("monitor-alert", &event);
            events.push(event);
        }
    }
    Ok(MonitorRunResult { checked: settings.alert_rules.len(), events, errors, completed_at: now.to_rfc3339() })
}

#[tauri::command]
fn run_monitor_now(app: tauri::AppHandle, runtime: State<'_, RuntimeSettings>, settings: AppSettings) -> Result<MonitorRunResult, String> {
    *runtime.0.lock().map_err(|_| "运行时设置锁异常".to_string())? = settings.clone();
    execute_monitor(&app, &settings)
}

#[tauri::command]
fn list_alert_events(app: tauri::AppHandle) -> Result<Vec<AlertEvent>, String> {
    let conn = Connection::open(db_path(&app)?).map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT event_json FROM alert_events ORDER BY created_at DESC LIMIT 100").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0)).map_err(|e| e.to_string())?;
    Ok(rows.filter_map(Result::ok).filter_map(|raw| serde_json::from_str(&raw).ok()).collect())
}

fn start_monitor_scheduler(app: tauri::AppHandle) {
    thread::spawn(move || {
        let mut elapsed_seconds = 0_u64;
        loop {
            thread::sleep(Duration::from_secs(30));
            elapsed_seconds = elapsed_seconds.saturating_add(30);
            let settings = match app.state::<RuntimeSettings>().0.lock() {
                Ok(settings) => settings.clone(), Err(_) => continue,
            };
            if !settings.monitor_enabled { elapsed_seconds = 0; continue; }
            let interval = settings.monitor_interval_minutes.max(1).saturating_mul(60);
            if elapsed_seconds < interval { continue; }
            elapsed_seconds = 0;
            if let Err(error) = execute_monitor(&app, &settings) {
                let _ = app.emit("monitor-error", error);
            }
        }
    });
}

#[tauri::command]
async fn install_mcp_grafana(app: tauri::AppHandle) -> Result<InstallResult, String> {
    let tools_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("tools");
    tauri::async_runtime::spawn_blocking(move || install_official_server(&tools_dir))
        .await
        .map_err(|e| format!("安装任务异常：{e}"))?
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let conn = Connection::open(db_path(app.handle())?)?;
            init_db(&conn)?;
            app.manage(Database(Mutex::new(conn)));
            let settings = load_settings(app.handle().clone()).unwrap_or_default();
            app.manage(RuntimeSettings(Mutex::new(settings)));
            start_monitor_scheduler(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_reports,
            generate_report,
            load_settings,
            save_settings,
            test_connection,
            list_mcp_tools,
            call_mcp_tool,
            run_monitor_now,
            list_alert_events,
            install_mcp_grafana
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Grafana Watch Dog");
}
