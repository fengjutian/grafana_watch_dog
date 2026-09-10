use chrono::Local;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{Manager, State};

struct Database(Mutex<Connection>);

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
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            grafana_url: "http://localhost:3000".into(),
            grafana_token: String::new(),
            mcp_command: "mcp-grafana".into(),
            mcp_args: "--disable-write".into(),
            ai_provider: "DeepSeek".into(),
            ai_base_url: "https://api.deepseek.com".into(),
            ai_model: "deepseek-chat".into(),
            ai_key: String::new(),
            schedule_enabled: true,
            schedule_time: "08:00".into(),
        }
    }
}

fn db_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("ai-ops-daily.db"))
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
        CREATE INDEX IF NOT EXISTS idx_reports_date ON reports(report_date DESC);",
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
fn save_settings(app: tauri::AppHandle, mut settings: AppSettings) -> Result<(), String> {
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
    Ok("配置检查通过；安装 mcp-grafana 后即可建立只读连接".into())
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let conn = Connection::open(db_path(app.handle())?)?;
            init_db(&conn)?;
            app.manage(Database(Mutex::new(conn)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_reports,
            generate_report,
            load_settings,
            save_settings,
            test_connection
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AI Ops Daily");
}
