//! `ReAct` loop skeleton.
//!
//! The engine is deliberately provider-agnostic:
//!
//! ```text
//!   ┌─────────────────────────────────────────────────────────────┐
//!   │ 1. call the model with the current messages + tool schemas  │
//!   │ 2. stream reasoning / content / tool calls to the client    │
//!   │ 3. if there are no tool calls -> answer, stop               │
//!   │ 4. execute the tools, append assistant + tool messages      │
//!   │ 5. repeat until the model answers without tool calls        │
//!   └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! Providers implement [`ChatProvider`]; tools implement [`ToolExecutor`].
//! Neither knows about HTTP/SSE: the route maps [`ReActEvent`] values to SSE
//! frames.

use futures_util::{StreamExt, stream};
use serde::Serialize;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{
    BoxFuture, ChatMessage, ChatProvider, ReasoningBlock, TokenUsage, ToolCall, ToolDefinition,
};
use crate::merror::{MError, MResult};

/// Executes one tool call and returns the observation text sent back to the
/// model.
pub trait ToolExecutor: Send + Sync {
    fn execute(&self, call: ToolCall) -> BoxFuture<'_, MResult<String>>;
}

#[derive(Debug, Clone)]
pub struct ReActConfig {
    /// Visible answer streamed when the model produced no text at all.
    pub fallback_answer: String,
}

impl Default for ReActConfig {
    fn default() -> Self {
        Self {
            fallback_answer: "I couldn't produce an answer for this request.".to_string(),
        }
    }
}

/// Events produced by the `ReAct` engine. The route serializes them directly as
/// SSE `data:` frames; `Stats` is followed by a final `[DONE]` frame.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ReActEvent {
    #[serde(rename = "status")]
    Status(StatusEvent),
    Content(String),
    Reasoning(String),
    ToolCall(ToolCall),
    ToolResult(ToolResultEvent),
    #[serde(rename = "error")]
    Error(ErrorEvent),
    #[serde(rename = "stats")]
    Stats(StatsEvent),
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorEvent {
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolResultEvent {
    pub call_id: String,
    pub name: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsEvent {
    pub duration_ms: i64,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cached_tokens: u32,
    pub iterations: usize,
}

/// Runs the `ReAct` loop in a background task and returns its event stream.
///
/// Dropping the returned stream does not automatically stop the task; the
/// caller should hold a [`CancellationToken`] and cancel it when the SSE
/// connection closes (see the `/llm/stream` route).
pub fn run_react_loop(
    provider: Arc<dyn ChatProvider>,
    tools: Arc<dyn ToolExecutor>,
    tool_definitions: Vec<ToolDefinition>,
    initial_messages: Vec<ChatMessage>,
    config: ReActConfig,
    reasoning_effort: Option<String>,
    cancel: CancellationToken,
) -> impl futures_util::Stream<Item = ReActEvent> + Send + 'static {
    let (tx, rx) = mpsc::channel::<ReActEvent>(256);

    tokio::spawn(async move {
        run(
            &tx,
            provider,
            tools,
            tool_definitions,
            initial_messages,
            config,
            reasoning_effort,
            &cancel,
        )
        .await;
    });

    stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|event| (event, rx))
    })
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn run(
    tx: &mpsc::Sender<ReActEvent>,
    provider: Arc<dyn ChatProvider>,
    tools: Arc<dyn ToolExecutor>,
    tool_definitions: Vec<ToolDefinition>,
    mut messages: Vec<ChatMessage>,
    config: ReActConfig,
    reasoning_effort: Option<String>,
    cancel: &CancellationToken,
) {
    let started = Instant::now();
    let mut total_usage = TokenUsage::default();
    let mut iterations = 0usize;
    let mut any_content = false;
    let mut had_error = false;

    send(tx, ReActEvent::Status(status("Thinking…"))).await;

    loop {
        if cancel.is_cancelled() {
            return;
        }

        let mut stream = match provider
            .stream_chat(
                messages.clone(),
                tool_definitions.clone(),
                reasoning_effort.clone(),
            )
            .await
        {
            Ok(stream) => stream,
            Err(error) => {
                had_error = true;
                send(tx, ReActEvent::Error(error_event(&error))).await;
                break;
            }
        };

        let mut content = String::new();
        let mut reasoning_blocks: Vec<ReasoningBlock> = Vec::new();
        let mut provider_items = Vec::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut stream_failed = false;

        loop {
            let next = tokio::select! {
                () = cancel.cancelled() => return,
                next = stream.next() => next,
            };

            let Some(part) = next else {
                break;
            };

            match part {
                Ok(super::LlmEvent::Content(text)) => {
                    any_content = true;
                    content.push_str(&text);
                    send(tx, ReActEvent::Content(text)).await;
                }
                Ok(super::LlmEvent::Reasoning(text)) => {
                    send(tx, ReActEvent::Reasoning(text)).await;
                }
                Ok(super::LlmEvent::ToolCall(call)) => {
                    send(tx, ReActEvent::ToolCall(call.clone())).await;
                    tool_calls.push(call);
                }
                Ok(super::LlmEvent::ReasoningBlocks(blocks)) => {
                    reasoning_blocks = blocks;
                }
                Ok(super::LlmEvent::ProviderItems(items)) => {
                    provider_items = items;
                }
                Ok(super::LlmEvent::Usage(usage)) => {
                    total_usage.add(usage);
                }
                Err(error) => {
                    had_error = true;
                    send(tx, ReActEvent::Error(error_event(&error))).await;
                    stream_failed = true;
                    break;
                }
            }
        }

        if stream_failed {
            break;
        }

        iterations += 1;

        if tool_calls.is_empty() {
            break;
        }

        messages.push(ChatMessage::assistant_tool_turn(
            content,
            reasoning_blocks,
            tool_calls.clone(),
            provider_items,
        ));

        for call in tool_calls {
            if cancel.is_cancelled() {
                return;
            }

            send(
                tx,
                ReActEvent::Status(status(format!("Running tool `{}`…", call.name))),
            )
            .await;

            let result = match tools.execute(call.clone()).await {
                Ok(result) => result,
                Err(error) => format!("Error: {error}"),
            };

            send(
                tx,
                ReActEvent::ToolResult(ToolResultEvent {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    result: result.clone(),
                }),
            )
            .await;

            messages.push(ChatMessage::tool(call.id, result));
        }
    }

    if !any_content && !had_error {
        send(tx, ReActEvent::Content(config.fallback_answer)).await;
    }

    send(
        tx,
        ReActEvent::Stats(StatsEvent {
            duration_ms: i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX),
            input_tokens: total_usage.input,
            output_tokens: total_usage.output,
            cached_tokens: total_usage.cached,
            iterations,
        }),
    )
    .await;
}

async fn send(tx: &mpsc::Sender<ReActEvent>, event: ReActEvent) {
    let _ = tx.send(event).await;
}

fn status(message: impl Into<String>) -> StatusEvent {
    StatusEvent {
        message: message.into(),
    }
}

fn error_event(error: &MError) -> ErrorEvent {
    ErrorEvent {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use futures_util::StreamExt;
    use serde_json::json;

    use super::{ReActConfig, ReActEvent, ToolExecutor, run_react_loop};
    use crate::llm::{
        BoxFuture, ChatMessage, ChatProvider, LlmApiType, LlmEvent, LlmStream, ToolCall,
        ToolDefinition,
    };
    use crate::merror::MResult;

    struct MockProvider {
        turns: Mutex<VecDeque<Vec<LlmEvent>>>,
    }

    impl ChatProvider for MockProvider {
        fn stream_chat(
            &self,
            _messages: Vec<ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _reasoning_effort: Option<String>,
        ) -> BoxFuture<'_, MResult<LlmStream>> {
            Box::pin(async move {
                let events = self.turns.lock().unwrap().pop_front().unwrap_or_default();
                Ok(Box::pin(futures_util::stream::iter(events.into_iter().map(Ok))) as LlmStream)
            })
        }

        fn api_type(&self) -> LlmApiType {
            LlmApiType::Anthropic
        }

        fn model(&self) -> &'static str {
            "mock"
        }
    }

    struct MockTools;

    impl ToolExecutor for MockTools {
        fn execute(&self, call: ToolCall) -> BoxFuture<'_, MResult<String>> {
            Box::pin(async move {
                assert_eq!(call.name, "get_board_space");
                Ok(json!({"name": "Old Kent Road"}).to_string())
            })
        }
    }

    #[tokio::test]
    async fn executes_tools_and_returns_the_final_answer() {
        let provider = Arc::new(MockProvider {
            turns: Mutex::new(VecDeque::from(vec![
                vec![
                    LlmEvent::Reasoning("looking up the board".to_string()),
                    LlmEvent::ToolCall(ToolCall {
                        id: "call_1".to_string(),
                        name: "get_board_space".to_string(),
                        arguments: json!({"position": 1}),
                    }),
                    LlmEvent::ReasoningBlocks(vec![crate::llm::ReasoningBlock::Thinking {
                        thinking: "looking up the board".to_string(),
                        signature: "sig".to_string(),
                    }]),
                ],
                vec![LlmEvent::Content("It is Old Kent Road.".to_string())],
            ])),
        });

        let stream = run_react_loop(
            provider,
            Arc::new(MockTools),
            Vec::new(),
            vec![ChatMessage::user("what is at position 1?")],
            ReActConfig::default(),
            Some("medium".to_string()),
            tokio_util::sync::CancellationToken::new(),
        );

        let mut events = Vec::new();
        futures_util::pin_mut!(stream);
        while let Some(event) = stream.next().await {
            events.push(event);
        }

        assert!(events.iter().any(|event| matches!(
            event,
            ReActEvent::ToolResult(result) if result.result == "{\"name\":\"Old Kent Road\"}"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            ReActEvent::Content(text) if text == "It is Old Kent Road."
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            ReActEvent::Stats(stats) if stats.output_tokens == 0 && stats.iterations == 2
        )));
    }
}
