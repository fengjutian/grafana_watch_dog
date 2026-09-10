const SERVICE: &str = "com.grafana-watch-dog.desktop";
const GRAFANA_TOKEN: &str = "grafana-service-account-token";
const AI_KEY: &str = "ai-provider-api-key";

fn entry(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, name).map_err(|error| format!("无法访问系统 Keychain：{error}"))
}

pub fn load_grafana_token() -> String {
    entry(GRAFANA_TOKEN)
        .and_then(|entry| entry.get_password().map_err(|error| error.to_string()))
        .unwrap_or_default()
}

pub fn load_ai_key() -> String {
    entry(AI_KEY)
        .and_then(|entry| entry.get_password().map_err(|error| error.to_string()))
        .unwrap_or_default()
}

pub fn save(grafana_token: &str, ai_key: &str) -> Result<(), String> {
    if !grafana_token.is_empty() {
        save_one(GRAFANA_TOKEN, grafana_token)?;
    }
    if !ai_key.is_empty() {
        save_one(AI_KEY, ai_key)?;
    }
    Ok(())
}

fn save_one(name: &str, value: &str) -> Result<(), String> {
    let entry = entry(name)?;
    entry
        .set_password(value)
        .map_err(|error| format!("无法写入系统 Keychain：{error}"))
}
