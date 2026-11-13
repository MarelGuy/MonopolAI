//! `POST /llm/stream` — Server-Sent Events endpoint for the `ReAct` agent.
//!
//! Every SSE frame is a JSON object with a `type` and a `data` field:
//!
//! ```text
//! data: {"type":"Reasoning","data":"..."}
//! data: {"type":"Content","data":"..."}
//! data: {"type":"ToolCall","data":{"id":"...","name":"...","arguments":{}}}
//! data: {"type":"ToolResult","data":{"call_id":"...","name":"...","result":"..."}}
//! data: {"type":"status","data":{"message":"..."}}
//! data: {"type":"error","data":{"message":"..."}}
//! data: {"type":"stats","data":{...}}
//! data: [DONE]
//! ```

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, header::CACHE_CONTROL},
    response::IntoResponse,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{StreamExt, stream};
use serde::Deserialize;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::appstate::AppState;
use crate::llm::{ChatMessage, LlmApiType, ReActConfig, ReActEvent, run_react_loop};
use crate::merror::{MError, MResult};
use crate::tools::MonopolyTools;

const DEFAULT_SYSTEM_PROMPT: &str = "You are MonopolAI, an expert Monopoly assistant and player.";

#[derive(Debug, Deserialize)]
pub struct ChatStreamRequest {
    /// The user's current question.
    pub question: String,
    /// Optional previous turns, oldest first.
    #[serde(default)]
    pub history: Vec<HistoryMessage>,
    /// Overrides the default `MonopolAI` system prompt.
    #[serde(default)]
    pub system: Option<String>,
    /// `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`.
    /// Defaults to `medium` for Anthropic/Responses and `none` for the legacy
    /// Chat Completions API type.
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
}

#[allow(clippy::too_many_lines)]
pub async fn chat_stream_handler(
    State(state): State<AppState>,
    Json(request): Json<ChatStreamRequest>,
) -> MResult<impl IntoResponse> {
    let provider = state
        .llm
        .as_ref()
        .ok_or_else(|| MError::internal("LLM provider is not configured"))?
        .clone();

    let question = request.question.trim();
    if question.is_empty() {
        return Err(MError::bad_request("`question` cannot be empty"));
    }

    let reasoning_effort = request
        .reasoning_effort
        .as_deref()
        .unwrap_or_else(|| match provider.api_type() {
            // Classic Chat Completions models often reject `reasoning_effort`;
            // only send it when the caller explicitly asks for it.
            LlmApiType::OpenAiChat => "none",
            LlmApiType::Anthropic | LlmApiType::OpenAiResponses => "medium",
        })
        .trim()
        .to_lowercase();

    let mut messages = Vec::with_capacity(request.history.len() + 2);
    let system_prompt = request
        .system
        .as_deref()
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
        .unwrap_or(DEFAULT_SYSTEM_PROMPT);
    messages.push(ChatMessage::system(system_prompt));

    for message in &request.history {
        let content = message.content.trim();
        if content.is_empty() {
            continue;
        }
        messages.push(if message.role.eq_ignore_ascii_case("assistant") {
            ChatMessage::assistant(content)
        } else {
            ChatMessage::user(content)
        });
    }
    messages.push(ChatMessage::user(question));

    let tools = Arc::new(MonopolyTools::new(state.edition.clone()));
    let cancel = CancellationToken::new();
    let events = run_react_loop(
        provider.clone(),
        tools,
        MonopolyTools::definitions(),
        messages,
        ReActConfig::default(),
        Some(reasoning_effort),
        cancel.clone(),
    );

    let meta = json!({
        "type": "meta",
        "data": {
            "api_type": provider.api_type().as_str(),
            "model": provider.model(),
            "recommended": provider.advice().is_none(),
            "advice": provider.advice(),
        },
    });

    let sse_events = events.flat_map(|event| {
        let is_stats = matches!(event, ReActEvent::Stats(_));
        let payload = serde_json::to_string(&event).unwrap_or_else(|error| {
            format!("{{\"type\":\"error\",\"data\":{{\"message\":\"{error}\"}}}}")
        });

        let mut frames: Vec<Result<Event, Infallible>> = vec![Ok(Event::default().data(payload))];
        if is_stats {
            frames.push(Ok(Event::default().data("[DONE]")));
        }
        stream::iter(frames)
    });

    let sse_stream =
        stream::once(async move { Ok::<_, Infallible>(Event::default().data(meta.to_string())) })
            .chain(sse_events);

    // Cancel the background ReAct task when the SSE stream is dropped (client
    // disconnect) or finishes.
    let guard = CancelOnDrop(cancel);
    let wrapped = stream::unfold(
        (Box::pin(sse_stream), guard),
        |(mut stream, guard)| async move { stream.next().await.map(|item| (item, (stream, guard))) },
    );

    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
    if let Some(advice) = provider.advice() {
        if let Ok(value) = advice.parse::<HeaderValue>() {
            headers.insert("x-llm-api-advice", value);
        }
        tracing::warn!("{advice}");
    }

    Ok((
        headers,
        Sse::new(Box::pin(wrapped)).keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        ),
    ))
}

struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
