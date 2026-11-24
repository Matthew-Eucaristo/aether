use std::future::Future;

use serde_json::Value;

use crate::HarnessError;

#[derive(Clone, Debug)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub id: String,
    pub content: String,
    pub is_error: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    User(String),
    Assistant(Value),
    ToolResults(Vec<ToolResult>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Turn {
    pub assistant: Value,
    pub text: String,
    pub calls: Vec<ToolCall>,
}

pub trait Model: Send + Sync {
    fn name(&self) -> &str;

    fn turn(
        &self,
        system: &str,
        tools: &[ToolSpec],
        messages: &[Message],
    ) -> impl Future<Output = Result<Turn, HarnessError>> + Send;
}
