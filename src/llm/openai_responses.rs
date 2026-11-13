//! `OpenAI` Responses API client (`POST /v1/responses`).
//!
//! This API type is recommended: it streams reasoning summaries, text, and
//! function calls as semantic events, and lets a stateless tool loop replay the
//! complete `output` array (including encrypted reasoning items).

use serde_json::{Value, json};

use super::{
    BoxFuture, ChatMessage, ChatProvider, LlmApiType, LlmEvent, LlmStream, Role, TokenUsage,
    ToolCall, ToolDefinition, endpoint, http_client, normalize_openai_effort, sse::SseParser,
    sse::sse_data, sse::sse_stream, sse::take_final_line, sse::take_lines,
};
use crate::merror::{MError, MResult};

#[derive(Clone)]
pub struct OpenAiResponsesClient {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl OpenAiResponsesClient {
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

    async fn send(&self, body: &Value) -> MResult<reqwest::Response> {
        let mut request = self
            .client
            .post(endpoint(&self.base_url, "responses"))
            .json(body);

        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }

        let response = request.send().await.map_err(|error| {
            MError::internal(format!("OpenAI Responses request failed: {error}"))
        })?;

        if response.status().is_success() {
            return Ok(response);
        }

        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        Err(MError::internal(format!(
            "OpenAI Responses API error ({status}): {text}"
        )))
    }
}

impl ChatProvider for OpenAiResponsesClient {
    fn stream_chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
        reasoning_effort: Option<String>,
    ) -> BoxFuture<'_, MResult<LlmStream>> {
        Box::pin(async move {
            let instructions = system_instructions(&messages);
            let mut body = json!({
                "model": self.model,
                "input": response_input(&messages),
                "stream": true,
                "store": false,
            });

            if let Some(instructions) = instructions {
                body["instructions"] = json!(instructions);
            }

            let reasoning_enabled = reasoning_effort
                .as_deref()
                .is_some_and(|effort| effort != "none");
            if let Some(effort) = reasoning_effort
                .as_deref()
                .filter(|effort| *effort != "none")
            {
                body["reasoning"] = json!({
                    "effort": normalize_openai_effort(effort),
                    "summary": "auto",
                });
                // Stateless reasoning replay: keep the encrypted reasoning
                // items so they can be sent back with tool outputs.
                body["include"] = json!(["reasoning.encrypted_content"]);
            }

            if !tools.is_empty() {
                body["tools"] = json!(response_tools(&tools));
                body["tool_choice"] = json!("auto");
            }

            let response = match self.send(&body).await {
                Ok(response) => response,
                Err(error) if reasoning_enabled && is_reasoning_error(&error) => {
                    tracing::warn!(
                        "OpenAI Responses rejected reasoning ({error}); retrying without it"
                    );
                    if let Some(object) = body.as_object_mut() {
                        object.remove("reasoning");
                        object.remove("include");
                    }
                    self.send(&body).await?
                }
                Err(error) => return Err(error),
            };

            Ok(sse_stream(response, OpenAiResponsesParser::default()))
        })
    }

    fn api_type(&self) -> LlmApiType {
        LlmApiType::OpenAiResponses
    }

    fn model(&self) -> &str {
        &self.model
    }
}

fn system_instructions(messages: &[ChatMessage]) -> Option<String> {
    let instructions = messages
        .iter()
        .filter(|message| message.role == Role::System)
        .map(|message| message.content.trim())
        .filter(|content| !content.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    (!instructions.is_empty()).then_some(instructions)
}

fn response_input(messages: &[ChatMessage]) -> Vec<Value> {
    let mut input = Vec::new();

    for message in messages {
        if message.role == Role::System {
            continue;
        }

        // A previous Responses turn is replayed as its original output items
        // (reasoning items carry encrypted_content, function calls keep their
        // call_id). This is required for reasoning models with `store: false`.
        if !message.provider_items.is_empty() {
            input.extend(message.provider_items.iter().cloned());
            continue;
        }

        match message.role {
            Role::User => input.push(json!({
                "role": "user",
                "content": message.content,
            })),
            Role::Assistant => {
                if !message.content.is_empty() {
                    input.push(json!({
                        "role": "assistant",
                        "content": message.content,
                    }));
                }
                for call in &message.tool_calls {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": call.id,
                        "name": call.name,
                        "arguments": serde_json::to_string(&call.arguments)
                            .unwrap_or_else(|_| "{}".to_string()),
                    }));
                }
            }
            Role::Tool => input.push(json!({
                "type": "function_call_output",
                "call_id": message.tool_call_id.clone().unwrap_or_default(),
                "output": message.content,
            })),
            Role::System => {}
        }
    }

    input
}

fn response_tools(tools: &[ToolDefinition]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters,
            })
        })
        .collect()
}

#[derive(Default)]
struct OpenAiResponsesParser {
    buffer: Vec<u8>,
    items: Vec<Value>,
    usage: TokenUsage,
    finished: bool,
}

impl SseParser for OpenAiResponsesParser {
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

impl OpenAiResponsesParser {
    fn handle_line(&mut self, line: &str) -> Vec<Result<LlmEvent, MError>> {
        let Some(data) = sse_data(line) else {
            return Vec::new();
        };
        if data == "[DONE]" {
            return self.finalize();
        }

        let Ok(event) = serde_json::from_str::<Value>(data) else {
            tracing::debug!("Ignoring malformed OpenAI Responses SSE payload: {data}");
            return Vec::new();
        };

        let event_type = event["type"].as_str().unwrap_or_default();

        match event_type {
            "response.output_text.delta" | "response.refusal.delta" => {
                event["delta"].as_str().map_or_else(Vec::new, |delta| {
                    if delta.is_empty() {
                        Vec::new()
                    } else {
                        vec![Ok(LlmEvent::Content(delta.to_string()))]
                    }
                })
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                event["delta"].as_str().map_or_else(Vec::new, |delta| {
                    if delta.is_empty() {
                        Vec::new()
                    } else {
                        vec![Ok(LlmEvent::Reasoning(delta.to_string()))]
                    }
                })
            }
            "response.output_item.done" => self.output_item_done(&event["item"]),
            "response.completed" => {
                let response = &event["response"];
                self.usage = usage_from(response);
                // The completed response is authoritative and contains every
                // output item, including reasoning items with encrypted_content.
                if let Some(output) = response["output"]
                    .as_array()
                    .filter(|output| !output.is_empty())
                {
                    self.items = replayable_items(output);
                }
                self.finalize()
            }
            "response.incomplete" => {
                let response = &event["response"];
                self.usage = usage_from(response);
                if let Some(output) = response["output"]
                    .as_array()
                    .filter(|output| !output.is_empty())
                {
                    self.items = replayable_items(output);
                }
                let reason = response["incomplete_details"]["reason"]
                    .as_str()
                    .unwrap_or("unknown reason");
                let mut events = self.finalize();
                events.push(Err(MError::internal(format!(
                    "OpenAI response incomplete: {reason}"
                ))));
                events
            }
            "response.failed" => {
                let message = event["response"]["error"]["message"]
                    .as_str()
                    .unwrap_or("response failed");
                self.finished = true;
                vec![Err(MError::internal(format!(
                    "OpenAI response failed: {message}"
                )))]
            }
            "error" => {
                let message = event["message"]
                    .as_str()
                    .or_else(|| event["error"]["message"].as_str())
                    .unwrap_or("unknown error");
                self.finished = true;
                vec![Err(MError::internal(format!(
                    "OpenAI Responses stream error: {message}"
                )))]
            }
            _ => Vec::new(),
        }
    }

    fn output_item_done(&mut self, item: &Value) -> Vec<Result<LlmEvent, MError>> {
        let item_type = item["type"].as_str().unwrap_or_default();
        if !matches!(item_type, "reasoning" | "function_call" | "message") {
            return Vec::new();
        }

        self.items.push(item.clone());

        if item_type != "function_call" {
            return Vec::new();
        }

        let name = item["name"].as_str().unwrap_or_default();
        if name.is_empty() {
            return Vec::new();
        }

        let arguments = item["arguments"]
            .as_str()
            .and_then(|arguments| serde_json::from_str::<Value>(arguments).ok())
            .unwrap_or_else(|| json!({}));

        vec![Ok(LlmEvent::ToolCall(ToolCall {
            id: item["call_id"]
                .as_str()
                .or_else(|| item["id"].as_str())
                .unwrap_or_default()
                .to_string(),
            name: name.to_string(),
            arguments,
        }))]
    }

    fn finalize(&mut self) -> Vec<Result<LlmEvent, MError>> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;

        let mut events = Vec::new();
        if !self.items.is_empty() {
            events.push(Ok(LlmEvent::ProviderItems(std::mem::take(&mut self.items))));
        }
        if !self.usage.is_zero() {
            events.push(Ok(LlmEvent::Usage(self.usage)));
        }
        events
    }
}

fn is_reasoning_error(error: &MError) -> bool {
    let message = error.to_string().to_lowercase();
    [
        "reasoning",
        "summary",
        "unsupported parameter",
        "unknown parameter",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

fn replayable_items(output: &[Value]) -> Vec<Value> {
    output
        .iter()
        .filter(|item| {
            matches!(
                item["type"].as_str(),
                Some("reasoning" | "function_call" | "message")
            )
        })
        .cloned()
        .collect()
}

fn usage_from(response: &Value) -> TokenUsage {
    let usage = &response["usage"];
    let input = usage["input_tokens"]
        .as_u64()
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let output = usage["output_tokens"]
        .as_u64()
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);
    let cached = usage["input_tokens_details"]["cached_tokens"]
        .as_u64()
        .unwrap_or(0)
        .try_into()
        .unwrap_or(u32::MAX);

    TokenUsage {
        input,
        output,
        cached,
    }
}

#[cfg(test)]
mod tests {
    use super::OpenAiResponsesParser;
    use crate::llm::sse::SseParser;
    use crate::llm::{LlmEvent, ReasoningBlock};

    fn collect(parser: &mut OpenAiResponsesParser, payload: &str) -> Vec<LlmEvent> {
        parser
            .feed(payload.as_bytes())
            .into_iter()
            .filter_map(Result::ok)
            .collect()
    }

    #[test]
    fn streams_reasoning_text_and_function_calls() {
        let mut parser = OpenAiResponsesParser::default();

        let reasoning = collect(
            &mut parser,
            "data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"thinking\"}\n",
        );
        assert!(matches!(reasoning.as_slice(), [LlmEvent::Reasoning(text)] if text == "thinking"));

        let content = collect(
            &mut parser,
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n",
        );
        assert!(matches!(content.as_slice(), [LlmEvent::Content(text)] if text == "hello"));

        let tool = collect(
            &mut parser,
            concat!(
                "data: {\"type\":\"response.output_item.done\",\"item\":{",
                "\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"get_board_space\",",
                "\"arguments\":\"{\\\"position\\\":1}\"}}\n"
            ),
        );
        assert!(matches!(
            tool.as_slice(),
            [LlmEvent::ToolCall(call)] if call.name == "get_board_space" && call.arguments["position"] == 1
        ));

        let completed = collect(
            &mut parser,
            concat!(
                "data: {\"type\":\"response.completed\",\"response\":{",
                "\"usage\":{\"input_tokens\":10,\"output_tokens\":5,",
                "\"input_tokens_details\":{\"cached_tokens\":4}},",
                "\"output\":[{\"type\":\"reasoning\",\"id\":\"rs_1\",",
                "\"summary\":[{\"type\":\"summary_text\",\"text\":\"thinking\"}],",
                "\"encrypted_content\":\"opaque\"},",
                "{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"get_board_space\",",
                "\"arguments\":\"{\\\"position\\\":1}\"}]}}\n"
            ),
        );

        assert!(completed.iter().any(|event| matches!(
            event,
            LlmEvent::ProviderItems(items)
                if items.len() == 2 && items[0]["type"] == "reasoning"
        )));
        assert!(completed.iter().any(|event| matches!(
            event,
            LlmEvent::Usage(usage)
                if usage.input == 10 && usage.output == 5 && usage.cached == 4
        )));
    }

    #[test]
    fn reasoning_blocks_are_optional_for_this_provider() {
        let block = ReasoningBlock::Thinking {
            thinking: "x".to_string(),
            signature: String::new(),
        };
        assert_eq!(block.text(), "x");
    }
}
