//! Golden grader-stability test: committed fixtures, committed answers,
//! committed expected grades. Any change to grading behavior — score
//! values, reasons, lint output — fails here and must be a conscious
//! golden update. This is rust-miniscript's crash-freeze discipline
//! (every fuzzer finding becomes a pinned regression) applied to our
//! own traffic: the answers mimic real model output shapes.
//!
//! Regenerate after an intentional grader change:
//!
//! ```text
//! BTC_BENCH_REGEN_GOLDEN=1 cargo test -p bench-core --test golden
//! ```

use std::path::PathBuf;

use bench_core::task::{Fixture, ResponseRecord, ScriptAnswer, TaskAnswer};
use bench_core::{grade_identify, grade_optimize, grade_write};

/// The exact notation the bench displays this script as (prompts and
/// tool reports). Submitting it must grade like submitting the hex —
/// the golden pin for the display/parse dialect asymmetry bug class.
fn displayed_asm(hex: &str) -> String {
    bench_core::human_asm::to_human_asm(
        bitcoin::ScriptBuf::from_hex(hex)
            .expect("fixture hex")
            .as_script(),
    )
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn read_jsonl<T: serde::de::DeserializeOwned>(name: &str) -> Vec<T> {
    let text =
        std::fs::read_to_string(dir().join(name)).unwrap_or_else(|e| panic!("read {name}: {e}"));
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("parse {name} line: {e}")))
        .collect()
}

/// One graded answer, as pinned by the golden file.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct GoldenScore {
    task_id: String,
    score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_score: Option<f64>,
    /// Absolute candidate weight/size for optimize answers: pins
    /// `weights_for` itself, not just curve ratios (a uniform weight
    /// shift would leave every endpoint score unchanged).
    #[serde(skip_serializing_if = "Option::is_none")]
    candidate_weight: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    candidate_size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lint: Option<Vec<String>>,
}
/// Derive the canonical answer set from the fixtures: perfect answers
/// (the answer keys themselves, hex and asm forms), no-ops, garbage,
/// always-true attacks, and unimproved baselines. Every answer shape a
/// real run produces.
fn answers_for(fixtures: &[Fixture]) -> Vec<ResponseRecord> {
    let mut out = Vec::new();
    for f in fixtures {
        match f {
            Fixture::Write(w) => {
                // Perfect, hex.
                out.push(record(w.id.clone(), w.reference_script_hex.clone()));
                // Perfect in a whitespace-padded form (parser robustness).
                out.push(record(
                    w.id.clone(),
                    format!("  {}  ", w.reference_script_hex),
                ));
                // Perfect in the bench's own display notation (decimal
                // timelocks); a model echoing what it is shown must
                // grade identically to hex.
                out.push(record(w.id.clone(), displayed_asm(&w.reference_script_hex)));
                // Always-true attack: decodes, not equivalent.
                out.push(record(w.id.clone(), "51".into()));
                // Garbage hex.
                out.push(record(w.id.clone(), "zznothex".into()));
                // Empty.
                out.push(record(w.id.clone(), String::new()));
            }
            Fixture::Optimize(o) => {
                // The unimproved baseline: equivalent, scores 0 on the
                // curve (candidate == baseline), carries lint findings
                // whenever the naive encoding is insane.
                out.push(record(o.id.clone(), o.baseline_script_hex.clone()));
                // The known optimum: full marks — in hex and in the
                // displayed notation.
                out.push(record(o.id.clone(), o.optimal_script_hex.clone()));
                out.push(record(o.id.clone(), displayed_asm(&o.optimal_script_hex)));
                // The baseline as displayed in the prompt: equivalent,
                // zero on the curve, never a parse error.
                out.push(record(o.id.clone(), displayed_asm(&o.baseline_script_hex)));
                // Not-equivalent rewrite.
                out.push(record(o.id.clone(), "51".into()));
            }
            Fixture::Tree(t) => {
                // The compiler's own tree: full marks.
                out.push(tr_record(t.id.clone(), t.reference_descriptor.clone()));
                // The single-leaf baseline: equivalent, scores 0 on
                // the curve.
                out.push(tr_record(t.id.clone(), t.baseline_descriptor.clone()));
                // Not a descriptor.
                out.push(tr_record(t.id.clone(), "not a descriptor".into()));
            }
            Fixture::Identify(_) => {}
        }
    }
    out
}

fn tr_record(task_id: String, descriptor: String) -> ResponseRecord {
    ResponseRecord {
        task_id,
        answer: TaskAnswer::Descriptor(bench_core::task::DescriptorAnswer { descriptor }),
        raw: None,
        output_tokens: None,
        finish_reason: None,
        tool_calls: None,
    }
}

fn record(task_id: String, script: String) -> ResponseRecord {
    ResponseRecord {
        task_id,
        answer: TaskAnswer::Script(ScriptAnswer { script }),
        raw: None,
        output_tokens: None,
        finish_reason: None,
        tool_calls: None,
    }
}

fn grade_all(fixtures: &[Fixture], responses: &[ResponseRecord]) -> Vec<GoldenScore> {
    let mut out = Vec::new();
    for r in responses {
        let f = fixtures
            .iter()
            .find(|f| f.id() == r.task_id)
            .unwrap_or_else(|| panic!("fixture for {}", r.task_id));
        let gs = match (&f, &r.answer) {
            (Fixture::Write(w), TaskAnswer::Script(a)) => {
                let res = grade_write(w, &a.script);
                GoldenScore {
                    task_id: r.task_id.clone(),
                    score: res.score,
                    size_score: None,
                    candidate_weight: None,
                    candidate_size: None,
                    reason: res.reason,
                    lint: (!res.lint.is_empty()).then_some(res.lint),
                }
            }
            (Fixture::Optimize(o), TaskAnswer::Script(a)) => {
                let res = grade_optimize(o, &a.script);
                GoldenScore {
                    task_id: r.task_id.clone(),
                    score: res.weight_score,
                    size_score: Some(res.size_score),
                    candidate_weight: res.candidate.map(|c| c.weight),
                    candidate_size: res.candidate.map(|c| c.size),
                    reason: res.reason,
                    lint: (!res.lint.is_empty()).then_some(res.lint),
                }
            }
            (Fixture::Tree(t), TaskAnswer::Descriptor(a)) => {
                let res = bench_core::grade_tree(t, &a.descriptor);
                GoldenScore {
                    task_id: r.task_id.clone(),
                    score: res.weight_score,
                    size_score: None,
                    candidate_weight: res.candidate_weight,
                    candidate_size: None,
                    reason: res.reason,
                    lint: (!res.lint.is_empty()).then_some(res.lint),
                }
            }
            _ => unreachable!("golden set has no identify fixtures"),
        };
        out.push(gs);
    }
    out
}

#[test]
fn golden_grades_are_stable() {
    let fixtures: Vec<Fixture> = read_jsonl("fixtures.jsonl");
    assert!(fixtures.len() >= 6, "golden set must cover both task kinds");
    let responses = answers_for(&fixtures);
    let got = grade_all(&fixtures, &responses);

    let expected_path = dir().join("expected.jsonl");
    if std::env::var("BTC_BENCH_REGEN_GOLDEN").is_ok() {
        let mut text = String::new();
        for g in &got {
            text.push_str(&serde_json::to_string(g).unwrap());
            text.push('\n');
        }
        std::fs::write(&expected_path, text).expect("write expected.jsonl");
        // Also commit the derived answers so the pinned input is
        // inspectable.
        let mut atext = String::new();
        for r in &responses {
            atext.push_str(&serde_json::to_string(r).unwrap());
            atext.push('\n');
        }
        std::fs::write(dir().join("responses.jsonl"), atext).expect("write responses.jsonl");
        panic!("golden files regenerated; re-run without BTC_BENCH_REGEN_GOLDEN");
    }

    let expected: Vec<GoldenScore> = read_jsonl("expected.jsonl");
    let committed: Vec<ResponseRecord> = read_jsonl("responses.jsonl");
    assert_eq!(committed.len(), responses.len(), "responses.jsonl stale");
    for (c, d) in committed.iter().zip(responses.iter()) {
        assert_eq!(c.task_id, d.task_id, "responses.jsonl task order stale");
        assert_eq!(
            serde_json::to_string(c).unwrap(),
            serde_json::to_string(d).unwrap(),
            "responses.jsonl content stale; regenerate"
        );
    }

    assert_eq!(expected.len(), got.len(), "golden size mismatch");
    for (e, g) in expected.iter().zip(got.iter()) {
        assert_eq!(e.task_id, g.task_id);
        assert_eq!(
            e.score, g.score,
            "score drift for {}: {:?} vs {:?}",
            e.task_id, e, g
        );
        assert_eq!(e.size_score, g.size_score, "size drift for {}", e.task_id);
        assert_eq!(
            e.candidate_weight, g.candidate_weight,
            "candidate weight drift for {}",
            e.task_id
        );
        assert_eq!(
            e.candidate_size, g.candidate_size,
            "candidate size drift for {}",
            e.task_id
        );
        assert_eq!(
            e.reason, g.reason,
            "reason drift for {}: {:?} vs {:?}",
            e.task_id, e.reason, g.reason
        );
        assert_eq!(e.lint, g.lint, "lint drift for {}", e.task_id);
    }

    // Invariants worth pinning beyond raw stability.
    let perfect = got.iter().filter(|g| g.score >= 0.999).count();
    assert!(perfect >= 4, "expected several full-credit answers");
    assert!(
        got.iter().any(|g| g.score == 0.0 && g.reason.is_some()),
        "expected rejected answers with reasons"
    );
}

// Keep the unused import away when identify grading joins the goldens.
#[allow(dead_code)]
fn _identify_grader_linked() {
    let _ = grade_identify
        as fn(
            &bench_core::task::IdentifyFixture,
            &bench_core::task::IdentifyAnswer,
        ) -> bench_core::IdentifyResult;
}
