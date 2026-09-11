use chrono::{Duration as ChronoDuration, Local};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    process::Command,
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
use monitor::{
    evaluate, extract_metric_samples, AlertEvent, AlertRule, AlertState, Comparison, MetricSample,
};

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
    selected_datasource_uids: Vec<String>,
    #[serde(default)]
    selected_dashboard_uids: Vec<String>,
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
        AlertRule { id:"database".into(), name:"数据库在线状态".into(), expr:"max by(instance, job) ({__name__=~\"mysql_up|pg_up|mongodb_up|redis_up\"})".into(), operator:Comparison::LessThan, threshold:1.0, for_checks:1, severity:"critical".into(), unit:"".into() },
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
            selected_datasource_uids: Vec::new(),
            selected_dashboard_uids: Vec::new(),
            alert_cooldown_minutes: default_cooldown(),
            alert_rules: default_alert_rules(),
            mcp_retry_attempts: default_retry_attempts(),
        }
    }
}

fn hydrate_credentials(mut settings: AppSettings) -> AppSettings {
    if !settings
        .alert_rules
        .iter()
        .any(|rule| rule.id == "database")
    {
        if let Some(rule) = default_alert_rules()
            .into_iter()
            .find(|rule| rule.id == "database")
        {
            settings.alert_rules.push(rule);
        }
    }
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
        CREATE INDEX IF NOT EXISTS idx_alert_events_created ON alert_events(created_at DESC);
        CREATE TABLE IF NOT EXISTS metric_samples (
          id INTEGER PRIMARY KEY AUTOINCREMENT, rule_id TEXT NOT NULL,
          rule_name TEXT NOT NULL, value REAL NOT NULL, unit TEXT NOT NULL,
          collected_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_metric_samples_rule_time ON metric_samples(rule_id, collected_at DESC);
        CREATE TABLE IF NOT EXISTS grafana_snapshots (
          id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL,
          resource_uid TEXT NOT NULL, payload_json TEXT NOT NULL,
          collected_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_grafana_snapshots_time ON grafana_snapshots(collected_at DESC);",
    )
}

#[tauri::command]
fn list_reports(db: State<'_, Database>) -> Result<Vec<Value>, String> {
    let conn = db.0.lock().map_err(|_| "数据库锁异常".to_string())?;
    let mut stmt = conn
        .prepare("SELECT report_json FROM reports ORDER BY created_at DESC LIMIT 90")
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GrafanaDatasource {
    uid: String,
    name: String,
    kind: String,
}

#[derive(Debug, Clone, Serialize)]
struct GrafanaDashboard {
    uid: String,
    title: String,
}

#[derive(Debug, Serialize)]
struct GrafanaDiscovery {
    datasources: Vec<GrafanaDatasource>,
    dashboards: Vec<GrafanaDashboard>,
}

fn mcp_payload(result: &Value) -> Value {
    result
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find_map(|item| item.get("text").and_then(Value::as_str))
        })
        .and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_else(|| result.clone())
}

fn collect_datasources(value: &Value, output: &mut Vec<GrafanaDatasource>) {
    match value {
        Value::Object(map) => {
            if let (Some(uid), Some(name)) = (
                map.get("uid").and_then(Value::as_str),
                map.get("name").and_then(Value::as_str),
            ) {
                let kind = map
                    .get("type")
                    .or_else(|| map.get("kind"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                if !output.iter().any(|item| item.uid == uid) {
                    output.push(GrafanaDatasource {
                        uid: uid.into(),
                        name: name.into(),
                        kind: kind.into(),
                    });
                }
            }
            for child in map.values() {
                collect_datasources(child, output);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_datasources(item, output);
            }
        }
        _ => {}
    }
}

fn collect_dashboards(value: &Value, output: &mut Vec<GrafanaDashboard>) {
    match value {
        Value::Object(map) => {
            let is_dashboard = map
                .get("type")
                .and_then(Value::as_str)
                .map(|kind| kind == "dash-db" || kind == "dashboard")
                .unwrap_or(true);
            if is_dashboard {
                if let (Some(uid), Some(title)) = (
                    map.get("uid").and_then(Value::as_str),
                    map.get("title").and_then(Value::as_str),
                ) {
                    if !output.iter().any(|item| item.uid == uid) {
                        output.push(GrafanaDashboard {
                            uid: uid.into(),
                            title: title.into(),
                        });
                    }
                }
            }
            for child in map.values() {
                collect_dashboards(child, output);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_dashboards(item, output);
            }
        }
        _ => {}
    }
}

#[tauri::command]
fn discover_grafana(settings: AppSettings) -> Result<GrafanaDiscovery, String> {
    let settings = hydrate_credentials(settings);
    let mut client = connect_with_retry(&settings)?;
    discover_resources(&mut client, &settings)
}

fn discover_resources(
    client: &mut GrafanaMcpClient,
    settings: &AppSettings,
) -> Result<GrafanaDiscovery, String> {
    let datasource_result = call_tool_with_retry(
        client,
        &settings,
        "list_datasources",
        json!({"limit":100,"offset":0}),
    )?;
    let dashboard_result = call_tool_with_retry(
        client,
        &settings,
        "search_dashboards",
        json!({"limit":100,"page":1}),
    )?;
    let mut datasources = Vec::new();
    let mut dashboards = Vec::new();
    collect_datasources(&mcp_payload(&datasource_result), &mut datasources);
    collect_dashboards(&mcp_payload(&dashboard_result), &mut dashboards);
    Ok(GrafanaDiscovery {
        datasources,
        dashboards,
    })
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
    let mut args: Vec<String> = settings
        .mcp_args
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let persistent_binary_available = settings.mcp_command == "uvx"
        && args.first().is_some_and(|arg| arg == "mcp-grafana")
        && Command::new("mcp-grafana")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
    let command = if persistent_binary_available {
        args.remove(0);
        "mcp-grafana".to_owned()
    } else {
        settings.mcp_command.clone()
    };
    GrafanaMcpConfig {
        command,
        args,
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
    #[serde(skip_serializing)]
    readings: Vec<MetricReading>,
}

#[derive(Debug, Clone)]
struct MetricReading {
    rule: AlertRule,
    value: f64,
    instance: String,
    job: String,
    datasource_uid: String,
}

fn execute_monitor(
    app: &tauri::AppHandle,
    settings: &AppSettings,
) -> Result<MonitorRunResult, String> {
    if settings.selected_datasource_uids.is_empty() && settings.selected_dashboard_uids.is_empty() {
        return Err("请先从 Grafana 自动发现结果中选择需要监控的数据源或 Dashboard".into());
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
    let discovery = discover_resources(&mut client, settings)?;
    let prometheus_uids: Vec<String> = discovery
        .datasources
        .iter()
        .filter(|source| {
            settings.selected_datasource_uids.contains(&source.uid)
                && source.kind.to_ascii_lowercase().contains("prometheus")
        })
        .map(|source| source.uid.clone())
        .collect();
    let now = Local::now();
    let mut events = Vec::new();
    let mut errors = Vec::new();
    let mut readings = Vec::new();

    for rule in &settings.alert_rules {
        let mut samples: Vec<(MetricSample, String)> = Vec::new();
        for datasource_uid in &prometheus_uids {
            match call_tool_with_retry(
                &mut client,
                settings,
                "query_prometheus",
                json!({
                    "datasourceUid": datasource_uid, "expr": rule.expr, "queryType": "instant",
                    "startTime": "now", "endTime": "now"
                }),
            )
            .and_then(|value| extract_metric_samples(&value))
            {
                Ok(found) => samples.extend(
                    found
                        .into_iter()
                        .map(|sample| (sample, datasource_uid.clone())),
                ),
                Err(error) => errors.push(format!("{} / {}：{}", rule.name, datasource_uid, error)),
            }
        }
        if samples.is_empty() && rule.id == "database" {
            continue;
        }
        if samples.is_empty() {
            errors.push(format!("{}：查询结果为空", rule.name));
            continue;
        }
        let mut instances: HashMap<(String, String, String), MetricSample> = HashMap::new();
        for (sample, datasource_uid) in samples {
            let key = (datasource_uid, sample.instance.clone(), sample.job.clone());
            instances
                .entry(key)
                .and_modify(|current| {
                    current.value = rule
                        .operator
                        .aggregate([current.value, sample.value].into_iter())
                        .unwrap_or(current.value)
                })
                .or_insert(sample);
        }
        for ((datasource_uid, instance, job), sample) in instances {
            let value = sample.value;
            let mut instance_rule = rule.clone();
            instance_rule.id = format!("{}@{}@{}", rule.id, datasource_uid, instance);
            instance_rule.name = format!("{} · {}", rule.name, instance);
            let previous = conn
                .query_row(
                    "SELECT state_json FROM alert_states WHERE rule_id=?1",
                    [&instance_rule.id],
                    |row| row.get::<_, String>(0),
                )
                .ok()
                .and_then(|raw| serde_json::from_str::<AlertState>(&raw).ok());
            conn.execute(
            "INSERT INTO metric_samples(rule_id,rule_name,value,unit,collected_at) VALUES (?1,?2,?3,?4,?5)",
            params![instance_rule.id, instance_rule.name, value, rule.unit, now.to_rfc3339()],
        ).map_err(|e| e.to_string())?;
            readings.push(MetricReading {
                rule: instance_rule.clone(),
                value,
                instance,
                job,
                datasource_uid,
            });
            let (state, event) = evaluate(
                &instance_rule,
                value,
                previous,
                settings.alert_cooldown_minutes.max(0),
                now,
            );
            conn.execute(
                "INSERT OR REPLACE INTO alert_states(rule_id,state_json) VALUES (?1,?2)",
                params![
                    instance_rule.id,
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
    }
    let collected_at = now.to_rfc3339();
    match call_tool_with_retry(
        &mut client,
        settings,
        "alerting_manage_rules",
        json!({"operation":"list", "rule_limit":200, "limit_alerts":20}),
    ) {
        Ok(alerts) => {
            conn.execute("INSERT INTO grafana_snapshots(kind,resource_uid,payload_json,collected_at) VALUES ('alerts','grafana',?1,?2)",
                params![mcp_payload(&alerts).to_string(), collected_at]).map_err(|e| e.to_string())?;
        }
        Err(error) => errors.push(format!("Grafana 告警：{}", error)),
    }
    for dashboard_uid in &settings.selected_dashboard_uids {
        match call_tool_with_retry(
            &mut client,
            settings,
            "get_dashboard_panel_queries",
            json!({"uid":dashboard_uid}),
        ) {
            Ok(panels) => {
                conn.execute("INSERT INTO grafana_snapshots(kind,resource_uid,payload_json,collected_at) VALUES ('dashboard_panels',?1,?2,?3)",
                    params![dashboard_uid, mcp_payload(&panels).to_string(), collected_at]).map_err(|e| e.to_string())?;
            }
            Err(error) => errors.push(format!("Dashboard {}：{}", dashboard_uid, error)),
        }
    }
    Ok(MonitorRunResult {
        checked: settings.alert_rules.len(),
        events,
        errors,
        completed_at: now.to_rfc3339(),
        readings,
    })
}

fn generate_and_store_report(
    app: &tauri::AppHandle,
    settings: &AppSettings,
) -> Result<Value, String> {
    let run = execute_monitor(app, settings)?;
    if run.readings.is_empty() {
        return Err(format!(
            "Grafana 未返回任何可用指标：{}",
            run.errors.join("；")
        ));
    }
    let conn = Connection::open(db_path(app)?).map_err(|e| e.to_string())?;
    let now = Local::now();
    let date = now.format("%Y-%m-%d").to_string();
    let window_start = now - ChronoDuration::hours(24);
    let mut critical = 0_i64;
    let mut warning = 0_i64;
    let mut healthy = 0_i64;
    let mut services = Vec::new();
    let mut trends = Vec::new();
    let mut issues = Vec::new();
    let mut total_samples = 0_i64;

    for reading in &run.readings {
        let mut stmt = conn.prepare("SELECT value FROM metric_samples WHERE rule_id=?1 AND collected_at>=?2 AND collected_at<=?3 ORDER BY collected_at ASC").map_err(|e| e.to_string())?;
        let day_values: Vec<f64> = stmt
            .query_map(
                params![reading.rule.id, window_start.to_rfc3339(), now.to_rfc3339()],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();
        total_samples += day_values.len() as i64;
        let average = day_values.iter().sum::<f64>() / day_values.len().max(1) as f64;
        let minimum = day_values
            .iter()
            .copied()
            .reduce(f64::min)
            .unwrap_or(reading.value);
        let maximum = day_values
            .iter()
            .copied()
            .reduce(f64::max)
            .unwrap_or(reading.value);
        let analyzed_value = reading
            .rule
            .operator
            .aggregate(day_values.iter().copied())
            .unwrap_or(reading.value);
        let breached = reading
            .rule
            .operator
            .matches(analyzed_value, reading.rule.threshold);
        if breached && reading.rule.severity == "critical" {
            critical += 1;
        } else if breached {
            warning += 1;
        } else {
            healthy += 1;
        }
        let service_score = if !breached {
            100
        } else if reading.rule.severity == "critical" {
            35
        } else {
            65
        };
        let category = if reading.rule.id.starts_with("cpu@") {
            "cpu"
        } else if reading.rule.id.starts_with("memory@") {
            "memory"
        } else if reading.rule.id.starts_with("disk@") {
            "disk"
        } else if reading.rule.id.starts_with("database@") {
            "database"
        } else {
            "availability"
        };
        services.push(json!({
            "name": reading.rule.name,
            "kind": format!("服务器 {} · Prometheus", reading.instance),
            "score": service_score,
            "metrics": [format!("{:.2}{}", reading.value, reading.rule.unit), format!("数据源 {}", reading.datasource_uid), format!("Job {}", if reading.job.is_empty() { "-" } else { &reading.job })],
            "instance": reading.instance, "category": category, "value": analyzed_value,
            "unit": reading.rule.unit, "threshold": reading.rule.threshold,
            "datasourceUid": reading.datasource_uid, "job": reading.job, "breached": breached,
            "average": average, "minimum": minimum, "maximum": maximum, "sampleCount": day_values.len()
        }));

        let history: Vec<f64> = day_values
            .iter()
            .step_by((day_values.len() / 24).max(1))
            .copied()
            .collect();
        let first = history.first().copied().unwrap_or(reading.value);
        let change = if first.abs() < f64::EPSILON {
            0.0
        } else {
            ((reading.value - first) / first * 100.0).round()
        };
        trends.push(json!({ "label":reading.rule.name, "value":reading.value, "unit":reading.rule.unit, "change":change, "history":history, "datasourceUid":reading.datasource_uid, "instance":reading.instance, "category":category }));

        if breached {
            issues.push(json!({
                "id": reading.rule.id,
                "severity": reading.rule.severity,
                "title": format!("{}超过告警阈值", reading.rule.name),
                "source": format!("服务器 {} · 数据源 {}", reading.instance, reading.datasource_uid),
                "change": format!("{:.2}{}", analyzed_value, reading.rule.unit),
                "reason": format!("服务器 {}（job={}）当日最需关注值 {:.2}{}，日均 {:.2}{}，阈值 {:.2}{}。", reading.instance, reading.job, analyzed_value, reading.rule.unit, average, reading.rule.unit, reading.rule.threshold, reading.rule.unit),
                "recommendations": ["核对对应实例和标签", "检查同一时间窗口的日志与发布记录", "确认指标是否持续异常"]
            }));
        }
    }
    for (index, error) in run.errors.iter().enumerate() {
        issues.push(json!({
            "id": format!("collection-error-{index}"), "severity":"warning",
            "title":"指标采集失败", "source":"Grafana MCP", "change":"采集错误",
            "reason":error, "recommendations":["运行连接诊断", "检查数据源 UID 与 PromQL", "确认 Service Account 查询权限"]
        }));
        warning += 1;
    }
    let score = (100 - critical * 25 - warning * 10).clamp(0, 100);
    let status = if critical > 0 {
        "critical"
    } else if warning > 0 {
        "warning"
    } else {
        "healthy"
    };
    let active_alerts: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM alert_states WHERE json_extract(state_json,'$.active')=1",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let summary = if critical > 0 {
        format!(
            "已分析过去 24 小时的 {} 条采样，发现 {} 项严重异常、{} 项警告。",
            total_samples, critical, warning
        )
    } else if warning > 0 {
        format!(
            "已分析过去 24 小时的 {} 条采样，发现 {} 项需要关注的问题。",
            total_samples, warning
        )
    } else {
        format!(
            "已分析过去 24 小时的 {} 条采样，均在配置阈值内。",
            total_samples
        )
    };
    let report_number: i64 = conn
        .query_row(
            "SELECT COUNT(*) + 1 FROM reports WHERE report_date=?1",
            [&date],
            |row| row.get(0),
        )
        .unwrap_or(1);
    let report = json!({
        "id":format!("report-{}-{}", date, now.timestamp_millis()), "date":date, "score":score, "status":status,
        "summary":summary, "generatedAt":now.format("%Y-%m-%d %H:%M:%S").to_string(),
        "analysisNumber":report_number, "windowStart":window_start.format("%Y-%m-%d %H:%M:%S").to_string(),
        "windowEnd":now.format("%Y-%m-%d %H:%M:%S").to_string(), "sampleCount":total_samples,
        "stats":{"critical":critical,"warning":warning,"healthy":healthy,"alerts":active_alerts},
        "services":services, "trends":trends, "issues":issues
    });
    conn.execute(
        "INSERT OR REPLACE INTO reports(id,report_date,score,status,summary,report_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![report["id"].as_str(), report["date"].as_str(), score, status, report["summary"].as_str(), report.to_string(), report["generatedAt"].as_str()]
    ).map_err(|e| e.to_string())?;
    let _ = app.emit("report-generated", &report);
    Ok(report)
}

#[tauri::command]
async fn generate_report(
    app: tauri::AppHandle,
    runtime: State<'_, RuntimeSettings>,
) -> Result<Value, String> {
    let settings = runtime
        .0
        .lock()
        .map_err(|_| "运行时设置锁异常".to_string())?
        .clone();
    tauri::async_runtime::spawn_blocking(move || generate_and_store_report(&app, &settings))
        .await
        .map_err(|error| format!("日报生成任务异常结束：{error}"))?
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

fn report_exists_for_date(app: &tauri::AppHandle, date: &str) -> bool {
    let path = match db_path(app) {
        Ok(path) => path,
        Err(_) => return false,
    };
    Connection::open(path)
        .ok()
        .and_then(|conn| {
            conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM reports WHERE report_date=?1)",
                [date],
                |row| row.get::<_, bool>(0),
            )
            .ok()
        })
        .unwrap_or(false)
}

fn start_monitor_scheduler(app: tauri::AppHandle) {
    thread::spawn(move || {
        let mut elapsed_seconds = 0_u64;
        let mut last_report_attempt_date: Option<String> = None;
        loop {
            thread::sleep(Duration::from_secs(30));
            elapsed_seconds = elapsed_seconds.saturating_add(30);
            let settings = match app.state::<RuntimeSettings>().0.lock() {
                Ok(settings) => settings.clone(),
                Err(_) => continue,
            };
            if settings.monitor_enabled {
                let interval = settings.monitor_interval_minutes.max(1).saturating_mul(60);
                if elapsed_seconds >= interval {
                    elapsed_seconds = 0;
                    if let Err(error) = execute_monitor(&app, &settings) {
                        let _ = app.emit("monitor-error", error);
                    }
                }
            } else {
                elapsed_seconds = 0;
            }

            if settings.schedule_enabled {
                let now = Local::now();
                let today = now.format("%Y-%m-%d").to_string();
                let current_time = now.format("%H:%M").to_string();
                if current_time >= settings.schedule_time
                    && last_report_attempt_date.as_deref() != Some(today.as_str())
                    && !report_exists_for_date(&app, &today)
                {
                    last_report_attempt_date = Some(today);
                    if let Err(error) = generate_and_store_report(&app, &settings) {
                        let _ = app.emit("report-error", error);
                    }
                }
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
            discover_grafana,
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
