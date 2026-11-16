//! "Test": one tiny request to a model, to tell a working key, endpoint
//! and model id from a broken one before a turn depends on it. The answer
//! is the round trip's latency, or an error that says what to fix.

use std::time::{Duration, Instant};

use futures::StreamExt;

use super::{ChatRequest, Message, Provider, ProviderError, StopReason, StreamEvent};

/// How long a test waits for the whole (tiny) answer.
pub const TIMEOUT: Duration = Duration::from_secs(30);

const PROMPT: &str = "Reply with the single word: pong";

/// A model that answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pong {
    /// Request sent to answer finished.
    pub latency: Duration,
    /// The start of what it said (may be empty: some models only stop).
    pub reply: String,
}

impl Pong {
    /// `Answered in 412 ms: "pong"`.
    pub fn summary(&self) -> String {
        let ms = self.latency.as_millis();
        if self.reply.is_empty() {
            format!("Answered in {ms} ms")
        } else {
            format!("Answered in {ms} ms: \"{}\"", self.reply)
        }
    }
}

/// Sends one short prompt to `model` and waits up to `timeout` for the
/// end of the answer.
pub async fn ping(provider: &dyn Provider, model: &str, timeout: Duration) -> Result<Pong, String> {
    let request = ChatRequest {
        model: model.to_string(),
        system: String::new(),
        messages: vec![Message::user(PROMPT)],
        tools: Vec::new(),
        max_tokens: 16,
    };
    let start = Instant::now();
    let answer = async {
        let mut stream = provider.stream(request).await?;
        let mut reply = String::new();
        while let Some(event) = stream.next().await {
            match event? {
                StreamEvent::TextDelta(text) => reply.push_str(&text),
                StreamEvent::Stop(StopReason::Error(message)) => {
                    return Err(ProviderError::Protocol(message));
                }
                StreamEvent::Stop(_) => break,
                _ => {}
            }
        }
        Ok(reply)
    };
    match tokio::time::timeout(timeout, answer).await {
        Err(_) if timeout >= Duration::from_secs(1) => {
            Err(format!("No answer within {} s.", timeout.as_secs()))
        }
        Err(_) => Err(format!("No answer within {} ms.", timeout.as_millis())),
        Ok(Err(error)) => Err(describe(&error)),
        Ok(Ok(reply)) => Ok(Pong {
            latency: start.elapsed(),
            reply: reply.trim().chars().take(40).collect(),
        }),
    }
}

/// A provider error in words that say what to check.
pub fn describe(error: &ProviderError) -> String {
    match error {
        ProviderError::MissingKey(..) => error.to_string(),
        ProviderError::Api { status, message } => {
            let hint = match status {
                401 | 403 => "The API key was rejected",
                404 => "Not found: check the base URL and the model id",
                400 | 422 => "The endpoint refused the request: check the model id",
                429 => "Rate limited or out of credit",
                500.. => "The provider had an error",
                _ => "The request failed",
            };
            format!("{hint} ({status}): {}", message.trim())
        }
        ProviderError::Http(message) => format!("Could not reach the endpoint: {message}"),
        ProviderError::Protocol(message) => format!("Unexpected answer: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::mock::MockProvider;
    use crate::provider::{EventStream, ModelInfo};
    use crate::secrets::ApiKey;
    use crate::settings::{ProviderKind, ProviderSettings};

    struct Failing(ProviderError);

    #[async_trait::async_trait]
    impl Provider for Failing {
        fn id(&self) -> &str {
            "failing"
        }

        async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
            Err(self.0.clone())
        }

        async fn stream(&self, _: ChatRequest) -> Result<EventStream, ProviderError> {
            Err(self.0.clone())
        }
    }

    #[tokio::test]
    async fn a_model_that_answers_reports_its_latency() {
        let mock = MockProvider::new();
        mock.push_text(" pong\n");
        let pong = ping(&mock, "mock-model", TIMEOUT).await.expect("pong");
        assert_eq!(pong.reply, "pong");
        assert!(pong.summary().starts_with("Answered in "), "{pong:?}");
        let sent = &mock.requests()[0];
        assert_eq!(sent.model, "mock-model");
        assert_eq!(sent.max_tokens, 16, "a tiny request");
        assert!(sent.tools.is_empty());
    }

    #[tokio::test]
    async fn errors_say_what_to_fix() {
        let api = |status, message: &str| ProviderError::Api {
            status,
            message: message.into(),
        };
        for (error, expected) in [
            (
                api(401, "invalid x-api-key"),
                "The API key was rejected (401): invalid x-api-key",
            ),
            (
                api(404, "model not found"),
                "Not found: check the base URL and the model id (404)",
            ),
            (api(429, "slow down"), "Rate limited or out of credit (429)"),
            (api(503, "overloaded"), "The provider had an error (503)"),
            (
                ProviderError::Http("connection refused".into()),
                "Could not reach the endpoint: connection refused",
            ),
            (
                ProviderError::MissingKey("Gateway".into(), "an env var".into()),
                "no API key for Gateway",
            ),
        ] {
            let message = ping(&Failing(error), "m", TIMEOUT)
                .await
                .expect_err("an error");
            assert!(message.starts_with(expected), "{message}");
        }

        // An error inside the stream, and an empty script.
        let mock = MockProvider::new();
        mock.push(vec![StreamEvent::Stop(StopReason::Error(
            "bad model".into(),
        ))]);
        assert_eq!(
            ping(&mock, "m", TIMEOUT).await.expect_err("an error"),
            "Unexpected answer: bad model"
        );
        assert!(ping(&mock, "m", TIMEOUT).await.is_err());
    }

    #[tokio::test]
    async fn a_slow_model_times_out() {
        let mut mock = MockProvider::new();
        mock.delay = Some(Duration::from_secs(5));
        mock.push_text("pong");
        let error = ping(&mock, "m", Duration::from_millis(50))
            .await
            .expect_err("an error");
        assert_eq!(error, "No answer within 50 ms.");
    }

    /// A local endpoint (loopback only) that answers 401: the real
    /// OpenAI-compatible client, its error parsing, and no key in the
    /// message.
    #[tokio::test]
    async fn a_rejected_key_from_a_real_client() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = vec![0u8; 8192];
            let _ = socket.read(&mut buf).await;
            let body = r#"{"error":{"message":"Incorrect API key provided"}}"#;
            let response = format!(
                "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.expect("write");
        });
        let settings = ProviderSettings {
            id: "custom-local".into(),
            name: "Local".into(),
            kind: ProviderKind::OpenAiChat,
            base_url: format!("http://{addr}/v1"),
            env_key: None,
            models: vec!["m".into()],
            default_model: "m".into(),
            enabled: true,
            max_tokens: 16,
            custom: true,
            labels: Default::default(),
        };
        let provider = crate::provider::build_with_key(
            &settings,
            Some(ApiKey::new("sk-secret-should-not-leak-1234")),
        );
        let error = ping(provider.as_ref(), "m", TIMEOUT)
            .await
            .expect_err("an error");
        assert_eq!(
            error,
            "The API key was rejected (401): Incorrect API key provided"
        );
        assert!(!error.contains("sk-secret"));
    }
}
