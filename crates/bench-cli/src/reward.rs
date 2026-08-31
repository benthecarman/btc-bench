//! Reward service: exposes the graders over HTTP for RL training loops.
//!
//! POST /reward        {"task": <fixture>, "answer": <answer>, "shaping"?: {...}}
//! POST /reward/batch  {"items": [{"task":..., "answer":...}], "shaping"?: {...}}
//! GET  /health        {"ok": true}
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
use bench_core::answer::parse_script_answer;
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
    /// Floor for an equivalent-but-unimproved optimize answer (the
    /// weight curve alone scores it 0; equivalence is worth reward
    /// during training).
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
struct RewardRequest {
    task: Fixture,
    answer: serde_json::Value,
    #[serde(default)]
    partial_credit: Option<f64>,
    #[serde(default)]
    shaping: Option<Shaping>,
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
    reference_hex: &str,
    answer: &str,
    equivalent: bool,
    lint_count: usize,
) -> Components {
    let Ok(candidate) = parse_script_answer(answer) else {
        return Components::default();
    };
    let decoded = decodes_in_context(ctx, &candidate);
    let reference = ScriptBuf::from_hex(reference_hex).expect("fixture hex is valid");
    let agreement = if decoded {
        semantic_agreement(ctx, &reference, &candidate)
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
/// curve).
fn shape_script(graded: f64, c: &Components, s: &Shaping) -> f64 {
    let mut shaped = if c.equivalent {
        graded.max(s.equivalent_floor)
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
    let answer = answer_from_value(req.answer)?;
    let partial = req.partial_credit.unwrap_or(0.5);
    let shaping = match req.shaping {
        Some(s) => {
            s.validate()?;
            s
        }
        None => *default_shaping,
    };
    match (&req.task, &answer) {
        (Fixture::Write(w), TaskAnswer::Script(a)) => {
            let r = grade_write(w, &a.script);
            let c = script_components(
                w.context,
                &w.reference_script_hex,
                &a.script,
                r.verdict.is_equivalent(),
                r.lint.len(),
            );
            Ok(RewardResponse {
                task_id: w.id.clone(),
                score: r.score,
                shaped: shape_script(r.score, &c, &shaping),
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
                &o.optimal_script_hex,
                &a.script,
                r.verdict.is_equivalent(),
                r.lint.len(),
            );
            Ok(RewardResponse {
                task_id: o.id.clone(),
                score: r.weight_score,
                shaped: shape_script(r.weight_score, &c, &shaping),
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
                shaped: shape_script(r.weight_score, &c, &shaping),
                size_score: None,
                reason: r.reason,
                lint: r.lint,
                components: c,
            })
        }
        (Fixture::Identify(i), TaskAnswer::Identify(a)) => {
            // Identify is already dense (per-param credit): no shaping.
            let r = grade_identify(i, a, partial);
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
                task: Fixture::Write(write_fixture()),
                answer: serde_json::Value::String(answer.into()),
                partial_credit: None,
                shaping: Some(shaping),
            },
            &Shaping::default(),
        )
        .unwrap()
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
}
