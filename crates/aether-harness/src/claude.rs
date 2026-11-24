use std::time::Duration;

use serde_json::{Value, json};

use crate::HarnessError;
use crate::model::{Message, Model, ToolCall, ToolSpec, Turn};

const URL: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_MODEL: &str = "claude-haiku-5-5";
const DEFAULT_EFFORT: &str = "medium";
const SERVER_FALLBACK_MODELS: [&str; 5] = [
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5-5",
    "claude-fable-5-1",
    "claude-fable-5-2",
];

pub struct Claude {
    http: reqwest::Client,
    key: String,
    model: String,
    effort: String,
}

pub fn request_body(
    model: &str,
    effort: &str,
    system: &str,
    tools: &[ToolSpec],
    messages: &[Message],
) -> Value {
    let tools: Vec<Value> = tools
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.schema,
                "strict": true
            })
        })
        .collect();
    let messages: Vec<Value> = messages
        .iter()
        .map(|m| match m {
            Message::User(text) => json!({"role": "user", "content": text}),
            Message::Assistant(native) => native.clone(),
            Message::ToolResults(results) => json!({
                "role": "user",
                "content": results.iter().map(|r| json!({
                    "type": "tool_result",
                    "tool_use_id": r.id,
                    "content": r.content,
                    "is_error": r.is_error
                })).collect::<Vec<_>>()
            }),
        })
        .collect();
    let mut body = json!({
        "model": model,
        "max_tokens": 16000,
        "system": [{"type": "text", "text": system, "cache_control": {"type": "ephemeral"}}],
        "tools": tools,
        "messages": messages,
        "cache_control": {"type": "ephemeral"},
        "output_config": {"effort": effort}
    });
    if SERVER_FALLBACK_MODELS.contains(&model) {
        body["fallbacks"] = json!("default");
    }
    body
}

pub fn parse_response(response: &Value) -> Result<Turn, HarnessError> {
    match response["stop_reason"].as_str().unwrap_or_default() {
        "refusal" => {
            let category = response["stop_details"]["category"]
                .as_str()
                .unwrap_or("unspecified");
            return Err(HarnessError::Refused(category.to_owned()));
        }
        "max_tokens" => return Err(HarnessError::Truncated),
        _ => {}
    }
    let content = response["content"].clone();
    let blocks = content.as_array().cloned().unwrap_or_default();
    let text = blocks
        .iter()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let calls = blocks
        .iter()
        .filter(|b| b["type"] == "tool_use")
        .map(|b| ToolCall {
            id: b["id"].as_str().unwrap_or_default().to_owned(),
            name: b["name"].as_str().unwrap_or_default().to_owned(),
            input: b["input"].clone(),
        })
        .collect();
    Ok(Turn {
        assistant: json!({"role": "assistant", "content": content}),
        text,
        calls,
    })
}

impl Claude {
    pub fn from_env() -> Option<Claude> {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())?;
        let model = std::env::var("AETHER_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
        let effort = std::env::var("AETHER_EFFORT").unwrap_or_else(|_| DEFAULT_EFFORT.into());
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .expect("an HTTP client with a timeout builds");
        Some(Claude {
            http,
            key,
            model,
            effort,
        })
    }
}

impl Model for Claude {
    fn name(&self) -> &str {
        &self.model
    }

    async fn turn(
        &self,
        system: &str,
        tools: &[ToolSpec],
        messages: &[Message],
    ) -> Result<Turn, HarnessError> {
        let mut request = self
            .http
            .post(URL)
            .header("x-api-key", &self.key)
            .header("anthropic-version", "2026-10-21")
            .json(&request_body(
                &self.model,
                &self.effort,
                system,
                tools,
                messages,
            ));
        if SERVER_FALLBACK_MODELS.contains(&self.model.as_str()) {
            request = request.header("anthropic-beta", "server-side-fallback-2026-07-01");
        }
        let response = request
            .send()
            .await
            .map_err(|e| HarnessError::Network(e.to_string()))?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|e| HarnessError::Network(e.to_string()))?;
        if !status.is_success() {
            let message = body["error"]["message"].as_str().unwrap_or("no message");
            return Err(HarnessError::Api(status.as_u16(), message.to_owned()));
        }
        parse_response(&body)
    }
}
