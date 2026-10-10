//! Reward service: exposes the graders over HTTP for RL training loops.
//!
//! POST /reward        {"task": <fixture>, "answer": <answer>, "shaping"?: {...}}
//! POST /reward/batch  {"items": [{"task":..., "answer":...}], "shaping"?: {...}}
//! POST /tool          {"kind": "check_script"|"check_descriptor",
//!                      "context"?: "legacy"|"segwitv0"|"tap", "input": "..."}
//! GET  /health        {"ok": true}
//!
//! /tool serves the same reference-free diagnostics the tool-assisted
//! runner offers (bench_core::toolbox), so an RL trainer driving its
//! own rollout loop can execute the model's check_* calls without
//! reimplementing them. Stateless: pure function of the request.
//!
//! Every response carries two scores:
//! - `score`: the benchmark score, identical to `btc-bench grade`.
//! - `shaped`: the RL training reward. With no shaping configured it
//!   equals `score`; with shaping it adds small rungs for clearing
//!   the parse and decode gates, a band scaled by balanced
//!   truth-table agreement (constant scripts cap at the band's
//!   floor), an equivalence floor for optimize, and a lint penalty
//!   or gate. Shaping defaults come from the server flags; a request
//!   may override them per call, so one server can serve eval
//!   (unshaped) and training (shaped) at once.
//!
//! `components` exposes the raw signals (parsed / decoded /
//! equivalent / agreement / lint) so a trainer can log or recombine
//! them without another round trip.
//!
//! The server is a local trust boundary: JSON in, JSON out, no auth.
//! Requests are served by a thread pool; grading is CPU-bound and
//! takes milliseconds per answer.

use std::sync::Arc;

use anyhow::{bail, Context as _, Result};
use bench_core::answer::parse_script_answer_in;
use bench_core::task::{Fixture, ScriptAnswer, TaskAnswer};
use bench_core::{
    decodes_in_context, grade_identify, grade_optimize, grade_write, semantic_agreement,
    ContextKind,
};
use bitcoin::ScriptBuf;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Reward shaping parameters. All-zero (the default) makes the shaped
/// reward identical to the benchmark score.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Shaping {
    /// Rung for an answer that parses as hex/asm at all.
    pub parse: f64,
    /// Additional rung for clearing the miniscript decode gate.
    pub decode: f64,
    /// Band scaled by normalized balanced truth-table agreement.
    /// Constant scripts (OP_1) normalize to 0, so the band pays for
    /// semantic progress only.
    pub agreement: f64,
    /// Floor for an equivalent-but-unimproved optimize/tree answer
    /// (the weight curve alone scores it 0; equivalence is worth
    /// reward during training). For optimize tasks the floor is paid
    /// only for a DISTINCT rewrite no heavier than the baseline: the
    /// baseline sits in the prompt, and a real run showed 80% of
    /// answers echoing it verbatim — an unconditional floor would
    /// teach copy-the-prompt as the dominant strategy.
    pub equivalent_floor: f64,
    /// Subtracted per lint finding (malleable, unsafe, ...) from the
    /// shaped score.
    pub lint_penalty: f64,
    /// Zero the shaped score of equivalent-but-linted answers (the
    /// --standard-mode analog for training).
    pub lint_gate: bool,
}

impl Shaping {
    /// Guardrail: a non-equivalent answer must never approach full
    /// credit, or the shaping itself becomes the reward hack.
    pub fn validate(&self) -> Result<()> {
        for (name, v) in [
            ("parse", self.parse),
            ("decode", self.decode),
            ("agreement", self.agreement),
            ("equivalent-floor", self.equivalent_floor),
            ("lint-penalty", self.lint_penalty),
        ] {
            if !(0.0..=1.0).contains(&v) {
                bail!("shape-{name} must be in [0, 1], got {v}");
            }
        }
        let ceiling = self.parse + self.decode + self.agreement;
        if ceiling > 0.5 {
            bail!(
                "parse + decode + agreement = {ceiling} exceeds 0.5; \
                 a non-equivalent answer would earn too much"
            );
        }
        if self.equivalent_floor > 0.5 {
            bail!("shape-equivalent-floor exceeds 0.5");
        }
        Ok(())
    }
}

/// Raw grading signals, independent of shaping weights.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Components {
    pub parsed: bool,
    pub decoded: bool,
    pub equivalent: bool,
    /// Balanced truth-table agreement (1.0 iff equivalent, constants
    /// cap at 0.5). None for identify tasks and undecodable answers.
    pub agreement: Option<f64>,
    pub lint_count: usize,
}

#[derive(Deserialize)]
struct ToolRequest {
    kind: String,
    #[serde(default)]
    context: Option<ContextKind>,
    input: String,
    /// Prompt version of the task the call belongs to; decides the asm
    /// dialect (0 = v1 Legacy, 2+ = Bitcoin Core asm).
    #[serde(default)]
    prompt_version: u32,
}

fn run_tool(req: ToolRequest) -> Result<serde_json::Value> {
    match req.kind.as_str() {
        "check_script" => {
            let ctx = req
                .context
                .ok_or_else(|| anyhow::anyhow!("check_script requires a context"))?;
            let c = bench_core::toolbox::check_script_in(
                ctx,
                &req.input,
                bench_core::task::dialect_for(req.prompt_version),
            );
            Ok(json!({"report": c.render(), "detail": c}))
        }
        "check_descriptor" => {
            let c = bench_core::toolbox::check_descriptor(&req.input);
            Ok(json!({"report": c.render(), "detail": c}))
        }
        other => bail!("unknown tool kind {other:?}; use check_script or check_descriptor"),
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RewardTask {
    Wallet(WalletRewardTask),
    Legacy(Fixture),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WalletRewardTask {
    task: WalletTag,
    fixture: bench_wallet::WalletFixture,
}

#[derive(Deserialize)]
enum WalletTag {
    #[serde(rename = "wallet")]
    Wallet,
}

/// Exactly one of `answer` (an extracted answer) or `completion` (a
/// raw assistant turn) per request.
#[derive(Deserialize)]
struct RewardRequest {
    task: RewardTask,
    #[serde(default)]
    answer: Option<serde_json::Value>,
    #[serde(default)]
    completion: Option<Completion>,
    #[serde(default)]
    shaping: Option<Shaping>,
}

/// One assistant turn as the serving stack parsed it: reasoning,
/// visible text and structured tool calls. The answer is taken with
/// the runner's own rule, so a rollout reward and `btc-bench run`
/// read the same turn the same way.
#[derive(Clone, Deserialize)]
struct Completion {
    #[serde(default)]
    reasoning: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<CompletionToolCall>,
}

#[derive(Clone, Deserialize)]
struct CompletionToolCall {
    name: String,
    #[serde(default)]
    arguments: serde_json::Map<String, serde_json::Value>,
}

/// The assistant message the runner's client builds for this turn,
/// blocks in streaming order.
fn completion_message(c: Completion) -> goose_providers::conversation::message::Message {
    use goose_providers::conversation::message::Message;
    let mut m = Message::assistant();
    if !c.reasoning.is_empty() {
        m = m.with_thinking(c.reasoning, "");
    }
    if !c.content.is_empty() {
        m = m.with_text(c.content);
    }
    for (i, call) in c.tool_calls.into_iter().enumerate() {
        let params =
            rmcp::model::CallToolRequestParams::new(call.name).with_arguments(call.arguments);
        m = m.with_tool_request(format!("call_{i}"), Ok(params));
    }
    m
}

fn completion_answer(c: Completion) -> Option<TaskAnswer> {
    crate::runner::extract_answer_with_id(&[completion_message(c)]).0
}

/// One assistant turn of a `--tools basic` conversation, handled as the
/// runner handles it with a single graded attempt: a submit call ends
/// the task and is graded; otherwise check calls get the runner's
/// replies (the same diagnostic text, the same budget) and the
/// conversation continues; a turn with neither ends the task at zero.
#[derive(Deserialize)]
struct TurnRequest {
    task: Fixture,
    completion: Completion,
    #[serde(default)]
    checks_used: u32,
}

fn take_turn(req: TurnRequest, shaping: &Shaping) -> Result<serde_json::Value> {
    let message = [completion_message(req.completion.clone())];
    let answered = crate::runner::extract_answer_with_id(&message).0.is_some();
    let checks = crate::runner::extract_check_calls(&message);
    if answered || checks.is_empty() {
        let reward = grade_one(
            RewardRequest {
                task: RewardTask::Legacy(req.task),
                answer: None,
                completion: Some(req.completion),
                shaping: None,
            },
            shaping,
        )?;
        return Ok(json!({"done": true, "reward": reward}));
    }
    let mut checks_used = req.checks_used;
    let replies: Vec<_> = checks
        .into_iter()
        .map(|(name, args, id)| {
            let content = crate::runner::check_reply(&req.task, &name, &args, &mut checks_used);
            // A textual-fallback call has no id; the runner answers it
            // with a user message instead of a tool response.
            json!({"tool_call_id": id, "content": content})
        })
        .collect();
    Ok(json!({"done": false, "replies": replies, "checks_used": checks_used}))
}

#[derive(Deserialize)]
struct BatchRequest {
    items: Vec<RewardRequest>,
    #[serde(default)]
    shaping: Option<Shaping>,
}

#[derive(Serialize)]
struct RewardResponse {
    task_id: String,
    /// Benchmark score, identical to `btc-bench grade`.
    score: f64,
    /// Training reward: benchmark score plus configured shaping.
    shaped: f64,
    /// Secondary metric when present (optimize tasks).
    size_score: Option<f64>,
    reason: Option<String>,
    lint: Vec<String>,
    components: Components,
}

fn answer_from_value(v: serde_json::Value) -> Result<TaskAnswer> {
    // Accept the structured answer object, or a bare string treated as
    // a script answer (hex/asm) for completion-style rollouts.
    if let Some(s) = v.as_str() {
        return Ok(TaskAnswer::Script(ScriptAnswer {
            script: s.to_string(),
        }));
    }
    serde_json::from_value(v).context("answer must be a string or a task answer object")
}

/// Signals for a script answer against a reference in a context.
fn script_components(
    ctx: ContextKind,
    dialect: bench_core::task::AsmDialect,
    reference_hex: &str,
    answer: &str,
    equivalent: bool,
    lint_count: usize,
) -> Components {
    let Ok(candidate) = parse_script_answer_in(answer, dialect) else {
        return Components::default();
    };
    // Read the answer the way the grader does: through an idiom
    // rewrite when it is not Miniscript as written.
    let read_as = bench_core::decodable(ctx, &candidate);
    let decoded = decodes_in_context(ctx, &read_as.script);
    let reference = ScriptBuf::from_hex(reference_hex).expect("fixture hex is valid");
    let agreement = if decoded {
        semantic_agreement(ctx, &reference, &read_as.script)
    } else {
        None
    };
    Components {
        parsed: true,
        decoded,
        equivalent,
        agreement,
        lint_count,
    }
}

/// Shaped reward for a write/optimize answer. `graded` is the
/// benchmark score (equivalence-gated; for optimize, the weight
/// curve). `floor_eligible` gates the equivalence floor: false for
/// an optimize answer that echoes the given baseline (or bloats it),
/// so the floor rewards rewrite skill, never prompt copying.
fn shape_script(graded: f64, c: &Components, s: &Shaping, floor_eligible: bool) -> f64 {
    let mut shaped = if c.equivalent {
        if floor_eligible {
            graded.max(s.equivalent_floor)
        } else {
            graded
        }
    } else {
        let mut v = 0.0;
        if c.parsed {
            v += s.parse;
        }
        if c.decoded {
            v += s.decode;
            // Normalize: 0.5 (a constant script's cap) maps to 0, so
            // the band pays only for beating the trivial strategies.
            let norm = ((c.agreement.unwrap_or(0.0) - 0.5) * 2.0).clamp(0.0, 1.0);
            v += s.agreement * norm;
        }
        v
    };
    if c.lint_count > 0 {
        if s.lint_gate {
            return 0.0;
        }
        shaped -= s.lint_penalty * c.lint_count as f64;
    }
    shaped.clamp(0.0, 1.0)
}

fn grade_one(req: RewardRequest, default_shaping: &Shaping) -> Result<RewardResponse> {
    let answer_value = match (req.answer, req.completion) {
        (Some(v), None) => Ok(v),
        (None, Some(c)) => Err(c),
        _ => bail!("send exactly one of answer or completion"),
    };
    if let RewardTask::Wallet(wallet) = &req.task {
        let _ = &wallet.task;
        let Ok(answer) = &answer_value else {
            bail!("wallet tasks take an extracted answer, not a completion");
        };
        // Wallet rewards are always binary. No syntax rung can reward an
        // unsafe or incomplete contract, even when legacy shaping is enabled.
        let text = match answer {
            serde_json::Value::String(s) => Some(s.as_str()),
            v if v["task"] == "descriptor" => v["descriptor"].as_str(),
            _ => None,
        };
        let result = bench_wallet::grade(&wallet.fixture, text.unwrap_or(""));
        return Ok(RewardResponse {
            task_id: result.task_id,
            score: result.score,
            shaped: result.score,
            size_score: None,
            reason: result.reason,
            lint: vec![],
            components: Components {
                equivalent: result.score == 1.0,
                ..Components::default()
            },
        });
    }
    let RewardTask::Legacy(task) = req.task else {
        unreachable!()
    };
    let answer = match answer_value {
        Ok(v) => answer_from_value(v)?,
        Err(c) => match completion_answer(c) {
            Some(a) => a,
            // No submit call: `btc-bench grade` counts it as zero.
            None => {
                return Ok(RewardResponse {
                    task_id: task.id().to_string(),
                    score: 0.0,
                    shaped: 0.0,
                    size_score: None,
                    reason: Some("no submit tool call".into()),
                    lint: Vec::new(),
                    components: Components::default(),
                })
            }
        },
    };
    let shaping = match req.shaping {
        Some(s) => {
            s.validate()?;
            s
        }
        None => *default_shaping,
    };
    match (&task, &answer) {
        (Fixture::Write(w), TaskAnswer::Script(a)) => {
            let r = grade_write(w, &a.script);
            let c = script_components(
                w.context,
                task.asm_dialect(),
                &w.reference_script_hex,
                &a.script,
                r.verdict.is_equivalent(),
                r.lint.len(),
            );
            Ok(RewardResponse {
                task_id: w.id.clone(),
                score: r.score,
                shaped: shape_script(r.score, &c, &shaping, true),
                size_score: None,
                reason: r.reason,
                lint: r.lint,
                components: c,
            })
        }
        (Fixture::Judgment(j), TaskAnswer::Script(a)) => {
            let r = bench_core::grade_judgment(j, &a.script);
            let script = parse_script_answer_in(&a.script, task.asm_dialect()).ok();
            let c = Components {
                parsed: script.is_some(),
                decoded: script.as_ref().is_some_and(|s| {
                    decodes_in_context(j.context, &bench_core::decodable(j.context, s).script)
                }),
                equivalent: r.score == 1.0,
                agreement: r.agreement,
                lint_count: r.lint.len(),
            };
            // Never pay shaping for a forbidden spend or an invalid contract.
            let shaped = if r.unsafe_spend || r.agreement.is_none() {
                0.0
            } else {
                shape_script(r.score, &c, &shaping, false)
            };
            Ok(RewardResponse {
                task_id: j.id.clone(),
                score: r.score,
                shaped,
                size_score: None,
                reason: r.reason,
                lint: r.lint,
                components: c,
            })
        }
        (Fixture::Optimize(o), TaskAnswer::Script(a)) => {
            let r = grade_optimize(o, &a.script);
            let c = script_components(
                o.context,
                task.asm_dialect(),
                &o.optimal_script_hex,
                &a.script,
                r.verdict.is_equivalent(),
                r.lint.len(),
            );
            // Echo guard: the floor is earned by a distinct rewrite no
            // heavier than the given baseline, never by copying it.
            let floor_eligible = match (
                parse_script_answer_in(&a.script, task.asm_dialect()),
                &r.candidate,
            ) {
                (Ok(script), Some(w)) => {
                    script.to_hex_string() != o.baseline_script_hex && w.weight <= o.baseline_weight
                }
                _ => false,
            };
            Ok(RewardResponse {
                task_id: o.id.clone(),
                score: r.weight_score,
                shaped: shape_script(r.weight_score, &c, &shaping, floor_eligible),
                size_score: Some(r.size_score),
                reason: r.reason,
                lint: r.lint,
                components: c,
            })
        }
        (Fixture::Tree(t), TaskAnswer::Descriptor(_) | TaskAnswer::Script(_)) => {
            // Accept a script-shaped or bare-string answer as descriptor
            // text: completion-style rollouts send plain strings.
            let text = match &answer {
                TaskAnswer::Descriptor(d) => d.descriptor.clone(),
                TaskAnswer::Script(s) => s.script.clone(),
                TaskAnswer::Identify(_) => unreachable!(),
            };
            let r = bench_core::grade_tree(t, &text);
            let parsed = bench_core::parse_tr_answer(&text).is_ok();
            let c = Components {
                parsed,
                decoded: parsed,
                equivalent: r.verdict.is_equivalent(),
                agreement: if parsed {
                    bench_core::tree_agreement(t, &text)
                } else {
                    None
                },
                lint_count: r.lint.len(),
            };
            Ok(RewardResponse {
                task_id: t.id.clone(),
                score: r.weight_score,
                // Tree tasks never see their baseline, so any
                // equivalent design earns the floor.
                shaped: shape_script(r.weight_score, &c, &shaping, true),
                size_score: None,
                reason: r.reason,
                lint: r.lint,
                components: c,
            })
        }
        (Fixture::Identify(i), TaskAnswer::Identify(a)) => {
            // Identify is label-only and binary: nothing to shape.
            let r = grade_identify(i, a);
            Ok(RewardResponse {
                task_id: i.id.clone(),
                score: r.score,
                shaped: r.score,
                size_score: None,
                reason: None,
                lint: Vec::new(),
                components: Components::default(),
            })
        }
        // A script answer for an identify task (or vice versa) is a
        // wrong-shaped rollout: zero reward, not an error, so training
        // loops never crash on policy noise.
        (f, _) => Ok(RewardResponse {
            task_id: f.id().to_string(),
            score: 0.0,
            shaped: 0.0,
            size_score: None,
            reason: Some("answer type does not match task type".into()),
            lint: Vec::new(),
            components: Components::default(),
        }),
    }
}

fn respond(request: tiny_http::Request, status: u16, body: String) {
    let response = tiny_http::Response::from_string(body).with_status_code(status);
    let _ = request.respond(response);
}

fn handle(request: tiny_http::Request, shaping: &Shaping) {
    if request.method() == &tiny_http::Method::Get && request.url() == "/health" {
        respond(request, 200, "{\"ok\":true}".into());
        return;
    }
    let mut request = request;
    let mut body = String::new();
    if request.as_reader().read_to_string(&mut body).is_err() {
        respond(request, 400, "{\"error\":\"unreadable body\"}".into());
        return;
    }
    let url = request.url().to_string();
    let payload = match url.as_str() {
        "/turn" => match serde_json::from_str::<TurnRequest>(&body) {
            Ok(req) => match take_turn(req, shaping) {
                Ok(v) => v.to_string(),
                Err(e) => {
                    respond(request, 400, json!({"error": e.to_string()}).to_string());
                    return;
                }
            },
            Err(e) => {
                respond(
                    request,
                    400,
                    json!({"error": format!("bad turn request: {e}")}).to_string(),
                );
                return;
            }
        },
        "/reward" => match serde_json::from_str::<RewardRequest>(&body) {
            Ok(req) => match grade_one(req, shaping) {
                Ok(r) => json!(r).to_string(),
                Err(e) => {
                    respond(request, 400, json!({"error": e.to_string()}).to_string());
                    return;
                }
            },
            Err(e) => {
                respond(
                    request,
                    400,
                    json!({"error": format!("bad request: {e}")}).to_string(),
                );
                return;
            }
        },
        "/tool" => match serde_json::from_str::<ToolRequest>(&body) {
            Ok(req) => match run_tool(req) {
                Ok(v) => v.to_string(),
                Err(e) => {
                    respond(request, 400, json!({"error": e.to_string()}).to_string());
                    return;
                }
            },
            Err(e) => {
                respond(
                    request,
                    400,
                    json!({"error": format!("bad tool request: {e}")}).to_string(),
                );
                return;
            }
        },
        "/reward/batch" => match serde_json::from_str::<BatchRequest>(&body) {
            Ok(batch) => {
                let batch_shaping = batch.shaping.unwrap_or(*shaping);
                if let Err(e) = batch_shaping.validate() {
                    respond(request, 400, json!({"error": e.to_string()}).to_string());
                    return;
                }
                let results: Result<Vec<_>> = batch
                    .items
                    .into_iter()
                    .map(|mut item| {
                        item.shaping = Some(item.shaping.unwrap_or(batch_shaping));
                        grade_one(item, &batch_shaping)
                    })
                    .collect();
                match results {
                    Ok(rs) => json!(rs).to_string(),
                    Err(e) => {
                        respond(request, 400, json!({"error": e.to_string()}).to_string());
                        return;
                    }
                }
            }
            Err(e) => {
                respond(
                    request,
                    400,
                    json!({"error": format!("bad batch: {e}")}).to_string(),
                );
                return;
            }
        },
        _ => {
            respond(request, 404, "{\"error\":\"unknown route\"}".into());
            return;
        }
    };
    respond(request, 200, payload);
}

/// Serve rewards on `bind` with `threads` workers until killed.
pub fn serve(bind: &str, threads: usize, shaping: Shaping) -> Result<()> {
    shaping.validate()?;
    let server =
        Arc::new(tiny_http::Server::http(bind).map_err(|e| anyhow::anyhow!("bind {bind}: {e}"))?);
    println!("reward service on {bind} ({threads} threads, shaping: {shaping:?})");
    let mut workers = Vec::new();
    for _ in 0..threads.max(1) {
        let server = Arc::clone(&server);
        workers.push(std::thread::spawn(move || {
            while let Ok(request) = server.recv() {
                handle(request, &shaping);
            }
        }));
    }
    for w in workers {
        let _ = w.join();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bench_core::task::{ContextKind, Tier, WriteFixture};
    use std::str::FromStr;

    fn ms_hex(s: &str) -> String {
        miniscript::Miniscript::<bitcoin::PublicKey, miniscript::Segwitv0>::from_str(s)
            .unwrap()
            .encode()
            .to_hex_string()
    }

    fn write_fixture() -> WriteFixture {
        WriteFixture {
            prompt_version: 0,
            choose_context: false,
            request: None,
            id: "t1-0000".into(),
            tier: Tier::Easy,
            context: ContextKind::SegwitV0,
            spec_en: String::new(),
            spec_family: 0,
            atoms: 2,
            keys: vec![],
            reference_policy: String::new(),
            reference_miniscript: String::new(),
            reference_script_hex: ms_hex("and_v(v:pk(0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798),pk(02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5))"),
            hash_preimages: Default::default(),
        }
    }

    fn reward(answer: &str, shaping: Shaping) -> RewardResponse {
        grade_one(
            RewardRequest {
                task: RewardTask::Legacy(Fixture::Write(write_fixture())),
                answer: Some(serde_json::Value::String(answer.into())),
                completion: None,
                shaping: Some(shaping),
            },
            &Shaping::default(),
        )
        .unwrap()
    }

    fn completion_reward(completion: serde_json::Value) -> Result<RewardResponse> {
        let task = Fixture::Write(write_fixture());
        grade_one(
            serde_json::from_value(json!({"task": task, "completion": completion}))?,
            &Shaping::default(),
        )
    }

    fn submit(script: &str) -> serde_json::Value {
        json!({"name": "submit_script", "arguments": {"script": script}})
    }

    #[test]
    fn completion_takes_the_runners_last_submit_call() {
        let right = write_fixture().reference_script_hex;
        let wrong =
            ms_hex("pk(0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798)");
        let r = completion_reward(json!({
            "reasoning": "thinking", "content": "Here it is.",
            "tool_calls": [submit(&right)]
        }))
        .unwrap();
        assert_eq!(r.score, 1.0);
        let r = completion_reward(json!({"tool_calls": [submit(&wrong), submit(&right)]})).unwrap();
        assert_eq!(r.score, 1.0, "the last submit call is the answer");
        let r = completion_reward(json!({"tool_calls": [submit(&right), submit(&wrong)]})).unwrap();
        assert_eq!(r.score, 0.0, "the last submit call is the answer");
    }

    #[test]
    fn completion_without_a_submit_call_scores_zero() {
        let r =
            completion_reward(json!({"reasoning": "hmm", "content": "I cannot decide."})).unwrap();
        assert_eq!((r.score, r.shaped), (0.0, 0.0));
        assert_eq!(r.reason.as_deref(), Some("no submit tool call"));
    }

    fn turn(completion: serde_json::Value, checks_used: u32) -> serde_json::Value {
        let task = Fixture::Write(write_fixture());
        take_turn(
            serde_json::from_value(
                json!({"task": task, "completion": completion, "checks_used": checks_used}),
            )
            .unwrap(),
            &Shaping::default(),
        )
        .unwrap()
    }

    fn check(script: &str) -> serde_json::Value {
        json!({"name": "check_script", "arguments": {"script": script}})
    }

    #[test]
    fn a_turn_follows_the_runners_tool_loop() {
        let right = write_fixture().reference_script_hex;
        let t = turn(json!({"tool_calls": [submit(&right)]}), 0);
        assert_eq!(
            (t["done"].as_bool(), t["reward"]["score"].as_f64()),
            (Some(true), Some(1.0))
        );

        let t = turn(json!({"tool_calls": [check("OP_1 OP_DROP")]}), 3);
        let expected =
            bench_core::toolbox::check_script(ContextKind::SegwitV0, "OP_1 OP_DROP").render();
        assert_eq!(t["done"], json!(false));
        assert_eq!(
            t["replies"],
            json!([{"tool_call_id": "call_0", "content": expected}])
        );
        assert_eq!(t["checks_used"], json!(4));

        let t = turn(json!({"content": "Thinking about it."}), 0);
        assert_eq!(
            (t["done"].as_bool(), t["reward"]["score"].as_f64()),
            (Some(true), Some(0.0))
        );
    }

    #[test]
    fn a_spent_diagnostic_budget_is_reported_not_run() {
        let t = turn(json!({"tool_calls": [check("OP_1")]}), 16);
        assert_eq!(
            t["replies"][0]["content"],
            json!("Diagnostic budget exhausted; call the submit tool with your final answer.")
        );
        assert_eq!(t["checks_used"], json!(16));
    }

    #[test]
    fn answer_and_completion_are_exclusive() {
        let task = Fixture::Write(write_fixture());
        for body in [
            json!({"task": task, "answer": "OP_1", "completion": {}}),
            json!({"task": task}),
        ] {
            let req: RewardRequest = serde_json::from_value(body).unwrap();
            assert!(grade_one(req, &Shaping::default()).is_err());
        }
    }

    #[test]
    fn wallet_dispatch_matches_offline_and_rejects_extra_spending_paths() {
        let owner = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let backup = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";
        let outsider = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
        let reference = format!("tr({owner},and_v(v:pk({backup}),older(144)))");
        let fixture: bench_wallet::WalletFixture = serde_json::from_value(json!({
            "id":"wallet-reward-test", "group":"test", "family":"test", "split":"training",
            "output_kind":"concrete", "request":"", "spec_en":"", "keys":[],
            "reference_template":"", "policy_template":"", "cleartext":[], "confusion_score":0,
            "derivations":[{"is_change":false,"address_index":0,"descriptor":reference,
              "policy":format!("or(pk({owner}),and(pk({backup}),older(144)))")}]
        }))
        .unwrap();
        let shaping = Shaping {
            parse: 0.1,
            decode: 0.1,
            agreement: 0.2,
            ..Default::default()
        };
        for answer in [
            reference.clone(),
            format!("tr({owner},{{and_v(v:pk({backup}),older(144)),pk({outsider})}})"),
            reference.replace("144", "143"),
            "OP_0".into(),
            "tr()".into(),
        ] {
            let offline = bench_wallet::grade(&fixture, &answer).score;
            for value in [
                json!(answer),
                json!({"task":"descriptor","descriptor":answer}),
            ] {
                let request = serde_json::from_value(
                    json!({"task":{"task":"wallet","fixture":fixture},"answer":value}),
                )
                .unwrap();
                let result = grade_one(request, &shaping).unwrap();
                assert_eq!(result.score, offline);
                assert_eq!(result.shaped, offline);
            }
        }
        let request = serde_json::from_value(json!({"task":{"task":"wallet","fixture":fixture},
            "answer":{"task":"script","script":reference}}))
        .unwrap();
        assert_eq!(grade_one(request, &shaping).unwrap().score, 0.0);
    }

    #[test]
    fn unshaped_equals_benchmark_score() {
        let f = write_fixture();
        let r = reward(&f.reference_script_hex, Shaping::default());
        assert_eq!((r.score, r.shaped), (1.0, 1.0));
        let r = reward("51", Shaping::default());
        assert_eq!((r.score, r.shaped), (0.0, 0.0));
        let r = reward("zz not hex", Shaping::default());
        assert_eq!((r.score, r.shaped), (0.0, 0.0));
    }

    #[test]
    fn shaping_staircase() {
        let s = Shaping {
            parse: 0.05,
            decode: 0.10,
            agreement: 0.25,
            ..Default::default()
        };
        s.validate().unwrap();
        // Unparseable: nothing.
        assert_eq!(reward("zz not hex", s).shaped, 0.0);
        // Parses but fails the decode gate (OP_RETURN): parse rung only.
        let r = reward("6a", s);
        assert!(r.components.parsed && !r.components.decoded);
        assert!((r.shaped - 0.05).abs() < 1e-12, "{}", r.shaped);
        // OP_1 decodes but is a constant: agreement normalizes to 0,
        // so it earns the parse+decode rungs and no band. The
        // always-true hack cannot farm the dense signal.
        let r = reward("51", s);
        assert!(r.components.decoded && !r.components.equivalent);
        assert_eq!(r.components.agreement, Some(0.5));
        assert!((r.shaped - 0.15).abs() < 1e-12, "{}", r.shaped);
        // A near-miss (one right key, one wrong) earns part of the band.
        let near = ms_hex("and_v(v:pk(0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798),pk(02f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9))");
        let r = reward(&near, s);
        assert!(r.shaped > 0.15 && r.shaped < 0.4, "{}", r.shaped);
        // Equivalent: full credit regardless of rungs.
        let f = write_fixture();
        assert_eq!(reward(&f.reference_script_hex, s).shaped, 1.0);
    }

    #[test]
    fn lint_gate_and_penalty() {
        // OP_1 lints as unsafe (no signature required).
        let gate = Shaping {
            parse: 0.05,
            decode: 0.10,
            lint_gate: true,
            ..Default::default()
        };
        assert_eq!(reward("51", gate).shaped, 0.0);
        let pen = Shaping {
            parse: 0.05,
            decode: 0.10,
            lint_penalty: 0.05,
            ..Default::default()
        };
        let r = reward("51", pen);
        assert_eq!(r.components.lint_count, 1);
        assert!((r.shaped - 0.10).abs() < 1e-12, "{}", r.shaped);
    }

    /// The equivalence floor must reward rewrite skill, never prompt
    /// copying: 80% of a real run's optimize answers echoed the
    /// baseline verbatim, and an unconditional floor would make that
    /// the dominant RL strategy.
    #[test]
    fn optimize_floor_rejects_baseline_echo() {
        use bench_gen::fixtures::{generate, GenParams};
        let fixtures = generate(&GenParams {
            seed: 11,
            write: 0,
            optimize: 1,
            identify: 0,
            ..GenParams::default()
        });
        let Fixture::Optimize(o) = &fixtures[0] else {
            panic!("optimize fixture")
        };
        let shaping = Shaping {
            equivalent_floor: 0.3,
            ..Default::default()
        };
        let reward = |answer: &str| {
            grade_one(
                RewardRequest {
                    task: RewardTask::Legacy(fixtures[0].clone()),
                    answer: Some(serde_json::Value::String(answer.into())),
                    completion: None,
                    shaping: Some(shaping),
                },
                &Shaping::default(),
            )
            .unwrap()
        };
        // Echoing the baseline: equivalent, zero benchmark score, and
        // NO floor — the hack pays nothing.
        let echo = reward(&o.baseline_script_hex);
        assert!(echo.components.equivalent);
        assert_eq!(echo.score, 0.0);
        assert_eq!(echo.shaped, 0.0, "baseline echo must not earn the floor");
        // A distinct equivalent rewrite (the optimum) scores above the
        // floor via the curve; floor eligibility is moot at 1.0.
        let opt = reward(&o.optimal_script_hex);
        assert_eq!(opt.shaped, 1.0);
        // Same echo in asm notation is still an echo (bytes compare).
        let asm = bench_core::human_asm::to_human_asm(
            ScriptBuf::from_hex(&o.baseline_script_hex)
                .unwrap()
                .as_script(),
        );
        let echo_asm = reward(&asm);
        assert_eq!(
            echo_asm.shaped, 0.0,
            "asm-notation echo must not earn the floor"
        );
    }

    #[test]
    fn shaping_validation_rejects_hackable_configs() {
        assert!(Shaping {
            parse: 0.3,
            decode: 0.2,
            agreement: 0.1,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Shaping {
            parse: -0.1,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Shaping {
            equivalent_floor: 0.9,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    #[test]
    fn judgment_reward_matches_offline_and_never_pays_for_forbidden_spends() {
        let fixtures = bench_gen::fixtures::generate(&bench_gen::fixtures::GenParams {
            seed: 2026,
            write: 0,
            optimize: 0,
            identify: 0,
            judgment: 3,
            ..Default::default()
        });
        for fixture in &fixtures {
            let Fixture::Judgment(j) = fixture else {
                unreachable!()
            };
            let reference = bench_gen::judgment::validate(j).unwrap().to_hex_string();
            for answer in [&reference, "OP_0", "OP_1", "bad script", "OP_RETURN"] {
                let offline = bench_core::grade_judgment(j, answer);
                for structured in [false, true] {
                    let r = grade_one(
                        RewardRequest {
                            task: RewardTask::Legacy(fixture.clone()),
                            answer: Some(if structured {
                                json!({"task": "script", "script": answer})
                            } else {
                                json!(answer)
                            }),
                            completion: None,
                            shaping: None,
                        },
                        &Shaping::default(),
                    )
                    .unwrap();
                    assert_eq!(r.score, offline.score);
                    assert_eq!(r.shaped, offline.score);
                }
            }
            let r = grade_one(
                RewardRequest {
                    task: RewardTask::Legacy(fixture.clone()),
                    answer: Some(json!("OP_1")),
                    completion: None,
                    shaping: None,
                },
                &Shaping {
                    parse: 0.05,
                    decode: 0.1,
                    agreement: 0.2,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(r.shaped, 0.0, "forbidden spends cannot collect shaping");
        }
    }
}
