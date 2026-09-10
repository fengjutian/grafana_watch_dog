use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct MetricSample {
    pub value: f64,
    pub instance: String,
    pub job: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub expr: String,
    pub operator: Comparison,
    pub threshold: f64,
    #[serde(default = "default_for")]
    pub for_checks: u32,
    #[serde(default = "default_severity")]
    pub severity: String,
    #[serde(default)]
    pub unit: String,
}

fn default_for() -> u32 {
    1
}
fn default_severity() -> String {
    "warning".into()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
    Equal,
}

impl Comparison {
    pub fn matches(self, value: f64, threshold: f64) -> bool {
        match self {
            Self::GreaterThan => value > threshold,
            Self::GreaterOrEqual => value >= threshold,
            Self::LessThan => value < threshold,
            Self::LessOrEqual => value <= threshold,
            Self::Equal => (value - threshold).abs() < f64::EPSILON,
        }
    }

    pub fn aggregate(self, values: impl Iterator<Item = f64>) -> Option<f64> {
        match self {
            Self::LessThan | Self::LessOrEqual => values.reduce(f64::min),
            _ => values.reduce(f64::max),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertState {
    pub rule_id: String,
    pub consecutive_failures: u32,
    pub active: bool,
    pub last_notified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlertEvent {
    pub id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub severity: String,
    pub kind: String,
    pub value: f64,
    pub threshold: f64,
    pub unit: String,
    pub message: String,
    pub created_at: String,
}

pub fn evaluate(
    rule: &AlertRule,
    value: f64,
    state: Option<AlertState>,
    cooldown_minutes: i64,
    now: DateTime<Local>,
) -> (AlertState, Option<AlertEvent>) {
    let mut state = state.unwrap_or(AlertState {
        rule_id: rule.id.clone(),
        consecutive_failures: 0,
        active: false,
        last_notified_at: None,
    });
    let breached = rule.operator.matches(value, rule.threshold);
    let mut event = None;

    if breached {
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        if state.consecutive_failures >= rule.for_checks.max(1) {
            let cooldown_elapsed = state
                .last_notified_at
                .as_deref()
                .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
                .map(|last| {
                    now.signed_duration_since(last.with_timezone(&Local))
                        .num_minutes()
                        >= cooldown_minutes
                })
                .unwrap_or(true);
            if !state.active || cooldown_elapsed {
                event = Some(make_event(rule, value, "firing", &now));
                state.last_notified_at = Some(now.to_rfc3339());
            }
            state.active = true;
        }
    } else {
        state.consecutive_failures = 0;
        if state.active {
            event = Some(make_event(rule, value, "resolved", &now));
            state.last_notified_at = Some(now.to_rfc3339());
        }
        state.active = false;
    }
    (state, event)
}

fn make_event(rule: &AlertRule, value: f64, kind: &str, now: &DateTime<Local>) -> AlertEvent {
    let state = if kind == "resolved" {
        "已恢复"
    } else {
        "触发告警"
    };
    AlertEvent {
        id: format!("{}-{}", rule.id, now.timestamp_millis()),
        rule_id: rule.id.clone(),
        rule_name: rule.name.clone(),
        severity: if kind == "resolved" {
            "info".into()
        } else {
            rule.severity.clone()
        },
        kind: kind.into(),
        value,
        threshold: rule.threshold,
        unit: rule.unit.clone(),
        message: format!(
            "{}：{}，当前值 {:.2}{}，阈值 {:.2}{}",
            rule.name, state, value, rule.unit, rule.threshold, rule.unit
        ),
        created_at: now.to_rfc3339(),
    }
}

/// Extract the largest numeric sample from an MCP tool result. mcp-grafana wraps
/// Prometheus JSON in MCP `content[].text`; direct Prometheus JSON is accepted too.
pub fn extract_metric_values(value: &Value) -> Result<Vec<f64>, String> {
    Ok(extract_metric_samples(value)?
        .into_iter()
        .map(|sample| sample.value)
        .collect())
}

pub fn extract_metric_samples(value: &Value) -> Result<Vec<MetricSample>, String> {
    let decoded = value
        .get("content")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find_map(|item| item.get("text").and_then(Value::as_str))
        })
        .and_then(|text| serde_json::from_str::<Value>(text).ok());
    let root = decoded.as_ref().unwrap_or(value);
    let mut samples = Vec::new();
    collect_labeled_samples(root, &mut samples);
    if samples.is_empty() {
        Err("MCP 查询结果中没有数值样本".into())
    } else {
        Ok(samples)
    }
}

fn collect_labeled_samples(value: &Value, output: &mut Vec<MetricSample>) {
    match value {
        Value::Object(map) => {
            if let Some(number) = map.get("value").and_then(Value::as_array)
                .and_then(|value| value.get(1)).and_then(json_number) {
                let labels = map.get("metric").and_then(Value::as_object);
                let instance = labels.and_then(|labels| labels.get("instance"))
                    .and_then(Value::as_str).unwrap_or("未知实例").to_string();
                let job = labels.and_then(|labels| labels.get("job"))
                    .and_then(Value::as_str).unwrap_or("").to_string();
                output.push(MetricSample { value: number, instance, job });
                return;
            }
            for child in map.values() { collect_labeled_samples(child, output); }
        }
        Value::Array(items) => {
            for item in items { collect_labeled_samples(item, output); }
        }
        _ => {}
    }
}

fn collect_samples(value: &Value, output: &mut Vec<f64>) {
    match value {
        Value::Object(map) => {
            if let Some(sample) = map
                .get("value")
                .and_then(Value::as_array)
                .and_then(|v| v.get(1))
            {
                if let Some(number) = json_number(sample) {
                    output.push(number);
                }
            }
            if let Some(samples) = map.get("values").and_then(Value::as_array) {
                for sample in samples {
                    if let Some(number) = sample
                        .as_array()
                        .and_then(|v| v.get(1))
                        .and_then(json_number)
                    {
                        output.push(number);
                    }
                }
            }
            for child in map.values() {
                collect_samples(child, output);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_samples(item, output);
            }
        }
        _ => {}
    }
}

fn json_number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rule() -> AlertRule {
        AlertRule {
            id: "cpu".into(),
            name: "CPU".into(),
            expr: "cpu".into(),
            operator: Comparison::GreaterThan,
            threshold: 85.0,
            for_checks: 2,
            severity: "critical".into(),
            unit: "%".into(),
        }
    }

    #[test]
    fn requires_consecutive_checks_and_recovers() {
        let now = Local::now();
        let (state, first) = evaluate(&rule(), 90.0, None, 30, now);
        assert!(first.is_none());
        let (state, firing) = evaluate(&rule(), 91.0, Some(state), 30, now);
        assert_eq!(firing.unwrap().kind, "firing");
        let (_, resolved) = evaluate(&rule(), 50.0, Some(state), 30, now);
        assert_eq!(resolved.unwrap().kind, "resolved");
    }

    #[test]
    fn extracts_wrapped_prometheus_values() {
        let result = json!({"content":[{"type":"text","text":"{\"data\":{\"result\":[{\"value\":[1,\"91.5\"]}]}}"}]});
        assert_eq!(extract_metric_values(&result).unwrap(), vec![91.5]);
    }
}
