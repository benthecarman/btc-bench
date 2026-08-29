//! The live model runner on goose-providers: single-shot, one
//! submit-tool per task, responses written as the JSONL that `grade`
//! consumes. No auxiliary tools, no feedback loop (DESIGN.md, "Runner").

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use bench_core::task::{
    Fixture, IdentifyAnswer, ParamValue, ResponseRecord, ScriptAnswer, TaskAnswer,
};
use bench_gen::prompt::for_fixture_fmt;
use futures_util::StreamExt;
use goose_providers::anthropic::AnthropicProviderBuilder;
use goose_providers::api_client::{ApiClient, AuthMethod};
use goose_providers::conversation::message::{Message, MessageContentBlock};
use goose_providers::errors::ProviderError;
use goose_providers::model::ModelConfig;
use goose_providers::openai::{OpenAiProvider, OpenAiProviderBuilder};
use goose_providers::openai_compatible::OpenAiCompatibleProvider;
use rmcp::model::Tool;
use serde::Deserialize;
use serde_json::json;

pub const SYSTEM_PROMPT: &str = "You are solving Bitcoin Script tasks in a \
benchmark. Read the task carefully, decide your answer, then call the \
provided tool exactly once with your final answer. Do not ask questions.";

/// One `[[model.<name>]]` entry from models.toml.
#[derive(Clone, Debug, Deserialize)]
pub struct ModelEntry {
    /// `openai`, `openai_compatible`, or `anthropic`.
    pub provider: String,
    pub model: String,
    /// Required for `openai_compatible`; defaults per provider otherwise.
    pub base_url: Option<String>,
    /// Environment variable holding the API key; absent means no auth.
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<i32>,
    /// Extra request body parameters, merged as-is (e.g. vLLM
    /// `chat_template_kwargs`).
    #[serde(default)]
    pub request_params: Option<std::collections::HashMap<String, serde_json::Value>>,
    /// Retries for transient transport/server errors (429/5xx/network),
    /// with 2s/8s/30s backoff. Default 3.
    #[serde(default)]
    pub retries: Option<u32>,
}

/// Backoff schedule between transient-error retries.
const RETRY_BACKOFF_SECS: [u64; 3] = [2, 8, 30];

/// Transient transport/server failures worth retrying; behavioral
/// results (no tool call) are NOT retried.
fn is_transient(e: &ProviderError) -> bool {
    let s = e.to_string().to_lowercase();
    s.contains("429")
        || s.contains("rate limit")
        || s.contains("500")
        || s.contains("502")
        || s.contains("503")
        || s.contains("504")
        || s.contains("server error")
        || s.contains("network error")
        || s.contains("timeout")
        || s.contains("connection")
}

#[derive(Debug, Deserialize)]
struct ModelsFile {
    model: BTreeMap<String, ModelEntry>,
}

/// Load models.toml and return (name -> entry) pairs.
pub fn load_models_config(path: &Path) -> Result<BTreeMap<String, ModelEntry>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let file: ModelsFile =
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    if file.model.is_empty() {
        bail!("{} defines no [[model.*]] tables", path.display());
    }
    Ok(file.model)
}

enum Backend {
    OpenAi(OpenAiProvider),
    Compat(OpenAiCompatibleProvider),
    Anthropic(goose_providers::anthropic::AnthropicProvider),
}

fn auth_for(entry: &ModelEntry) -> Result<AuthMethod> {
    match &entry.api_key_env {
        Some(var) => {
            let key = std::env::var(var)
                .with_context(|| format!("environment variable {var} (api key) is not set"))?;
            Ok(AuthMethod::BearerToken(key))
        }
        None => Ok(AuthMethod::NoAuth),
    }
}

fn build_backend(entry: &ModelEntry) -> Result<Backend> {
    match entry.provider.as_str() {
        "openai" => {
            let host = entry
                .base_url
                .clone()
                .unwrap_or_else(|| "https://api.openai.com/v1".into());
            let api = ApiClient::new_with_tls(host, auth_for(entry)?, None)?;
            Ok(Backend::OpenAi(
                OpenAiProviderBuilder::new(api).name("openai").build(),
            ))
        }
        "openai_compatible" => {
            let Some(host) = &entry.base_url else {
                bail!("openai_compatible model entries require base_url");
            };
            // Non-streaming keeps the single-shot loop simple and the
            // mock-server test deterministic.
            let provider = OpenAiCompatibleProvider::new(
                "openai_compatible".into(),
                ApiClient::new_with_tls(host.clone(), auth_for(entry)?, None)?,
                String::new(),
            )
            .with_supports_streaming(false);
            Ok(Backend::Compat(provider))
        }
        "anthropic" => {
            let host = entry
                .base_url
                .clone()
                .unwrap_or_else(|| "https://api.anthropic.com".into());
            let auth = match &entry.api_key_env {
                Some(var) => {
                    let key = std::env::var(var).with_context(|| {
                        format!("environment variable {var} (api key) is not set")
                    })?;
                    AuthMethod::ApiKey {
                        header_name: "x-api-key".into(),
                        key,
                    }
                }
                None => AuthMethod::NoAuth,
            };
            let api = ApiClient::new_with_tls(host, auth, None)?
                .with_header("anthropic-version", "2023-06-01")?;
            Ok(Backend::Anthropic(
                AnthropicProviderBuilder::new(api).name("anthropic").build(),
            ))
        }
        other => bail!("unknown provider {other:?}; use openai, openai_compatible, or anthropic"),
    }
}

impl Backend {
    async fn complete(
        &self,
        cfg: &ModelConfig,
        user: Message,
        tool: Tool,
    ) -> Result<Vec<Message>, ProviderError> {
        use goose_providers::base::MessageStream;
        let stream: MessageStream = match self {
            Backend::OpenAi(p) => {
                p.stream_for_model(
                    cfg,
                    &cfg.model_name,
                    &cfg.model_name,
                    SYSTEM_PROMPT,
                    &[user],
                    &[tool],
                )
                .await?
            }
            Backend::Compat(p) => {
                p.stream_for_model(
                    cfg,
                    &cfg.model_name,
                    &cfg.model_name,
                    SYSTEM_PROMPT,
                    &[user],
                    &[tool],
                )
                .await?
            }
            Backend::Anthropic(p) => {
                p.stream_for_model(cfg, &cfg.model_name, SYSTEM_PROMPT, &[user], &[tool])
                    .await?
            }
        };
        let mut collected = Vec::new();
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            let (msg, _usage) = item?;
            if let Some(m) = msg {
                collected.push(m);
            }
        }
        Ok(collected)
    }
}

fn submit_script_tool() -> Tool {
    let schema = json!({
        "type": "object",
        "properties": {
            "script": {
                "type": "string",
                "description": "The Bitcoin Script as a hex string or Bitcoin Core asm"
            }
        },
        "required": ["script"]
    });
    Tool::new(
        "submit_script",
        "Submit your final Bitcoin Script answer.",
        Arc::new(schema.as_object().expect("object schema").clone()),
    )
}

fn submit_identify_tool() -> Tool {
    let schema = json!({
        "type": "object",
        "properties": {
            "label": {
                "type": "string",
                "description": "The script family label"
            },
            "params": {
                "type": "object",
                "description": "Numeric parameters where applicable, e.g. k, n, timeout"
            }
        },
        "required": ["label"]
    });
    Tool::new(
        "submit_identify",
        "Submit your final identification answer.",
        Arc::new(schema.as_object().expect("object schema").clone()),
    )
}

/// Parse a textual Qwen-style tool call out of content, for endpoints
/// that stream the marker as plain text instead of structured
/// `tool_calls` (SGLang without `--tool-call-parser`). Accepts the
/// XML-ish form `<tool_call><function=name><parameter=k>v</parameter>`
/// and the JSON form `{"name": ..., "arguments": {...}}`.
fn parse_textual_tool_call(
    text: &str,
) -> Option<(String, serde_json::Map<String, serde_json::Value>)> {
    let start = text.find("<tool_call>")?;
    let end = text[start..].find("</tool_call>")? + start;
    let body = &text[start + "<tool_call>".len()..end];
    let body = body.trim();
    if body.trim_start().starts_with('{') {
        // JSON form: {"name": "...", "arguments": {...}}
        let v: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
        let name = v.get("name")?.as_str()?.to_string();
        let args = match v.get("arguments") {
            Some(serde_json::Value::Object(m)) => m.clone(),
            _ => Default::default(),
        };
        return Some((name, args));
    }
    // XML-ish form.
    let fstart = body.find("<function=")? + "<function=".len();
    let fend = body[fstart..].find('>')? + fstart;
    let name = body[fstart..fend].trim().to_string();
    let mut args = serde_json::Map::new();
    let mut i = 0;
    while let Some(pstart) = body[i..].find("<parameter=") {
        let pstart = i + pstart;
        let pname_from = pstart + "<parameter=".len();
        let pname_to = body[pname_from..].find('>')? + pname_from;
        let pname = body[pname_from..pname_to].trim().to_string();
        let vfrom = pname_to + 1;
        let vto_abs = body[vfrom..].find("</parameter>")? + vfrom;
        let value = body[vfrom..vto_abs].trim().to_string();
        args.insert(pname, serde_json::Value::String(value));
        i = vto_abs + "</parameter>".len();
        if i >= body.len() {
            break;
        }
    }
    Some((name, args))
}

/// Pull the submitted answer and any plain text out of the streamed
/// messages. Tool calls arrive complete even on streaming providers.
/// Falls back to parsing a textual `<tool_call>` marker when the endpoint
/// produced no structured tool call.
fn extract_answer(messages: &[Message]) -> (Option<TaskAnswer>, String) {
    let mut raw = String::new();
    for m in messages {
        for block in &m.content {
            match block {
                MessageContentBlock::Text(t) => raw.push_str(&t.text),
                MessageContentBlock::Thinking(t) => raw.push_str(&t.thinking),
                MessageContentBlock::ToolRequest(tr) => {
                    if let Ok(params) = &tr.tool_call {
                        let args = params.arguments.clone().unwrap_or_default();
                        if let Some(answer) = task_answer_from(&params.name, &args) {
                            return (Some(answer), raw);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if let Some((name, args)) = parse_textual_tool_call(&raw) {
        let answer = task_answer_from(&name, &args);
        if answer.is_some() {
            return (answer, raw);
        }
    }
    (None, raw)
}

fn task_answer_from(
    name: &str,
    args: &serde_json::Map<String, serde_json::Value>,
) -> Option<TaskAnswer> {
    if name == "submit_script" {
        args.get("script").and_then(|v| v.as_str()).map(|script| {
            TaskAnswer::Script(ScriptAnswer {
                script: script.to_string(),
            })
        })
    } else if name == "submit_identify" {
        let label = args
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let params: BTreeMap<String, ParamValue> = args
            .get("params")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        Some(TaskAnswer::Identify(IdentifyAnswer {
            label: label.to_string(),
            params,
        }))
    } else {
        None
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct RunStats {
    pub answered: usize,
    pub failed: usize,
}

/// Run the benchmark: one request per fixture, sequential. Writes
/// `responses.jsonl` (grade-ready) and `failures.jsonl` (no tool call or
/// transport error, with the raw text for auditing).
pub async fn run(
    fixtures: &[Fixture],
    entry: &ModelEntry,
    out_dir: &Path,
    concurrency: usize,
    display: bench_gen::prompt::DisplayFormat,
) -> Result<RunStats> {
    std::fs::create_dir_all(out_dir)?;
    let backend = Arc::new(build_backend(entry)?);
    let cfg = ModelConfig {
        model_name: entry.model.clone(),
        context_limit: None,
        temperature: Some(entry.temperature.unwrap_or(0.0)),
        max_tokens: entry.max_tokens,
        toolshim: false,
        toolshim_model: None,
        request_params: entry.request_params.clone(),
        reasoning: None,
        supports_vision: None,
        request_headers: None,
    };

    // Work queue of owned fixtures; N workers, one collector that owns
    // the output files. Response order is completion order, not task
    // order — grading matches by task id.
    let concurrency = concurrency.max(1);
    let (fix_tx, fix_rx) = tokio::sync::mpsc::channel::<Fixture>(concurrency);
    // Feeder task: the buffer is small, so enqueueing inline before the
    // workers exist would deadlock once it fills.
    let to_send: Vec<Fixture> = fixtures.to_vec();
    tokio::spawn(async move {
        for f in to_send {
            fix_tx.send(f).await.expect("worker alive");
        }
    });
    let fix_rx = std::sync::Arc::new(tokio::sync::Mutex::new(fix_rx));

    let entry_retries = entry.retries.unwrap_or(3);
    let (res_tx, mut res_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..concurrency {
        let rx = Arc::clone(&fix_rx);
        let backend = Arc::clone(&backend);
        let cfg = cfg.clone();
        let res_tx = res_tx.clone();
        let display = display;
        set.spawn(async move {
            loop {
                let f = rx.lock().await.recv().await;
                let Some(f) = f else { break };
                let prompt = for_fixture_fmt(&f, display);
                let (tool, is_identify) = match &f {
                    Fixture::Identify(_) => (submit_identify_tool(), true),
                    _ => (submit_script_tool(), false),
                };
                let retries = entry_retries;
                let mut attempts: u32 = 0;
                let result = loop {
                    let user = Message::user().with_text(prompt.as_str());
                    match backend.complete(&cfg, user, tool.clone()).await {
                        Err(e) if is_transient(&e) && attempts < retries => {
                            let backoff = RETRY_BACKOFF_SECS
                                [(attempts as usize).min(RETRY_BACKOFF_SECS.len() - 1)];
                            tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                            attempts += 1;
                        }
                        other => break other,
                    }
                };
                let _ = res_tx.send((f, is_identify, result, attempts));
            }
        });
    }
    drop(res_tx);

    let out_dir = out_dir.to_path_buf();
    let collector = tokio::spawn(async move {
        let responses_path = out_dir.join("responses.jsonl");
        let failures_path = out_dir.join("failures.jsonl");
        let mut responses = std::fs::File::create(&responses_path)?;
        let mut failures = std::fs::File::create(&failures_path)?;
        let mut stats = RunStats {
            answered: 0,
            failed: 0,
        };
        while let Some((f, is_identify, outcome, attempts)) = res_rx.recv().await {
            match outcome {
                Ok(messages) => {
                    let (answer, raw) = extract_answer(&messages);
                    match answer {
                        Some(a) => {
                            let record = ResponseRecord {
                                task_id: f.id().to_string(),
                                answer: a,
                                raw: if raw.is_empty() { None } else { Some(raw) },
                            };
                            writeln!(responses, "{}", serde_json::to_string(&record)?)?;
                            stats.answered += 1;
                        }
                        None => {
                            writeln!(
                                failures,
                                "{}",
                                serde_json::json!({
                                    "task_id": f.id(),
                                    "error": "no tool call in response",
                                    "raw": raw,
                                    "identify_task": is_identify,
                                    "attempts": attempts,
                                })
                            )?;
                            stats.failed += 1;
                        }
                    }
                }
                Err(e) => {
                    writeln!(
                        failures,
                        "{}",
                        serde_json::json!({
                            "task_id": f.id(),
                            "error": e.to_string(),
                            "identify_task": is_identify,
                            "attempts": attempts,
                        })
                    )?;
                    stats.failed += 1;
                }
            }
        }
        Ok::<_, anyhow::Error>(stats)
    });

    set.join_all().await;
    let stats = collector
        .await
        .map_err(|e| anyhow::anyhow!("collector panicked: {e}"))??;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grade;
    use bench_gen::fixtures::{generate, GenParams};
    use bench_gen::prompt::DisplayFormat;
    use std::collections::VecDeque;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};

    fn read_request(stream: &mut TcpStream) -> String {
        let mut data = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = stream.read(&mut chunk).expect("read request");
            if n == 0 {
                break;
            }
            data.extend_from_slice(&chunk[..n]);
            if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&data[..pos]).to_ascii_lowercase();
                let len: usize = headers
                    .lines()
                    .find(|l| l.starts_with("content-length:"))
                    .and_then(|l| l.split(':').nth(1))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
                if data.len() >= pos + 4 + len {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&data).into_owned()
    }

    /// Spawn a mock OpenAI-compatible server answering each request with
    /// the next canned body; returns (base_url, captured requests).
    /// Mock that fails the first `fail_first` requests with `status`,
    /// then answers every request with `final_body`.
    fn spawn_mock_scenario(
        fail_first: usize,
        status: u16,
        final_body: serde_json::Value,
    ) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let cap = Arc::clone(&captured);
        std::thread::spawn(move || {
            let mut served = 0usize;
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(s) => s,
                    Err(_) => break,
                };
                let req = read_request(&mut stream);
                cap.lock().expect("lock").push(req);
                let (code, body) = if served < fail_first {
                    (
                        status,
                        json!({"error": {"message": "Server error 429 rate limited"}}),
                    )
                } else {
                    (200, final_body.clone())
                };
                served += 1;
                let payload = serde_json::to_string(&body).expect("serialize body");
                let reason = if code == 200 { "OK" } else { "Error" };
                let resp = format!(
                    "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                );
                stream.write_all(resp.as_bytes()).expect("write response");
            }
        });
        (format!("http://{addr}/v1"), captured)
    }

    fn spawn_mock(bodies: Vec<serde_json::Value>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let cap = Arc::clone(&captured);
        std::thread::spawn(move || {
            let mut queue: VecDeque<serde_json::Value> = bodies.into();
            while let Some(body) = queue.pop_front() {
                let (mut stream, _) = match listener.accept() {
                    Ok(s) => s,
                    Err(_) => break,
                };
                let req = read_request(&mut stream);
                cap.lock().expect("lock").push(req);
                let payload = serde_json::to_string(&body).expect("serialize body");
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                );
                stream.write_all(resp.as_bytes()).expect("write response");
            }
        });
        (format!("http://{addr}/v1"), captured)
    }

    fn completion_with_tool(name: &str, args: serde_json::Value) -> serde_json::Value {
        json!({
            "id": "1", "object": "chat.completion", "created": 0, "model": "mock",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "c1", "type": "function",
                        "function": {"name": name, "arguments": args.to_string()}
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        })
    }

    fn completion_text(text: &str) -> serde_json::Value {
        json!({
            "id": "1", "object": "chat.completion", "created": 0, "model": "mock",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": text},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        })
    }

    fn entry(base_url: String) -> ModelEntry {
        ModelEntry {
            provider: "openai_compatible".into(),
            model: "mock".into(),
            base_url: Some(base_url),
            api_key_env: None,
            temperature: Some(0.0),
            max_tokens: None,
            request_params: None,
            retries: None,
        }
    }

    fn tmpdir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("btc-bench-runner-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[tokio::test]
    async fn runner_end_to_end_mock() {
        let fixtures = generate(&GenParams {
            seed: 5,
            write: 1,
            optimize: 0,
            identify: 0,
        });
        let hex = match &fixtures[0] {
            Fixture::Write(w) => w.reference_script_hex.clone(),
            other => panic!("expected write fixture, got {}", other.id()),
        };
        let (base, captured) = spawn_mock(vec![completion_with_tool(
            "submit_script",
            json!({ "script": hex }),
        )]);
        let out = tmpdir("e2e");
        let stats = run(&fixtures, &entry(base), &out, 1, DisplayFormat::Hex)
            .await
            .expect("run");
        assert_eq!(stats.answered, 1);
        assert_eq!(stats.failed, 0);

        // The request carried the tool schema and the prompt.
        let reqs = captured.lock().expect("lock");
        assert!(
            reqs[0].contains("submit_script"),
            "tool missing from request"
        );
        assert!(
            reqs[0].contains("Bitcoin Script"),
            "prompt missing from request"
        );

        // The response file grades perfectly.
        let text = std::fs::read_to_string(out.join("responses.jsonl")).expect("responses");
        let record: ResponseRecord =
            serde_json::from_str(text.trim_end()).expect("parse response record");
        assert_eq!(record.task_id, fixtures[0].id());
        let (_, summary) = grade(&fixtures, &[record], 0.5).expect("grade");
        assert_eq!(summary.write_n, 1);
        assert!((summary.write_mean - 1.0).abs() < 1e-9, "{summary:?}");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[tokio::test]
    async fn no_tool_call_is_a_recorded_failure() {
        let fixtures = generate(&GenParams {
            seed: 5,
            write: 1,
            optimize: 0,
            identify: 0,
        });
        let (base, _) = spawn_mock(vec![completion_text("I would rather not.")]);
        let out = tmpdir("no-tool");
        let stats = run(&fixtures, &entry(base), &out, 1, DisplayFormat::Hex)
            .await
            .expect("run");
        assert_eq!(stats.answered, 0);
        assert_eq!(stats.failed, 1);
        let failures = std::fs::read_to_string(out.join("failures.jsonl")).expect("failures");
        assert!(failures.contains("no tool call in response"));
        assert!(failures.contains("I would rather not."));
        let responses =
            std::fs::read_to_string(out.join("responses.jsonl")).expect("responses file");
        assert!(responses.trim().is_empty(), "no answers should be recorded");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[tokio::test]
    async fn identify_round_trip_mock() {
        let fixtures = generate(&GenParams {
            seed: 6,
            write: 0,
            optimize: 0,
            identify: 1,
        });
        let bodies: Vec<serde_json::Value> = fixtures
            .iter()
            .map(|f| match f {
                Fixture::Identify(i) => completion_with_tool(
                    "submit_identify",
                    json!({ "label": i.family, "params": i.params }),
                ),
                other => panic!("expected identify fixture, got {}", other.id()),
            })
            .collect();
        let (base, _) = spawn_mock(bodies);
        let out = tmpdir("identify");
        let stats = run(&fixtures, &entry(base), &out, 1, DisplayFormat::Hex)
            .await
            .expect("run");
        assert_eq!(stats.answered, fixtures.len());
        assert_eq!(stats.failed, 0);
        let text = std::fs::read_to_string(out.join("responses.jsonl")).expect("responses");
        let records: Vec<ResponseRecord> = text
            .lines()
            .map(|l| serde_json::from_str(l).expect("record"))
            .collect();
        let (_, summary) = grade(&fixtures, &records, 0.5).expect("grade");
        assert_eq!(summary.identify_n, fixtures.len());
        assert!((summary.identify_mean - 1.0).abs() < 1e-9, "{summary:?}");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[tokio::test]
    async fn concurrent_workers_all_answer() {
        let fixtures = generate(&GenParams {
            seed: 9,
            write: 4,
            optimize: 0,
            identify: 0,
        });
        let bodies: Vec<serde_json::Value> = fixtures
            .iter()
            .map(|f| match f {
                Fixture::Write(w) => completion_with_tool(
                    "submit_script",
                    json!({ "script": w.reference_script_hex }),
                ),
                other => panic!("expected write fixture, got {}", other.id()),
            })
            .collect();
        let (base, _) = spawn_mock(bodies);
        let out = tmpdir("concurrent");
        let stats = run(&fixtures, &entry(base), &out, 4, DisplayFormat::Hex)
            .await
            .expect("run");
        assert_eq!(stats.answered, 4);
        assert_eq!(stats.failed, 0);
        let text = std::fs::read_to_string(out.join("responses.jsonl")).expect("responses");
        let records: Vec<ResponseRecord> = text
            .lines()
            .map(|l| serde_json::from_str(l).expect("record"))
            .collect();
        assert_eq!(records.len(), 4);
        let (_, summary) = grade(&fixtures, &records, 0.5).expect("grade");
        assert_eq!(summary.write_n, 4);
        assert!((summary.write_mean - 1.0).abs() < 1e-9, "{summary:?}");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn textual_tool_call_qwen_format() {
        let text = "</think>\n\n<tool_call>\n<function=submit_script>\n<parameter=script>\n5221028c6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b6b52ae\n</parameter>\n</function>\n</tool_call>";
        let (name, args) = parse_textual_tool_call(text).expect("parsed");
        assert_eq!(name, "submit_script");
        assert!(args["script"].as_str().unwrap().starts_with("5221"));
    }

    #[test]
    fn textual_tool_call_json_format() {
        let text = r#"<tool_call>{"name": "submit_identify", "arguments": {"label": "p2wsh_multisig", "params": {"k": 2, "n": 3}}}</tool_call>"#;
        let (name, args) = parse_textual_tool_call(text).expect("parsed");
        assert_eq!(name, "submit_identify");
        assert_eq!(args["label"], "p2wsh_multisig");
        assert_eq!(args["params"]["k"], 2);
    }

    #[tokio::test]
    async fn content_only_tool_marker_is_parsed() {
        let fixtures = generate(&GenParams {
            seed: 5,
            write: 1,
            optimize: 0,
            identify: 0,
        });
        let hex = match &fixtures[0] {
            Fixture::Write(w) => w.reference_script_hex.clone(),
            other => panic!("expected write fixture, got {}", other.id()),
        };
        let content = format!(
            "reasoning...\n</think>\n\n<tool_call>\n<function=submit_script>\n<parameter=script>\n{hex}\n</parameter>\n</function>\n</tool_call>"
        );
        let (base, _) = spawn_mock(vec![completion_text(&content)]);
        let out = tmpdir("text-tool");
        let stats = run(&fixtures, &entry(base), &out, 1, DisplayFormat::Hex)
            .await
            .expect("run");
        assert_eq!(stats.answered, 1);
        assert_eq!(stats.failed, 0);
        let text = std::fs::read_to_string(out.join("responses.jsonl")).expect("responses");
        let record: ResponseRecord = serde_json::from_str(text.trim_end()).expect("record");
        match record.answer {
            TaskAnswer::Script(a) => assert_eq!(a.script, hex),
            other => panic!("expected script answer, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&out);
    }

    #[tokio::test]
    async fn transient_errors_are_retried() {
        let fixtures = generate(&GenParams {
            seed: 5,
            write: 1,
            optimize: 0,
            identify: 0,
        });
        let hex = match &fixtures[0] {
            Fixture::Write(w) => w.reference_script_hex.clone(),
            other => panic!("expected write fixture, got {}", other.id()),
        };
        // Two 500s, then success. Backoff between attempts is 2s then 8s.
        let (base, captured) = spawn_mock_scenario(
            2,
            500,
            completion_with_tool("submit_script", json!({ "script": hex })),
        );
        let out = tmpdir("retry");
        let stats = run(&fixtures, &entry(base), &out, 1, DisplayFormat::Hex)
            .await
            .expect("run");
        assert_eq!(stats.answered, 1);
        assert_eq!(stats.failed, 0);
        assert!(
            captured.lock().expect("lock").len() >= 3,
            "failures then success observed"
        );
        let _ = std::fs::remove_dir_all(&out);
    }

    #[tokio::test]
    async fn persistent_errors_exhaust_retries() {
        let fixtures = generate(&GenParams {
            seed: 5,
            write: 1,
            optimize: 0,
            identify: 0,
        });
        let (base, captured) = spawn_mock_scenario(usize::MAX, 500, json!({}));
        let out = tmpdir("retry-exhaust");
        let stats = run(&fixtures, &entry(base), &out, 1, DisplayFormat::Hex)
            .await
            .expect("run");
        assert_eq!(stats.answered, 0);
        assert_eq!(stats.failed, 1);
        // 1 initial attempt + 3 retries from our layer (goose may add
        // internal retries on top).
        assert!(captured.lock().expect("lock").len() >= 4);
        let failures = std::fs::read_to_string(out.join("failures.jsonl")).expect("failures");
        assert!(
            failures.contains("\"attempts\":3"),
            "attempts recorded: {failures}"
        );
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn models_config_parses() {
        let dir = tmpdir("cfg");
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("models.toml");
        std::fs::write(
            &path,
            "[model.test]\nprovider = \"openai_compatible\"\nmodel = \"m1\"\nbase_url = \"http://localhost:9/v1\"\napi_key_env = \"KEY\"\n",
        )
        .expect("write");
        let models = load_models_config(&path).expect("parse");
        assert_eq!(models["test"].model, "m1");
        assert_eq!(models["test"].api_key_env.as_deref(), Some("KEY"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
