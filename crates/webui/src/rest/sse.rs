//! Server-sent events over `fetch`, because `EventSource` cannot send the bearer token.

use gloo::net::http::RequestBuilder;
use serde::Serialize;
use wasm_bindgen::JsCast as _;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AbortController, ReadableStreamDefaultReader};

#[derive(Debug, PartialEq)]
pub struct SseEvent {
    pub event: String,
    pub data: String,
}

/// Splits a byte stream into events, ignoring ids and keep-alive comments.
#[derive(Default)]
struct SseParser {
    buffer: Vec<u8>,
    event: Option<String>,
    data: Option<String>,
}

impl SseParser {
    fn push(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
    }

    fn next_event(&mut self) -> Option<SseEvent> {
        while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line[..end]);
            let line = line.strip_suffix('\r').unwrap_or(&line);
            if line.is_empty() {
                let event = self.event.take();
                if let Some(data) = self.data.take() {
                    return Some(SseEvent {
                        event: event.unwrap_or_else(|| "message".to_string()),
                        data,
                    });
                }
            } else if !line.starts_with(':') {
                let (field, value) = line.split_once(':').unwrap_or((line, ""));
                let value = value.strip_prefix(' ').unwrap_or(value);
                match field {
                    "event" => self.event = Some(value.to_string()),
                    "data" => match &mut self.data {
                        Some(data) => {
                            data.push('\n');
                            data.push_str(value);
                        }
                        None => self.data = Some(value.to_string()),
                    },
                    _ => {}
                }
            }
        }
        None
    }
}

/// An open event stream, the request is aborted on drop.
pub struct SseStream {
    reader: ReadableStreamDefaultReader,
    abort: AbortController,
    parser: SseParser,
}

impl Drop for SseStream {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

impl SseStream {
    pub async fn open(
        request: RequestBuilder,
        body: Option<&impl Serialize>,
    ) -> Result<Self, String> {
        let abort = AbortController::new().map_err(|error| format!("{error:?}"))?;
        let mut request = request
            .header("Accept", "text/event-stream")
            .abort_signal(Some(&abort.signal()));
        if let Some(token) = crate::auth::token() {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
        let request = match body {
            Some(body) => request.json(body),
            None => request.build(),
        }
        .map_err(|error| error.to_string())?;
        let response = request.send().await.map_err(|error| error.to_string())?;
        if response.status() == 401 {
            crate::auth::auth_required();
        }
        if !response.ok() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("HTTP {status}: {body}"));
        }
        let reader = response
            .body()
            .ok_or("event stream has no body")?
            .get_reader()
            .unchecked_into::<ReadableStreamDefaultReader>();
        Ok(Self {
            reader,
            abort,
            parser: SseParser::default(),
        })
    }

    /// Next event, `None` once the server ends the stream.
    pub async fn next(&mut self) -> Result<Option<SseEvent>, String> {
        loop {
            if let Some(event) = self.parser.next_event() {
                return Ok(Some(event));
            }
            let chunk = JsFuture::from(self.reader.read())
                .await
                .map_err(|error| format!("{error:?}"))?;
            let done = js_sys::Reflect::get(&chunk, &"done".into())
                .ok()
                .and_then(|done| done.as_bool())
                .unwrap_or(true);
            if done {
                return Ok(None);
            }
            let value = js_sys::Reflect::get(&chunk, &"value".into())
                .map_err(|error| format!("{error:?}"))?;
            self.parser
                .push(&value.unchecked_into::<js_sys::Uint8Array>().to_vec());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_events_split_across_chunks() {
        let mut parser = SseParser::default();
        parser.push(b": heartbeat\n\nevent: statuses\r\nid: 1\ndata: [1,");
        assert_eq!(parser.next_event(), None);
        parser.push(b"2]\ndata:3\n\ndata: x\n\n");
        assert_eq!(
            parser.next_event(),
            Some(SseEvent {
                event: "statuses".to_string(),
                data: "[1,2]\n3".to_string(),
            })
        );
        assert_eq!(
            parser.next_event(),
            Some(SseEvent {
                event: "message".to_string(),
                data: "x".to_string(),
            })
        );
        assert_eq!(parser.next_event(), None);
    }
}
