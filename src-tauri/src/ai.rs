use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::Duration;

pub fn analyze(base_url: &str, api_key: &str, model: &str, question: &str, evidence: &Value) -> Result<String, String> {
    if base_url.trim().is_empty() || model.trim().is_empty() || api_key.trim().is_empty() {
        return Err("请先完整配置 AI Base URL、模型和 API Key".into());
    }
    let endpoint = chat_completions_endpoint(base_url);
    let response = Client::builder()
        .timeout(Duration::from_secs(60))
        .build().map_err(|error| format!("创建 AI 客户端失败：{error}"))?
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&json!({
            "model": model,
            "temperature": 0.2,
            "messages": [
                {"role":"system","content":"你是只读 SRE 异常分析助手。只能根据提供的真实告警证据回答；明确区分事实、推断和未知信息。输出简洁中文，包含结论、证据、可能原因、建议检查步骤。不得声称执行过任何修复。"},
                {"role":"user","content":format!("问题：{question}\n\n最近真实告警事件：{}", evidence)}
            ]
        }))
        .send().map_err(|error| format!("AI 请求失败：{error}"))?;
    let status = response.status();
    let payload: Value = response.json().map_err(|error| format!("AI 响应不是有效 JSON：{error}"))?;
    if !status.is_success() {
        let message = payload.pointer("/error/message").and_then(Value::as_str).unwrap_or("未知错误");
        return Err(format!("AI 服务返回 HTTP {}：{}", status.as_u16(), message));
    }
    payload.pointer("/choices/0/message/content").and_then(Value::as_str).map(str::to_owned)
        .ok_or_else(|| "AI 响应缺少 choices[0].message.content".into())
}

fn chat_completions_endpoint(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") { format!("{base}/chat/completions") } else { format!("{base}/v1/chat/completions") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_compatible_endpoint() {
        assert_eq!(chat_completions_endpoint("https://api.openai.com/v1"), "https://api.openai.com/v1/chat/completions");
        assert_eq!(chat_completions_endpoint("https://api.deepseek.com"), "https://api.deepseek.com/v1/chat/completions");
    }
}
