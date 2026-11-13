//! Classic `OpenAI` Chat Completions client (`POST /v1/chat/completions`).
//!
//! This is the "normal `OpenAI`" API type. It is supported for compatibility,
//! but [`super::LlmApiType::OpenAiChat`] logs a warning recommending the
//! Anthropic Messages API or the `OpenAI` Responses API.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    BoxFuture, ChatMessage, ChatProvider, LlmApiType, LlmEvent, LlmStream, Role, TokenUsage,
    ToolCall, ToolDefinition, endpoint, http_client, normalize_openai_effort, sse::SseParser,
    sse::sse_data, sse::sse_stream, sse::take_final_line, sse::take_lines,
};
use crate::merror::{MError, MResult};

#[derive(Clone)]
pub struct OpenAiChatClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl OpenAiChatClient {
    #[must_use]
    pub fn new(base_url: String, api_key: Option<String>, model: String) -> Self {
        Self {
            client: http_client(),
            base_url,
            api_key,
            model,
        }
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
}

impl ChatProvider for OpenAiChatClient {
    fn stream_chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
        reasoning_effort: Option<String>,
    ) -> BoxFuture<'_, MResult<LlmStream>> {
        Box::pin(async move {
            let mut body = json!({
                "model": self.model,
                "messages": chat_messages(&messages),
                "stream": true,
                "stream_options": { "include_usage": true },
            });

            if let Some(effort) = reasoning_effort
                .as_deref()
                .filter(|effort| *effort != "none")
            {
                body["reasoning_effort"] = json!(normalize_openai_effort(effort));
            }
            if !tools.is_empty() {
                body["tools"] = json!(chat_tools(&tools));
                body["tool_choice"] = json!("auto");
            }

            let mut request = self
                .client
                .post(endpoint(&self.base_url, "chat/completions"))
                .json(&body);

            if let Some(api_key) = &self.api_key {
                request = request.bearer_auth(api_key);
            }

            let response = request.send().await.map_err(|error| {
                MError::internal(format!("OpenAI Chat request failed: {error}"))
            })?;

            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                return Err(MError::internal(format!(
                    "OpenAI Chat API error ({status}): {text}"
                )));
            }

            Ok(sse_stream(response, OpenAiChatParser::default()))
        })
    }

    fn api_type(&self) -> LlmApiType {
        LlmApiType::OpenAiChat
    }

    fn model(&self) -> &str {
        &self.model
    }
}

fn chat_messages(messages: &[ChatMessage]) -> Vec<Value> {
    messages
        .iter()
        .map(|message| match message.role {
            Role::System | Role::User => json!({
                "role": if message.role == Role::System { "system" } else { "user" },
                "content": message.content,
            }),
            Role::Assistant => {
                let mut object = json!({
                    "role": "assistant",
                    "content": message.content,
                });

                let reasoning = message.reasoning_text();
                if !reasoning.is_empty() {
                    object["reasoning_content"] = json!(reasoning);
                }
                if !message.tool_calls.is_empty() {
                    object["tool_calls"] = json!(
                        message
                            .tool_calls
                            .iter()
                            .map(|call| json!({
                                "id": call.id,
                                "type": "function",
                                "function": {
                                    "name": call.name,
                                    "arguments": serde_json::to_string(&call.arguments)
                                        .unwrap_or_else(|_| "{}".to_string()),
                                },
                            }))
                            .collect::<Vec<_>>()
                    );
                }

                object
            }
            Role::Tool => json!({
                "role": "tool",
                "tool_call_id": message.tool_call_id.clone().unwrap_or_default(),
                "content": message.content,
            }),
        })
        .collect()
}

fn chat_tools(tools: &[ToolDefinition]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                },
            })
        })
        .collect()
}

#[derive(Default)]
struct OpenAiChatParser {
    buffer: Vec<u8>,
    reasoning: String,
    tool_calls: Vec<PartialToolCall>,
    usage: TokenUsage,
    finished: bool,
}

#[derive(Default)]
struct PartialToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl SseParser for OpenAiChatParser {
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

impl OpenAiChatParser {
    fn handle_line(&mut self, line: &str) -> Vec<Result<LlmEvent, MError>> {
        let Some(data) = sse_data(line) else {
            return Vec::new();
        };

        if data == "[DONE]" {
            return self.finalize();
        }

        let Ok(chunk) = serde_json::from_str::<ChatChunk>(data) else {
            tracing::debug!("Ignoring malformed OpenAI Chat SSE payload: {data}");
            return Vec::new();
        };

        let mut events = Vec::new();

        if let Some(usage) = chunk.usage {
            self.usage = usage.into();
        }

        for choice in chunk.choices {
            let delta = choice.delta;

            if let Some(content) = delta.content.filter(|text| !text.is_empty()) {
                events.push(Ok(LlmEvent::Content(content)));
            }

            let reasoning = delta
                .reasoning_content
                .or(delta.reasoning)
                .filter(|text| !text.is_empty());
            if let Some(reasoning) = reasoning {
                self.reasoning.push_str(&reasoning);
                events.push(Ok(LlmEvent::Reasoning(reasoning)));
            }

            for call in delta.tool_calls {
                if self.tool_calls.len() <= call.index {
                    self.tool_calls
                        .resize_with(call.index + 1, PartialToolCall::default);
                }
                let target = &mut self.tool_calls[call.index];

                if let Some(id) = call.id {
                    target.id = Some(id);
                }
                if let Some(function) = call.function {
                    if let Some(name) = function.name {
                        target.name = Some(name);
                    }
                    if let Some(arguments) = function.arguments {
                        target.arguments.push_str(&arguments);
                    }
                }
            }
        }

        events
    }

    fn finalize(&mut self) -> Vec<Result<LlmEvent, MError>> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;

        let mut events = Vec::new();

        if !self.reasoning.is_empty() {
            events.push(Ok(LlmEvent::ReasoningBlocks(vec![
                super::ReasoningBlock::Thinking {
                    thinking: std::mem::take(&mut self.reasoning),
                    signature: String::new(),
                },
            ])));
        }

        for (index, partial) in std::mem::take(&mut self.tool_calls).into_iter().enumerate() {
            let Some(name) = partial.name else {
                tracing::debug!("Discarding tool call without a function name");
                continue;
            };
            let arguments =
                serde_json::from_str::<Value>(&partial.arguments).unwrap_or_else(|_| json!({}));
            events.push(Ok(LlmEvent::ToolCall(ToolCall {
                id: partial.id.unwrap_or_else(|| format!("call_{index}")),
                name,
                arguments,
            })));
        }

        if !self.usage.is_zero() {
            events.push(Ok(LlmEvent::Usage(self.usage)));
        }

        events
    }
}

#[derive(Deserialize)]
struct ChatChunk {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    delta: ChatDelta,
}

#[derive(Deserialize, Default)]
struct ChatDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    reasoning: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatToolCallDelta>,
}

#[derive(Deserialize)]
struct ChatToolCallDelta {
    index: usize,
    id: Option<String>,
    function: Option<ChatFunctionDelta>,
}

#[derive(Deserialize)]
struct ChatFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    prompt_tokens_details: Option<ChatPromptTokensDetails>,
}

#[derive(Deserialize)]
struct ChatPromptTokensDetails {
    #[serde(default)]
    cached_tokens: u32,
}

impl From<ChatUsage> for TokenUsage {
    fn from(usage: ChatUsage) -> Self {
        Self {
            input: usage.prompt_tokens,
            output: usage.completion_tokens,
            cached: usage
                .prompt_tokens_details
                .map_or(0, |details| details.cached_tokens),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OpenAiChatParser;
    use crate::llm::sse::SseParser;
    use crate::llm::{LlmEvent, ReasoningBlock};

    fn events(parser: &mut OpenAiChatParser, payload: &str) -> Vec<LlmEvent> {
        parser
            .feed(payload.as_bytes())
            .into_iter()
            .filter_map(Result::ok)
            .collect()
    }

    #[test]
    fn streams_content_reasoning_and_tool_calls() {
        let mut parser = OpenAiChatParser::default();

        let first = events(
            &mut parser,
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think \"}}]}\n",
        );
        assert!(matches!(first.as_slice(), [LlmEvent::Reasoning(text)] if text == "think "));

        let second = events(
            &mut parser,
            "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\n",
        );
        assert!(matches!(second.as_slice(), [LlmEvent::Content(text)] if text == "hello"));

        let third = events(
            &mut parser,
            concat!(
                "data: {\"choices\":[{\"delta\":{\"tool_calls\":[",
                "{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"get_board_space\",\"arguments\":\"{\\\"position\\\":\"}},",
                "{\"index\":0,\"function\":{\"arguments\":\"1}\"}}",
                "]}}]}\n"
            ),
        );
        assert!(third.is_empty());

        let usage = events(
            &mut parser,
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":4}}\n",
        );
        assert!(usage.is_empty());

        let done = events(&mut parser, "data: [DONE]\n");
        assert!(matches!(
            done.as_slice(),
            [
                LlmEvent::ReasoningBlocks(blocks),
                LlmEvent::ToolCall(call),
                LlmEvent::Usage(_)
            ] if blocks.len() == 1
                && matches!(&blocks[0], ReasoningBlock::Thinking { thinking, .. } if thinking == "think ")
                && call.name == "get_board_space"
                && call.arguments["position"] == 1
        ));
    }

    #[test]
    fn parser_finishes_idempotently() {
        let mut parser = OpenAiChatParser::default();
        assert!(parser.finish().is_empty());
    }
}
