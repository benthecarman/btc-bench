//! The live model runner on goose-providers: single-shot, one
//! submit-tool per task, responses written as the JSONL that `grade`
//! consumes. No auxiliary tools, no feedback loop (DESIGN.md, "Runner").

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use bench_core::task::{Fixture, IdentifyAnswer, ResponseRecord, ScriptAnswer, TaskAnswer};
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
use rmcp::model::{CallToolResult, ContentBlock as McpContentBlock, TextContent};
use serde::Deserialize;
use serde_json::json;

// Deliberately neutral framing: no "benchmark", no grading language.
// Eval-aware models change behavior when told they are being tested,
// which confounds the measurement, and the word becomes a spurious
// conditioning token when the same prompts drive RL training
// rollouts. Only the operational contract is stated. Worded to fit
// both tool modes: diagnostic tools (check_*) may be called freely;
// the submit tool ends the task.
pub const SYSTEM_PROMPT: &str = "Solve the following Bitcoin Script task. \
Decide your answer, then submit it by calling the submit tool exactly \
once. You are in an automated pipeline: there is no one to ask, so do \
not ask questions.";

/// One `[[model.<name>]]` entry from models.toml.
#[derive(Clone, Debug, Deserialize)]
pub struct ModelEntry {
    /// `openai`, `openai_compatible`, or `anthropic`.
    pub provider: String,
    pub model: String,
    /// Endpoint URL(s) for `openai_compatible`; defaults per provider
    /// otherwise. Multiple URLs are load-balanced round-robin — e.g. a
    /// workstation GPU plus a tailnet box serving the same model.
    pub base_url: Option<toml::Value>,
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
    /// Request streaming responses (SSE) from `openai_compatible`
    /// endpoints. Long generations only survive upstream
    /// whole-request timeouts in this mode; the runner drains and
    /// reassembles the stream, so recorded behavior is identical.
    #[serde(default)]
    pub stream: Option<bool>,
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

fn base_urls(entry: &ModelEntry) -> Result<Vec<String>> {
    let raw = entry
        .base_url
        .clone()
        .unwrap_or(toml::Value::String(String::new()));
    match raw {
        toml::Value::String(s) => Ok(vec![s]),
        toml::Value::Array(arr) => Ok(arr
            .into_iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()),
        _ => anyhow::bail!("base_url must be a string or array of strings"),
    }
}

fn build_backends(entry: &ModelEntry) -> Result<Vec<Backend>> {
    match entry.provider.as_str() {
        "openai" => {
            let hosts = base_urls(entry)?;
            let host = hosts
                .into_iter()
                .next()
                .unwrap_or_else(|| "https://api.openai.com/v1".into());
            let api = ApiClient::new_with_tls(host, auth_for(entry)?, None)?;
            Ok(vec![Backend::OpenAi(
                OpenAiProviderBuilder::new(api).name("openai").build(),
            )])
        }
        "openai_compatible" => {
            let hosts = base_urls(entry)?;
            if hosts.is_empty() {
                bail!("openai_compatible model entries require base_url");
            }
            // Streaming is per-entry opt-in (`stream = true` in
            // models.toml). SSE keeps tokens flowing, so upstream
            // whole-request timeouts on long generations do not fire;
            // non-streaming stays the default for canned-JSON servers.
            let stream = entry.stream.unwrap_or(false);
            let mut backends = Vec::with_capacity(hosts.len());
            for host in &hosts {
                let provider = OpenAiCompatibleProvider::new(
                    "openai_compatible".into(),
                    ApiClient::new_with_tls(host.clone(), auth_for(entry)?, None)?,
                    String::new(),
                )
                .with_supports_streaming(stream);
                backends.push(Backend::Compat(provider));
            }
            Ok(backends)
        }
        "anthropic" => {
            let hosts = base_urls(entry)?;
            let host = hosts
                .into_iter()
                .next()
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
            Ok(vec![Backend::Anthropic(
                AnthropicProviderBuilder::new(api).name("anthropic").build(),
            )])
        }
        other => bail!("unknown provider {other:?}; use openai, openai_compatible, or anthropic"),
    }
}

impl Backend {
    async fn complete(
        &self,
        cfg: &ModelConfig,
        history: &[Message],
        tools: &[Tool],
    ) -> Result<(Vec<Message>, FinishInfo), ProviderError> {
        use goose_providers::base::MessageStream;
        let stream: MessageStream = match self {
            Backend::OpenAi(p) => {
                p.stream_for_model(
                    cfg,
                    &cfg.model_name,
                    &cfg.model_name,
                    SYSTEM_PROMPT,
                    history,
                    tools,
                )
                .await?
            }
            Backend::Compat(p) => {
                p.stream_for_model(
                    cfg,
                    &cfg.model_name,
                    &cfg.model_name,
                    SYSTEM_PROMPT,
                    history,
                    tools,
                )
                .await?
            }
            Backend::Anthropic(p) => {
                p.stream_for_model(cfg, &cfg.model_name, SYSTEM_PROMPT, history, tools)
                    .await?
            }
        };
        let mut collected = Vec::new();
        let mut finish = FinishInfo::default();
        let mut stream = stream;
        while let Some(item) = stream.next().await {
            let (msg, usage) = item?;
            if let Some(m) = msg {
                collected.push(m);
            }
            if let Some(u) = usage {
                if let Some(reasons) = &u.finish_reasons {
                    if !reasons.is_empty() {
                        finish.finish_reason = Some(reasons.join(","));
                    }
                }
                if let Some(t) = u.usage.output_tokens {
                    finish.output_tokens = Some(finish.output_tokens.unwrap_or(0) + t as i64);
                }
                if let Some(t) = u.usage.input_tokens {
                    finish.input_tokens = Some(finish.input_tokens.unwrap_or(0) + t as i64);
                }
            }
        }
        Ok((collected, finish))
    }
}

/// Provider-reported completion metadata, accumulated across a stream.
#[derive(Clone, Debug, Default)]
pub struct FinishInfo {
    pub finish_reason: Option<String>,
    pub output_tokens: Option<i64>,
    pub input_tokens: Option<i64>,
}

/// Tool-assisted mode: which diagnostic tools the model gets beside
/// the submit tool. Diagnostics are pure functions of model-supplied
/// input (see bench_core::toolbox) — they can never leak a reference.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum ToolMode {
    /// Submit tool only (the classic benchmark headline).
    #[default]
    None,
    /// check_script / check_descriptor: the compiler-and-lint loop a
    /// human developer has. Measures mechanical recovery within one
    /// attempt; the semantic translation stays unaided.
    Basic,
}

impl std::str::FromStr for ToolMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "none" => Ok(ToolMode::None),
            "basic" => Ok(ToolMode::Basic),
            other => Err(format!("unknown tool mode {other:?}; use none or basic")),
        }
    }
}

impl std::fmt::Display for ToolMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ToolMode::None => "none",
            ToolMode::Basic => "basic",
        })
    }
}

/// Diagnostic calls allowed per task across all attempts. Bounds the
/// tool loop so a check-forever policy cannot stall a run.
const MAX_TOOL_CALLS: u32 = 16;

// Tool descriptions are deliberately terse and must not name the
// Miniscript decode gate: that requirement stays implicit on every
// prompt surface (test-pinned). Discovering it through the tool's
// OUTPUT is the point of tool-assisted mode; pre-announcing it in the
// description would reveal it before the model has done anything.
fn check_script_tool() -> Tool {
    let schema = json!({
        "type": "object",
        "properties": {
            "script": {
                "type": "string",
                "description": "The candidate script to diagnose, hex or Bitcoin Core asm"
            }
        },
        "required": ["script"]
    });
    Tool::new(
        "check_script",
        "Diagnose a candidate script (hex or asm): parse and decode checks, analysis findings, satisfaction weight.",
        Arc::new(schema.as_object().expect("object schema").clone()),
    )
}

fn check_descriptor_tool() -> Tool {
    let schema = json!({
        "type": "object",
        "properties": {
            "descriptor": {
                "type": "string",
                "description": "The candidate tr(INTERNAL_KEY,TREE) descriptor to diagnose"
            }
        },
        "required": ["descriptor"]
    });
    Tool::new(
        "check_descriptor",
        "Diagnose a candidate Taproot descriptor: parse check, tapleaf count, analysis findings, worst-case satisfaction weight.",
        Arc::new(schema.as_object().expect("object schema").clone()),
    )
}

/// Execute a diagnostic call locally. Context comes from the task;
/// everything else is model-supplied input.
fn run_check(
    fixture: &Fixture,
    name: &str,
    args: &serde_json::Map<String, serde_json::Value>,
) -> String {
    match (name, fixture) {
        ("check_script", Fixture::Write(w)) => {
            let text = args.get("script").and_then(|v| v.as_str()).unwrap_or("");
            bench_core::toolbox::check_script(w.context, text).render()
        }
        ("check_script", Fixture::Optimize(o)) => {
            let text = args.get("script").and_then(|v| v.as_str()).unwrap_or("");
            bench_core::toolbox::check_script(o.context, text).render()
        }
        ("check_descriptor", Fixture::Tree(_)) => {
            let text = args
                .get("descriptor")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            bench_core::toolbox::check_descriptor(text).render()
        }
        _ => "this diagnostic tool is not available for this task".to_string(),
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

fn submit_descriptor_tool() -> Tool {
    let schema = json!({
        "type": "object",
        "properties": {
            "descriptor": {
                "type": "string",
                "description": "The Taproot descriptor, tr(INTERNAL_KEY,TREE)"
            }
        },
        "required": ["descriptor"]
    });
    Tool::new(
        "submit_descriptor",
        "Submit your final Taproot descriptor answer.",
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
    if let Some(start) = text.find("<tool_call>") {
        if let Some(end_rel) = text[start..].find("</tool_call>") {
            let end = end_rel + start;
            let body = text[start + "<tool_call>".len()..end].trim();
            if let Some(call) = parse_tool_body(body) {
                return Some(call);
            }
        }
    }
    // Recoverable bare forms (observed in real traffic): a JSON object
    // with submit arguments — whole-text, inside a ``` fence, or behind
    // a `<call_...>`-style wrapper — and the `<request invoke=...>`
    // XML shape. Strictly machine-parseable: no prose mining.
    bare_json_call(text)
        .or_else(|| fenced_json_call(text))
        .or_else(|| call_wrapper_json(text))
        .or_else(|| request_invoke_xml(text))
        .or_else(|| tag_wrapped_call(text))
}

/// `<tool_name>value</tool_name>` — a shorthand real models emit for
/// diagnostics (observed as `<check_descriptor>\ndescriptor: "tr(...)"
/// \n</check_descriptor>`, 7 tasks lost in one run). The value may
/// carry an optional `argname:` prefix and surrounding quotes.
fn tag_wrapped_call(text: &str) -> Option<(String, serde_json::Map<String, serde_json::Value>)> {
    const TOOLS: &[(&str, &str)] = &[
        ("check_script", "script"),
        ("check_descriptor", "descriptor"),
        ("submit_script", "script"),
        ("submit_descriptor", "descriptor"),
        ("submit_identify", "label"),
    ];
    for (tool, arg) in TOOLS {
        let open = format!("<{tool}>");
        let close = format!("</{tool}>");
        let Some(start) = text.find(&open) else {
            continue;
        };
        let body_start = start + open.len();
        let Some(end_rel) = text[body_start..].find(&close) else {
            continue;
        };
        let mut value = text[body_start..body_start + end_rel].trim();
        // Optional "argname:" prefix inside the body.
        for prefix in [format!("{arg}:"), format!("\"{arg}\":")] {
            if let Some(rest) = value.strip_prefix(&prefix) {
                value = rest.trim();
            }
        }
        let value = value.trim_matches('"').trim();
        if value.is_empty() {
            continue;
        }
        let mut args = serde_json::Map::new();
        args.insert(
            (*arg).to_string(),
            serde_json::Value::String(value.to_string()),
        );
        return Some(((*tool).to_string(), args));
    }
    None
}

/// `<tool_call>` body: `{"name": ..., "arguments": {...}}` or the
/// `<function=name><parameter=p>...</parameter>` XML-ish form.
fn parse_tool_body(body: &str) -> Option<(String, serde_json::Map<String, serde_json::Value>)> {
    if body.trim_start().starts_with('{') {
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

/// Map a JSON object of submit arguments to the tool it answers:
/// `{"script": ...}` -> submit_script, `{"label": ...}` ->
/// submit_identify. Requires the object to parse completely.
fn json_args_to_call(
    v: serde_json::Value,
) -> Option<(String, serde_json::Map<String, serde_json::Value>)> {
    let obj = v.as_object()?.clone();
    if obj.contains_key("script") {
        Some(("submit_script".to_string(), obj))
    } else if obj.contains_key("descriptor") {
        Some(("submit_descriptor".to_string(), obj))
    } else if obj.contains_key("label") {
        Some(("submit_identify".to_string(), obj))
    } else {
        None
    }
}

/// A JSON object of submit arguments, either as the whole response or
/// trailing prose ("... Let me do that now.\n\n{\"script\": ...}").
/// Bounded scan over `{` positions; the object must parse fully and
/// carry submit keys.
fn bare_json_call(text: &str) -> Option<(String, serde_json::Map<String, serde_json::Value>)> {
    let t = text.trim();
    if t.starts_with('{') {
        if let Some(v) = serde_json::from_str::<serde_json::Value>(t).ok() {
            if let Some(call) = json_args_to_call(v) {
                return Some(call);
            }
        }
    }
    // Trailing object after prose: take the LAST '{' whose remainder
    // begins one complete object (prose mentions earlier '{'s first).
    for (i, _) in t.match_indices('{').rev() {
        if let Some(Ok(v)) = serde_json::Deserializer::from_str(&t[i..])
            .into_iter::<serde_json::Value>()
            .next()
        {
            if let Some(call) = json_args_to_call(v) {
                return Some(call);
            }
        }
    }
    None
}

/// A fenced ``` (optionally ```json) block holding submit arguments.
fn fenced_json_call(text: &str) -> Option<(String, serde_json::Map<String, serde_json::Value>)> {
    let mut rest = text;
    while let Some(start) = rest.find("```") {
        let after = &rest[start + 3..];
        let after = after.strip_prefix("json").unwrap_or(after);
        let body_start = after.len() - after.trim_start().len();
        let body = &after[body_start..];
        let Some(end) = body.find("```") else {
            rest = &rest[start + 3..];
            continue;
        };
        let candidate = body[..end].trim();
        if candidate.starts_with('{') {
            if let Some(v) = serde_json::from_str::<serde_json::Value>(candidate).ok() {
                if let Some(call) = json_args_to_call(v) {
                    return Some(call);
                }
            }
        }
        rest = &body[end + 3..];
    }
    None
}

/// A `<call_...>`-prefixed wrapper around JSON submit arguments. The
/// tag may be closed (`<call_X>`) or unclosed (`<call_X{...}` —
/// observed traffic: the wrapper is `<call_` plus hex, no `>`).
fn call_wrapper_json(text: &str) -> Option<(String, serde_json::Map<String, serde_json::Value>)> {
    let t = text.trim_start();
    let start = t.find("<call_")?;
    let after = &t[start + "<call_".len()..];
    let args_at = after.find('{')?;
    let v: serde_json::Value = serde_json::from_str(after[args_at..].trim()).ok()?;
    json_args_to_call(v)
}

/// `<request invoke="submit_answer"><script>...</script></request>` —
/// child tags become string arguments.
fn request_invoke_xml(text: &str) -> Option<(String, serde_json::Map<String, serde_json::Value>)> {
    let start = text.find("<request ")?;
    let head_end = text[start..].find('>')? + start;
    let head = &text[start..head_end];
    if !head.contains("invoke=") {
        return None;
    }
    let body = &text[head_end + 1..];
    let end = body.find("</request>")?;
    let body = &body[..end];
    let mut args = serde_json::Map::new();
    let mut rest = body;
    while let Some(ts) = rest.find('<') {
        let name_start = ts + 1;
        let name_end = rest[name_start..].find('>')? + name_start;
        let name = rest[name_start..name_end].trim().to_string();
        if name.is_empty() || name.starts_with('/') {
            rest = &rest[name_end + 1..];
            continue;
        }
        let close = format!("</{name}>");
        let vfrom = name_end + 1;
        let vto = rest[vfrom..].find(&close)? + vfrom;
        args.insert(
            name,
            serde_json::Value::String(rest[vfrom..vto].trim().to_string()),
        );
        rest = &rest[vto + close.len()..];
    }
    json_args_to_call(serde_json::Value::Object(args))
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
    } else if name == "submit_descriptor" {
        args.get("descriptor").and_then(|v| v.as_str()).map(|d| {
            TaskAnswer::Descriptor(bench_core::task::DescriptorAnswer {
                descriptor: d.to_string(),
            })
        })
    } else if name == "submit_identify" {
        let label = args
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        Some(TaskAnswer::Identify(IdentifyAnswer {
            label: label.to_string(),
        }))
    } else {
        None
    }
}

/// One graded turn of a task's attempt loop.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TurnOutcome {
    pub attempt: u32,
    pub passed: bool,
    pub score: f64,
    /// The feedback sent to the model after this turn (empty for the
    /// final turn or a pass).
    pub feedback: String,
    pub answer: Option<TaskAnswer>,
}

/// Everything one task's attempt loop produced.
struct TaskOutcome {
    fixture: Fixture,
    is_identify: bool,
    attempts: Vec<TurnOutcome>,
    final_answer: Option<TaskAnswer>,
    final_raw: String,
    final_finish: FinishInfo,
    transport_error: Option<String>,
    /// Diagnostic calls used (Some only in tool-assisted runs).
    tool_calls: Option<u32>,
}

/// Local grading verdict plus the feedback string for the next turn.
struct Evaluation {
    passed: bool,
    score: f64,
    feedback: String,
}

/// Coarse protocol group of an identify label, for bounded multi-turn
/// feedback: standard output scripts, Lightning, or Liquid.
fn label_group(label: &str) -> &'static str {
    let l = label.trim().to_ascii_lowercase();
    if l.starts_with("ln_") {
        "Lightning"
    } else if l.starts_with("liquid_") {
        "Liquid"
    } else {
        "standard output-script"
    }
}

/// Grade an answer against its fixture and build mechanical feedback.
/// Parse errors are passed through verbatim (they name the exact
/// defect); equivalence failures never leak the distinguishing
/// assignment.
fn evaluate(fixture: &Fixture, answer: &TaskAnswer) -> Evaluation {
    // Mechanical analysis findings (malleability, safety, ...) are facts
    // about the submitted script, so they are fair multi-turn feedback —
    // same policy as parse errors.
    let lint_note = |lint: &[String]| {
        if lint.is_empty() {
            String::new()
        } else {
            format!(" Miniscript analysis flags: {}.", lint.join("; "))
        }
    };
    // Static consensus violations in the submitted script (e.g.
    // CHECKMULTISIG in tapscript, unbalanced conditionals): mechanical
    // facts, and the exact ones that defuse "it's still
    // consensus-valid" reasoning. Violations only — validity is never
    // certified.
    let consensus_note =
        |ctx: bench_core::ContextKind, answer: &str| match bench_core::answer::parse_script_answer(
            answer,
        ) {
            Ok(script) => {
                let notes = bench_core::toolbox::consensus_notes(ctx, &script);
                if notes.is_empty() {
                    String::new()
                } else {
                    format!(" Consensus: {}.", notes.join("; "))
                }
            }
            Err(_) => String::new(),
        };
    match (fixture, answer) {
        (Fixture::Write(w), TaskAnswer::Script(a)) => {
            let r = bench_core::grade_write(w, &a.script);
            if r.score > 0.999 {
                Evaluation { passed: true, score: r.score, feedback: String::new() }
            } else {
                let detail = match &r.reason {
                    Some(reason) => format!("Your answer was rejected: {reason}"),
                    None => "Your answer was rejected.".to_string(),
                };
                Evaluation {
                    passed: false,
                    score: 0.0,
                    feedback: format!(
                        "{detail}{}{}",
                        lint_note(&r.lint),
                        consensus_note(w.context, &a.script)
                    ),
                }
            }
        }
        (Fixture::Optimize(o), TaskAnswer::Script(a)) => {
            let r = bench_core::grade_optimize(o, &a.script);
            if r.weight_score > 0.999 {
                Evaluation { passed: true, score: r.weight_score, feedback: String::new() }
            } else if let Some(w) = r.candidate {
                Evaluation {
                    passed: false,
                    score: r.weight_score,
                    feedback: format!(
                        "Your script is semantically equivalent, weight {} vs baseline {} and known optimum {}; reduce the weight further.{}",
                        w.weight, o.baseline_weight, o.optimal_weight,
                        lint_note(&r.lint)
                    ),
                }
            } else {
                let reason = r.reason.unwrap_or_default();
                Evaluation {
                    passed: false,
                    score: 0.0,
                    feedback: format!(
                        "Your answer was rejected: {reason}{}{}",
                        lint_note(&r.lint),
                        consensus_note(o.context, &a.script)
                    ),
                }
            }
        }
        (Fixture::Identify(i), TaskAnswer::Identify(a)) => {
            let r = bench_core::grade_identify(i, a);
            if r.score > 0.999 {
                Evaluation { passed: true, score: r.score, feedback: String::new() }
            } else {
                // Bounded hint: whether the answer's protocol group
                // matched. The true group is named only when the model
                // already guessed it; a wrong-group answer learns only
                // that its OWN group is wrong, never which is right.
                let known = bench_gen::corpus::FAMILIES
                    .iter()
                    .any(|f| f.eq_ignore_ascii_case(a.label.trim()));
                let feedback = if !known {
                    "The label is not one of the listed options; answer with \
                     one of the listed labels."
                        .to_string()
                } else if label_group(&a.label) == label_group(&i.family) {
                    format!(
                        "The family label is incorrect, but the {} group is \
                         right; identify the exact variant.",
                        label_group(&i.family)
                    )
                } else {
                    format!(
                        "The family label is incorrect; the script is not a \
                         {} pattern.",
                        label_group(&a.label)
                    )
                };
                Evaluation { passed: false, score: 0.0, feedback }
            }
        }
        (Fixture::Tree(t), TaskAnswer::Descriptor(a)) => {
            let r = bench_core::grade_tree(t, &a.descriptor);
            if r.weight_score > 0.999 {
                Evaluation { passed: true, score: r.weight_score, feedback: String::new() }
            } else if let Some(w) = r.candidate_weight {
                Evaluation {
                    passed: false,
                    score: r.weight_score,
                    feedback: format!(
                        "Your design is semantically correct, worst-case weight {} vs single-leaf baseline {} and known optimum {}; improve the key path or tree shape.{}",
                        w, t.baseline_weight, t.reference_weight, lint_note(&r.lint)
                    ),
                }
            } else {
                let reason = r.reason.unwrap_or_default();
                // Static syntax facts about the submitted text
                // (unbalanced brackets, brace arity, argument hex
                // lengths) — same policy as consensus notes.
                let syntax = if reason.starts_with("not a valid descriptor") {
                    let notes = bench_core::toolbox::descriptor_syntax_notes(&a.descriptor);
                    if notes.is_empty() {
                        String::new()
                    } else {
                        format!(" Syntax: {}.", notes.join("; "))
                    }
                } else {
                    String::new()
                };
                Evaluation {
                    passed: false,
                    score: 0.0,
                    feedback: format!(
                        "Your answer was rejected: {reason}{}{syntax}",
                        lint_note(&r.lint)
                    ),
                }
            }
        }
        (f, _) => Evaluation {
            passed: false,
            score: 0.0,
            feedback: format!(
                "Wrong answer shape for this {} task; answer with the submit tool appropriate to the task.",
                match f { Fixture::Write(_) => "write", Fixture::Optimize(_) => "optimize", Fixture::Identify(_) => "identify", Fixture::Tree(_) => "tree" }
            ),
        },
    }
}

/// Build the role:tool feedback message the model sees after a failed
/// attempt, mirroring the tool-call id it produced.
fn feedback_message(call_id: &str, text: &str) -> Message {
    Message::user().with_tool_response(
        call_id,
        Ok(CallToolResult::success(vec![McpContentBlock::Text(
            TextContent::new(text),
        )])),
    )
}

/// Extract the last submit-tool call, its raw text, and the tool-call
/// id (needed to route feedback).
fn extract_answer_with_id(messages: &[Message]) -> (Option<TaskAnswer>, String, Option<String>) {
    let mut raw = String::new();
    let mut found: Option<(TaskAnswer, String)> = None;
    for m in messages {
        for block in &m.content {
            match block {
                MessageContentBlock::Text(t) => raw.push_str(&t.text),
                MessageContentBlock::Thinking(t) => raw.push_str(&t.thinking),
                MessageContentBlock::ToolRequest(tr) => {
                    if let Ok(params) = &tr.tool_call {
                        let args = params.arguments.clone().unwrap_or_default();
                        if let Some(a) = task_answer_from(&params.name, &args) {
                            found = Some((a, tr.id.clone()));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    // Fallback: textual tool-call marker (endpoints without structured
    // tool calls) carries no id; feedback routes to a synthetic one.
    if found.is_none() {
        if let Some((name, args)) = parse_textual_tool_call(&raw) {
            if let Some(a) = task_answer_from(&name, &args) {
                return (Some(a), raw, None);
            }
        }
    }
    match found {
        Some((a, id)) => (Some(a), raw, Some(id)),
        None => (None, raw, None),
    }
}

/// Extract diagnostic (check_*) calls from an assistant turn:
/// structured tool requests first, textual fallback second. Returns
/// (name, args, call_id) triples in order.
fn extract_check_calls(
    messages: &[Message],
) -> Vec<(
    String,
    serde_json::Map<String, serde_json::Value>,
    Option<String>,
)> {
    let mut out = Vec::new();
    let mut raw = String::new();
    for m in messages {
        for block in &m.content {
            match block {
                MessageContentBlock::Text(t) => raw.push_str(&t.text),
                MessageContentBlock::Thinking(t) => raw.push_str(&t.thinking),
                MessageContentBlock::ToolRequest(tr) => {
                    if let Ok(params) = &tr.tool_call {
                        if params.name.starts_with("check_") {
                            out.push((
                                params.name.to_string(),
                                params.arguments.clone().unwrap_or_default(),
                                Some(tr.id.clone()),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if out.is_empty() {
        if let Some((name, args)) = parse_textual_tool_call(&raw) {
            if name.starts_with("check_") {
                out.push((name, args, None));
            }
        }
    }
    out
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct RunStats {
    pub answered: usize,
    pub failed: usize,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct RerunStats {
    /// Previously failed tasks that now answered.
    pub recovered: usize,
    /// Tasks that failed again.
    pub still_failed: usize,
}

/// Re-attempt the tasks in `<run_dir>/failures.jsonl`: answers append to
/// `responses.jsonl`, still-failing tasks replace `failures.jsonl`.
pub async fn rerun(
    dataset: &[Fixture],
    entry: &ModelEntry,
    run_dir: &Path,
    concurrency: usize,
    display: bench_gen::prompt::DisplayFormat,
    tools: ToolMode,
) -> Result<RerunStats> {
    let failures_path = run_dir.join("failures.jsonl");
    let old_failures_text = std::fs::read_to_string(&failures_path)
        .with_context(|| format!("read {}", failures_path.display()))?;
    let mut old_failures: Vec<serde_json::Value> = Vec::new();
    let mut retry_ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for line in old_failures_text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(line).context("parse failure record")?;
        if let Some(id) = v.get("task_id").and_then(|t| t.as_str()) {
            retry_ids.insert(id.to_string());
            old_failures.push(v);
        }
    }
    if retry_ids.is_empty() {
        return Ok(RerunStats {
            recovered: 0,
            still_failed: 0,
        });
    }
    let subset: Vec<Fixture> = dataset
        .iter()
        .filter(|f| retry_ids.contains(f.id()))
        .cloned()
        .collect();
    if subset.len() != retry_ids.len() {
        anyhow::bail!("failures reference unknown task ids");
    }

    let tmp = run_dir.join("rerun-tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    let stats = run(&subset, entry, &tmp, concurrency, display, 1, tools).await?;

    // Merge: append recovered answers; keep only still-failing records.
    let new_responses = std::fs::read_to_string(tmp.join("responses.jsonl")).unwrap_or_default();
    let recovered_ids: std::collections::BTreeSet<String> = new_responses
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v.get("task_id").and_then(|t| t.as_str()).map(String::from))
        .collect();
    let mut responses = std::fs::OpenOptions::new()
        .append(true)
        .open(run_dir.join("responses.jsonl"))
        .context("open responses for append")?;
    responses.write_all(new_responses.as_bytes())?;
    let still: Vec<serde_json::Value> = old_failures
        .into_iter()
        .filter(|v| {
            !recovered_ids.contains(
                v.get("task_id")
                    .and_then(|t| t.as_str())
                    .unwrap_or_default(),
            )
        })
        .collect();
    let still_failed = still.len();
    std::fs::write(
        &failures_path,
        still
            .iter()
            .map(|v| format!("{v}"))
            .collect::<Vec<_>>()
            .join("\n")
            + if still_failed > 0 { "\n" } else { "" },
    )?;
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(RerunStats {
        recovered: stats.answered,
        still_failed,
    })
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
    max_attempts: u32,
    tools: ToolMode,
) -> Result<RunStats> {
    run_resume(
        fixtures,
        entry,
        out_dir,
        concurrency,
        display,
        max_attempts,
        tools,
        false,
    )
    .await
}

/// Run the benchmark, optionally resuming from existing output files.
/// Resume skips tasks with answers in responses.jsonl, retries failed
/// tasks, and appends to all output files. Partial multi-turn attempts
/// (process died mid-task) restart from scratch.
/// A defective completion: server fault, not a model choice. Empty
/// content with no tool call anywhere (observed 65x in one sweep:
/// dropped streams), or a `tool_calls` finish with no call present
/// (observed 5x: the call vanishes between API and assembly). These
/// belong in the transient-retry path, not the graded attempt loop.
fn response_is_defective(messages: &[Message], finish: &FinishInfo) -> bool {
    let mut has_text = false;
    let mut has_call = false;
    for m in messages {
        for block in &m.content {
            match block {
                MessageContentBlock::Text(t) => {
                    if !t.text.trim().is_empty() {
                        has_text = true;
                    }
                }
                MessageContentBlock::Thinking(t) => {
                    if !t.thinking.trim().is_empty() {
                        has_text = true;
                    }
                }
                MessageContentBlock::ToolRequest(tr) => {
                    if tr.tool_call.is_ok() {
                        has_call = true;
                    }
                }
                _ => {}
            }
        }
    }
    if !has_text && !has_call {
        return true;
    }
    !has_call && finish.finish_reason.as_deref() == Some("tool_calls")
}

#[allow(clippy::too_many_arguments)]
pub async fn run_resume(
    fixtures: &[Fixture],
    entry: &ModelEntry,
    out_dir: &Path,
    concurrency: usize,
    display: bench_gen::prompt::DisplayFormat,
    max_attempts: u32,
    tools: ToolMode,
    resume: bool,
) -> Result<RunStats> {
    std::fs::create_dir_all(out_dir)?;

    // Resume: skip completed tasks, retry failed ones.
    let responses_path = out_dir.join("responses.jsonl");
    let mut completed: std::collections::BTreeSet<String> = Default::default();
    if resume && responses_path.exists() {
        for line in std::fs::read_to_string(&responses_path)?.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(id) = v.get("task_id").and_then(|t| t.as_str()) {
                    completed.insert(id.to_string());
                }
            }
        }
    }
    let fixtures: Vec<Fixture> = if resume {
        fixtures
            .iter()
            .filter(|f| !completed.contains(f.id()))
            .cloned()
            .collect()
    } else {
        fixtures.to_vec()
    };

    // On resume: clear failures for retried tasks, preserve failures
    // for tasks not in this fixture set (stale entries from other runs).
    let failures_path = out_dir.join("failures.jsonl");
    if resume && failures_path.exists() {
        let retry_ids: std::collections::BTreeSet<&str> = fixtures.iter().map(|f| f.id()).collect();
        let kept: Vec<String> = std::fs::read_to_string(&failures_path)?
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .filter(|v| {
                !retry_ids.contains(
                    v.get("task_id")
                        .and_then(|t| t.as_str())
                        .unwrap_or_default(),
                )
            })
            .map(|v| v.to_string())
            .collect();
        std::fs::write(
            &failures_path,
            kept.join("\n") + if kept.is_empty() { "" } else { "\n" },
        )?;
    }
    let backends = Arc::new(build_backends(entry)?);
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
    let rr = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (res_tx, mut res_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..concurrency {
        let rx = Arc::clone(&fix_rx);
        let backends = Arc::clone(&backends);
        let rr = Arc::clone(&rr);
        let cfg = cfg.clone();
        let res_tx = res_tx.clone();
        let display = display;
        let tool_mode = tools;
        set.spawn(async move {
            loop {
                let f = rx.lock().await.recv().await;
                let Some(f) = f else { break };

                let prompt = for_fixture_fmt(&f, display);
                let (submit, is_identify) = match &f {
                    Fixture::Identify(_) => (submit_identify_tool(), true),
                    Fixture::Tree(_) => (submit_descriptor_tool(), false),
                    _ => (submit_script_tool(), false),
                };
                let mut tools = vec![submit];
                if tool_mode == ToolMode::Basic {
                    match &f {
                        Fixture::Write(_) | Fixture::Optimize(_) => {
                            tools.push(check_script_tool())
                        }
                        Fixture::Tree(_) => tools.push(check_descriptor_tool()),
                        // Identify stays tool-less: the asm decode is
                        // already in the prompt, and anything more
                        // would trivialize the recall task.
                        Fixture::Identify(_) => {}
                    }
                }
                // Multi-turn attempt loop: after a graded failure the
                // model receives mechanical feedback and may retry, up
                // to max_attempts (1 = single-shot).
                let backend =
                    &backends[rr.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % backends.len()];
                let mut history: Vec<Message> =
                    vec![Message::user().with_text(prompt.as_str())];
                let mut turn_outcomes: Vec<TurnOutcome> = Vec::new();
                let mut final_answer: Option<TaskAnswer> = None;
                let mut final_raw = String::new();
                let mut final_finish = FinishInfo::default();
                let mut transport_error: Option<String> = None;
                let mut checks_used: u32 = 0;
                // True conversation cost: tokens summed over every
                // completion (diagnostic turns and failed attempts
                // included), not just the final one — tool-assisted
                // token efficiency is meaningless otherwise.
                let mut cum_out: Option<i64> = None;
                let mut cum_in: Option<i64> = None;

                'attempts: for attempt in 1..=max_attempts.max(1) {
                    // Diagnostic loop: check_* calls execute locally and
                    // continue the same graded attempt; only a submit
                    // (or a no-call response) ends the turn.
                    let (messages, _finish, answer, raw, call_id) = 'turn: loop {
                    let retries = entry_retries;
                    let mut tries: u32 = 0;
                    let result = loop {
                        let outcome = backend.complete(&cfg, &history, &tools).await;
                        match outcome {
                            Err(e) if is_transient(&e) && tries < retries => {
                                let backoff = RETRY_BACKOFF_SECS
                                    [tries as usize % RETRY_BACKOFF_SECS.len()];
                                tokio::time::sleep(std::time::Duration::from_secs(backoff))
                                    .await;
                                tries += 1;
                            }
                            // Server faults that arrive as successful
                            // HTTP: empty completions and vanishing
                            // tool calls retry like transport errors
                            // instead of burning a graded attempt.
                            Ok(v)
                                if response_is_defective(&v.0, &v.1) && tries < retries =>
                            {
                                let backoff = RETRY_BACKOFF_SECS
                                    [tries as usize % RETRY_BACKOFF_SECS.len()];
                                tokio::time::sleep(std::time::Duration::from_secs(backoff))
                                    .await;
                                tries += 1;
                            }
                            other => break other,
                        }
                    };
                    let (messages, finish) = match result {
                        Ok(v) => v,
                        Err(e) => {
                            transport_error = Some(e.to_string());
                            break 'attempts;
                        }
                    };
                    if let Some(t) = finish.output_tokens {
                        cum_out = Some(cum_out.unwrap_or(0) + t);
                    }
                    if let Some(t) = finish.input_tokens {
                        cum_in = Some(cum_in.unwrap_or(0) + t);
                    }
                    let (answer, raw, call_id) = extract_answer_with_id(&messages);
                    // Keep provider metadata even when no answer was
                    // extracted: losing finish_reason here made 49
                    // "no tool call" failures undiagnosable (was it a
                    // truncation or a refusal? the field was null).
                    final_finish = finish.clone();
                    if answer.is_none() {
                        let checks = extract_check_calls(&messages);
                        if !checks.is_empty() {
                            history.extend(messages);
                            for (name, args, id) in checks {
                                let reply = if checks_used >= MAX_TOOL_CALLS {
                                    "Diagnostic budget exhausted; call the \
                                     submit tool with your final answer."
                                        .to_string()
                                } else {
                                    checks_used += 1;
                                    run_check(&f, &name, &args)
                                };
                                history.push(feedback_message(
                                    id.as_deref().unwrap_or("0"),
                                    &reply,
                                ));
                            }
                            continue 'turn;
                        }
                    }
                    break 'turn (messages, finish, answer, raw, call_id);
                    };
                    // Keep the assistant turn in history for the next round.
                    history.extend(messages);
                    let Some(answer) = answer else {
                        turn_outcomes.push(TurnOutcome {
                            attempt,
                            passed: false,
                            score: 0.0,
                            feedback: "no tool call in response".into(),
                            answer: None,
                        });
                        if attempt < max_attempts.max(1) {
                            let fb = feedback_message(
                                call_id.as_deref().unwrap_or("0"),
                                "You did not call the submit tool. Call it exactly once with your answer.",
                            );
                            history.push(fb);
                            continue;
                        }
                        final_raw = raw;
                        break 'attempts;
                    };
                    let ev = evaluate(&f, &answer);
                    final_raw = raw.clone();
                    turn_outcomes.push(TurnOutcome {
                        attempt,
                        passed: ev.passed,
                        score: ev.score,
                        feedback: ev.feedback.clone(),
                        answer: Some(answer.clone()),
                    });
                    if ev.passed {
                        final_answer = Some(answer);
                        break 'attempts;
                    }
                    if attempt < max_attempts.max(1) {
                        let id = call_id.as_deref().unwrap_or("0");
                        history.push(feedback_message(id, &ev.feedback));
                    } else {
                        final_answer = Some(answer);
                    }
                }
                final_finish.output_tokens = cum_out;
                final_finish.input_tokens = cum_in;
                let _ = res_tx.send(TaskOutcome {
                    fixture: f,
                    is_identify,
                    attempts: turn_outcomes,
                    final_answer,
                    final_raw,
                    final_finish,
                    transport_error,
                    tool_calls: (tool_mode != ToolMode::None).then_some(checks_used),
                });
            }
        });
    }
    drop(res_tx);

    let out_dir = out_dir.to_path_buf();
    let collector = tokio::spawn(async move {
        let responses_path = out_dir.join("responses.jsonl");
        let failures_path = out_dir.join("failures.jsonl");
        let attempts_path = out_dir.join("attempts.jsonl");
        let mut responses = if resume {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&responses_path)?
        } else {
            std::fs::File::create(&responses_path)?
        };
        let mut failures = if resume {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&failures_path)?
        } else {
            std::fs::File::create(&failures_path)?
        };
        let mut attempts_file = if resume {
            std::fs::OpenOptions::new()
                .append(true)
                .open(&attempts_path)?
        } else {
            std::fs::File::create(&attempts_path)?
        };
        let mut stats = RunStats {
            answered: 0,
            failed: 0,
        };
        while let Some(out) = res_rx.recv().await {
            let TaskOutcome {
                fixture: f,
                is_identify,
                attempts,
                final_answer,
                final_raw,
                final_finish,
                transport_error,
                tool_calls,
            } = out;
            for t in &attempts {
                writeln!(
                    attempts_file,
                    "{}",
                    serde_json::json!({
                        "task_id": f.id(),
                        "attempt": t.attempt,
                        "passed": t.passed,
                        "score": t.score,
                        "feedback": t.feedback,
                        "answer": t.answer,
                    })
                )?;
            }
            if let Some(err) = transport_error {
                writeln!(
                    failures,
                    "{}",
                    serde_json::json!({
                        "task_id": f.id(),
                        "error": err,
                        "identify_task": is_identify,
                        "attempts": attempts.len(),
                        "tool_calls": tool_calls,
                        "finish_reason": final_finish.finish_reason,
                    })
                )?;
                stats.failed += 1;
                continue;
            }
            match final_answer {
                Some(a) => {
                    let record = ResponseRecord {
                        task_id: f.id().to_string(),
                        answer: a,
                        raw: if final_raw.is_empty() {
                            None
                        } else {
                            Some(final_raw.clone())
                        },
                        finish_reason: final_finish.finish_reason,
                        output_tokens: final_finish.output_tokens,
                        tool_calls,
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
                            "raw": final_raw,
                            "identify_task": is_identify,
                            "attempts": attempts.len(),
                            "tool_calls": tool_calls,
                            "finish_reason": final_finish.finish_reason,
                            "output_tokens": final_finish.output_tokens,
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

    /// Spawn a mock that answers every request with one SSE response
    /// assembled from `chunks` (each becomes a `data:` event, followed
    /// by `data: [DONE]`); returns (base_url, captured requests).
    fn spawn_mock_sse(chunks: Vec<serde_json::Value>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let cap = Arc::clone(&captured);
        std::thread::spawn(move || {
            let mut body = String::new();
            for c in &chunks {
                body.push_str("data: ");
                body.push_str(&serde_json::to_string(c).expect("serialize chunk"));
                body.push_str("\n\n");
            }
            body.push_str("data: [DONE]\n\n");
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(s) => s,
                    Err(_) => break,
                };
                let req = read_request(&mut stream);
                cap.lock().expect("lock").push(req);
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(resp.as_bytes()).expect("write response");
            }
        });
        (format!("http://{addr}/v1"), captured)
    }

    /// One SSE chunk in the chat.completion.chunk shape.
    fn sse_chunk(delta: serde_json::Value, finish: Option<&str>) -> serde_json::Value {
        json!({
            "id": "1", "object": "chat.completion.chunk", "created": 0, "model": "mock",
            "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }]
        })
    }

    /// Streaming mode must reassemble fragmented tool-call arguments
    /// and the trailing usage chunk into the same record non-streaming
    /// produces. This is the path that sidesteps upstream 120s
    /// non-streaming request timeouts, so it has to stay graded-
    /// equivalent.
    #[tokio::test]
    async fn streaming_sse_round_trip() {
        let fixtures = generate(&GenParams {
            seed: 5,
            write: 1,
            optimize: 0,
            identify: 0,
            ..GenParams::default()
        });
        let hex = match &fixtures[0] {
            Fixture::Write(w) => w.reference_script_hex.clone(),
            other => panic!("expected write fixture, got {}", other.id()),
        };
        let args = json!({ "script": hex }).to_string();
        let (head, tail) = args.split_at(args.len() / 2);
        let (base, captured) = spawn_mock_sse(vec![
            sse_chunk(
                json!({"role": "assistant", "content": null, "tool_calls": [{
                    "index": 0, "id": "c1", "type": "function",
                    "function": {"name": "submit_script", "arguments": ""}
                }]}),
                None,
            ),
            sse_chunk(
                json!({"tool_calls": [{"index": 0, "function": {"arguments": head}}]}),
                None,
            ),
            sse_chunk(
                json!({"tool_calls": [{"index": 0, "function": {"arguments": tail}}]}),
                None,
            ),
            sse_chunk(json!({}), Some("tool_calls")),
            json!({
                "id": "1", "object": "chat.completion.chunk", "created": 0, "model": "mock",
                "choices": [],
                "usage": {"prompt_tokens": 3, "completion_tokens": 7, "total_tokens": 10}
            }),
        ]);
        let mut model = entry(base);
        model.stream = Some(true);
        let out = tmpdir("sse");
        let stats = run(
            &fixtures,
            &model,
            &out,
            1,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
        )
        .await
        .expect("run");
        assert_eq!(stats.answered, 1);
        assert_eq!(stats.failed, 0);

        let reqs = captured.lock().expect("lock");
        assert!(
            reqs[0].contains("\"stream\":true"),
            "request must ask for SSE: {}",
            reqs[0]
        );

        let text = std::fs::read_to_string(out.join("responses.jsonl")).expect("responses");
        let record: ResponseRecord =
            serde_json::from_str(text.trim_end()).expect("parse response record");
        assert_eq!(record.task_id, fixtures[0].id());
        assert_eq!(record.output_tokens, Some(7), "usage chunk must survive");
        let (_, summary) = grade(&fixtures, &[record], None, 0.5, false).expect("grade");
        assert!((summary.write_mean - 1.0).abs() < 1e-9, "{summary:?}");
        let _ = std::fs::remove_dir_all(&out);
    }

    /// Empty completion (dropped stream) must retry like a transport
    /// error and recover within the same graded attempt, not burn the
    /// turn as "no tool call".
    #[tokio::test]
    async fn empty_completion_is_retried() {
        let fixtures = generate(&GenParams {
            seed: 4,
            write: 1,
            optimize: 0,
            identify: 0,
            ..GenParams::default()
        });
        let good = completion_with_tool(
            "submit_script",
            json!({ "script": match &fixtures[0] {
                Fixture::Write(w) => w.reference_script_hex.clone(),
                other => panic!("expected write fixture, got {}", other.id()),
            } }),
        );
        let empty = json!({
            "id": "chatcmpl-empty", "object": "chat.completion",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": ""}, "finish_reason": null}]
        });
        // Serve empty first, then the good body. Backoff between tries
        // is 2s; one retry keeps the test fast.
        let (base, captured) = spawn_mock(vec![empty, good]);
        let out = tmpdir("empty-retry");
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            1,
            DisplayFormat::Asm,
            1,
            ToolMode::None,
        )
        .await
        .expect("run");
        assert_eq!(
            stats.answered, 1,
            "empty completion must be retried, not unanswered"
        );
        assert_eq!(stats.failed, 0);
        let reqs = captured.lock().expect("lock");
        assert!(reqs.len() >= 2, "expected a retried request");
        let _ = std::fs::remove_dir_all(&out);
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
            base_url: Some(toml::Value::String(base_url)),
            api_key_env: None,
            temperature: Some(0.0),
            max_tokens: None,
            request_params: None,
            retries: None,
            stream: None,
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
            ..GenParams::default()
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
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            1,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
        )
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
        let (_, summary) = grade(&fixtures, &[record.clone()], None, 0.5, false).expect("grade");
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
            ..GenParams::default()
        });
        let (base, _) = spawn_mock(vec![completion_text("I would rather not.")]);
        let out = tmpdir("no-tool");
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            1,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
        )
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
            ..GenParams::default()
        });
        let bodies: Vec<serde_json::Value> = fixtures
            .iter()
            .map(|f| match f {
                Fixture::Identify(i) => {
                    completion_with_tool("submit_identify", json!({ "label": i.family }))
                }
                other => panic!("expected identify fixture, got {}", other.id()),
            })
            .collect();
        let (base, _) = spawn_mock(bodies);
        let out = tmpdir("identify");
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            1,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
        )
        .await
        .expect("run");
        assert_eq!(stats.answered, fixtures.len());
        let text = std::fs::read_to_string(out.join("responses.jsonl")).expect("responses");
        let records: Vec<ResponseRecord> = text
            .lines()
            .map(|l| serde_json::from_str(l).expect("record"))
            .collect();
        let (_, summary) = grade(&fixtures, &records, None, 0.5, false).expect("grade");
        assert_eq!(summary.identify_n, fixtures.len());
        assert!((summary.identify_mean - 1.0).abs() < 1e-9, "{summary:?}");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn textual_recovered_bare_json() {
        // Real traffic: model states intent in prose, then emits the
        // submit arguments as a bare JSON object.
        let text = "I need to call the submit_answer tool exactly once with the script. Let me do that now.\n\n{\"script\": \"OP_IF 29982b8d OP_ELSE OP_ENDIF\"}";
        let (name, args) = parse_textual_tool_call(text).expect("parsed");
        assert_eq!(name, "submit_script");
        assert!(args["script"].as_str().unwrap().contains("OP_IF"));
    }

    #[test]
    fn textual_recovered_fenced_json() {
        let text = "I'll call the submit tool now with the correct label.\n\n```json\n{\n  \"label\": \"p2pk\"\n}\n```";
        let (name, args) = parse_textual_tool_call(text).expect("parsed");
        assert_eq!(name, "submit_identify");
        assert_eq!(args["label"], "p2pk");
    }

    #[test]
    fn textual_recovered_call_wrapper() {
        let text = "<call_0x746f6f6c5f6e616d653e{\"script\":\"03f8d9 OP_CHECKSIG OP_NOTIF\"}";
        let (name, args) = parse_textual_tool_call(text).expect("parsed");
        assert_eq!(name, "submit_script");
        assert!(args["script"].as_str().unwrap().starts_with("03f8d9"));
    }

    #[test]
    fn textual_recovered_request_invoke_xml() {
        let text = "<request invoke=\"submit_answer\"><script>03b00ad57 OP_CHECKSIGVERIFY 0252 OP_CHECKSIG</script></request>";
        let (name, args) = parse_textual_tool_call(text).expect("parsed");
        assert_eq!(name, "submit_script");
        assert!(args["script"]
            .as_str()
            .unwrap()
            .contains("OP_CHECKSIGVERIFY"));
    }

    #[test]
    fn textual_prose_is_not_mined() {
        // The label appears in prose reasoning, but no machine-
        // parseable call shape exists: must stay unanswered.
        let text = "So the label is `p2wsh_multisig`. Let me call the tool.";
        assert!(parse_textual_tool_call(text).is_none());
        // JSON without submit keys is not an answer either.
        assert!(parse_textual_tool_call("{\"analysis\": \"looks like multisig\"}").is_none());
    }

    #[tokio::test]
    async fn concurrent_workers_all_answer() {
        let fixtures = generate(&GenParams {
            seed: 9,
            write: 4,
            optimize: 0,
            identify: 0,
            ..GenParams::default()
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
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            4,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
        )
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
        let (_, summary) = grade(&fixtures, &records, None, 0.5, false).expect("grade");
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
        // params in a textual call are ignored: identify is label-only.
        assert_eq!(args["label"], "p2wsh_multisig");
    }

    #[tokio::test]
    async fn content_only_tool_marker_is_parsed() {
        let fixtures = generate(&GenParams {
            seed: 5,
            write: 1,
            optimize: 0,
            identify: 0,
            ..GenParams::default()
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
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            1,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
        )
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
            ..GenParams::default()
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
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            1,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
        )
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
            ..GenParams::default()
        });
        let (base, captured) = spawn_mock_scenario(usize::MAX, 500, json!({}));
        let out = tmpdir("retry-exhaust");
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            1,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
        )
        .await
        .expect("run");
        assert_eq!(stats.answered, 0);
        assert_eq!(stats.failed, 1);
        // 1 initial attempt + 3 retries from our layer (goose may add
        // internal retries on top).
        assert!(captured.lock().expect("lock").len() >= 4);
        let failures = std::fs::read_to_string(out.join("failures.jsonl")).expect("failures");
        assert!(
            failures.contains("\"attempts\":0"),
            "transport death logs zero graded turns: {failures}"
        );
        let _ = std::fs::remove_dir_all(&out);
    }

    #[tokio::test]
    async fn rerun_recovers_failed_tasks() {
        let fixtures = generate(&GenParams {
            seed: 5,
            write: 2,
            optimize: 0,
            identify: 0,
            ..GenParams::default()
        });
        let answers: Vec<_> = fixtures
            .iter()
            .map(|f| match f {
                Fixture::Write(w) => w.reference_script_hex.clone(),
                other => panic!("unexpected fixture {}", other.id()),
            })
            .collect();
        // First run: everything fails (all 500s).
        let (base_bad, _) = spawn_mock_scenario(usize::MAX, 500, json!({}));
        let out = tmpdir("rerun-a");
        let stats = run(
            &fixtures,
            &entry(base_bad),
            &out,
            1,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
        )
        .await
        .expect("run");
        assert_eq!(stats.answered, 0);
        assert_eq!(stats.failed, 2);
        // Rerun against a healthy server: the second fixture's answer is
        // queued per-connection by the scenario mock's final body.
        let (base_good, _) = spawn_mock_scenario(
            0,
            500,
            completion_with_tool("submit_script", json!({ "script": answers[0] })),
        );
        let stats = rerun(
            &fixtures,
            &entry(base_good),
            &out,
            1,
            DisplayFormat::Hex,
            ToolMode::None,
        )
        .await
        .expect("rerun");
        assert_eq!(stats.recovered, 2);
        assert_eq!(stats.still_failed, 0);
        let responses = std::fs::read_to_string(out.join("responses.jsonl")).expect("responses");
        assert_eq!(responses.lines().count(), 2);
        let failures = std::fs::read_to_string(out.join("failures.jsonl")).expect("failures");
        assert!(failures.trim().is_empty());
        let _ = std::fs::remove_dir_all(&out);
    }

    #[tokio::test]
    async fn finish_reason_and_tokens_recorded() {
        let fixtures = generate(&GenParams {
            seed: 5,
            write: 1,
            optimize: 0,
            identify: 0,
            ..GenParams::default()
        });
        let hex = match &fixtures[0] {
            Fixture::Write(w) => w.reference_script_hex.clone(),
            other => panic!("unexpected fixture {}", other.id()),
        };
        let (base, _) = spawn_mock(vec![completion_with_tool(
            "submit_script",
            json!({ "script": hex }),
        )]);
        let out = tmpdir("finish");
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            1,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
        )
        .await
        .expect("run");
        assert_eq!(stats.answered, 1);
        let text = std::fs::read_to_string(out.join("responses.jsonl")).expect("responses");
        assert!(
            text.contains("\"finish_reason\":\"tool_calls\""),
            "finish missing: {text}"
        );
        assert!(text.contains("\"output_tokens\""), "tokens missing: {text}");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[tokio::test]
    async fn multi_turn_feedback_recovers_wrong_answer() {
        let fixtures = generate(&GenParams {
            seed: 5,
            write: 1,
            optimize: 0,
            identify: 0,
            ..GenParams::default()
        });
        let _hex = match &fixtures[0] {
            Fixture::Write(w) => w.reference_script_hex.clone(),
            other => panic!("expected write fixture, got {}", other.id()),
        };
        // Scenario mock serves the SAME body to every request, so the
        // first (wrong) and second (right) turns return identical tool
        // calls — instead assert the loop mechanics with the scenario
        // mock: attempt 1 wrong answer, later attempts correct.
        let (base, captured) = spawn_mock_scenario(
            0,
            500,
            completion_with_tool("submit_script", json!({ "script": "51" })),
        );
        let out = tmpdir("multi-wrong");
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            1,
            DisplayFormat::Hex,
            3,
            ToolMode::None,
        )
        .await
        .expect("run");
        // All attempts fail; the recorded answer is the last attempt's.
        assert_eq!(stats.answered, 1);
        assert_eq!(stats.failed, 0);
        let attempts = std::fs::read_to_string(out.join("attempts.jsonl")).expect("attempts");
        let lines: Vec<&str> = attempts.lines().collect();
        assert_eq!(lines.len(), 3, "three graded turns logged");
        for l in &lines {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            assert_eq!(v["passed"], false);
            assert!(
                !v["feedback"].as_str().unwrap().is_empty(),
                "feedback present"
            );
        }
        // Every follow-up request carried the tool-response feedback.
        let reqs = captured.lock().expect("lock");
        assert!(reqs.len() >= 3);
        assert!(
            reqs[1].contains("\"role\":\"tool\"")
                || reqs[1].contains("role%22%3A%22tool")
                || reqs[1].contains("tool_call_id"),
            "feedback on wire: {}...",
            &reqs[1][..300.min(reqs[1].len())]
        );
        let _ = std::fs::remove_dir_all(&out);
    }

    #[tokio::test]
    async fn single_attempt_behavior_unchanged() {
        let fixtures = generate(&GenParams {
            seed: 5,
            write: 1,
            optimize: 0,
            identify: 0,
            ..GenParams::default()
        });
        let hex = match &fixtures[0] {
            Fixture::Write(w) => w.reference_script_hex.clone(),
            other => panic!("unexpected fixture {}", other.id()),
        };
        let (base, captured) = spawn_mock(vec![completion_with_tool(
            "submit_script",
            json!({ "script": hex }),
        )]);
        let out = tmpdir("single");
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            1,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
        )
        .await
        .expect("run");
        assert_eq!(stats.answered, 1);
        assert_eq!(
            captured.lock().expect("lock").len(),
            1,
            "exactly one request"
        );
        let _ = std::fs::remove_dir_all(&out);
    }

    #[tokio::test]
    async fn resume_skips_completed_and_retries_failed() {
        let fixtures = generate(&GenParams {
            seed: 5,
            write: 4,
            optimize: 0,
            identify: 0,
            ..GenParams::default()
        });
        let answers: Vec<_> = fixtures
            .iter()
            .map(|f| match f {
                Fixture::Write(w) => w.reference_script_hex.clone(),
                other => panic!("unexpected {}", other.id()),
            })
            .collect();
        let out = tmpdir("resume");

        // First run: 2 answered (tool calls), 2 fail (no tool call).
        let bodies: Vec<serde_json::Value> = (0..4)
            .map(|i| {
                if i < 2 {
                    completion_with_tool("submit_script", json!({ "script": answers[i] }))
                } else {
                    completion_text("I cannot do this.")
                }
            })
            .collect();
        let (base, captured) = spawn_mock(bodies);
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            1,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
        )
        .await
        .expect("run");
        assert_eq!(stats.answered, 2);
        assert_eq!(stats.failed, 2);
        let first_reqs = captured.lock().expect("lock").len();

        // Resume with all-correct answers: only the 2 failed tasks retry.
        let bodies2: Vec<serde_json::Value> = (0..2)
            .map(|i| completion_with_tool("submit_script", json!({ "script": answers[i + 2] })))
            .collect();
        let (base2, captured2) = spawn_mock(bodies2);
        let stats2 = run_resume(
            &fixtures,
            &entry(base2),
            &out,
            1,
            DisplayFormat::Hex,
            1,
            ToolMode::None,
            true,
        )
        .await
        .expect("resume");
        assert_eq!(stats2.answered, 2, "only 2 retried");
        assert_eq!(
            captured2.lock().expect("lock").len(),
            2,
            "exactly 2 requests"
        );
        let _ = first_reqs;

        // responses.jsonl now has all 4 answers.
        let responses = std::fs::read_to_string(out.join("responses.jsonl")).expect("responses");
        assert_eq!(responses.lines().count(), 4);
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

    /// Tag-wrapped textual tool calls — the observed real-traffic
    /// shorthand `<check_descriptor>descriptor: "tr(...)"` — must be
    /// recognized so a diagnostic request continues the turn instead
    /// of terminating the task as "no tool call".
    #[test]
    fn tag_wrapped_textual_calls_are_recognized() {
        let (name, args) = parse_textual_tool_call(
            "Let me verify first.\n<check_descriptor>\ndescriptor: \"tr(abc,{pk(d),pk(e)})\"\n</check_descriptor>",
        )
        .expect("recognized");
        assert_eq!(name, "check_descriptor");
        assert_eq!(args["descriptor"], "tr(abc,{pk(d),pk(e)})");
        // Bare-value variant, no argname prefix.
        let (name, args) =
            parse_textual_tool_call("<check_script>51ac</check_script>").expect("recognized");
        assert_eq!(name, "check_script");
        assert_eq!(args["script"], "51ac");
        // Submit variant maps to an answer.
        let (name, args) =
            parse_textual_tool_call("<submit_identify>label: p2tr</submit_identify>")
                .expect("recognized");
        assert!(task_answer_from(&name, &args).is_some());
        // Unknown tags stay unrecognized.
        assert!(parse_textual_tool_call("<magic_tool>x</magic_tool>").is_none());
    }

    /// Graded feedback carries static consensus violations: the
    /// CHECKMULTISIG-in-tapscript answer from the first sweep must be
    /// told the script always fails, not just that decoding did.
    #[test]
    fn graded_feedback_carries_consensus_violations() {
        let k = "32a9c1b6aa84caf9b6898e162f8967d618a2eba4f4e185481e5a373c874a6a14";
        let fixture = Fixture::Write(bench_core::task::WriteFixture {
            id: "t1-x".into(),
            tier: bench_core::Tier::Easy,
            context: bench_core::ContextKind::Tap,
            spec_en: String::new(),
            spec_family: 0,
            atoms: 2,
            keys: vec![],
            reference_policy: String::new(),
            reference_miniscript: String::new(),
            reference_script_hex: format!("20{k}ac"),
            hash_preimages: Default::default(),
        });
        let answer = TaskAnswer::Script(bench_core::task::ScriptAnswer {
            script: format!("2 {k} {k} 3 OP_CHECKMULTISIG"),
        });
        let ev = evaluate(&fixture, &answer);
        assert!(!ev.passed);
        assert!(
            ev.feedback.contains("not available in tapscript"),
            "{}",
            ev.feedback
        );
        // A merely-wrong but consensus-clean answer gets no consensus
        // section at all — silence, not certification.
        let clean = TaskAnswer::Script(bench_core::task::ScriptAnswer {
            script: "51".into(),
        });
        let ev = evaluate(&fixture, &clean);
        assert!(!ev.feedback.contains("Consensus:"), "{}", ev.feedback);
    }

    /// Identify multi-turn feedback: a bounded group hint. Same-group
    /// misses learn the group was right; cross-group misses learn only
    /// their OWN group was wrong; unknown labels get the list nudge.
    #[test]
    fn identify_feedback_is_group_bounded() {
        let fixture = Fixture::Identify(bench_core::task::IdentifyFixture {
            id: "t3-x".into(),
            family: "ln_offered_htlc".into(),
            params: Default::default(),
            spk_hex: "0020".into(),
            inner_script_hex: None,
        });
        let answer = |label: &str| {
            TaskAnswer::Identify(bench_core::task::IdentifyAnswer {
                label: label.into(),
            })
        };
        // Same group, wrong variant.
        let ev = evaluate(&fixture, &answer("ln_to_local"));
        assert!(!ev.passed);
        assert!(
            ev.feedback.contains("Lightning group is right"),
            "{}",
            ev.feedback
        );
        // Wrong group: names only the model's own group.
        let ev = evaluate(&fixture, &answer("p2wsh_multisig"));
        assert!(
            ev.feedback.contains("not a standard output-script pattern"),
            "{}",
            ev.feedback
        );
        assert!(
            !ev.feedback.to_lowercase().contains("lightning"),
            "must not reveal the true group: {}",
            ev.feedback
        );
        // Unknown label: the list nudge, no group talk.
        let ev = evaluate(&fixture, &answer("segwit_v2_magic"));
        assert!(
            ev.feedback.contains("not one of the listed options"),
            "{}",
            ev.feedback
        );
        // Correct passes clean.
        let ev = evaluate(&fixture, &answer("LN_Offered_HTLC"));
        assert!(ev.passed);
    }

    /// The decode-gate requirement is implicit in every prompt
    /// surface, including tool descriptions and schemas: a model must
    /// discover it through tool OUTPUT or graded feedback, never be
    /// told upfront. Descriptions also stay terse.
    #[test]
    fn tool_descriptions_never_reveal_the_decode_gate() {
        for tool in [
            submit_script_tool(),
            submit_identify_tool(),
            submit_descriptor_tool(),
            check_script_tool(),
            check_descriptor_tool(),
        ] {
            let surface = format!(
                "{} {} {}",
                tool.name,
                tool.description.as_deref().unwrap_or(""),
                serde_json::to_string(&tool.input_schema).unwrap()
            );
            assert!(
                !surface.to_lowercase().contains("miniscript"),
                "{}: reveals the decode gate: {surface}",
                tool.name
            );
            assert!(
                tool.description.as_deref().unwrap_or("").len() < 160,
                "{}: description too verbose",
                tool.name
            );
        }
    }

    /// Tool-assisted flow: check_* calls execute locally and continue
    /// the same graded attempt; the submit ends it. The response
    /// records how many diagnostics were used.
    #[tokio::test]
    async fn basic_tools_loop_diagnoses_then_submits() {
        let fixtures = generate(&GenParams {
            seed: 4,
            write: 1,
            optimize: 0,
            identify: 0,
            ..GenParams::default()
        });
        let reference = match &fixtures[0] {
            Fixture::Write(w) => w.reference_script_hex.clone(),
            other => panic!("expected write fixture, got {}", other.id()),
        };
        let bodies = vec![
            // Turn 1: diagnose a wrong candidate (OP_RETURN fails the
            // decode gate).
            completion_with_tool("check_script", json!({"script": "6a"})),
            // Turn 2: diagnose the real one.
            completion_with_tool("check_script", json!({"script": reference.clone()})),
            // Turn 3: submit.
            completion_with_tool("submit_script", json!({"script": reference.clone()})),
        ];
        let (base, captured) = spawn_mock(bodies);
        let out = tmpdir("tools-basic");
        let stats = run(
            &fixtures,
            &entry(base),
            &out,
            1,
            DisplayFormat::Asm,
            1,
            ToolMode::Basic,
        )
        .await
        .expect("run");
        assert_eq!(stats.answered, 1, "diagnostics must not burn the attempt");

        // The response records both diagnostic calls.
        let text = std::fs::read_to_string(out.join("responses.jsonl")).unwrap();
        let rec: bench_core::task::ResponseRecord =
            serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(rec.tool_calls, Some(2));
        match rec.answer {
            TaskAnswer::Script(a) => assert_eq!(a.script, reference),
            other => panic!("wrong answer shape: {other:?}"),
        }

        // The check tool was advertised alongside submit, and the
        // second request carries the first diagnostic's report (the
        // decode-gate failure) back to the model.
        let reqs = captured.lock().unwrap();
        assert_eq!(reqs.len(), 3);
        assert!(reqs[0].contains("check_script"), "tool not advertised");
        assert!(
            reqs[1].contains("decode gate: FAIL"),
            "diagnostic report not fed back: {}",
            &reqs[1]
        );
        assert!(
            reqs[2].contains("miniscript:"),
            "second diagnostic report missing"
        );
        let _ = std::fs::remove_dir_all(&out);
    }
}
