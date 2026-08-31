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
        let mut out = Vec::new();
        if !ms.requires_sig() {
            out.push("unsafe: a spend path requires no signature");
        }
        if !ms.is_non_malleable() {
            out.push("malleable");
        }
        if !ms.within_resource_limits() {
            out.push("a spend path exceeds consensus resource limits");
        }
        if ms.has_repeated_keys() {
            out.push("repeated public keys");
        }
        if ms.has_mixed_timelocks() {
            out.push("unspendable path: mixes height-based and time-based timelocks");
        }
        if ms.contains_raw_pkh() {
            out.push("raw pkh fragment without a corresponding key");
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
            Some(format!("{verdict:?}"))
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
pub struct IdentifyResult {
    pub label_correct: bool,
    /// Fraction of parameters answered correctly (Jaccard-style: extra
    /// invented params count against the denominator). 1.0 when the
    /// fixture has no params.
    pub param_fraction: f64,
    pub score: f64,
}

/// Task 3: flat label + per-parameter credit. A wrong label scores 0.
/// A correct label scores `partial_credit` plus the remaining
/// `1 - partial_credit` scaled by the fraction of correct parameters,
/// so exact params still reach 1.0 and each correct param earns
/// credit. Extra params not in the fixture dilute the fraction, so
/// spamming keys never pays.
pub fn grade_identify(
    fixture: &IdentifyFixture,
    answer: &IdentifyAnswer,
    partial_credit: f64,
) -> IdentifyResult {
    let label_correct = answer
        .label
        .trim()
        .eq_ignore_ascii_case(fixture.family.trim());
    let matched = fixture
        .params
        .iter()
        .filter(|(k, v)| answer.params.get(*k) == Some(v))
        .count();
    let extra = answer
        .params
        .keys()
        .filter(|k| !fixture.params.contains_key(*k))
        .count();
    let denom = fixture.params.len() + extra;
    let param_fraction = if denom == 0 {
        1.0
    } else {
        matched as f64 / denom as f64
    };
    let score = if label_correct {
        partial_credit + (1.0 - partial_credit) * param_fraction
    } else {
        0.0
    };
    IdentifyResult {
        label_correct,
        param_fraction,
        score,
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
            lints.iter().any(|l| l.contains("malleable")),
            "expected malleable finding, got {lints:?}"
        );
        // Unsafe: OP_1 decodes as Trivial, spends with no signature.
        let script = ScriptBuf::from_hex("51").unwrap();
        let lints = lint_report(ContextKind::SegwitV0, &script);
        assert!(
            lints.iter().any(|l| l.contains("no signature")),
            "expected unsafe finding, got {lints:?}"
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
    fn identify_partial_credit() {
        use crate::task::ParamValue;
        use std::collections::BTreeMap;
        let mut params = BTreeMap::new();
        params.insert("k".to_string(), ParamValue::Int(2));
        params.insert("n".to_string(), ParamValue::Int(3));
        let f = IdentifyFixture {
            id: "t3-0001".into(),
            family: "p2wsh_multisig".into(),
            params: params.clone(),
            spk_hex: "0020..".into(),
            inner_script_hex: None,
        };
        let full = grade_identify(
            &f,
            &IdentifyAnswer {
                label: "P2WSH_Multisig".into(),
                params: params.clone(),
            },
            0.5,
        );
        assert_eq!(full.score, 1.0);
        // One of two params wrong: label credit plus half the param band.
        let mut wrong = params.clone();
        wrong.insert("k".to_string(), ParamValue::Int(3));
        let half = grade_identify(
            &f,
            &IdentifyAnswer {
                label: "p2wsh_multisig".into(),
                params: wrong,
            },
            0.5,
        );
        assert_eq!(half.param_fraction, 0.5);
        assert_eq!(half.score, 0.75);
        // No params at all: label credit only.
        let bare = grade_identify(
            &f,
            &IdentifyAnswer {
                label: "p2wsh_multisig".into(),
                params: BTreeMap::new(),
            },
            0.5,
        );
        assert_eq!(bare.score, 0.5);
        // Spamming an extra invented param dilutes the fraction: two
        // correct out of three claimed.
        let mut spam = params.clone();
        spam.insert("bogus".to_string(), ParamValue::Int(9));
        let diluted = grade_identify(
            &f,
            &IdentifyAnswer {
                label: "p2wsh_multisig".into(),
                params: spam,
            },
            0.5,
        );
        assert!((diluted.param_fraction - 2.0 / 3.0).abs() < 1e-12);
        assert!(diluted.score < 1.0);
        let none = grade_identify(
            &f,
            &IdentifyAnswer {
                label: "p2wpkh".into(),
                params,
            },
            0.5,
        );
        assert_eq!(none.score, 0.0);
    }
}
