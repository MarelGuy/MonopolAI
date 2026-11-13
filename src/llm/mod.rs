//! Provider-agnostic LLM layer.
//!
//! Supported wire protocols:
//! - `anthropic`: Anthropic Messages API (`POST /v1/messages`)
//! - `openai-responses`: `OpenAI` Responses API (`POST /v1/responses`)
//! - `openai-chat`: classic `OpenAI` Chat Completions API (`POST /v1/chat/completions`)
//!
//! The first two are the recommended API types. The classic Chat Completions
//! protocol is kept for compatibility and logs a warning at startup.

pub mod anthropic;
pub mod openai_chat;
pub mod openai_responses;
pub mod react;
pub mod sse;

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use futures_util::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::merror::{MError, MResult};

pub use anthropic::AnthropicClient;
pub use openai_chat::OpenAiChatClient;
pub use openai_responses::OpenAiResponsesClient;
pub use react::{ReActConfig, ReActEvent, ToolExecutor, run_react_loop};

/// A boxed future used by the object-safe provider/tool traits.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A streaming model response. Each item is either an incremental event or an
/// error produced while decoding the provider stream.
pub type LlmStream = Pin<Box<dyn Stream<Item = Result<LlmEvent, MError>> + Send + 'static>>;

/// Supported API flavours. `OpenAiChat` is the legacy "normal `OpenAI`" API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
pub enum LlmApiType {
    /// Anthropic Messages API (recommended).
    #[value(name = "anthropic", alias = "claude")]
    Anthropic,
    /// `OpenAI` Responses API. Consider Anthropic for better performance.
    #[value(name = "openai-responses", alias = "responses")]
    OpenAiResponses,
    /// Classic `OpenAI` Chat Completions API. Kept for compatibility: prefer
    /// `anthropic`, or `openai-responses` when available.
    #[value(name = "openai-chat", alias = "openai", alias = "chat-completions")]
    OpenAiChat,
}

impl LlmApiType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAiResponses => "openai-responses",
            Self::OpenAiChat => "openai-chat",
        }
    }

    /// Default endpoint host when `--llm-api-url` is omitted.
    #[must_use]
    pub const fn default_base_url(self) -> &'static str {
        match self {
            Self::Anthropic => "https://api.anthropic.com",
            Self::OpenAiResponses | Self::OpenAiChat => "https://api.openai.com",
        }
    }

    /// Non-`None` when a different API type would perform better.
    #[must_use]
    pub const fn advice(self) -> Option<&'static str> {
        match self {
            Self::Anthropic => None,
            Self::OpenAiResponses => Some(
                "Consider using `--llm-api-type anthropic` (Anthropic Messages API) for better \
                 performance.",
            ),
            Self::OpenAiChat => Some(
                "Consider using `--llm-api-type anthropic` (Anthropic Messages API) for better \
                 performance, or `--llm-api-type openai-responses` (OpenAI Responses API) if \
                 available.",
            ),
        }
    }
}

impl std::fmt::Display for LlmApiType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Token usage reported by a provider. `cached` is the portion of `input`
/// served from the provider prompt cache.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input: u32,
    pub output: u32,
    pub cached: u32,
}

impl TokenUsage {
    pub const fn add(&mut self, other: Self) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cached = self.cached.saturating_add(other.cached);
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.input == 0 && self.output == 0 && self.cached == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A reasoning block in a provider-neutral shape.
///
/// Anthropic requires thinking blocks (including their opaque `signature`) to
/// be sent back unchanged during a tool-use turn. Other providers only use the
/// text, but keeping the full block preserves the information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningBlock {
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: String,
    },
    RedactedThinking {
        data: String,
    },
}

impl ReasoningBlock {
    #[must_use]
    pub fn text(&self) -> &str {
        match self {
            Self::Thinking { thinking, .. } => thinking,
            Self::RedactedThinking { .. } => "",
        }
    }
}

/// A conversation message in the internal, provider-neutral format.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    pub reasoning_blocks: Vec<ReasoningBlock>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
    /// Raw output items of a Responses API turn, replayed verbatim on the next
    /// tool iteration. Ignored by the other providers.
    pub provider_items: Vec<Value>,
}

impl ChatMessage {
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }

    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }

    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(Role::Assistant, content)
    }

    #[must_use]
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            reasoning_blocks: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            provider_items: Vec::new(),
        }
    }

    #[must_use]
    pub fn assistant_tool_turn(
        content: impl Into<String>,
        reasoning_blocks: Vec<ReasoningBlock>,
        tool_calls: Vec<ToolCall>,
        provider_items: Vec<Value>,
    ) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            reasoning_blocks,
            tool_calls,
            tool_call_id: None,
            provider_items,
        }
    }

    #[must_use]
    fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            reasoning_blocks: Vec::new(),
            tool_calls: Vec::new(),
            tool_call_id: None,
            provider_items: Vec::new(),
        }
    }

    /// Concatenated visible thinking text of this message.
    #[must_use]
    pub fn reasoning_text(&self) -> String {
        self.reasoning_blocks
            .iter()
            .map(ReasoningBlock::text)
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

impl ToolDefinition {
    #[must_use]
    pub fn new(name: impl Into<String>, description: impl Into<String>, parameters: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }
}

/// Incremental events decoded from a provider stream.
#[derive(Debug, Clone)]
pub enum LlmEvent {
    Content(String),
    Reasoning(String),
    ToolCall(ToolCall),
    /// Complete thinking blocks of the current assistant turn, used to replay
    /// Anthropic signatures on the tool continuation.
    ReasoningBlocks(Vec<ReasoningBlock>),
    /// Raw provider output items of the current turn (Responses API replay).
    ProviderItems(Vec<Value>),
    Usage(TokenUsage),
}

/// Object-safe interface implemented by every wire protocol.
pub trait ChatProvider: Send + Sync {
    fn stream_chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
        reasoning_effort: Option<String>,
    ) -> BoxFuture<'_, MResult<LlmStream>>;

    fn api_type(&self) -> LlmApiType;

    fn model(&self) -> &str;

    fn advice(&self) -> Option<&'static str> {
        self.api_type().advice()
    }
}

/// Concrete dispatching client built from the CLI configuration.
#[derive(Clone)]
pub enum LlmClient {
    Anthropic(AnthropicClient),
    OpenAiResponses(OpenAiResponsesClient),
    OpenAiChat(OpenAiChatClient),
}

impl LlmClient {
    #[must_use]
    pub fn new(
        api_type: LlmApiType,
        base_url: String,
        api_key: Option<String>,
        model: String,
    ) -> Self {
        match api_type {
            LlmApiType::Anthropic => {
                Self::Anthropic(AnthropicClient::new(base_url, api_key, model))
            }
            LlmApiType::OpenAiResponses => {
                Self::OpenAiResponses(OpenAiResponsesClient::new(base_url, api_key, model))
            }
            LlmApiType::OpenAiChat => {
                Self::OpenAiChat(OpenAiChatClient::new(base_url, api_key, model))
            }
        }
    }

    #[must_use]
    pub const fn api_type(&self) -> LlmApiType {
        match self {
            Self::Anthropic(_) => LlmApiType::Anthropic,
            Self::OpenAiResponses(_) => LlmApiType::OpenAiResponses,
            Self::OpenAiChat(_) => LlmApiType::OpenAiChat,
        }
    }

    #[must_use]
    pub fn model(&self) -> &str {
        match self {
            Self::Anthropic(client) => client.model(),
            Self::OpenAiResponses(client) => client.model(),
            Self::OpenAiChat(client) => client.model(),
        }
    }
}

impl ChatProvider for LlmClient {
    fn stream_chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
        reasoning_effort: Option<String>,
    ) -> BoxFuture<'_, MResult<LlmStream>> {
        Box::pin(async move {
            match self {
                Self::Anthropic(client) => {
                    client.stream_chat(messages, tools, reasoning_effort).await
                }
                Self::OpenAiResponses(client) => {
                    client.stream_chat(messages, tools, reasoning_effort).await
                }
                Self::OpenAiChat(client) => {
                    client.stream_chat(messages, tools, reasoning_effort).await
                }
            }
        })
    }

    fn api_type(&self) -> LlmApiType {
        Self::api_type(self)
    }

    fn model(&self) -> &str {
        Self::model(self)
    }
}

/// Shared HTTP client builder: long read timeout for streaming, short connect
/// timeout, and a bounded idle pool.
#[must_use]
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .pool_max_idle_per_host(64)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(600))
        .user_agent(concat!("MonopolAI/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_else(|error| {
            tracing::error!("Failed to build the LLM HTTP client: {error}");
            reqwest::Client::new()
        })
}

/// Builds a provider endpoint while tolerating base URLs that already end in
/// `/v1` (common for OpenAI-compatible proxies).
#[must_use]
pub fn endpoint(base_url: &str, path: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/{path}")
    } else {
        format!("{base}/v1/{path}")
    }
}

/// Converts a reasoning effort for APIs that only accept the `OpenAI` values.
#[must_use]
pub fn normalize_openai_effort(effort: &str) -> &str {
    match effort {
        "none" | "minimal" | "low" | "medium" | "high" => effort,
        "xhigh" | "max" => "high",
        _ => "medium",
    }
}

#[cfg(test)]
mod tests {
    use super::{endpoint, normalize_openai_effort};

    #[test]
    fn builds_endpoints_with_and_without_version() {
        assert_eq!(
            endpoint("https://api.openai.com", "responses"),
            "https://api.openai.com/v1/responses"
        );
        assert_eq!(
            endpoint("https://api.openai.com/v1/", "responses"),
            "https://api.openai.com/v1/responses"
        );
    }

    #[test]
    fn normalizes_extended_efforts() {
        assert_eq!(normalize_openai_effort("xhigh"), "high");
        assert_eq!(normalize_openai_effort("max"), "high");
        assert_eq!(normalize_openai_effort("low"), "low");
    }
}
