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
    save_one(GRAFANA_TOKEN, grafana_token)?;
    save_one(AI_KEY, ai_key)
}

fn save_one(name: &str, value: &str) -> Result<(), String> {
    let entry = entry(name)?;
    if value.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(format!("无法删除系统凭据：{error}")),
        }
    } else {
        entry
            .set_password(value)
            .map_err(|error| format!("无法写入系统 Keychain：{error}"))
    }
}
