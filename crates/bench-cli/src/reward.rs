//! Reward service: exposes the graders over HTTP for training loops.
//!
//! POST /reward        {"task": <fixture>, "answer": <answer>}   -> single
//! POST /reward/batch  {"items": [{"task":..., "answer":...}]}   -> list
//!
//! Response: {"task_id", "score", "reason"} where score uses the same
//! scale as the benchmark (write/optimize equivalence-gated; identify
//! with partial credit). The service is deliberately tiny: local trust
//! boundary, JSON in, JSON out, no auth.

use anyhow::{Context as _, Result};
use bench_core::task::{Fixture, IdentifyAnswer, ParamValue, ScriptAnswer, TaskAnswer};
use bench_core::{grade_identify, grade_optimize, grade_write};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Deserialize)]
struct RewardRequest {
    task: Fixture,
    answer: serde_json::Value,
    #[serde(default)]
    partial_credit: Option<f64>,
}

#[derive(Serialize)]
struct RewardResponse {
    task_id: String,
    score: f64,
    /// Secondary metric when present (optimize tasks).
    size_score: Option<f64>,
    reason: Option<String>,
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

fn grade_one(req: RewardRequest) -> Result<RewardResponse> {
    let answer = answer_from_value(req.answer)?;
    let partial = req.partial_credit.unwrap_or(0.5);
    match (&req.task, &answer) {
        (Fixture::Write(w), TaskAnswer::Script(a)) => {
            let r = grade_write(w, &a.script);
            Ok(RewardResponse {
                task_id: w.id.clone(),
                score: r.score,
                size_score: None,
                reason: r.reason,
            })
        }
        (Fixture::Optimize(o), TaskAnswer::Script(a)) => {
            let r = grade_optimize(o, &a.script);
            Ok(RewardResponse {
                task_id: o.id.clone(),
                score: r.weight_score,
                size_score: Some(r.size_score),
                reason: r.reason,
            })
        }
        (Fixture::Identify(i), TaskAnswer::Identify(a)) => {
            let r = grade_identify(i, a, partial);
            Ok(RewardResponse {
                task_id: i.id.clone(),
                score: r.score,
                size_score: None,
                reason: None,
            })
        }
        // A script answer for an identify task (or vice versa) is a
        // wrong-shaped rollout: zero reward, not an error, so training
        // loops never crash on policy noise.
        (f, _) => Ok(RewardResponse {
            task_id: f.id().to_string(),
            score: 0.0,
            size_score: None,
            reason: Some("answer type does not match task type".into()),
        }),
    }
}

fn respond(request: tiny_http::Request, status: u16, body: String) {
    let response = tiny_http::Response::from_string(body).with_status_code(status);
    let _ = request.respond(response);
}

/// Serve rewards on the given port until killed.
pub fn serve(port: u16) -> Result<()> {
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow::anyhow!("bind reward port: {e}"))?;
    println!("reward service on 127.0.0.1:{port}");
    for mut request in server.incoming_requests() {
        let mut body = String::new();
        let ok = request.as_reader().read_to_string(&mut body).is_ok();
        let url = request.url().to_string();
        if !ok {
            respond(request, 400, "{\"error\":\"unreadable body\"}".into());
            continue;
        }
        let payload = match url.as_str() {
            "/reward" => match serde_json::from_str::<RewardRequest>(&body) {
                Ok(req) => match grade_one(req) {
                    Ok(r) => json!(r).to_string(),
                    Err(e) => {
                        respond(request, 400, json!({"error": e.to_string()}).to_string());
                        continue;
                    }
                },
                Err(e) => {
                    respond(
                        request,
                        400,
                        json!({"error": format!("bad request: {e}")}).to_string(),
                    );
                    continue;
                }
            },
            "/reward/batch" => match serde_json::from_str::<Vec<RewardRequest>>(&body) {
                Ok(reqs) => match reqs.into_iter().map(grade_one).collect::<Result<Vec<_>>>() {
                    Ok(rs) => json!(rs).to_string(),
                    Err(e) => {
                        respond(request, 400, json!({"error": e.to_string()}).to_string());
                        continue;
                    }
                },
                Err(e) => {
                    respond(
                        request,
                        400,
                        json!({"error": format!("bad batch: {e}")}).to_string(),
                    );
                    continue;
                }
            },
            _ => {
                respond(request, 404, "{\"error\":\"unknown route\"}".into());
                continue;
            }
        };
        respond(request, 200, payload);
    }
    Ok(())
}

// Keep unused-import warnings away when ParamValue isn't referenced
// directly in this module beyond deserialization.
#[allow(dead_code)]
fn _param_type_check(_p: ParamValue) {}
#[allow(dead_code)]
fn _identify_type_check(_a: IdentifyAnswer) {}
