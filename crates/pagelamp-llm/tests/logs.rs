//! Canary test on the driver path (design §4.4): prompt text, answer text, provider messages
//! (which can echo the prompt) and keys never reach the logs, even at TRACE. (The drivers open
//! no files at all.) Its own test binary, so the global log subscriber sees only this.

use std::sync::{Arc, Mutex};

use pagelamp_core::ai::Effort;
use pagelamp_core::ai_gate::RenderedPrompt;
use pagelamp_llm::backend::Timeouts;
use pagelamp_llm::profile::{ApiKey, preset};
use pagelamp_llm::{
    Backend, CancellationToken, GenerateRequest, HttpDriver, OutputSpec, check_base_url,
};
use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "sk-demo-not-a-real-key-canarykey";
const CANARY: &str = "zqxcanaryword";

#[derive(Clone)]
struct Buffer(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn capture() -> Arc<Mutex<Vec<u8>>> {
    let logs = Arc::new(Mutex::new(Vec::new()));
    let writer = Buffer(logs.clone());
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .init();
    logs
}

fn backend(base: &str) -> Backend {
    let profile = preset("openai")
        .unwrap()
        .with_base_url(check_base_url(base).unwrap());
    Backend::Http(
        HttpDriver::new(profile, Some(ApiKey::new(KEY)))
            .unwrap()
            .with_timeouts(Timeouts {
                first_byte: std::time::Duration::from_secs(5),
                idle: std::time::Duration::from_secs(5),
            }),
    )
}

fn request() -> GenerateRequest {
    GenerateRequest {
        model: "gpt-6-luna".into(),
        prompt: RenderedPrompt::for_tests(
            "Explain the week.",
            &format!(
                "<course_material id=\"c1\" title=\"Week 3\">\n{CANARY} stomata\n</course_material>"
            ),
        ),
        output: OutputSpec::Text,
        effort: Effort::Lowest,
        max_output_tokens: 100,
    }
}

#[tokio::test]
async fn prompts_answers_provider_messages_and_keys_never_reach_the_logs() {
    let logs = capture();
    let server = MockServer::start().await;
    // 1. A provider error that echoes the prompt back (some do).
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error": {
            "message": format!("Invalid input near '{CANARY} stomata'"),
            "type": "invalid_request_error", "code": null
        }})))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    // 2. A successful answer that repeats it.
    let stream = format!(
        "event: response.output_text.delta\ndata: {{\"type\":\"response.output_text.delta\",\"delta\":\"{CANARY} means\"}}\n\n\
         event: response.completed\ndata: {{\"type\":\"response.completed\",\"response\":{{\"usage\":{{\"input_tokens\":5,\"output_tokens\":2}}}}}}\n\n"
    );
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(stream),
        )
        .mount(&server)
        .await;
    let backend = backend(&format!("{}/v1", server.uri()));
    let failed = backend
        .generate(request(), &|_| {}, CancellationToken::new())
        .await
        .unwrap_err();
    // The student sees the provider's message; the log doesn't.
    assert!(failed.to_string().contains(CANARY));
    let answered = backend
        .generate(request(), &|_| {}, CancellationToken::new())
        .await
        .unwrap();
    assert!(answered.text.contains(CANARY));

    let text = String::from_utf8_lossy(&logs.lock().unwrap()).into_owned();
    assert!(
        text.contains("model call failed"),
        "the failure is logged: {text}"
    );
    assert!(
        !text.contains(CANARY),
        "course or answer text in the log: {text}"
    );
    assert!(
        !text.contains("canarykey") && !text.contains("sk-demo"),
        "key in the log: {text}"
    );
}
