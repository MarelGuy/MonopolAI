//! Minimal Server-Sent Events decoder shared by all providers.
//!
//! Providers differ in the JSON events they send, but not in the framing:
//! bytes arrive in arbitrary chunks and are decoded into `\n`-terminated
//! lines. Each parser owns its own line buffer and implements [`SseParser`].

use std::sync::Arc;

use futures_util::{StreamExt, stream};
use reqwest::Response;
use tokio::sync::Mutex;

use super::{LlmEvent, LlmStream};
use crate::merror::MError;

/// Parses one provider's SSE payloads.
pub trait SseParser: Send + 'static {
    /// Feed a raw byte chunk from the HTTP body.
    fn feed(&mut self, chunk: &[u8]) -> Vec<Result<LlmEvent, MError>>;

    /// Flush a final unterminated line and any accumulated state.
    fn finish(&mut self) -> Vec<Result<LlmEvent, MError>>;
}

/// Turns an HTTP response into a stream of parsed events.
///
/// The parser is shared with a tail future so `finish` runs once when the HTTP
/// body ends, even if the provider did not emit its usual terminator.
pub fn sse_stream(response: Response, parser: impl SseParser) -> LlmStream {
    let parser = Arc::new(Mutex::new(parser));
    let body_parser = Arc::clone(&parser);

    let body = response
        .bytes_stream()
        .then(move |chunk| {
            let parser = Arc::clone(&body_parser);
            async move {
                match chunk {
                    Ok(bytes) => {
                        let events = {
                            let mut parser = parser.lock().await;
                            parser.feed(&bytes)
                        };
                        stream::iter(events)
                    }
                    Err(error) => stream::iter(vec![Err(MError::internal(format!(
                        "LLM stream error: {error}"
                    )))]),
                }
            }
        })
        .flatten();

    let tail = stream::once({
        let parser = Arc::clone(&parser);
        async move {
            let events = {
                let mut parser = parser.lock().await;
                parser.finish()
            };
            stream::iter(events)
        }
    })
    .flatten();

    Box::pin(body.chain(tail))
}

/// Extracts the payload of an SSE `data:` line, ignoring other fields.
#[must_use]
pub fn sse_data(line: &str) -> Option<&str> {
    line.trim().strip_prefix("data:").map(str::trim_start)
}

/// Splits a byte chunk into complete lines, keeping the remainder buffered.
pub fn take_lines(buffer: &mut Vec<u8>, chunk: &[u8]) -> Vec<String> {
    buffer.extend_from_slice(chunk);
    let mut lines = Vec::new();

    while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
        let bytes: Vec<u8> = buffer.drain(..=position).collect();
        lines.push(trim_line(&bytes));
    }

    lines
}

/// Returns the last unterminated line, if any.
pub fn take_final_line(buffer: &mut Vec<u8>) -> Option<String> {
    if buffer.is_empty() {
        return None;
    }

    let bytes = std::mem::take(buffer);
    let line = trim_line(&bytes);
    (!line.is_empty()).then_some(line)
}

fn trim_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches(['\r', '\n'])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{sse_data, take_final_line, take_lines};

    #[test]
    fn splits_chunks_on_newlines_and_keeps_the_remainder() {
        let mut buffer = Vec::new();
        assert_eq!(take_lines(&mut buffer, b"data: a\n"), vec!["data: a"]);
        assert_eq!(take_lines(&mut buffer, b"data: b"), Vec::<String>::new());
        assert_eq!(take_final_line(&mut buffer).as_deref(), Some("data: b"));
    }

    #[test]
    fn extracts_data_payloads() {
        assert_eq!(sse_data("data: {\"a\":1}"), Some("{\"a\":1}"));
        assert_eq!(sse_data("data:{\"a\":1}"), Some("{\"a\":1}"));
        assert_eq!(sse_data("event: message"), None);
    }
}
