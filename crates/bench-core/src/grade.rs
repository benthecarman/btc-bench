//! Graders for the three task types. All judge-free.

use bitcoin::{ScriptBuf, XOnlyPublicKey};
use miniscript::descriptor::{TapTree, Tr};
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

#[derive(Clone, Debug)]
pub struct WriteResult {
    pub verdict: Verdict,
    pub score: f64,
    /// Parse/decode failure detail when score is 0.
    pub reason: Option<String>,
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
            }
        }
    };
    let reference =
        ScriptBuf::from_hex(&fixture.reference_script_hex).expect("fixture hex is valid");
    let verdict = check_equivalence(fixture.context, &reference, &candidate);
    let score = if verdict.is_equivalent() { 1.0 } else { 0.0 };
    WriteResult {
        reason: if score == 0.0 {
            Some(format!("{verdict:?}"))
        } else {
            None
        },
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
    }
}

#[derive(Clone, Debug)]
pub struct IdentifyResult {
    pub label_correct: bool,
    pub params_correct: bool,
    pub score: f64,
}

/// Task 3: flat label + parameters, configurable partial credit
/// (`partial_credit` is the score for a correct label with wrong params;
/// default per DESIGN.md is 0.5).
pub fn grade_identify(
    fixture: &IdentifyFixture,
    answer: &IdentifyAnswer,
    partial_credit: f64,
) -> IdentifyResult {
    let label_correct = answer
        .label
        .trim()
        .eq_ignore_ascii_case(fixture.family.trim());
    let params_correct = label_correct && answer.params == fixture.params;
    let score = if params_correct {
        1.0
    } else if label_correct {
        partial_credit
    } else {
        0.0
    };
    IdentifyResult {
        label_correct,
        params_correct,
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
            keys: vec![],
            reference_policy: String::new(),
            reference_miniscript: String::new(),
            reference_script_hex: reference_hex,
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
        assert_eq!(half.score, 0.5);
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
