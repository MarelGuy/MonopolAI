//! Anthropic Messages API client (`POST /v1/messages`).
//!
//! This is one of the two recommended API types. It supports streaming
//! thinking blocks (`thinking_delta` + `signature_delta`), text, and tool use.
//! Thinking blocks are replayed verbatim on tool continuations, as required by
//! the Messages API.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::{
    BoxFuture, ChatMessage, ChatProvider, LlmApiType, LlmEvent, LlmStream, ReasoningBlock, Role,
    TokenUsage, ToolCall, ToolDefinition, endpoint, http_client, sse::SseParser, sse::sse_data,
    sse::sse_stream, sse::take_final_line, sse::take_lines,
};
use crate::merror::{MError, MResult};

const ANTHROPIC_VERSION_ADAPTIVE: &str = "2026-10-21";
const ANTHROPIC_VERSION_EXTENDED: &str = "2023-06-01";
const INTERLEAVED_THINKING_BETA: &str = "interleaved-thinking-2025-05-14";
const DEFAULT_MAX_TOKENS: u32 = 16_384;

#[derive(Clone)]
pub struct AnthropicClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
    max_tokens: u32,
}

impl AnthropicClient {
    #[must_use]
    pub fn new(base_url: String, api_key: Option<String>, model: String) -> Self {
        Self {
            client: http_client(),
            base_url,
            api_key,
            model,
            max_tokens: DEFAULT_MAX_TOKENS,
        }
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
}

impl ChatProvider for AnthropicClient {
    fn stream_chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
        reasoning_effort: Option<String>,
    ) -> BoxFuture<'_, MResult<LlmStream>> {
        Box::pin(async move {
            let modes = thinking_modes(reasoning_effort.as_deref());
            let mut last_error = None;

            for mode in modes {
                let body = anthropic_body(&self.model, self.max_tokens, &messages, &tools, mode);
                match self.send(body, mode, &tools).await {
                    Ok(response) => return Ok(sse_stream(response, AnthropicParser::default())),
                    Err(error) if should_retry_without_thinking(&error) => {
                        tracing::warn!(
                            "Anthropic request with {mode:?} thinking failed ({error}); \
                             retrying with a more compatible thinking configuration"
                        );
                        last_error = Some(error);
                    }
                    Err(error) => return Err(error),
                }
            }

            Err(last_error.unwrap_or_else(|| {
                MError::internal("Anthropic request failed for every thinking configuration")
            }))
        })
    }

    fn api_type(&self) -> LlmApiType {
        LlmApiType::Anthropic
    }

    fn model(&self) -> &str {
        &self.model
    }
}

impl AnthropicClient {
    async fn send(
        &self,
        body: Value,
        mode: ThinkingMode,
        tools: &[ToolDefinition],
    ) -> MResult<reqwest::Response> {
        let mut request = self
            .client
            .post(endpoint(&self.base_url, "messages"))
            .header("anthropic-version", mode.api_version())
            .json(&body);

        if let Some(api_key) = &self.api_key {
            request = request.header("x-api-key", api_key);
        }
        if mode.is_enabled() && !tools.is_empty() {
            // Accepted by the Claude API on every model and ignored where
            // interleaved manual thinking is unsupported.
            request = request.header("anthropic-beta", INTERLEAVED_THINKING_BETA);
        }

        let response = request
            .send()
            .await
            .map_err(|error| MError::internal(format!("Anthropic request failed: {error}")))?;

        if response.status().is_success() {
            return Ok(response);
        }

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        Err(MError::internal(format!(
            "Anthropic API error ({status}): {text}"
        )))
    }
}

#[derive(Debug, Clone, Copy)]
enum ThinkingMode {
    Adaptive(&'static str),
    Manual(u32),
    Disabled,
}

impl ThinkingMode {
    const fn is_enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }

    const fn budget(self) -> u32 {
        match self {
            Self::Manual(budget) => budget,
            Self::Adaptive(_) | Self::Disabled => 0,
        }
    }

    /// Adaptive thinking is only exposed by the current API version; the
    /// legacy manual mode stays on the long-lived `2023-06-01` version so it
    /// keeps working on Claude 4.5 and earlier models.
    const fn api_version(self) -> &'static str {
        match self {
            Self::Adaptive(_) => ANTHROPIC_VERSION_ADAPTIVE,
            Self::Manual(_) | Self::Disabled => ANTHROPIC_VERSION_EXTENDED,
        }
    }
}

fn thinking_modes(effort: Option<&str>) -> Vec<ThinkingMode> {
    match effort {
        None | Some("none") => vec![ThinkingMode::Disabled],
        Some(effort) => {
            let budget = effort_budget(effort);
            vec![
                ThinkingMode::Adaptive(anthropic_effort(effort)),
                ThinkingMode::Manual(budget),
                ThinkingMode::Disabled,
            ]
        }
    }
}

fn effort_budget(effort: &str) -> u32 {
    match effort {
        "minimal" => 1024,
        "low" => 2048,
        "high" => 8192,
        "xhigh" => 16_384,
        "max" => 24_576,
        _ => 4096,
    }
}

fn anthropic_effort(effort: &str) -> &'static str {
    match effort {
        "minimal" | "low" => "low",
        "high" => "high",
        "xhigh" => "xhigh",
        "max" => "max",
        _ => "medium",
    }
}

fn anthropic_body(
    model: &str,
    max_tokens: u32,
    messages: &[ChatMessage],
    tools: &[ToolDefinition],
    mode: ThinkingMode,
) -> Value {
    let (system, messages) = anthropic_messages(messages);
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens.max(mode.budget().saturating_add(1024)),
        "stream": true,
        "messages": messages,
    });

    if let Some(system) = system {
        body["system"] = json!(system);
    }
    if !tools.is_empty() {
        body["tools"] = json!(
            tools
                .iter()
                .map(|tool| json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters,
                }))
                .collect::<Vec<_>>()
        );
        body["tool_choice"] = json!({ "type": "auto" });
    }

    match mode {
        ThinkingMode::Adaptive(effort) => {
            body["thinking"] = json!({
                "type": "adaptive",
                "display": "summarized",
            });
            body["output_config"] = json!({ "effort": effort });
        }
        ThinkingMode::Manual(budget) => {
            body["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": budget,
            });
        }
        ThinkingMode::Disabled => {}
    }

    body
}

/// Converts the neutral conversation into Anthropic's `system` string plus a
/// `messages` array. Consecutive tool results are grouped into one user turn.
fn anthropic_messages(messages: &[ChatMessage]) -> (Option<String>, Vec<Value>) {
    let mut system_parts = Vec::new();
    let mut output = Vec::new();
    let mut tool_results = Vec::new();

    let flush_tool_results = |output: &mut Vec<Value>, tool_results: &mut Vec<Value>| {
        if !tool_results.is_empty() {
            output.push(json!({
                "role": "user",
                "content": std::mem::take(tool_results),
            }));
        }
    };

    for message in messages {
        match message.role {
            Role::System => {
                let content = message.content.trim();
                if !content.is_empty() {
                    system_parts.push(content.to_string());
                }
            }
            Role::Tool => {
                tool_results.push(json!({
                    "type": "tool_result",
                    "tool_use_id": message.tool_call_id.clone().unwrap_or_default(),
                    "content": message.content,
                }));
            }
            Role::User => {
                flush_tool_results(&mut output, &mut tool_results);
                output.push(json!({
                    "role": "user",
                    "content": message.content,
                }));
            }
            Role::Assistant => {
                flush_tool_results(&mut output, &mut tool_results);
                let blocks = assistant_blocks(message);
                if !blocks.is_empty() {
                    output.push(json!({
                        "role": "assistant",
                        "content": blocks,
                    }));
                }
            }
        }
    }

    flush_tool_results(&mut output, &mut tool_results);

    let system = (!system_parts.is_empty()).then(|| system_parts.join("\n\n"));
    (system, output)
}

fn assistant_blocks(message: &ChatMessage) -> Vec<Value> {
    let mut blocks = Vec::new();

    for block in &message.reasoning_blocks {
        match block {
            ReasoningBlock::Thinking {
                thinking,
                signature,
            } if !signature.is_empty() => blocks.push(json!({
                "type": "thinking",
                "thinking": thinking,
                "signature": signature,
            })),
            ReasoningBlock::RedactedThinking { data } => blocks.push(json!({
                "type": "redacted_thinking",
                "data": data,
            })),
            ReasoningBlock::Thinking { .. } => {}
        }
    }

    if !message.content.is_empty() {
        blocks.push(json!({
            "type": "text",
            "text": message.content,
        }));
    }

    for call in &message.tool_calls {
        blocks.push(json!({
            "type": "tool_use",
            "id": call.id,
            "name": call.name,
            "input": call.arguments,
        }));
    }

    blocks
}

fn should_retry_without_thinking(error: &MError) -> bool {
    let message = error.to_string().to_lowercase();
    [
        "thinking",
        "adaptive",
        "display",
        "budget",
        "max_tokens",
        "output_config",
        "effort",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

#[derive(Default)]
struct AnthropicParser {
    buffer: Vec<u8>,
    blocks: HashMap<usize, BlockState>,
    reasoning: Vec<ReasoningBlock>,
    usage: TokenUsage,
    usage_seen: bool,
    finished: bool,
}

enum BlockState {
    Text,
    Thinking {
        thinking: String,
        signature: String,
    },
    Redacted {
        data: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
        input_json: String,
    },
}

impl SseParser for AnthropicParser {
    fn feed(&mut self, chunk: &[u8]) -> Vec<Result<LlmEvent, MError>> {
        let lines = take_lines(&mut self.buffer, chunk);
        let mut events = Vec::new();
        for line in lines {
            events.extend(self.handle_line(&line));
        }
        events
    }

    fn finish(&mut self) -> Vec<Result<LlmEvent, MError>> {
        let mut events = Vec::new();
        if let Some(line) = take_final_line(&mut self.buffer) {
            events.extend(self.handle_line(&line));
        }
        events.extend(self.finalize());
        events
    }
}

impl AnthropicParser {
    fn handle_line(&mut self, line: &str) -> Vec<Result<LlmEvent, MError>> {
        let Some(data) = sse_data(line) else {
            return Vec::new();
        };
        let Ok(event) = serde_json::from_str::<Value>(data) else {
            tracing::debug!("Ignoring malformed Anthropic SSE payload: {data}");
            return Vec::new();
        };

        match event["type"].as_str().unwrap_or_default() {
            "message_start" => {
                self.usage = anthropic_usage(&event["message"]["usage"]);
                self.usage_seen = true;
                Vec::new()
            }
            "content_block_start" => self.block_start(&event),
            "content_block_delta" => self.block_delta(&event),
            "content_block_stop" => self.block_stop(&event),
            "message_delta" => {
                if let Some(output) = event["usage"]["output_tokens"].as_u64() {
                    self.usage.output = output.try_into().unwrap_or(u32::MAX);
                    self.usage_seen = true;
                }
                Vec::new()
            }
            "message_stop" => self.finalize(),
            "error" => {
                let message = event["error"]["message"]
                    .as_str()
                    .unwrap_or("unknown error");
                self.finished = true;
                vec![Err(MError::internal(format!(
                    "Anthropic stream error: {message}"
                )))]
            }
            _ => Vec::new(),
        }
    }

    fn block_start(&mut self, event: &Value) -> Vec<Result<LlmEvent, MError>> {
        let Some(index) = event["index"]
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
        else {
            return Vec::new();
        };
        let block = &event["content_block"];

        match block["type"].as_str().unwrap_or_default() {
            "text" => {
                self.blocks.insert(index, BlockState::Text);
                block["text"]
                    .as_str()
                    .filter(|text| !text.is_empty())
                    .map_or_else(Vec::new, |text| {
                        vec![Ok(LlmEvent::Content(text.to_string()))]
                    })
            }
            "thinking" => {
                let thinking = block["thinking"].as_str().unwrap_or_default().to_string();
                let signature = block["signature"].as_str().unwrap_or_default().to_string();
                let events = if thinking.is_empty() {
                    Vec::new()
                } else {
                    vec![Ok(LlmEvent::Reasoning(thinking.clone()))]
                };
                self.blocks.insert(
                    index,
                    BlockState::Thinking {
                        thinking,
                        signature,
                    },
                );
                events
            }
            "redacted_thinking" => {
                self.blocks.insert(
                    index,
                    BlockState::Redacted {
                        data: block["data"].as_str().unwrap_or_default().to_string(),
                    },
                );
                Vec::new()
            }
            "tool_use" => {
                self.blocks.insert(
                    index,
                    BlockState::ToolUse {
                        id: block["id"].as_str().unwrap_or_default().to_string(),
                        name: block["name"].as_str().unwrap_or_default().to_string(),
                        input: block["input"].clone(),
                        input_json: String::new(),
                    },
                );
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn block_delta(&mut self, event: &Value) -> Vec<Result<LlmEvent, MError>> {
        let Some(index) = event["index"]
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
        else {
            return Vec::new();
        };
        let delta = &event["delta"];

        match delta["type"].as_str().unwrap_or_default() {
            "text_delta" => delta["text"].as_str().map_or_else(Vec::new, |text| {
                if text.is_empty() {
                    Vec::new()
                } else {
                    vec![Ok(LlmEvent::Content(text.to_string()))]
                }
            }),
            "thinking_delta" => {
                let Some(text) = delta["thinking"].as_str().filter(|text| !text.is_empty()) else {
                    return Vec::new();
                };
                if let Some(BlockState::Thinking { thinking, .. }) = self.blocks.get_mut(&index) {
                    thinking.push_str(text);
                }
                vec![Ok(LlmEvent::Reasoning(text.to_string()))]
            }
            "signature_delta" => {
                if let Some(signature) = delta["signature"].as_str()
                    && let Some(BlockState::Thinking {
                        signature: target, ..
                    }) = self.blocks.get_mut(&index)
                {
                    target.push_str(signature);
                }
                Vec::new()
            }
            "input_json_delta" => {
                if let Some(partial) = delta["partial_json"].as_str()
                    && let Some(BlockState::ToolUse { input_json, .. }) =
                        self.blocks.get_mut(&index)
                {
                    input_json.push_str(partial);
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn block_stop(&mut self, event: &Value) -> Vec<Result<LlmEvent, MError>> {
        let Some(index) = event["index"]
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
        else {
            return Vec::new();
        };
        let Some(block) = self.blocks.remove(&index) else {
            return Vec::new();
        };

        match block {
            BlockState::Text => Vec::new(),
            BlockState::Thinking {
                thinking,
                signature,
            } => {
                self.reasoning.push(ReasoningBlock::Thinking {
                    thinking,
                    signature,
                });
                Vec::new()
            }
            BlockState::Redacted { data } => {
                self.reasoning
                    .push(ReasoningBlock::RedactedThinking { data });
                Vec::new()
            }
            BlockState::ToolUse {
                id,
                name,
                input,
                input_json,
            } => {
                let arguments = if input_json.trim().is_empty() {
                    input
                } else {
                    serde_json::from_str::<Value>(&input_json).unwrap_or_else(|error| {
                        tracing::warn!("Invalid Anthropic tool input JSON: {error}");
                        json!({})
                    })
                };
                if name.is_empty() {
                    return Vec::new();
                }
                vec![Ok(LlmEvent::ToolCall(ToolCall {
                    id: if id.is_empty() {
                        format!("call_{index}")
                    } else {
                        id
                    },
                    name,
                    arguments,
                }))]
            }
        }
    }

    fn finalize(&mut self) -> Vec<Result<LlmEvent, MError>> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;

        let mut events = Vec::new();
        if !self.reasoning.is_empty() {
            events.push(Ok(LlmEvent::ReasoningBlocks(std::mem::take(
                &mut self.reasoning,
            ))));
        }
        if self.usage_seen && !self.usage.is_zero() {
            events.push(Ok(LlmEvent::Usage(self.usage)));
        }
        events
    }
}

fn anthropic_usage(usage: &Value) -> TokenUsage {
    let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
    let cache_read = usage["cache_read_input_tokens"].as_u64().unwrap_or(0);
    let cache_creation = usage["cache_creation_input_tokens"].as_u64().unwrap_or(0);
    let output = usage["output_tokens"].as_u64().unwrap_or(0);

    TokenUsage {
        input: (input_tokens + cache_read + cache_creation)
            .try_into()
            .unwrap_or(u32::MAX),
        output: output.try_into().unwrap_or(u32::MAX),
        cached: cache_read.try_into().unwrap_or(u32::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::{AnthropicParser, anthropic_messages};
    use crate::llm::sse::SseParser;
    use crate::llm::{ChatMessage, LlmEvent, ReasoningBlock, ToolCall};
    use serde_json::json;

    fn collect(parser: &mut AnthropicParser, payload: &str) -> Vec<LlmEvent> {
        parser
            .feed(payload.as_bytes())
            .into_iter()
            .filter_map(Result::ok)
            .collect()
    }

    #[test]
    fn streams_thinking_text_and_tool_use_with_signature() {
        let mut parser = AnthropicParser::default();

        let start = collect(
            &mut parser,
            concat!(
                "data: {\"type\":\"message_start\",\"message\":{\"usage\":{",
                "\"input_tokens\":10,\"cache_read_input_tokens\":3,\"output_tokens\":1}}}\n"
            ),
        );
        assert!(start.is_empty());

        collect(
            &mut parser,
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n",
        );
        let reasoning = collect(
            &mut parser,
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"plan\"}}\n",
        );
        assert!(matches!(reasoning.as_slice(), [LlmEvent::Reasoning(text)] if text == "plan"));
        collect(
            &mut parser,
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig\"}}\n",
        );
        collect(
            &mut parser,
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n",
        );

        collect(
            &mut parser,
            "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n",
        );
        let content = collect(
            &mut parser,
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n",
        );
        assert!(matches!(content.as_slice(), [LlmEvent::Content(text)] if text == "hello"));
        collect(
            &mut parser,
            "data: {\"type\":\"content_block_stop\",\"index\":1}\n",
        );

        collect(
            &mut parser,
            concat!(
                "data: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{",
                "\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"get_board_space\",\"input\":{}}}\n"
            ),
        );
        collect(
            &mut parser,
            "data: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"position\\\":1}\"}}\n",
        );
        let tool = collect(
            &mut parser,
            "data: {\"type\":\"content_block_stop\",\"index\":2}\n",
        );
        assert!(matches!(
            tool.as_slice(),
            [LlmEvent::ToolCall(call)] if call.name == "get_board_space" && call.arguments["position"] == 1
        ));

        collect(
            &mut parser,
            "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":8}}\n",
        );
        let done = collect(&mut parser, "data: {\"type\":\"message_stop\"}\n");

        assert!(done.iter().any(|event| matches!(
            event,
            LlmEvent::ReasoningBlocks(blocks)
                if matches!(&blocks[0], ReasoningBlock::Thinking { signature, .. } if signature == "sig")
        )));
        assert!(done.iter().any(|event| matches!(
            event,
            LlmEvent::Usage(usage) if usage.input == 13 && usage.output == 8 && usage.cached == 3
        )));
    }

    #[test]
    fn replays_thinking_blocks_and_groups_tool_results() {
        let assistant = ChatMessage::assistant_tool_turn(
            "checking",
            vec![ReasoningBlock::Thinking {
                thinking: "consider this".to_string(),
                signature: "sig".to_string(),
            }],
            vec![ToolCall {
                id: "toolu_1".to_string(),
                name: "get_board_space".to_string(),
                arguments: json!({"position": 1}),
            }],
            Vec::new(),
        );
        assert_eq!(assistant.reasoning_blocks[0].text(), "consider this");

        let messages = vec![
            ChatMessage::system("be useful"),
            ChatMessage::user("which property is at position 1?"),
            assistant,
            ChatMessage::tool("toolu_1", "{\"total\":7}"),
            ChatMessage::tool("toolu_1", "{\"total\":8}"),
        ];

        let (system, converted) = anthropic_messages(&messages);
        assert_eq!(system.as_deref(), Some("be useful"));
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[1]["role"], "assistant");
        assert_eq!(converted[1]["content"][0]["type"], "thinking");
        assert_eq!(converted[1]["content"][0]["signature"], "sig");
        assert_eq!(converted[2]["role"], "user");
        assert_eq!(converted[2]["content"].as_array().unwrap().len(), 2);
    }
}
