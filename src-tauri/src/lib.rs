use chrono::Local;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, State};

mod ai;
mod credentials;
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
    #[serde(default = "default_retry_attempts")]
    mcp_retry_attempts: u32,
}

fn default_monitor_interval() -> u64 {
    5
}
fn default_cooldown() -> i64 {
    30
}
fn default_retry_attempts() -> u32 {
    3
}
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
            ai_provider: "MiniMax（国内）".into(),
            ai_base_url: "https://api.minimaxi.com/v1".into(),
            ai_model: "MiniMax-M2.7".into(),
            ai_key: String::new(),
            schedule_enabled: false,
            schedule_time: "08:00".into(),
            monitor_enabled: false,
            monitor_interval_minutes: default_monitor_interval(),
            prometheus_datasource_uid: String::new(),
            alert_cooldown_minutes: default_cooldown(),
            alert_rules: default_alert_rules(),
            mcp_retry_attempts: default_retry_attempts(),
        }
    }
}

fn hydrate_credentials(mut settings: AppSettings) -> AppSettings {
    if settings.grafana_token.is_empty() {
        settings.grafana_token = credentials::load_grafana_token();
    }
    if settings.ai_key.is_empty() {
        settings.ai_key = credentials::load_ai_key();
    }
    settings
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

#[tauri::command]
fn list_reports(db: State<'_, Database>) -> Result<Vec<Value>, String> {
    let conn = db.0.lock().map_err(|_| "数据库锁异常".to_string())?;
    let mut stmt = conn
        .prepare("SELECT report_json FROM reports ORDER BY report_date DESC LIMIT 90")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    Ok(rows
        .filter_map(Result::ok)
        .filter_map(|s| serde_json::from_str(&s).ok())
        .collect())
}

#[tauri::command]
fn generate_report() -> Result<Value, String> {
    Err("真实日报采集器尚未接入；未生成任何占位数据".into())
}

#[tauri::command]
fn load_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let path = settings_path(&app)?;
    if !path.exists() {
        return Ok(hydrate_credentials(AppSettings::default()));
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let settings: AppSettings = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    Ok(hydrate_credentials(settings))
}

#[tauri::command]
fn save_settings(
    app: tauri::AppHandle,
    runtime: State<'_, RuntimeSettings>,
    settings: AppSettings,
) -> Result<(), String> {
    let settings = hydrate_credentials(settings);
    credentials::save(&settings.grafana_token, &settings.ai_key)?;
    *runtime
        .0
        .lock()
        .map_err(|_| "运行时设置锁异常".to_string())? = settings.clone();
    let mut settings = settings;
    // Secrets live in the OS credential store and are never serialized to settings.json.
    settings.grafana_token.clear();
    settings.ai_key.clear();
    let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(&app)?, raw).map_err(|e| e.to_string())
}

#[tauri::command]
fn test_connection(settings: AppSettings) -> Result<String, String> {
    let settings = hydrate_credentials(settings);
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
    let mut client = connect_with_retry(&settings)?;
    let tools = client.list_tools()?;
    Ok(format!(
        "已连接官方 mcp-grafana，共发现 {} 个只读工具",
        tools.len()
    ))
}

fn connect_with_retry(settings: &AppSettings) -> Result<GrafanaMcpClient, String> {
    let attempts = settings.mcp_retry_attempts.clamp(1, 5);
    let mut last_error = String::new();
    for attempt in 1..=attempts {
        match GrafanaMcpClient::connect(mcp_config(settings)) {
            Ok(client) => return Ok(client),
            Err(error) => last_error = format!("第 {attempt}/{attempts} 次：{error}"),
        }
        if attempt < attempts {
            thread::sleep(Duration::from_millis(300 * 2_u64.pow(attempt - 1)));
        }
    }
    Err(last_error)
}

fn call_tool_with_retry(
    client: &mut GrafanaMcpClient,
    settings: &AppSettings,
    name: &str,
    arguments: Value,
) -> Result<Value, String> {
    let attempts = settings.mcp_retry_attempts.clamp(1, 5);
    let mut last_error = String::new();
    for attempt in 1..=attempts {
        match client.call_tool(name, arguments.clone()) {
            Ok(value) => return Ok(value),
            Err(error) => last_error = format!("第 {attempt}/{attempts} 次：{error}"),
        }
        if attempt < attempts {
            thread::sleep(Duration::from_millis(300 * 2_u64.pow(attempt - 1)));
            *client = connect_with_retry(settings)?;
        }
    }
    Err(last_error)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticStep {
    name: String,
    success: bool,
    detail: String,
    duration_ms: u128,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionDiagnostic {
    success: bool,
    attempts: u32,
    steps: Vec<DiagnosticStep>,
}

#[tauri::command]
fn diagnose_connection(settings: AppSettings) -> ConnectionDiagnostic {
    let settings = hydrate_credentials(settings);
    let mut steps = Vec::new();
    let started = Instant::now();
    let config_error = if settings.grafana_url.trim().is_empty() {
        Some("Grafana 地址为空")
    } else if settings.grafana_token.trim().is_empty() {
        Some("Grafana Token 为空")
    } else if !settings
        .mcp_args
        .split_whitespace()
        .any(|arg| arg == "--disable-write")
    {
        Some("启动参数缺少 --disable-write")
    } else {
        None
    };
    steps.push(DiagnosticStep {
        name: "配置检查".into(),
        success: config_error.is_none(),
        detail: config_error
            .unwrap_or("地址、Token 与只读参数已配置")
            .into(),
        duration_ms: started.elapsed().as_millis(),
    });
    if config_error.is_some() {
        return ConnectionDiagnostic {
            success: false,
            attempts: 0,
            steps,
        };
    }

    let handshake = Instant::now();
    let mut client = match connect_with_retry(&settings) {
        Ok(client) => {
            steps.push(DiagnosticStep {
                name: "MCP 握手".into(),
                success: true,
                detail: "子进程启动并完成 initialize".into(),
                duration_ms: handshake.elapsed().as_millis(),
            });
            client
        }
        Err(error) => {
            steps.push(DiagnosticStep {
                name: "MCP 握手".into(),
                success: false,
                detail: error,
                duration_ms: handshake.elapsed().as_millis(),
            });
            return ConnectionDiagnostic {
                success: false,
                attempts: settings.mcp_retry_attempts.clamp(1, 5),
                steps,
            };
        }
    };
    let discovery = Instant::now();
    let tools = match client.list_tools() {
        Ok(tools) => {
            steps.push(DiagnosticStep {
                name: "工具发现".into(),
                success: true,
                detail: format!("发现 {} 个工具", tools.len()),
                duration_ms: discovery.elapsed().as_millis(),
            });
            tools
        }
        Err(error) => {
            steps.push(DiagnosticStep {
                name: "工具发现".into(),
                success: false,
                detail: error,
                duration_ms: discovery.elapsed().as_millis(),
            });
            return ConnectionDiagnostic {
                success: false,
                attempts: 1,
                steps,
            };
        }
    };
    let auth = Instant::now();
    let auth_result = if tools.iter().any(|tool| tool.name == "list_datasources") {
        call_tool_with_retry(
            &mut client,
            &settings,
            "list_datasources",
            json!({"limit":1}),
        )
        .map(|_| "Grafana API 鉴权成功".to_string())
    } else {
        Err("MCP 未提供 list_datasources，无法验证 Grafana 鉴权".into())
    };
    let success = auth_result.is_ok();
    steps.push(DiagnosticStep {
        name: "Grafana 鉴权".into(),
        success,
        detail: auth_result.unwrap_or_else(|error| error),
        duration_ms: auth.elapsed().as_millis(),
    });
    ConnectionDiagnostic {
        success,
        attempts: 1,
        steps,
    }
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
    let settings = hydrate_credentials(settings);
    if !settings
        .mcp_args
        .split_whitespace()
        .any(|arg| arg == "--disable-write")
    {
        return Err("安全检查失败：MVP 必须使用 --disable-write".into());
    }
    connect_with_retry(&settings)?.list_tools()
}

#[tauri::command]
fn call_mcp_tool(settings: AppSettings, name: String, arguments: Value) -> Result<Value, String> {
    let settings = hydrate_credentials(settings);
    if !settings
        .mcp_args
        .split_whitespace()
        .any(|arg| arg == "--disable-write")
    {
        return Err("安全检查失败：MVP 必须使用 --disable-write".into());
    }
    let mut client = connect_with_retry(&settings)?;
    call_tool_with_retry(&mut client, &settings, &name, arguments)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MonitorRunResult {
    checked: usize,
    events: Vec<AlertEvent>,
    errors: Vec<String>,
    completed_at: String,
}

fn execute_monitor(
    app: &tauri::AppHandle,
    settings: &AppSettings,
) -> Result<MonitorRunResult, String> {
    if settings.prometheus_datasource_uid.trim().is_empty() {
        return Err("请先配置 Prometheus 数据源 UID".into());
    }
    if !settings
        .mcp_args
        .split_whitespace()
        .any(|arg| arg == "--disable-write")
    {
        return Err("安全检查失败：监控必须使用 --disable-write".into());
    }
    let conn = Connection::open(db_path(app)?).map_err(|e| e.to_string())?;
    let mut client = connect_with_retry(settings)?;
    let now = Local::now();
    let mut events = Vec::new();
    let mut errors = Vec::new();

    for rule in &settings.alert_rules {
        let response = call_tool_with_retry(
            &mut client,
            settings,
            "query_prometheus",
            json!({
                "datasourceUid": settings.prometheus_datasource_uid,
                "expr": rule.expr,
                "queryType": "instant",
                "startTime": "now",
                "endTime": "now"
            }),
        );
        let value = match response
            .and_then(|value| extract_metric_values(&value))
            .and_then(|values| {
                rule.operator
                    .aggregate(values.into_iter())
                    .ok_or_else(|| "查询结果为空".into())
            }) {
            Ok(value) => value,
            Err(error) => {
                errors.push(format!("{}：{}", rule.name, error));
                continue;
            }
        };
        let previous = conn
            .query_row(
                "SELECT state_json FROM alert_states WHERE rule_id=?1",
                [&rule.id],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|raw| serde_json::from_str::<AlertState>(&raw).ok());
        let (state, event) = evaluate(
            rule,
            value,
            previous,
            settings.alert_cooldown_minutes.max(0),
            now,
        );
        conn.execute(
            "INSERT OR REPLACE INTO alert_states(rule_id,state_json) VALUES (?1,?2)",
            params![
                rule.id,
                serde_json::to_string(&state).map_err(|e| e.to_string())?
            ],
        )
        .map_err(|e| e.to_string())?;
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
    Ok(MonitorRunResult {
        checked: settings.alert_rules.len(),
        events,
        errors,
        completed_at: now.to_rfc3339(),
    })
}

#[tauri::command]
fn run_monitor_now(
    app: tauri::AppHandle,
    runtime: State<'_, RuntimeSettings>,
    settings: AppSettings,
) -> Result<MonitorRunResult, String> {
    let settings = hydrate_credentials(settings);
    *runtime
        .0
        .lock()
        .map_err(|_| "运行时设置锁异常".to_string())? = settings.clone();
    execute_monitor(&app, &settings)
}

#[tauri::command]
fn list_alert_events(app: tauri::AppHandle) -> Result<Vec<AlertEvent>, String> {
    let conn = Connection::open(db_path(&app)?).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT event_json FROM alert_events ORDER BY created_at DESC LIMIT 100")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    Ok(rows
        .filter_map(Result::ok)
        .filter_map(|raw| serde_json::from_str(&raw).ok())
        .collect())
}

#[tauri::command]
fn analyze_alerts(
    app: tauri::AppHandle,
    settings: AppSettings,
    question: String,
) -> Result<String, String> {
    let settings = hydrate_credentials(settings);
    if question.trim().is_empty() {
        return Err("请输入需要分析的问题".into());
    }
    let conn = Connection::open(db_path(&app)?).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT event_json FROM alert_events ORDER BY created_at DESC LIMIT 30")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let evidence: Vec<Value> = rows
        .filter_map(Result::ok)
        .filter_map(|raw| serde_json::from_str(&raw).ok())
        .collect();
    if evidence.is_empty() {
        return Err("暂无真实告警事件，无法进行有证据的异常分析".into());
    }
    let attempts = settings.mcp_retry_attempts.clamp(1, 3);
    let mut last_error = String::new();
    for attempt in 1..=attempts {
        match ai::analyze(
            &settings.ai_base_url,
            &settings.ai_key,
            &settings.ai_model,
            question.trim(),
            &json!(evidence),
        ) {
            Ok(answer) => return Ok(answer),
            Err(error) => last_error = format!("第 {attempt}/{attempts} 次：{error}"),
        }
        if attempt < attempts {
            thread::sleep(Duration::from_millis(500 * 2_u64.pow(attempt - 1)));
        }
    }
    Err(last_error)
}

fn start_monitor_scheduler(app: tauri::AppHandle) {
    thread::spawn(move || {
        let mut elapsed_seconds = 0_u64;
        loop {
            thread::sleep(Duration::from_secs(30));
            elapsed_seconds = elapsed_seconds.saturating_add(30);
            let settings = match app.state::<RuntimeSettings>().0.lock() {
                Ok(settings) => settings.clone(),
                Err(_) => continue,
            };
            if !settings.monitor_enabled {
                elapsed_seconds = 0;
                continue;
            }
            let interval = settings.monitor_interval_minutes.max(1).saturating_mul(60);
            if elapsed_seconds < interval {
                continue;
            }
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
            diagnose_connection,
            list_mcp_tools,
            call_mcp_tool,
            run_monitor_now,
            list_alert_events,
            analyze_alerts,
            install_mcp_grafana
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Grafana Watch Dog");
}
