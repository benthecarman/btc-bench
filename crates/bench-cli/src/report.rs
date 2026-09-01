//! Sweep analysis: `report` (cross-model comparison with tier/context
//! cuts, failure taxonomy, lint rates, optimize-curve histograms) and
//! `passk` (pass@k / pass^k across N sampled runs of the same model).
//!
//! Both consume committed datasets plus run directories produced by
//! `btc-bench run` + `btc-bench grade` — fully offline, no endpoints.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context as _, Result};
use bench_core::task::{ContextKind, Fixture, ResponseRecord, TaskAnswer, Tier};
use serde::Deserialize;

use crate::load_dataset;

type GradedTask = crate::TaskScore;

/// Grade a run's stored responses live. The report never reads
/// graded/results.json: stored grades go stale whenever grading
/// changes (a stale run dir once corrupted a sweep's whole failure
/// taxonomy), while responses.jsonl is the immutable source of truth
/// and re-grading it is offline and takes seconds. Standard-mode
/// gating is not applied here; the report grades in default mode.
fn grade_run(fixtures: &[Fixture], run_dir: &Path) -> Result<Vec<GradedTask>> {
    let responses = crate::load_responses(&run_dir.join("responses.jsonl"))
        .with_context(|| format!("read {}/responses.jsonl", run_dir.display()))?;
    let (scores, _) = crate::grade(fixtures, &responses, None, 0.5, false)?;
    Ok(scores)
}

fn kind_of(f: &Fixture) -> &'static str {
    match f {
        Fixture::Write(_) => "write",
        Fixture::Optimize(_) => "optimize",
        Fixture::Identify(_) => "identify",
        Fixture::Tree(_) => "tree",
    }
}

fn tier_of(f: &Fixture) -> Option<Tier> {
    match f {
        Fixture::Write(w) => Some(w.tier),
        Fixture::Optimize(o) => Some(o.tier),
        Fixture::Identify(_) => None,
        Fixture::Tree(t) => Some(t.tier),
    }
}

fn ctx_of(f: &Fixture) -> Option<ContextKind> {
    match f {
        Fixture::Write(w) => Some(w.context),
        Fixture::Optimize(o) => Some(o.context),
        Fixture::Identify(_) => None,
        // Tree tasks are taproot by definition; keeping them out of
        // the context table keeps it a write/optimize comparison.
        Fixture::Tree(_) => None,
    }
}

use crate::classify_failure;

fn atoms_of(f: &Fixture) -> Option<usize> {
    match f {
        Fixture::Write(w) => (w.atoms > 0).then_some(w.atoms),
        Fixture::Optimize(o) => (o.atoms > 0).then_some(o.atoms),
        Fixture::Identify(_) => None,
        Fixture::Tree(t) => (t.atoms > 0).then_some(t.atoms),
    }
}

fn family_of(f: &Fixture) -> Option<u32> {
    match f {
        Fixture::Write(w) => Some(w.spec_family),
        Fixture::Optimize(o) => Some(o.spec_family),
        Fixture::Identify(_) => None,
        Fixture::Tree(t) => Some(t.spec_family),
    }
}

/// The lint rate among perfect answers: equivalent-but-insane scripts.
/// These score 1.0 by default and 0.0 under --standard-mode.
fn linted_perfect(t: &GradedTask) -> bool {
    t.score >= 0.999 && t.lint.as_ref().is_some_and(|l| !l.is_empty())
}

/// Render the cross-model report. `runs` is (label, run-dir) — each dir
/// must already contain graded/results.json.
pub fn report(dataset: &Path, runs: &[(String, std::path::PathBuf)], out: &Path) -> Result<()> {
    let fixtures = load_dataset(dataset)?;
    let by_id: BTreeMap<&str, &Fixture> = fixtures.iter().map(|f| (f.id(), f)).collect();

    let mut md = String::new();
    md.push_str(&format!(
        "# Sweep report\n\ngraded live from responses.jsonl by btc-bench {}\n\n",
        crate::build_stamp()
    ));

    // Headline table. Means are over answered tasks; the 95% CI is a
    // percentile bootstrap over the same per-task scores.
    md.push_str("| model | write | optimize (w) | optimize (s) | identify | tree (w) | unanswered | linted-perfect |\n");
    md.push_str("|---|---|---|---|---|---|---|---|\n");
    let mut loaded: Vec<(String, Vec<GradedTask>)> = Vec::new();
    for (label, dir) in runs {
        let g = grade_run(&fixtures, dir)?;
        loaded.push((label.clone(), g));
    }
    for (label, g) in &loaded {
        let mut vecs: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
        let mut answered = 0usize;
        let mut linted = 0usize;
        let mut perfect = 0usize;
        for t in g {
            answered += 1;
            if linted_perfect(t) {
                linted += 1;
            }
            if t.score >= 0.999 {
                perfect += 1;
            }
            let Some(f) = by_id.get(t.task_id.as_str()) else {
                bail!("task {} not in dataset", t.task_id);
            };
            let k = kind_of(f);
            // Optimize: track weight and size separately.
            if k == "optimize" {
                vecs.entry("optimize_w").or_default().push(t.score);
                vecs.entry("optimize_s")
                    .or_default()
                    .push(t.size_score.unwrap_or(0.0));
            } else {
                vecs.entry(k).or_default().push(t.score);
            }
        }
        let unanswered = fixtures.len().saturating_sub(answered);
        let mean = |k: &str| {
            vecs.get(k)
                .filter(|v| !v.is_empty())
                .map(|v| v.iter().sum::<f64>() / v.len() as f64)
                .unwrap_or(f64::NAN)
        };
        let cell = |k: &str, seed: u64| {
            let m = mean(k);
            match vecs.get(k).and_then(|v| crate::bootstrap_ci(v, 1000, seed)) {
                Some((lo, hi)) => format!("{m:.3} [{lo:.3}, {hi:.3}]"),
                None => format!("{m:.3}"),
            }
        };
        md.push_str(&format!(
            "| {} | {} | {} | {:.3} | {} | {} | {} | {}/{} |\n",
            label,
            cell("write", 1),
            cell("optimize_w", 2),
            mean("optimize_s"),
            cell("identify", 3),
            cell("tree", 4),
            unanswered,
            linted,
            perfect,
        ));
    }
    md.push('\n');
    md.push_str(
        "linted-perfect = score 1.0 with insanity findings (malleable etc.); \
                 the fraction a --standard-mode gate would zero.\n\n",
    );

    // Format vs reasoning: an answer is well-formed when it cleared the
    // parse and decode gates; its failure, if any, is semantic. The
    // sem column is the mean score among well-formed answers.
    md.push_str("## Well-formed vs semantic (write/optimize)\n\n");
    md.push_str(
        "| model | write wf | write sem | optimize wf | optimize sem |\n|---|---|---|---|---|\n",
    );
    for (label, g) in &loaded {
        let mut cells: BTreeMap<&str, (usize, usize, f64)> = BTreeMap::new();
        for t in g {
            let Some(f) = by_id.get(t.task_id.as_str()) else {
                continue;
            };
            let k = kind_of(f);
            if k == "identify" {
                continue;
            }
            let e = cells.entry(k).or_insert((0, 0, 0.0));
            e.0 += 1;
            if crate::is_wellformed(t.score, &t.failure, &t.reason) {
                e.1 += 1;
                e.2 += t.score;
            }
        }
        let render = |k: &str| {
            let (n, wf, sum) = cells.get(k).copied().unwrap_or((0, 0, 0.0));
            let sem = if wf == 0 { f64::NAN } else { sum / wf as f64 };
            (format!("{wf}/{n}"), format!("{sem:.3}"))
        };
        let (wwf, wsem) = render("write");
        let (owf, osem) = render("optimize");
        let (twf, tsem) = render("tree");
        md.push_str(&format!(
            "| {label} | {wwf} | {wsem} | {owf} | {osem} | {twf} | {tsem} |\n"
        ));
    }
    md.push('\n');

    // Tier x kind breakdown per model.
    md.push_str("## Tier breakdown (mean score)\n\n");
    for (label, g) in &loaded {
        md.push_str(&format!("### {label}\n\n"));
        md.push_str("| kind | tier | n | mean |\n|---|---|---|---|\n");
        let mut cells: BTreeMap<(&str, &str), (f64, usize)> = BTreeMap::new();
        for t in g {
            let Some(f) = by_id.get(t.task_id.as_str()) else {
                continue;
            };
            let tier = match tier_of(f) {
                Some(t) => format!("{t:?}").to_lowercase(),
                None => "n/a".to_string(),
            };
            let key: (&str, &str) = (kind_of(f), tier.leak());
            let e = cells.entry(key).or_insert((0.0, 0));
            e.0 += t.score;
            e.1 += 1;
        }
        for ((k, tier), (s, n)) in &cells {
            md.push_str(&format!("| {k} | {tier} | {n} | {:.3} |\n", s / *n as f64));
        }
        md.push('\n');
    }

    // Atom-count breakdown: the continuous difficulty axis. Rendered
    // only for datasets whose fixtures record atom counts.
    if fixtures.iter().any(|f| atoms_of(f).is_some()) {
        md.push_str("## Atom-count breakdown (write/optimize, mean score)\n\n");
        for (label, g) in &loaded {
            md.push_str(&format!("### {label}\n\n"));
            md.push_str("| kind | atoms | n | mean |\n|---|---|---|---|\n");
            let mut cells: BTreeMap<(&str, usize), (f64, usize)> = BTreeMap::new();
            for t in g {
                let Some(f) = by_id.get(t.task_id.as_str()) else {
                    continue;
                };
                let Some(a) = atoms_of(f) else { continue };
                let e = cells.entry((kind_of(f), a)).or_insert((0.0, 0));
                e.0 += t.score;
                e.1 += 1;
            }
            for ((k, a), (s, n)) in &cells {
                md.push_str(&format!("| {k} | {a} | {n} | {:.3} |\n", s / *n as f64));
            }
            md.push('\n');
        }
    }

    // Spec-family breakdown: catches template-phrasing overfit (a
    // model that only solves the canonical family 0 memorized prose,
    // not semantics). Rendered only when >1 family is present.
    let families: std::collections::BTreeSet<u32> = fixtures.iter().filter_map(family_of).collect();
    if families.len() > 1 {
        md.push_str("## Spec-family breakdown (write/optimize, mean score)\n\n");
        md.push_str("| model | family | n | mean |\n|---|---|---|---|\n");
        for (label, g) in &loaded {
            let mut cells: BTreeMap<u32, (f64, usize)> = BTreeMap::new();
            for t in g {
                let Some(f) = by_id.get(t.task_id.as_str()) else {
                    continue;
                };
                let Some(fam) = family_of(f) else { continue };
                let e = cells.entry(fam).or_insert((0.0, 0));
                e.0 += t.score;
                e.1 += 1;
            }
            for (fam, (s, n)) in &cells {
                md.push_str(&format!(
                    "| {label} | {fam} | {n} | {:.3} |\n",
                    s / *n as f64
                ));
            }
        }
        md.push('\n');
    }

    // Context breakdown.
    md.push_str("## Context breakdown (mean score)\n\n");
    md.push_str("| model | legacy | segwit | tap |\n|---|---|---|---|\n");
    for (label, g) in &loaded {
        let mut cells: BTreeMap<&str, (f64, usize)> = BTreeMap::new();
        for t in g {
            let Some(f) = by_id.get(t.task_id.as_str()) else {
                continue;
            };
            if let Some(c) = ctx_of(f) {
                let key = format!("{c:?}").to_lowercase();
                let e = cells.entry(key.leak()).or_insert((0.0, 0));
                e.0 += t.score;
                e.1 += 1;
            }
        }
        let pct = |k: &str| cells.get(k).map(|(s, n)| s / *n as f64).unwrap_or(f64::NAN);
        md.push_str(&format!(
            "| {label} | {:.3} | {:.3} | {:.3} |\n",
            pct("legacy"),
            pct("segwitv0"),
            pct("tap")
        ));
    }
    md.push('\n');

    // Failure taxonomy for zero scores.
    md.push_str("## Zero-score taxonomy\n\n");
    md.push_str("| model | answer parse error | decode-gate reject | wrong semantics | wrong label | unimproved | gated | other |\n|---|---|---|---|---|---|---|---|\n");
    for (label, g) in &loaded {
        let mut tax: BTreeMap<&str, usize> = BTreeMap::new();
        for t in g {
            if t.score == 0.0 {
                *tax.entry(classify_failure(&t.failure, &t.reason))
                    .or_insert(0) += 1;
            }
        }
        md.push_str(&format!(
            "| {label} | {} | {} | {} | {} | {} | {} | {} |\n",
            tax.get("answer parse error").copied().unwrap_or(0),
            tax.get("decode-gate reject").copied().unwrap_or(0),
            tax.get("wrong semantics").copied().unwrap_or(0),
            tax.get("wrong label").copied().unwrap_or(0),
            tax.get("unimproved (equivalent)").copied().unwrap_or(0),
            tax.get("gated (standard mode)").copied().unwrap_or(0),
            tax.get("other").copied().unwrap_or(0),
        ));
    }
    md.push('\n');

    // Optimize curve histogram.
    md.push_str("## Optimize weight-score distribution\n\n");
    md.push_str("| model | 0 | (0,.25] | (.25,.5] | (.5,.75] | (.75,1) | 1 |\n|---|---|---|---|---|---|---|\n");
    for (label, g) in &loaded {
        let mut buckets = [0usize; 6];
        for t in g {
            let Some(f) = by_id.get(t.task_id.as_str()) else {
                continue;
            };
            if kind_of(f) != "optimize" {
                continue;
            }
            let s = t.score;
            let i = if s == 0.0 {
                0
            } else if s <= 0.25 {
                1
            } else if s <= 0.5 {
                2
            } else if s <= 0.75 {
                3
            } else if s < 1.0 {
                4
            } else {
                5
            };
            buckets[i] += 1;
        }
        md.push_str(&format!(
            "| {label} | {}\n",
            buckets.map(|b| b.to_string()).join(" | ")
        ));
    }
    md.push('\n');

    std::fs::write(out, md).with_context(|| format!("write {}", out.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------
// pass@k / pass^k
// ---------------------------------------------------------------------

/// Re-grade one run's stored answers against the dataset and produce a
/// per-task pass vector (true = solved, score >= 0.999). Tasks the run
/// never answered count as failed samples.
fn pass_vector(fixtures: &[Fixture], responses: &[ResponseRecord]) -> BTreeMap<String, bool> {
    let by_id: BTreeMap<&str, &Fixture> = fixtures.iter().map(|f| (f.id(), f)).collect();
    let mut out: BTreeMap<String, bool> = BTreeMap::new();
    for r in responses {
        let Some(f) = by_id.get(r.task_id.as_str()) else {
            continue;
        };
        let solved = match (&f, &r.answer) {
            (Fixture::Write(w), TaskAnswer::Script(a)) => {
                bench_core::grade_write(w, &a.script).score >= 0.999
            }
            (Fixture::Optimize(o), TaskAnswer::Script(a)) => {
                bench_core::grade_optimize(o, &a.script).weight_score >= 0.999
            }
            (Fixture::Identify(i), TaskAnswer::Identify(a)) => {
                bench_core::grade_identify(i, a).score >= 0.999
            }
            (Fixture::Tree(t), TaskAnswer::Descriptor(a)) => {
                bench_core::grade_tree(t, &a.descriptor).weight_score >= 0.999
            }
            _ => false,
        };
        out.insert(r.task_id.clone(), solved);
    }
    out
}

fn n_choose_k(n: usize, k: usize) -> u64 {
    if k > n {
        return 0;
    }
    let mut c = 1u64;
    for i in 0..k {
        c = c * (n - i) as u64 / (i + 1) as u64;
    }
    c
}

/// pass@k and pass^k across N runs of the same model against one
/// dataset. Run order defines the prefix for pass^k. A task missing
/// from a run counts as a failed sample in that run.
pub fn passk(dataset: &Path, runs: &[(String, std::path::PathBuf)], out: &Path) -> Result<()> {
    if runs.len() < 2 {
        bail!(
            "passk needs at least 2 runs to be meaningful (got {})",
            runs.len()
        );
    }
    let fixtures = load_dataset(dataset)?;
    let n = runs.len();

    // Per task: ordered sample vector across runs.
    let mut samples: BTreeMap<String, Vec<bool>> = BTreeMap::new();
    for (label, dir) in runs {
        let text = std::fs::read_to_string(dir.join("responses.jsonl"))
            .with_context(|| format!("read {}/responses.jsonl", dir.display()))?;
        let mut records: Vec<ResponseRecord> = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            records.push(
                serde_json::from_str(line)
                    .with_context(|| format!("parse response in {}", dir.display()))?,
            );
        }
        let pv = pass_vector(&fixtures, &records);
        for f in &fixtures {
            samples
                .entry(f.id().to_string())
                .or_default()
                .push(pv.get(f.id()).copied().unwrap_or(false));
        }
        println!("{label}: {} answered / {}", pv.len(), fixtures.len());
    }

    let mut md = String::new();
    md.push_str(&format!(
        "# pass@k / pass^k — {} runs, {} tasks\n\n",
        n,
        samples.len()
    ));
    md.push_str(
        "pass@k = P(>=1 of k samples solves) [unbiased estimator]; \
                 pass^k = P(first k samples all solve).\n\n",
    );
    md.push_str("| k | pass@k | pass^k |\n|---|---|---|\n");
    for k in 1..=n {
        let mut at_k = 0.0f64;
        let mut hat_k = 0.0f64;
        let mut count = 0usize;
        for v in samples.values() {
            let c = v.iter().filter(|x| **x).count();
            // Unbiased pass@k: 1 - C(n-c, k)/C(n, k).
            let denom = n_choose_k(n, k);
            let est = if denom == 0 {
                0.0
            } else {
                1.0 - n_choose_k(n - c, k) as f64 / denom as f64
            };
            at_k += est;
            // pass^k: all of the first k.
            hat_k += v[..k].iter().all(|x| *x) as u8 as f64;
            count += 1;
        }
        md.push_str(&format!(
            "| {k} | {:.4} | {:.4} |\n",
            at_k / count as f64,
            hat_k / count as f64
        ));
    }
    md.push('\n');

    // Per-kind cuts at k = n (the fully-observed pass rate).
    md.push_str(
        "## By kind (pass@n / pass^n)\n\n| kind | n | pass@n | pass^n |\n|---|---|---|---|\n",
    );
    let by_id: BTreeMap<&str, &Fixture> = fixtures.iter().map(|f| (f.id(), f)).collect();
    let mut kinds: BTreeMap<&str, (usize, f64, f64)> = BTreeMap::new();
    for (id, v) in &samples {
        let Some(f) = by_id.get(id.as_str()) else {
            continue;
        };
        let c = v.iter().filter(|x| **x).count();
        let any = c >= 1;
        let all = v.iter().all(|x| *x);
        let e = kinds.entry(kind_of(f)).or_insert((0, 0.0, 0.0));
        e.0 += 1;
        e.1 += any as u8 as f64;
        e.2 += all as u8 as f64;
    }
    for (k, (n, a, al)) in &kinds {
        md.push_str(&format!(
            "| {k} | {n} | {:.4} | {:.4} |\n",
            a / *n as f64,
            al / *n as f64
        ));
    }
    md.push('\n');

    // Tier cuts (write/optimize only).
    md.push_str(
        "## By tier (pass@n / pass^n)\n\n| kind+tier | n | pass@n | pass^n |\n|---|---|---|---|\n",
    );
    let mut tiers: BTreeMap<String, (usize, f64, f64)> = BTreeMap::new();
    for (id, v) in &samples {
        let Some(f) = by_id.get(id.as_str()) else {
            continue;
        };
        let Some(t) = tier_of(f) else { continue };
        let key = format!("{}:{:?}", kind_of(f), t).to_lowercase();
        let c = v.iter().filter(|x| **x).count();
        let e = tiers.entry(key).or_insert((0, 0.0, 0.0));
        e.0 += 1;
        e.1 += (c >= 1) as u8 as f64;
        e.2 += v.iter().all(|x| *x) as u8 as f64;
    }
    for (k, (n, a, al)) in &tiers {
        md.push_str(&format!(
            "| {k} | {n} | {:.4} | {:.4} |\n",
            a / *n as f64,
            al / *n as f64
        ));
    }
    md.push('\n');

    std::fs::write(out, md).with_context(|| format!("write {}", out.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("btc-bench-report-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Two tiny runs against one dataset: one perfect, one with a
    /// wrong answer. pass@1 == pass^1 == mean; pass@2 == 1 for the
    /// mixed task (any-of-2), pass^2 == 0 for it.
    #[test]
    fn passk_two_runs() {
        let dir = tmpdir("passk");
        crate::gen_dataset(
            &dir,
            &bench_gen::fixtures::GenParams {
                seed: 5,
                write: 2,
                optimize: 0,
                identify: 0,
                ..bench_gen::fixtures::GenParams::default()
            },
            "test",
        )
        .unwrap();
        let fixtures = load_dataset(&dir).unwrap();
        let ids: Vec<String> = fixtures.iter().map(|f| f.id().to_string()).collect();

        let perfect_answer = |f: &Fixture| match f {
            Fixture::Write(w) => w.reference_script_hex.clone(),
            Fixture::Optimize(o) => o.optimal_script_hex.clone(),
            Fixture::Identify(_) | Fixture::Tree(_) => unreachable!(),
        };

        for (label, sabotage) in [("a", false), ("b", true)] {
            let rd = dir.join(label);
            std::fs::create_dir_all(&rd).unwrap();
            let mut text = String::new();
            for f in &fixtures {
                let mut script = perfect_answer(f);
                if sabotage && f.id() == ids[0] {
                    script = "51".into();
                }
                text.push_str(
                    &serde_json::json!({
                        "task_id": f.id(),
                        "answer": {"task": "script", "script": script},
                    })
                    .to_string(),
                );
                text.push('\n');
            }
            std::fs::write(rd.join("responses.jsonl"), text).unwrap();
        }

        let out = dir.join("passk.md");
        passk(
            &dir,
            &[("a".into(), dir.join("a")), ("b".into(), dir.join("b"))],
            &out,
        )
        .unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        // Task 0 passes in run a only; task 1 in both.
        // Task 0 samples [pass(a), fail(b)] → c=1: pass@1 = 1 - 1/2 =
        // 0.5. Task 1 [pass, pass] → 1.0. Mean = 0.75. Run a is the
        // prefix, so pass^1 = 1.0; pass@2 = 1.0 (all tasks have >=1
        // pass); pass^2 = 0.5 (only task 1 passes in both).
        assert!(text.contains("| 1 | 0.7500 | 1.0000 |"), "{text}");
        assert!(text.contains("| 2 | 1.0000 | 0.5000 |"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_renders_from_graded_run() {
        let dir = tmpdir("report");
        crate::gen_dataset(
            &dir,
            &bench_gen::fixtures::GenParams {
                seed: 5,
                write: 2,
                optimize: 1,
                identify: 0,
                ..bench_gen::fixtures::GenParams::default()
            },
            "test",
        )
        .unwrap();
        let fixtures = load_dataset(&dir).unwrap();
        // Answer everything perfectly except one sabotage.
        let mut text = String::new();
        for (i, f) in fixtures.iter().enumerate() {
            let script = match f {
                Fixture::Write(w) => {
                    if i == 0 {
                        "51".into()
                    } else {
                        w.reference_script_hex.clone()
                    }
                }
                Fixture::Optimize(o) => o.optimal_script_hex.clone(),
                Fixture::Identify(_) | Fixture::Tree(_) => unreachable!(),
            };
            text.push_str(
                &serde_json::json!({
                    "task_id": f.id(),
                    "answer": {"task": "script", "script": script},
                })
                .to_string(),
            );
            text.push('\n');
        }
        let rd = dir.join("run");
        std::fs::create_dir_all(&rd).unwrap();
        std::fs::write(rd.join("responses.jsonl"), text).unwrap();
        let records = crate::load_responses(&rd.join("responses.jsonl")).unwrap();
        let (scores, _) = crate::grade(&fixtures, &records, None, 0.5, false).unwrap();
        std::fs::create_dir_all(rd.join("graded")).unwrap();
        std::fs::write(
            rd.join("graded").join("results.json"),
            serde_json::to_string_pretty(&scores).unwrap(),
        )
        .unwrap();
        let out = dir.join("report.md");
        report(&dir, &[("model".into(), rd.clone())], &out).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains("| model | write |"), "headline table: {text}");
        assert!(text.contains("wrong semantics"), "taxonomy: {text}");
        assert!(text.contains("Tier breakdown"), "tier section");
        assert!(
            text.contains("Optimize weight-score distribution"),
            "curve section"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn n_choose_k_matches_pascal() {
        assert_eq!(n_choose_k(5, 0), 1);
        assert_eq!(n_choose_k(5, 2), 10);
        assert_eq!(n_choose_k(5, 5), 1);
        assert_eq!(n_choose_k(3, 4), 0);
    }
}
