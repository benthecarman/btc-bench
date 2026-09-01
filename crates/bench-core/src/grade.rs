//! Graders for the three task types. All judge-free.

use bitcoin::{ScriptBuf, XOnlyPublicKey};
use miniscript::descriptor::{TapTree, Tr};
use miniscript::ScriptContext;
use miniscript::{Descriptor, Legacy, Miniscript, Segwitv0, Tap};

use crate::answer::parse_script_answer;
use crate::oracle::{check_equivalence, Verdict};
use crate::task::{ContextKind, IdentifyAnswer, IdentifyFixture, OptimizeFixture, WriteFixture};

/// Deterministic unspendable internal key for taproot weight wrapping.
fn dummy_internal_key() -> XOnlyPublicKey {
    XOnlyPublicKey::from_slice(&[0x51; 32]).expect("32 bytes is a valid x-only key")
}

/// Script size (bytes) and max satisfaction weight of the inner script
/// wrapped in its natural descriptor: `sh(ms)`, `wsh(ms)`, or
/// `tr(<dummy>, ms)` with a single leaf.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Weights {
    pub size: usize,
    pub weight: usize,
}

fn legacy_weights(ms: Miniscript<bitcoin::PublicKey, Legacy>) -> Weights {
    let size = ms.encode().len();
    let weight = Descriptor::new_sh(ms)
        .expect("decoded legacy miniscript wraps in sh")
        .max_weight_to_satisfy()
        .expect("max satisfaction weight exists for in-range miniscript")
        .to_wu() as usize;
    Weights { size, weight }
}

fn segwit_weights(ms: Miniscript<bitcoin::PublicKey, Segwitv0>) -> Weights {
    let size = ms.encode().len();
    let weight = Descriptor::new_wsh(ms)
        .expect("decoded segwit miniscript wraps in wsh")
        .max_weight_to_satisfy()
        .expect("max satisfaction weight exists for in-range miniscript")
        .to_wu() as usize;
    Weights { size, weight }
}

fn tap_weights(ms: Miniscript<XOnlyPublicKey, Tap>) -> Weights {
    let size = ms.encode().len();
    let weight = Tr::new(dummy_internal_key(), Some(TapTree::leaf(ms)))
        .expect("decoded tap miniscript wraps in a single-leaf tr")
        .max_weight_to_satisfy()
        .expect("max satisfaction weight exists for in-range miniscript")
        .to_wu() as usize;
    Weights { size, weight }
}

/// Decode a script in the given context and compute its weights.
pub fn weights_for(kind: ContextKind, script: &ScriptBuf) -> Result<Weights, String> {
    match kind {
        ContextKind::Legacy => {
            let ms: Miniscript<bitcoin::PublicKey, Legacy> =
                Miniscript::decode_consensus(script.as_script()).map_err(|e| e.to_string())?;
            Ok(legacy_weights(ms))
        }
        ContextKind::SegwitV0 => {
            let ms: Miniscript<bitcoin::PublicKey, Segwitv0> =
                Miniscript::decode_consensus(script.as_script()).map_err(|e| e.to_string())?;
            Ok(segwit_weights(ms))
        }
        ContextKind::Tap => {
            let ms: Miniscript<XOnlyPublicKey, Tap> =
                Miniscript::decode_consensus(script.as_script()).map_err(|e| e.to_string())?;
            Ok(tap_weights(ms))
        }
    }
}

/// Insanity categories detected on a decodable script (miniscript's
/// analyzable predicates: safety, malleability, resource limits,
/// repeated keys, timelock mixing, raw pkh). Empty for a sane script or
/// one that fails to decode (decode failures carry their own reason).
/// Purely informational unless the grader runs in standard mode.
pub fn lint_report(kind: ContextKind, script: &ScriptBuf) -> Vec<&'static str> {
    fn lints<Ctx: ScriptContext>(script: &ScriptBuf) -> Vec<&'static str> {
        let Ok(ms) = Miniscript::<Ctx::Key, Ctx>::decode_consensus(script.as_script()) else {
            return Vec::new();
        };
        // Finding texts are rust-miniscript's own AnalysisError
        // Display strings, verbatim (typos included): a model trained
        // against this feedback meets the same words in real work.
        let mut out = Vec::new();
        if !ms.requires_sig() {
            out.push("All spend paths must require a signature");
        }
        if !ms.is_non_malleable() {
            out.push("Miniscript is malleable");
        }
        if !ms.within_resource_limits() {
            out.push(
                "At least one spend path exceeds the resource limits(stack depth/satisfaction size..)",
            );
        }
        if ms.has_repeated_keys() {
            out.push("Miniscript contains repeated pubkeys or pubkeyhashes");
        }
        if ms.has_mixed_timelocks() {
            out.push("Contains a combination of heightlock and timelock");
        }
        if ms.contains_raw_pkh() {
            out.push("Miniscript contains raw pkh");
        }
        out
    }
    match kind {
        ContextKind::Legacy => lints::<Legacy>(script),
        ContextKind::SegwitV0 => lints::<Segwitv0>(script),
        ContextKind::Tap => lints::<Tap>(script),
    }
}

#[derive(Clone, Debug)]
pub struct WriteResult {
    pub verdict: Verdict,
    pub score: f64,
    /// Parse/decode failure detail when score is 0.
    pub reason: Option<String>,
    /// Insanity findings on the decoded candidate (see [`lint_report`]).
    /// Reported, never scored, outside standard mode.
    pub lint: Vec<String>,
}

/// Task 1: parse, decode-gate, prove equivalence.
pub fn grade_write(fixture: &WriteFixture, answer: &str) -> WriteResult {
    let candidate = match parse_script_answer(answer) {
        Ok(s) => s,
        Err(e) => {
            return WriteResult {
                verdict: Verdict::InvalidScript(e.to_string()),
                score: 0.0,
                reason: Some(e.to_string()),
                lint: Vec::new(),
            }
        }
    };
    let reference =
        ScriptBuf::from_hex(&fixture.reference_script_hex).expect("fixture hex is valid");
    let verdict = check_equivalence(fixture.context, &reference, &candidate);
    let score = if verdict.is_equivalent() { 1.0 } else { 0.0 };
    let lint = lint_report(fixture.context, &candidate)
        .into_iter()
        .map(str::to_string)
        .collect();
    WriteResult {
        reason: if score == 0.0 {
            Some(verdict.to_string())
        } else {
            None
        },
        lint,
        verdict,
        score,
    }
}

#[derive(Clone, Debug)]
pub struct OptimizeResult {
    pub verdict: Verdict,
    /// Headline: (baseline − candidate) / (baseline − optimal), clamped.
    pub weight_score: f64,
    /// Secondary: same curve on script byte size.
    pub size_score: f64,
    pub candidate: Option<Weights>,
    pub reason: Option<String>,
    /// Insanity findings on the decoded candidate (see [`lint_report`]).
    pub lint: Vec<String>,
}

fn curve(base: usize, cand: usize, optimal: usize) -> f64 {
    if base <= optimal {
        return if cand <= optimal { 1.0 } else { 0.0 };
    }
    let v = (base as f64 - cand as f64) / (base as f64 - optimal as f64);
    v.clamp(0.0, 1.0)
}

/// Task 2: equivalence gate + weight/size improvement curve.
pub fn grade_optimize(fixture: &OptimizeFixture, answer: &str) -> OptimizeResult {
    let candidate = match parse_script_answer(answer) {
        Ok(s) => s,
        Err(e) => {
            return OptimizeResult {
                verdict: Verdict::InvalidScript(e.to_string()),
                weight_score: 0.0,
                size_score: 0.0,
                candidate: None,
                reason: Some(e.to_string()),
                lint: Vec::new(),
            }
        }
    };
    let reference = ScriptBuf::from_hex(&fixture.optimal_script_hex).expect("fixture hex is valid");
    let verdict = check_equivalence(fixture.context, &reference, &candidate);
    if !verdict.is_equivalent() {
        let reason = verdict.to_string();
        return OptimizeResult {
            verdict,
            weight_score: 0.0,
            size_score: 0.0,
            candidate: None,
            reason: Some(reason),
            lint: lint_report(fixture.context, &candidate)
                .into_iter()
                .map(str::to_string)
                .collect(),
        };
    }
    let cand_weights = match weights_for(fixture.context, &candidate) {
        Ok(w) => w,
        Err(e) => {
            return OptimizeResult {
                verdict: Verdict::InvalidScript(e.clone()),
                weight_score: 0.0,
                size_score: 0.0,
                candidate: None,
                reason: Some(e),
                lint: Vec::new(),
            }
        }
    };
    OptimizeResult {
        weight_score: curve(
            fixture.baseline_weight,
            cand_weights.weight,
            fixture.optimal_weight,
        ),
        size_score: curve(
            fixture.baseline_size,
            cand_weights.size,
            fixture.optimal_size,
        ),
        candidate: Some(cand_weights),
        verdict,
        reason: None,
        lint: lint_report(fixture.context, &candidate)
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

#[derive(Clone, Debug)]
pub struct TreeResult {
    pub verdict: Verdict,
    /// Headline: the optimize curve between the single-leaf baseline
    /// and the compiler tree's weight, gated on equivalence. Beating
    /// the compiler (possible: Huffman optimizes expected cost, the
    /// metric is worst-case) clamps to 1.0.
    pub weight_score: f64,
    /// Max satisfaction weight of the candidate descriptor, when it
    /// parsed and proved equivalent.
    pub candidate_weight: Option<usize>,
    pub reason: Option<String>,
    /// Union of insanity findings across the candidate's leaves.
    pub lint: Vec<String>,
}

/// Unwrap singleton brace groups: `{X}` with no top-level comma
/// becomes `X`, applied to fixpoint. Models frequently write
/// `tr(KEY,{leaf})` for a single-leaf tree — invalid per the strict
/// grammar (braces demand exactly two children) but with exactly one
/// possible meaning; 28 answers in one run were byte-perfect inside
/// the braces and scored 0 on this alone. Flat n-ary groups
/// (`{a,b,c}`) are left untouched: multiple pairings exist, so they
/// stay genuine errors.
fn unwrap_singleton_braces(text: &str) -> String {
    let mut cur = text.to_string();
    loop {
        let bytes = cur.as_bytes();
        // (open index, saw top-level comma, paren depth at open). A
        // comma only counts against the group when it sits at the
        // group's own paren depth — commas inside fragment parens
        // (and_v(x,y)) are argument separators, not tree branches.
        let mut stack: Vec<(usize, bool, u32)> = Vec::new();
        let mut paren: u32 = 0;
        let mut change: Option<(usize, usize)> = None;
        for (i, b) in bytes.iter().enumerate() {
            match b {
                b'(' => paren += 1,
                b')' => paren = paren.saturating_sub(1),
                b'{' => stack.push((i, false, paren)),
                b',' => {
                    if let Some(top) = stack.last_mut() {
                        if paren == top.2 {
                            top.1 = true;
                        }
                    }
                }
                b'}' => {
                    if let Some((open, saw_comma, _)) = stack.pop() {
                        if !saw_comma {
                            change = Some((open, i));
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        match change {
            Some((open, close)) => {
                cur = format!(
                    "{}{}{}",
                    &cur[..open],
                    &cur[open + 1..close],
                    &cur[close + 1..]
                );
            }
            None => return cur,
        }
    }
}

/// Parse a `tr(...)` descriptor answer. Descriptor checksums are
/// accepted but not required; singleton brace groups are unwrapped
/// first (see [`unwrap_singleton_braces`]).
pub fn parse_tr_answer(answer: &str) -> Result<miniscript::descriptor::Tr<XOnlyPublicKey>, String> {
    let text = answer.trim().trim_matches('`').trim();
    let text = unwrap_singleton_braces(text);
    let desc: Descriptor<XOnlyPublicKey> = text
        .parse()
        .map_err(|e| format!("not a valid descriptor: {e}"))?;
    match desc {
        Descriptor::Tr(tr) => Ok(tr),
        _ => Err("the answer must be a tr() descriptor".to_string()),
    }
}

/// Balanced truth-table agreement of a tree answer against the
/// reference descriptor, unspendable key pinned false. None when the
/// answer does not parse as a tr() descriptor.
pub fn tree_agreement(fixture: &crate::task::TreeFixture, answer: &str) -> Option<f64> {
    use miniscript::policy::Liftable as _;
    let tr = parse_tr_answer(answer).ok()?;
    let reference: Descriptor<XOnlyPublicKey> = fixture
        .reference_descriptor
        .parse()
        .expect("fixture descriptor is valid");
    let sem_ref = reference.lift().expect("fixture descriptor lifts");
    let sem_cand = tr.lift().ok()?;
    crate::oracle::agreement_semantic(&sem_ref, &sem_cand, Some(&fixture.unspendable_key))
}

/// Task 4: parse a tr() descriptor, prove the lifted semantics
/// equivalent to the reference (with the unspendable key pinned
/// false on both sides), then score tree quality on the weight curve.
pub fn grade_tree(fixture: &crate::task::TreeFixture, answer: &str) -> TreeResult {
    use miniscript::policy::Liftable as _;
    let tr = match parse_tr_answer(answer) {
        Ok(t) => t,
        Err(e) => {
            return TreeResult {
                verdict: Verdict::InvalidScript(e.clone()),
                weight_score: 0.0,
                candidate_weight: None,
                reason: Some(e),
                lint: Vec::new(),
            }
        }
    };
    let mut lint: Vec<String> = Vec::new();
    for leaf in tr.leaves() {
        for l in lint_report(ContextKind::Tap, &leaf.miniscript().encode()) {
            let l = l.to_string();
            if !lint.contains(&l) {
                lint.push(l);
            }
        }
    }
    let reference: Descriptor<XOnlyPublicKey> = fixture
        .reference_descriptor
        .parse()
        .expect("fixture descriptor is valid");
    let (sem_ref, sem_cand) = match (reference.lift(), tr.lift()) {
        (Ok(r), Ok(c)) => (r, c),
        (_, Err(e)) => {
            let e = format!("descriptor failed to lift: {e}");
            return TreeResult {
                verdict: Verdict::InvalidScript(e.clone()),
                weight_score: 0.0,
                candidate_weight: None,
                reason: Some(e),
                lint,
            };
        }
        (Err(e), _) => unreachable!("fixture descriptor lifts: {e}"),
    };
    let verdict =
        crate::oracle::check_semantic(&sem_ref, &sem_cand, Some(&fixture.unspendable_key));
    if !verdict.is_equivalent() {
        let reason = verdict.to_string();
        return TreeResult {
            verdict,
            weight_score: 0.0,
            candidate_weight: None,
            reason: Some(reason),
            lint,
        };
    }
    let weight = match Descriptor::Tr(tr).max_weight_to_satisfy() {
        Ok(w) => w.to_wu() as usize,
        Err(e) => {
            let e = format!("satisfaction weight not computable: {e}");
            return TreeResult {
                verdict: Verdict::InvalidScript(e.clone()),
                weight_score: 0.0,
                candidate_weight: None,
                reason: Some(e),
                lint,
            };
        }
    };
    TreeResult {
        verdict,
        weight_score: curve(fixture.baseline_weight, weight, fixture.reference_weight),
        candidate_weight: Some(weight),
        reason: None,
        lint,
    }
}

#[derive(Clone, Debug)]
pub struct IdentifyResult {
    pub label_correct: bool,
    pub score: f64,
}

/// Task 3: label-only, binary. Parameters were removed deliberately:
/// param names were a naming-convention tax (a model that reads 980
/// off the CSV but says "delay" instead of "to_self_delay" knows the
/// script; docking it measures vocabulary recall, not comprehension).
/// Right family or wrong family, nothing else.
pub fn grade_identify(fixture: &IdentifyFixture, answer: &IdentifyAnswer) -> IdentifyResult {
    let label_correct = answer
        .label
        .trim()
        .eq_ignore_ascii_case(fixture.family.trim());
    IdentifyResult {
        label_correct,
        score: if label_correct { 1.0 } else { 0.0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{KeyVar, Tier};
    use std::str::FromStr;

    fn fix(reference_hex: String) -> WriteFixture {
        WriteFixture {
            id: "t1-0001".into(),
            tier: Tier::Easy,
            context: ContextKind::SegwitV0,
            spec_en: "unused".into(),
            spec_family: 0,
            atoms: 0,
            keys: vec![],
            reference_policy: String::new(),
            reference_miniscript: String::new(),
            reference_script_hex: reference_hex,
            hash_preimages: Default::default(),
        }
    }

    fn ms_hex(s: &str) -> String {
        miniscript::Miniscript::<bitcoin::PublicKey, Segwitv0>::from_str(s)
            .unwrap()
            .encode()
            .to_hex_string()
    }

    #[test]
    fn grade_write_pass_and_fail() {
        let reference = ms_hex("and_v(v:pk(0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798),pk(02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5))");
        let f = fix(reference.clone());
        assert_eq!(grade_write(&f, &reference).score, 1.0);
        assert_eq!(grade_write(&f, "51").score, 0.0);
        assert_eq!(grade_write(&f, "not hex!").score, 0.0);
    }

    #[test]
    fn lint_report_flags_insane_but_not_sane() {
        use bitcoin::hex::FromHex;
        // Sane: compiled and_v.
        let sane = ms_hex("and_v(v:pk(0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798),pk(02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5))");
        let script = ScriptBuf::from_hex(&sane).unwrap();
        assert!(lint_report(ContextKind::SegwitV0, &script).is_empty());
        // Malleable: the alloy-spec vector or_b(un:multi, al:older) —
        // the older arm has no non-malleable dissatisfaction.
        let malleable = miniscript::Miniscript::<bitcoin::PublicKey, Segwitv0>::from_str_insane(
            "or_b(un:multi(2,03d01115d548e7561b15c38f004d734633687cf4419620095bc5b0f47070afe85a,02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5),al:older(16))",
        )
        .unwrap()
        .encode()
        .to_hex_string();
        let script = ScriptBuf::from_hex(&malleable).unwrap();
        let lints = lint_report(ContextKind::SegwitV0, &script);
        assert!(
            lints.iter().any(|l| *l == "Miniscript is malleable"),
            "expected miniscript's own malleability text, got {lints:?}"
        );
        // Unsafe: OP_1 decodes as Trivial, spends with no signature.
        let script = ScriptBuf::from_hex("51").unwrap();
        let lints = lint_report(ContextKind::SegwitV0, &script);
        assert!(
            lints
                .iter()
                .any(|l| *l == "All spend paths must require a signature"),
            "expected miniscript's own sigless text, got {lints:?}"
        );
        // Undecodable garbage carries no lint (the gate reports it).
        let script = ScriptBuf::from_hex("6a").unwrap();
        assert!(lint_report(ContextKind::SegwitV0, &script).is_empty());
    }

    #[test]
    fn optimize_curve_endpoints() {
        let optimal = ms_hex("and_v(v:pk(0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798),pk(02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5))");
        // Fixture numbers derived from the real optimal weights: baseline
        // is exactly 2x optimal, so candidate == optimal scores 1.0 and
        // candidate == baseline scores 0.0.
        let opt_script = ScriptBuf::from_hex(&optimal).unwrap();
        let w = weights_for(ContextKind::SegwitV0, &opt_script).unwrap();
        let f = OptimizeFixture {
            id: "t2-0001".into(),
            tier: Tier::Easy,
            context: ContextKind::SegwitV0,
            spec_en: String::new(),
            spec_family: 0,
            atoms: 0,
            keys: vec![KeyVar {
                label: "A".into(),
                pubkey: "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798".into(),
            }],
            baseline_script_hex: optimal.clone(),
            baseline_size: 2 * w.size,
            baseline_weight: 2 * w.weight,
            optimal_script_hex: optimal.clone(),
            optimal_size: w.size,
            optimal_weight: w.weight,
            reference_policy: String::new(),
            reference_miniscript: String::new(),
            hash_preimages: Default::default(),
        };
        // Candidate == optimal bytes -> full credit.
        let r = grade_optimize(&f, &optimal);
        assert_eq!(r.weight_score, 1.0);
        assert_eq!(r.size_score, 1.0);
        // Non-equivalent answer zeroes out.
        let bad = grade_optimize(&f, "51");
        assert_eq!(bad.weight_score, 0.0);
        assert!(!bad.verdict.is_equivalent());
    }

    #[test]
    fn grade_tree_curve_and_nums_pinning() {
        use crate::task::TreeFixture;
        use std::str::FromStr as _;
        let nums = "50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0";
        let (a, b, c) = (
            "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5",
            "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
        );
        // or(pk(A), or(and(pk(B), older(144)), and(pk(C), after(700000))))
        let policy = format!("or(pk({a}),or(and(pk({b}),older(144)),and(pk({c}),after(700000))))");
        let concrete = miniscript::policy::Concrete::<XOnlyPublicKey>::from_str(&policy).unwrap();
        let nums_key = XOnlyPublicKey::from_str(nums).unwrap();
        let reference = concrete.compile_tr(Some(nums_key)).unwrap();
        let reference_weight = reference.max_weight_to_satisfy().unwrap().to_wu() as usize;
        let single = concrete.compile::<Tap>().unwrap();
        let baseline =
            miniscript::Descriptor::new_tr(nums_key, Some(TapTree::leaf(single))).unwrap();
        let baseline_weight = baseline.max_weight_to_satisfy().unwrap().to_wu() as usize;
        assert!(baseline_weight > reference_weight, "task would be vacuous");
        let f = TreeFixture {
            id: "t4-0000".into(),
            tier: Tier::Easy,
            spec_en: String::new(),
            spec_family: 0,
            atoms: 3,
            keys: vec![],
            unspendable_key: nums.into(),
            reference_policy: policy.clone(),
            reference_descriptor: reference.to_string(),
            reference_weight,
            baseline_descriptor: baseline.to_string(),
            baseline_weight,
            hash_preimages: Default::default(),
        };
        // The compiler's own tree: full credit. Note compile_tr
        // extracts pk(A) as the internal key here.
        let perfect = grade_tree(&f, &reference.to_string());
        assert!(perfect.verdict.is_equivalent());
        assert_eq!(perfect.weight_score, 1.0);
        // The single-leaf baseline: equivalent, zero on the curve —
        // and NUMS pinning makes it equivalent even though the
        // reference's lift has pk(A) as internal key while the
        // baseline's lift has the NUMS atom instead.
        let base = grade_tree(&f, &baseline.to_string());
        assert!(base.verdict.is_equivalent(), "{:?}", base.reason);
        assert_eq!(base.weight_score, 0.0);
        // A hand-built two-leaf tree with NUMS internal: equivalent,
        // partial-or-better credit.
        let hand = format!(
            "tr({nums},{{pk({a}),{{and_v(v:pk({b}),older(144)),and_v(v:pk({c}),after(700000))}}}})"
        );
        let r = grade_tree(&f, &hand);
        assert!(r.verdict.is_equivalent(), "{:?}", r.reason);
        assert!(r.weight_score > 0.0, "{:?}", r);
        // A tree that drops a branch: not equivalent.
        let wrong = format!("tr({nums},pk({a}))");
        let r = grade_tree(&f, &wrong);
        assert!(!r.verdict.is_equivalent());
        assert_eq!(r.weight_score, 0.0);
        // Not a descriptor at all / not tr(): rejected with reasons.
        assert!(grade_tree(&f, "garbage").reason.is_some());
        let wsh = format!("wsh(pk(02{b}))");
        assert!(grade_tree(&f, &wsh).reason.is_some());
        // Agreement: perfect = 1.0; dropped-branch sits in (0.5, 1).
        assert_eq!(tree_agreement(&f, &reference.to_string()), Some(1.0));
        let g = tree_agreement(&f, &wrong).unwrap();
        assert!(g > 0.5 && g < 1.0, "{g}");
    }

    #[test]
    fn singleton_braces_unwrap_but_nary_stays_invalid() {
        // {X} with no top-level comma unwraps, at any depth, to
        // fixpoint; real pairs and n-ary groups are untouched.
        assert_eq!(unwrap_singleton_braces("tr(K,{pk(a)})"), "tr(K,pk(a))");
        // The real-world shape: fragment-argument commas inside parens
        // must not read as tree branches (the bug the first sweep
        // exposed: 112 singleton wraps stayed rejected because of the
        // comma in and_v(x,y)).
        assert_eq!(
            unwrap_singleton_braces("tr(K,{and_v(v:pk(a),older(841))})"),
            "tr(K,and_v(v:pk(a),older(841)))"
        );
        assert_eq!(
            unwrap_singleton_braces("tr(K,{{pk(a),pk(b)}})"),
            "tr(K,{pk(a),pk(b)})"
        );
        assert_eq!(
            unwrap_singleton_braces("tr(K,{pk(a),{pk(b)}})"),
            "tr(K,{pk(a),pk(b)})"
        );
        assert_eq!(
            unwrap_singleton_braces("tr(K,{pk(a),pk(b)})"),
            "tr(K,{pk(a),pk(b)})"
        );
        assert_eq!(unwrap_singleton_braces("{a,b,c}"), "{a,b,c}");
        // End-to-end: a singleton-wrapped single-leaf answer parses
        // and grades like the bare form.
        let a = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let b = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";
        let wrapped = format!("tr({a},{{pk({b})}})");
        assert!(
            parse_tr_answer(&wrapped).is_ok(),
            "singleton brace must parse"
        );
        // Flat ternary is still rejected (ambiguous pairing).
        let c = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
        let nary = format!("tr({a},{{pk({a}),pk({b}),pk({c})}})");
        assert!(parse_tr_answer(&nary).is_err(), "n-ary group stays invalid");
    }

    #[test]
    fn identify_is_label_only_and_binary() {
        use crate::task::ParamValue;
        use std::collections::BTreeMap;
        let mut params = BTreeMap::new();
        params.insert("k".to_string(), ParamValue::Int(2));
        let f = IdentifyFixture {
            id: "t3-0001".into(),
            family: "p2wsh_multisig".into(),
            params,
            spk_hex: "0020..".into(),
            inner_script_hex: None,
        };
        // Case-insensitive label match is full marks; fixture params
        // are metadata and never graded.
        let hit = grade_identify(
            &f,
            &IdentifyAnswer {
                label: "P2WSH_Multisig".into(),
            },
        );
        assert!(hit.label_correct);
        assert_eq!(hit.score, 1.0);
        let miss = grade_identify(
            &f,
            &IdentifyAnswer {
                label: "p2wpkh".into(),
            },
        );
        assert!(!miss.label_correct);
        assert_eq!(miss.score, 0.0);
    }
}
