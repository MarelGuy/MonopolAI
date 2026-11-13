use std::sync::Arc;

use axum::http::{Method, StatusCode};
use futures_util::stream;
use serde_json::json;

use super::common::TestApp;
use crate::llm::{
    BoxFuture, ChatMessage, ChatProvider, LlmApiType, LlmEvent, LlmStream, TokenUsage,
    ToolDefinition,
};
use crate::merror::MResult;

struct MockProvider;

impl ChatProvider for MockProvider {
    fn stream_chat(
        &self,
        _messages: Vec<ChatMessage>,
        _tools: Vec<ToolDefinition>,
        _reasoning_effort: Option<String>,
    ) -> BoxFuture<'_, MResult<LlmStream>> {
        Box::pin(async move {
            let events = vec![
                Ok(LlmEvent::Reasoning("thinking".to_string())),
                Ok(LlmEvent::Content("hello".to_string())),
                Ok(LlmEvent::Usage(TokenUsage {
                    input: 10,
                    output: 2,
                    cached: 3,
                })),
            ];
            Ok(Box::pin(stream::iter(events)) as LlmStream)
        })
    }

    fn api_type(&self) -> LlmApiType {
        LlmApiType::Anthropic
    }

    fn model(&self) -> &'static str {
        "mock-model"
    }
}

#[tokio::test]
async fn llm_stream_emits_reasoning_content_and_done() {
    let app = TestApp::new_with_llm(Arc::new(MockProvider)).await;

    let (status, body) = app
        .raw_request(
            Method::POST,
            "/llm/stream",
            Some("application/json"),
            &json!({ "question": "hello?" }).to_string(),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("\"type\":\"meta\""));
    assert!(body.contains("\"type\":\"Reasoning\",\"data\":\"thinking\""));
    assert!(body.contains("\"type\":\"Content\",\"data\":\"hello\""));
    assert!(body.contains("\"type\":\"stats\""));
    assert!(body.contains("data: [DONE]"));
}

#[tokio::test]
async fn llm_stream_rejects_an_empty_question() {
    let app = TestApp::new_with_llm(Arc::new(MockProvider)).await;

    let (status, body) = app
        .raw_request(
            Method::POST,
            "/llm/stream",
            Some("application/json"),
            &json!({ "question": "   " }).to_string(),
        )
        .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("question"));
}
