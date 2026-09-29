//! The HTTP drivers against local mock servers with SYNTHETIC stream fixtures (`fixtures/`,
//! each shaped after the vendor's documented stream format, see its header). Live coverage is
//! the owner's smoke test with their own key.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pagelamp_core::ai::{BlockReason, Effort, ModelErrorKind};
use pagelamp_core::ai_gate::RenderedPrompt;
use pagelamp_llm::backend::Timeouts;
use pagelamp_llm::profile::{ApiKey, preset};
use pagelamp_llm::{
    Backend, CancellationToken, GenerateRequest, HttpDriver, LlmError, OutputSpec, StopReason,
    StreamEvent, Usage, check_base_url,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const KEY: &str = "sk-demo-not-a-real-key-1234";
const MATERIAL: &str = "<course_material id=\"c1\" title=\"Week 3 slides\">\nStomata open in light.\n</course_material>\n";

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

fn driver(preset_id: &str, base: &str) -> Backend {
    let profile = preset(preset_id)
        .unwrap()
        .with_base_url(check_base_url(base).unwrap());
    let driver = HttpDriver::new(profile, Some(ApiKey::new(KEY)))
        .unwrap()
        .with_timeouts(Timeouts {
            first_byte: Duration::from_secs(5),
            idle: Duration::from_secs(5),
        });
    Backend::Http(driver)
}

fn request(model: &str, output: OutputSpec) -> GenerateRequest {
    GenerateRequest {
        model: model.to_string(),
        prompt: RenderedPrompt::for_tests("Explain the week. Cite handles.", MATERIAL),
        output,
        effort: Effort::Lowest,
        max_output_tokens: 1000,
    }
}

fn json_output() -> OutputSpec {
    OutputSpec::Json {
        name: "week_summary",
        schema: json!({
            "type": "object",
            "properties": {
                "summary": { "type": "string" },
                "citations": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["summary", "citations"],
            "additionalProperties": false
        }),
    }
}

fn sse(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .insert_header("x-request-id", "req_demo")
        .set_body_string(body)
}

/// Run `request` and collect the streamed events.
async fn run(
    backend: &Backend,
    request: GenerateRequest,
) -> (Result<pagelamp_llm::Outcome, LlmError>, Vec<StreamEvent>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let result = backend
        .generate(
            request,
            &move |event| sink.lock().unwrap().push(event),
            CancellationToken::new(),
        )
        .await;
    let events = events.lock().unwrap().clone();
    (result, events)
}

fn texts(events: &[StreamEvent]) -> String {
    events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::TextDelta(text) => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn body_of(request: &Request) -> Value {
    serde_json::from_slice(&request.body).unwrap()
}

fn model_error(result: Result<pagelamp_llm::Outcome, LlmError>) -> pagelamp_llm::ModelError {
    match result {
        Err(LlmError::Model(error)) => error,
        other => panic!("{other:?}"),
    }
}

// ----- OpenAI Responses -----------------------------------------------------------------------

#[tokio::test]
async fn openai_streams_text_and_usage_with_store_off() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .and(header("authorization", format!("Bearer {KEY}").as_str()))
        .respond_with(sse(fixture("openai_responses/text.sse")))
        .expect(1)
        .mount(&server)
        .await;
    let backend = driver("openai", &format!("{}/v1", server.uri()));
    let (result, events) = run(&backend, request("gpt-6-luna", OutputSpec::Text)).await;
    let outcome = result.unwrap();
    assert_eq!(outcome.text, "Photosynthesis turns light into sugar [c1].");
    assert_eq!(texts(&events), outcome.text);
    assert_eq!(outcome.stop, StopReason::Complete);
    assert_eq!(outcome.json, None);
    assert_eq!(
        outcome.usage,
        Usage {
            input_uncached: 200,
            cache_read: 1000,
            cache_write: 0,
            output: 90,
            reasoning: Some(64),
            estimated: false
        }
    );
    assert_eq!(
        outcome.model_reported.as_deref(),
        Some("gpt-6-luna-2026-08-01")
    );
    assert_eq!(outcome.request_id.as_deref(), Some("req_demo"));

    let received = server.received_requests().await.unwrap();
    let sent = body_of(&received[0]);
    assert_eq!(sent["store"], false);
    assert_eq!(sent["stream"], true);
    assert_eq!(sent["instructions"], "Explain the week. Cite handles.");
    assert_eq!(sent["input"][0]["content"][0]["text"], MATERIAL);
    assert_eq!(sent["reasoning"]["effort"], "low");
    assert_eq!(sent["max_output_tokens"], 1000);
    for absent in [
        "temperature",
        "top_p",
        "tools",
        "previous_response_id",
        "text",
    ] {
        assert!(sent.get(absent).is_none(), "{absent} sent: {sent}");
    }
    let agent = received[0].headers["user-agent"].to_str().unwrap();
    assert!(agent.starts_with("PageLamp/"), "{agent}");
}

#[tokio::test]
async fn openai_structured_output_uses_a_strict_schema_and_parses_the_answer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(sse(fixture("openai_responses/json.sse")))
        .mount(&server)
        .await;
    let backend = driver("openai", &format!("{}/v1", server.uri()));
    let (result, _) = run(&backend, request("gpt-6-luna", json_output())).await;
    let outcome = result.unwrap();
    assert_eq!(
        outcome.json,
        Some(json!({ "summary": "Week 3 covers stomata.", "citations": ["c1"] }))
    );
    let sent = body_of(&server.received_requests().await.unwrap()[0]);
    assert_eq!(sent["text"]["format"]["type"], "json_schema");
    assert_eq!(sent["text"]["format"]["strict"], true);
    assert_eq!(sent["text"]["format"]["name"], "week_summary");
    assert_eq!(
        sent["text"]["format"]["schema"]["additionalProperties"],
        false
    );
}

#[tokio::test]
async fn openai_incomplete_and_refused_answers_say_why() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(sse(fixture("openai_responses/incomplete.sse")))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(sse(fixture("openai_responses/refusal.sse")))
        .mount(&server)
        .await;
    let backend = driver("openai", &format!("{}/v1", server.uri()));
    let (result, _) = run(&backend, request("gpt-6-luna", json_output())).await;
    let outcome = result.unwrap();
    assert_eq!(outcome.stop, StopReason::MaxTokens);
    assert_eq!(outcome.json, None, "a cut-off answer is not parsed");
    let (result, _) = run(&backend, request("gpt-6-luna", OutputSpec::Text)).await;
    assert_eq!(
        result.unwrap().stop,
        StopReason::Refusal("I can't write answers to graded work.".to_string())
    );
}

#[tokio::test]
async fn openai_errors_map_to_kinds_and_quota_is_never_retried() {
    let cases: [(u16, Value, ModelErrorKind, u64); 6] = [
        (
            401,
            json!({"error": {"message": "Incorrect API key provided", "type": "invalid_request_error", "code": "invalid_api_key"}}),
            ModelErrorKind::AuthRejected,
            1,
        ),
        (
            429,
            json!({"error": {"message": "You exceeded your current quota", "type": "insufficient_quota", "code": "insufficient_quota"}}),
            ModelErrorKind::BillingOrQuota,
            1,
        ),
        (
            429,
            json!({"error": {"message": "Project spend limit reached", "type": "requests", "code": "project_spend_limit_exceeded"}}),
            ModelErrorKind::BillingOrQuota,
            1,
        ),
        (
            400,
            json!({"error": {"message": "Your input exceeds the context window", "type": "invalid_request_error", "code": "context_length_exceeded"}}),
            ModelErrorKind::ContextTooLong,
            1,
        ),
        (
            404,
            json!({"error": {"message": "The model `gpt-9` does not exist", "type": "invalid_request_error", "code": "model_not_found"}}),
            ModelErrorKind::ModelNotFound,
            1,
        ),
        (
            503,
            json!({"error": {"message": "The engine is currently overloaded", "type": "server_error", "code": null}}),
            ModelErrorKind::Overloaded,
            3,
        ),
    ];
    for (status, body, kind, tries) in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body.clone()))
            .expect(tries)
            .mount(&server)
            .await;
        let backend = driver("openai", &format!("{}/v1", server.uri()));
        let error = model_error(
            run(&backend, request("gpt-6-luna", OutputSpec::Text))
                .await
                .0,
        );
        assert_eq!(error.kind, kind, "{status} {body}");
        assert_eq!(error.http_status, Some(status));
        assert_eq!(error.message, body["error"]["message"].as_str().unwrap());
        assert!(!error.message.contains(KEY));
        server.verify().await;
    }
}

#[tokio::test]
async fn a_rate_limit_waits_for_retry_after_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "1")
                .set_body_json(json!({"error": {"message": "Rate limit reached", "type": "requests", "code": "rate_limit_exceeded"}})),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("openai_responses/text.sse")))
        .expect(1)
        .mount(&server)
        .await;
    let backend = driver("openai", &format!("{}/v1", server.uri()));
    let started = Instant::now();
    let (result, _) = run(&backend, request("gpt-6-luna", OutputSpec::Text)).await;
    assert!(result.is_ok(), "{result:?}");
    assert!(started.elapsed() >= Duration::from_secs(1));
}

#[tokio::test]
async fn an_error_after_text_is_not_retried_and_says_so() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("openai_responses/midstream_error.sse")))
        .expect(1)
        .mount(&server)
        .await;
    let backend = driver("openai", &format!("{}/v1", server.uri()));
    let (result, events) = run(&backend, request("gpt-6-luna", OutputSpec::Text)).await;
    let error = model_error(result);
    assert_eq!(error.kind, ModelErrorKind::Overloaded);
    assert!(error.after_output);
    assert_eq!(texts(&events), "Photosynthesis");
}

// ----- Anthropic Messages ---------------------------------------------------------------------

#[tokio::test]
async fn anthropic_streams_text_skips_thinking_and_counts_cache() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", KEY))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(sse(fixture("anthropic/text.sse")))
        .expect(1)
        .mount(&server)
        .await;
    let backend = driver("anthropic", &server.uri());
    let (result, events) = run(&backend, request("claude-sonnet-5", OutputSpec::Text)).await;
    let outcome = result.unwrap();
    assert_eq!(outcome.text, "Photosynthesis turns light into sugar [c1].");
    assert_eq!(texts(&events), outcome.text);
    assert_eq!(outcome.stop, StopReason::Complete);
    assert_eq!(
        outcome.usage,
        Usage {
            input_uncached: 200,
            cache_read: 0,
            cache_write: 1000,
            output: 95,
            reasoning: None,
            estimated: false
        }
    );
    assert_eq!(
        outcome.model_reported.as_deref(),
        Some("claude-sonnet-5-20260801")
    );

    let received = server.received_requests().await.unwrap();
    let sent = body_of(&received[0]);
    assert_eq!(sent["system"], "Explain the week. Cite handles.");
    assert_eq!(sent["messages"][0]["content"], MATERIAL);
    assert_eq!(sent["output_config"]["effort"], "low");
    assert_eq!(sent["max_tokens"], 1000);
    for absent in [
        "temperature",
        "top_p",
        "top_k",
        "tools",
        "tool_choice",
        "thinking",
    ] {
        assert!(sent.get(absent).is_none(), "{absent} sent: {sent}");
    }
    assert_eq!(sent["messages"].as_array().unwrap().len(), 1, "no prefill");
    assert!(received[0].headers.get("authorization").is_none());
}

#[tokio::test]
async fn anthropic_structured_output_goes_in_output_config() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("anthropic/json.sse")))
        .mount(&server)
        .await;
    let backend = driver("anthropic", &server.uri());
    // Haiku 4.5 takes no effort parameter (a model quirk from the preset data).
    let (result, _) = run(&backend, request("claude-haiku-4-5", json_output())).await;
    assert_eq!(
        result.unwrap().json,
        Some(json!({ "summary": "Week 3 covers stomata.", "citations": ["c1"] }))
    );
    let sent = body_of(&server.received_requests().await.unwrap()[0]);
    assert_eq!(sent["output_config"]["format"]["type"], "json_schema");
    assert!(sent["output_config"].get("effort").is_none(), "{sent}");
}

#[tokio::test]
async fn anthropic_stop_reasons_and_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("anthropic/max_tokens.sse")))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("anthropic/midstream_error.sse")))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let backend = driver("anthropic", &server.uri());
    let (result, _) = run(&backend, request("claude-sonnet-5", OutputSpec::Text)).await;
    assert_eq!(result.unwrap().stop, StopReason::MaxTokens);
    let error = model_error(
        run(&backend, request("claude-sonnet-5", OutputSpec::Text))
            .await
            .0,
    );
    assert_eq!(error.kind, ModelErrorKind::Overloaded);
    assert!(error.after_output);

    let cases: [(u16, Value, ModelErrorKind, u64); 5] = [
        (
            401,
            json!({"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}}),
            ModelErrorKind::AuthRejected,
            1,
        ),
        (
            400,
            json!({"type": "error", "error": {"type": "invalid_request_error", "message": "prompt is too long: 250000 tokens > 200000 maximum"}}),
            ModelErrorKind::ContextTooLong,
            1,
        ),
        (
            400,
            json!({"type": "error", "error": {"type": "invalid_request_error", "message": "Your credit balance is too low to access the Anthropic API."}}),
            ModelErrorKind::BillingOrQuota,
            1,
        ),
        (
            429,
            json!({"type": "error", "error": {"type": "rate_limit_error", "message": "You have reached your specified workspace spend limit."}}),
            ModelErrorKind::BillingOrQuota,
            1,
        ),
        (
            529,
            json!({"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}}),
            ModelErrorKind::Overloaded,
            3,
        ),
    ];
    for (status, body, kind, tries) in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body.clone()))
            .expect(tries)
            .mount(&server)
            .await;
        let backend = driver("anthropic", &server.uri());
        let error = model_error(
            run(&backend, request("claude-sonnet-5", OutputSpec::Text))
                .await
                .0,
        );
        assert_eq!(error.kind, kind, "{status} {body}");
        server.verify().await;
    }
}

// ----- transport rules ------------------------------------------------------------------------

#[tokio::test]
async fn redirects_are_never_followed() {
    let elsewhere = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("openai_responses/text.sse")))
        .expect(0)
        .mount(&elsewhere)
        .await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(307).insert_header(
            "location",
            format!("{}/v1/responses", elsewhere.uri()).as_str(),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let backend = driver("openai", &format!("{}/v1", server.uri()));
    let error = model_error(
        run(&backend, request("gpt-6-luna", OutputSpec::Text))
            .await
            .0,
    );
    assert_eq!(error.http_status, Some(307));
    elsewhere.verify().await;
}

#[test]
fn coding_plan_endpoints_are_refused_before_any_request() {
    let profile = preset("custom")
        .unwrap()
        .with_base_url(check_base_url("https://api.z.ai/api/coding/paas/v4").unwrap());
    match HttpDriver::new(profile, Some(ApiKey::new(KEY))) {
        Err(LlmError::Blocked { reason, message }) => {
            assert_eq!(reason, BlockReason::CodingPlanKey);
            assert!(
                message.contains("directly invoking model APIs"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
}

/// A server that sends HTTP headers and `first` events, then keeps the connection open
/// without sending anything more.
async fn stalling_server(first: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 64 * 1024];
                let _ = socket.read(&mut buffer).await;
                let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                            connection: close\r\n\r\n";
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(first.as_bytes()).await;
                let _ = socket.flush().await;
                tokio::time::sleep(Duration::from_secs(60)).await;
            });
        }
    });
    format!("http://{address}")
}

const FIRST_DELTA: &str = "event: response.output_text.delta\n\
    data: {\"type\":\"response.output_text.delta\",\"delta\":\"Photo\"}\n\n";

#[tokio::test]
async fn cancelling_mid_stream_stops_at_once() {
    let base = stalling_server(FIRST_DELTA).await;
    let backend = driver("openai", &format!("{base}/v1"));
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let started = Instant::now();
    let result = backend
        .generate(
            request("gpt-6-luna", OutputSpec::Text),
            &move |event| {
                if matches!(event, StreamEvent::TextDelta(_)) {
                    trigger.cancel();
                }
            },
            cancel,
        )
        .await;
    assert!(matches!(result, Err(LlmError::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn a_silent_stream_times_out_after_its_text() {
    let base = stalling_server(FIRST_DELTA).await;
    let profile = preset("openai")
        .unwrap()
        .with_base_url(check_base_url(&format!("{base}/v1")).unwrap());
    let backend = Backend::Http(
        HttpDriver::new(profile, Some(ApiKey::new(KEY)))
            .unwrap()
            .with_timeouts(Timeouts {
                first_byte: Duration::from_secs(5),
                idle: Duration::from_millis(300),
            }),
    );
    let error = model_error(
        run(&backend, request("gpt-6-luna", OutputSpec::Text))
            .await
            .0,
    );
    assert_eq!(error.kind, ModelErrorKind::Timeout);
    assert!(error.after_output, "not retried: text was already shown");
}

// ----- OpenAI-compatible Chat Completions -----------------------------------------------------

#[tokio::test]
async fn openrouter_streams_with_privacy_routing_attribution_and_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(header("authorization", format!("Bearer {KEY}").as_str()))
        .and(header("x-openrouter-title", "PageLamp"))
        .respond_with(sse(fixture("openai_chat/text.sse")))
        .expect(1)
        .mount(&server)
        .await;
    let backend = driver("openrouter", &format!("{}/api/v1", server.uri()));
    let (result, events) = run(&backend, request("openai/gpt-6-luna", OutputSpec::Text)).await;
    let outcome = result.unwrap();
    assert_eq!(outcome.text, "Photosynthesis turns light into sugar [c1].");
    assert_eq!(texts(&events), outcome.text);
    assert_eq!(outcome.stop, StopReason::Complete);
    assert_eq!(
        outcome.usage,
        Usage {
            input_uncached: 200,
            cache_read: 1000,
            cache_write: 0,
            output: 90,
            reasoning: Some(64),
            estimated: false
        }
    );
    assert_eq!(outcome.model_reported.as_deref(), Some("openai/gpt-6-luna"));
    let sent = body_of(&server.received_requests().await.unwrap()[0]);
    assert_eq!(sent["messages"][0]["role"], "system");
    assert_eq!(
        sent["messages"][0]["content"],
        "Explain the week. Cite handles."
    );
    assert_eq!(sent["messages"][1]["content"], MATERIAL);
    assert_eq!(sent["stream_options"]["include_usage"], true);
    assert_eq!(sent["provider"]["data_collection"], "deny");
    assert_eq!(sent["provider"]["require_parameters"], true);
    assert_eq!(sent["reasoning"]["effort"], "low");
    assert_eq!(sent["max_tokens"], 1000);
    for absent in ["temperature", "tools", "tool_choice", "reasoning_effort"] {
        assert!(sent.get(absent).is_none(), "{absent} sent: {sent}");
    }
}

#[tokio::test]
async fn gemini_and_lm_studio_send_effort_only_where_the_preset_says() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("openai_chat/json.sse")))
        .mount(&server)
        .await;
    let gemini = driver("gemini", &format!("{}/v1beta/openai", server.uri()));
    let (result, _) = run(&gemini, request("gemini-3.1-flash-lite", json_output())).await;
    let outcome = result.unwrap();
    assert_eq!(
        outcome.json,
        Some(json!({ "summary": "Week 3 covers stomata.", "citations": ["c1"] }))
    );
    assert_eq!(
        outcome.json_tier,
        Some(pagelamp_llm::request::JsonTier::NativeSchema)
    );
    let lm_studio = driver("lm_studio", &format!("{}/v1", server.uri()));
    run(&lm_studio, request("qwen3.5-9b", OutputSpec::Text))
        .await
        .0
        .unwrap();

    let received = server.received_requests().await.unwrap();
    let gemini_sent = body_of(&received[0]);
    assert_eq!(gemini_sent["reasoning_effort"], "low");
    assert_eq!(gemini_sent["response_format"]["type"], "json_schema");
    assert_eq!(
        gemini_sent["response_format"]["json_schema"]["strict"],
        true
    );
    assert_eq!(
        gemini_sent["response_format"]["json_schema"]["name"],
        "week_summary"
    );
    let lm_sent = body_of(&received[1]);
    assert!(lm_sent.get("reasoning_effort").is_none() && lm_sent.get("reasoning").is_none());
    assert!(lm_sent.get("response_format").is_none());
    // A local server gets no key even if one was stored.
    assert!(received[1].headers.get("authorization").is_none());
}

#[tokio::test]
async fn a_server_ending_without_done_still_completes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("openai_chat/no_done.sse")))
        .mount(&server)
        .await;
    let backend = driver("custom", &format!("{}/v1", server.uri()));
    let outcome = run(&backend, request("local-model", OutputSpec::Text))
        .await
        .0
        .unwrap();
    assert_eq!(outcome.text, "Photosynthesis turns light");
    assert_eq!(outcome.stop, StopReason::MaxTokens);
    assert_eq!(outcome.usage, Usage::default(), "no usage reported");
}

#[tokio::test]
async fn chat_errors_mid_stream_and_in_responses() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("openai_chat/midstream_error.sse")))
        .expect(1)
        .mount(&server)
        .await;
    let backend = driver("openrouter", &format!("{}/api/v1", server.uri()));
    let error = model_error(
        run(&backend, request("openai/gpt-6-luna", OutputSpec::Text))
            .await
            .0,
    );
    assert_eq!(error.kind, ModelErrorKind::Overloaded);
    assert!(error.after_output);

    let cases: [(u16, Value, ModelErrorKind); 4] = [
        (
            402,
            json!({"error": {"code": 402, "message": "Insufficient credits"}}),
            ModelErrorKind::BillingOrQuota,
        ),
        (
            401,
            json!({"error": {"code": 401, "message": "No auth credentials found"}}),
            ModelErrorKind::AuthRejected,
        ),
        // Gemini's compatible endpoint wraps its error in a list.
        (
            429,
            json!([{"error": {"code": 429, "message": "Resource has been exhausted", "status": "RESOURCE_EXHAUSTED"}}]),
            ModelErrorKind::RateLimited,
        ),
        (
            404,
            json!({"error": {"message": "The model `nope` does not exist", "type": "invalid_request_error", "code": "model_not_found"}}),
            ModelErrorKind::ModelNotFound,
        ),
    ];
    for (status, body, kind) in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(status)
                    .insert_header("retry-after", "0")
                    .set_body_json(body.clone()),
            )
            .mount(&server)
            .await;
        let backend = driver("custom", &format!("{}/v1", server.uri()));
        let error = model_error(run(&backend, request("m", OutputSpec::Text)).await.0);
        assert_eq!(error.kind, kind, "{status} {body}");
        assert!(!error.message.is_empty());
    }
}

// ----- structured output: tiers and repair ------------------------------------------------------

#[tokio::test]
async fn a_server_without_schemas_gets_json_object_then_the_answer_is_checked() {
    let server = MockServer::start().await;
    // Rejects the strict schema, like an older compatible server.
    Mock::given(method("POST"))
        .and(wiremock::matchers::body_partial_json(
            json!({ "response_format": { "type": "json_schema" } }),
        ))
        .respond_with(ResponseTemplate::new(400).set_body_json(
            json!({"error": {"message": "response_format json_schema is not supported", "type": "invalid_request_error"}}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(wiremock::matchers::body_partial_json(
            json!({ "response_format": { "type": "json_object" } }),
        ))
        .respond_with(sse(fixture("openai_chat/json.sse")))
        .expect(1)
        .mount(&server)
        .await;
    let backend = driver("custom", &format!("{}/v1", server.uri()));
    let (result, events) = run(&backend, request("local-model", json_output())).await;
    let outcome = result.unwrap();
    assert_eq!(
        outcome.json_tier,
        Some(pagelamp_llm::request::JsonTier::JsonObject)
    );
    assert!(!outcome.repaired);
    assert!(events.contains(&StreamEvent::Notice(
        pagelamp_llm::request::Notice::JsonFallback
    )));
}

#[tokio::test]
async fn an_answer_off_the_format_is_repaired_once_with_the_static_instruction() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(wiremock::matchers::body_partial_json(
            json!({ "response_format": { "type": "json_schema" } }),
        ))
        .respond_with(ResponseTemplate::new(422).set_body_json(
            json!({"error": {"message": "unknown field response_format.json_schema"}}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("openai_chat/json_invalid.sse")))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("openai_chat/json.sse")))
        .mount(&server)
        .await;
    let backend = driver("custom", &format!("{}/v1", server.uri()));
    let (result, events) = run(&backend, request("local-model", json_output())).await;
    let outcome = result.unwrap();
    assert!(outcome.repaired);
    assert_eq!(
        outcome.json,
        Some(json!({ "summary": "Week 3 covers stomata.", "citations": ["c1"] }))
    );
    assert_eq!(
        outcome.usage.input_uncached,
        700 + 800,
        "both calls counted"
    );
    assert!(events.contains(&StreamEvent::Notice(
        pagelamp_llm::request::Notice::Repairing
    )));

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 3);
    let repair = body_of(&received[2]);
    let messages = repair["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[1]["content"], MATERIAL, "the same prompt");
    assert_eq!(messages[2]["role"], "assistant");
    assert_eq!(
        messages[2]["content"],
        "{\"summary\":\"Week 3 covers stomata.\"}"
    );
    assert_eq!(messages[3]["role"], "user");
    assert!(
        messages[3]["content"]
            .as_str()
            .unwrap()
            .starts_with("Your previous answer did not match")
    );
}

#[tokio::test]
async fn a_failed_repair_or_a_bad_strict_answer_is_bad_output() {
    // Below the native tier: one repair, then bad output.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("openai_chat/json_invalid.sse")))
        .expect(2)
        .mount(&server)
        .await;
    let profile = preset("custom")
        .unwrap()
        .with_base_url(check_base_url(&format!("{}/v1", server.uri())).unwrap());
    let mut profile = profile;
    profile.quirks.json_object_only = true;
    let backend = Backend::Http(HttpDriver::new(profile, None).unwrap());
    let error = model_error(run(&backend, request("local-model", json_output())).await.0);
    assert_eq!(error.kind, ModelErrorKind::BadOutput);
    server.verify().await;

    // The strict tier guarantees the format: a violation isn't repaired.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(sse(fixture("openai_chat/json_invalid.sse")))
        .expect(1)
        .mount(&server)
        .await;
    let backend = driver("custom", &format!("{}/v1", server.uri()));
    let error = model_error(run(&backend, request("local-model", json_output())).await.0);
    assert_eq!(error.kind, ModelErrorKind::BadOutput);
    server.verify().await;
}

// ----- Ollama -----------------------------------------------------------------------------------

fn ndjson(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "application/x-ndjson")
        .set_body_string(body)
}

#[tokio::test]
async fn ollama_streams_ndjson_with_a_fitted_context_and_thinking_off() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ndjson(fixture("ollama/text.ndjson")))
        .expect(1)
        .mount(&server)
        .await;
    let backend = driver("ollama", &server.uri());
    let (result, events) = run(&backend, request("qwen3.5:9b", OutputSpec::Text)).await;
    let outcome = result.unwrap();
    assert_eq!(outcome.text, "Photosynthesis turns light into sugar [c1].");
    assert_eq!(texts(&events), outcome.text);
    assert_eq!(outcome.usage.input_uncached, 1300);
    assert_eq!(outcome.usage.output, 24);
    let received = server.received_requests().await.unwrap();
    let sent = body_of(&received[0]);
    assert_eq!(sent["think"], false);
    assert_eq!(sent["stream"], true);
    assert_eq!(sent["options"]["num_predict"], 1000);
    let num_ctx = sent["options"]["num_ctx"].as_u64().unwrap();
    assert!(num_ctx >= 4096 && num_ctx.is_multiple_of(1024), "{num_ctx}");
    assert!(received[0].headers.get("authorization").is_none());
}

#[tokio::test]
async fn ollama_json_uses_the_schema_as_format_and_maps_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ndjson(fixture("ollama/json.ndjson")))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ndjson(fixture("ollama/midstream_error.ndjson")))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"error": "model \"nope\" not found, try pulling it first"})),
        )
        .mount(&server)
        .await;
    let backend = driver("ollama", &server.uri());
    let outcome = run(&backend, request("gemma4:e4b", json_output()))
        .await
        .0
        .unwrap();
    assert_eq!(
        outcome.json,
        Some(json!({ "summary": "Week 3 covers stomata.", "citations": ["c1"] }))
    );
    let sent = body_of(&server.received_requests().await.unwrap()[0]);
    assert_eq!(sent["format"]["type"], "object");
    assert_eq!(sent["format"]["additionalProperties"], false);

    let error = model_error(
        run(&backend, request("qwen3.5:9b", OutputSpec::Text))
            .await
            .0,
    );
    assert!(error.after_output);
    let error = model_error(run(&backend, request("nope", OutputSpec::Text)).await.0);
    assert_eq!(error.kind, ModelErrorKind::ModelNotFound);
}
