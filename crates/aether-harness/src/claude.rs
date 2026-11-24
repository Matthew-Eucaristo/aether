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
                "strict": true,
                "eager_input_streaming": true
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
        "max_tokens": 64000,
        "stream": true,
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

#[derive(Default)]
pub struct Stream {
    message: Value,
    partial: Vec<String>,
    buffer: String,
    done: bool,
}

impl Stream {
    pub fn push(&mut self, chunk: &str) -> Result<(), HarnessError> {
        self.buffer.push_str(chunk);
        if self.buffer.contains('\r') {
            self.buffer = self.buffer.replace("\r\n", "\n");
        }
        while let Some(end) = self.buffer.find("\n\n") {
            let event: String = self.buffer.drain(..end + 2).collect();
            let data: Vec<&str> = event
                .lines()
                .filter_map(|l| l.strip_prefix("data:"))
                .map(str::trim_start)
                .collect();
            if data.is_empty() {
                continue;
            }
            let json: Value = serde_json::from_str(&data.join("\n"))
                .map_err(|e| HarnessError::Network(format!("bad stream event: {e}")))?;
            self.apply(&json)?;
        }
        Ok(())
    }

    fn apply(&mut self, event: &Value) -> Result<(), HarnessError> {
        let index = event["index"].as_u64().unwrap_or(0) as usize;
        match event["type"].as_str().unwrap_or_default() {
            "message_start" => self.message = event["message"].clone(),
            "content_block_start" => {
                let content = self.message["content"]
                    .as_array_mut()
                    .ok_or_else(|| HarnessError::Network("content before message_start".into()))?;
                if index != content.len() {
                    return Err(HarnessError::Network(format!(
                        "content block {index} started after {} blocks",
                        content.len()
                    )));
                }
                content.push(event["content_block"].clone());
                self.partial.resize(index + 1, String::new());
            }
            "content_block_delta" => {
                let delta = &event["delta"];
                let content = self.message["content"]
                    .as_array_mut()
                    .ok_or_else(|| HarnessError::Network("content before message_start".into()))?;
                let Some(block) = content.get_mut(index) else {
                    return Err(HarnessError::Network(format!(
                        "delta for content block {index} that never started"
                    )));
                };
                match delta["type"].as_str().unwrap_or_default() {
                    "text_delta" => append(block, "text", &delta["text"]),
                    "thinking_delta" => append(block, "thinking", &delta["thinking"]),
                    "signature_delta" => block["signature"] = delta["signature"].clone(),
                    "input_json_delta" => {
                        if let Some(p) = self.partial.get_mut(index) {
                            p.push_str(delta["partial_json"].as_str().unwrap_or_default());
                        }
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let content = self.message["content"]
                    .as_array_mut()
                    .ok_or_else(|| HarnessError::Network("content before message_start".into()))?;
                let Some(block) = content.get_mut(index) else {
                    return Err(HarnessError::Network(format!(
                        "stop for content block {index} that never started"
                    )));
                };
                if block["type"] == "tool_use" {
                    let raw = self.partial.get(index).map(String::as_str).unwrap_or("");
                    block["input"] = if raw.trim().is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(raw).unwrap_or_else(|e| {
                            json!({"invalid_json": format!("the tool input was not valid JSON: {e}")})
                        })
                    };
                }
            }
            "message_delta" => {
                if let Some(delta) = event["delta"].as_object() {
                    for (k, v) in delta {
                        self.message[k] = v.clone();
                    }
                }
            }
            "message_stop" => self.done = true,
            "error" => {
                let message = event["error"]["message"].as_str().unwrap_or("stream error");
                return Err(HarnessError::Api(0, message.to_owned()));
            }
            _ => {}
        }
        Ok(())
    }

    pub fn finish(self) -> Result<Value, HarnessError> {
        match self.done {
            true => Ok(self.message),
            false => Err(HarnessError::Network(
                "the stream ended before message_stop".into(),
            )),
        }
    }
}

fn append(block: &mut Value, key: &str, piece: &Value) {
    let mut text = block[key].as_str().unwrap_or_default().to_owned();
    text.push_str(piece.as_str().unwrap_or_default());
    block[key] = Value::String(text);
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
        let mut response = request
            .send()
            .await
            .map_err(|e| HarnessError::Network(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            let body: Value = response
                .json()
                .await
                .map_err(|e| HarnessError::Network(e.to_string()))?;
            let message = body["error"]["message"].as_str().unwrap_or("no message");
            return Err(HarnessError::Api(status.as_u16(), message.to_owned()));
        }
        let mut stream = Stream::default();
        let mut pending = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| HarnessError::Network(e.to_string()))?
        {
            pending.extend_from_slice(&chunk);
            let valid = match std::str::from_utf8(&pending) {
                Ok(_) => pending.len(),
                Err(e) => e.valid_up_to(),
            };
            let text = String::from_utf8(pending.drain(..valid).collect())
                .expect("the prefix was checked to be UTF-8");
            stream.push(&text)?;
        }
        parse_response(&stream.finish()?)
    }
}
