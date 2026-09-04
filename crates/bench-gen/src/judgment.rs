//! Judgment tasks: underspecified design requests, graded on whether
//! the submitted script honours a set of requirements.
//!
//! Every other task type compares a candidate to one reference, and
//! every reference is the Miniscript compiler's canonical output. Train
//! on that and the optimum is to become the compiler — which is what
//! happened: a fine-tuned 4B reproduced the compiler byte-for-byte on
//! 25 of the 25 hardest write tasks and scored 1.000. That measures
//! compiler emulation, not design ability.
//!
//! Here there is no canonical answer. Requirements are derived from a
//! sampled policy, stated in the prompt as a person would state them,
//! and checked against the candidate's own truth table. Any design
//! meeting them is correct, whatever encoding it chose.

use bench_core::task::{ContextKind, KeyVar, Requirement};

use crate::policy::Abs;
use crate::rng::SeededRng;

/// Evaluate an abstract policy at one point, mirroring the semantics
/// the grader applies to the candidate.
fn holds(p: &Abs, keys: &[usize], hashes: &[String], height: u32, age: u32) -> bool {
    match p {
        Abs::Key(i) => keys.contains(i),
        Abs::After(t) => height >= *t,
        Abs::Older(t) => age >= *t,
        Abs::Sha256(h) => hashes.contains(&hex_of(h)),
        Abs::Hash160(h) => hashes.contains(&hex_of(h)),
        Abs::And(v) => v.iter().all(|c| holds(c, keys, hashes, height, age)),
        Abs::Or(v) => v.iter().any(|c| holds(c, keys, hashes, height, age)),
        Abs::Thresh(k, ks) => ks.iter().filter(|i| keys.contains(i)).count() >= *k,
    }
}

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn key_indices(p: &Abs, out: &mut Vec<usize>) {
    match p {
        Abs::Key(i) => {
            if !out.contains(i) {
                out.push(*i);
            }
        }
        Abs::Thresh(_, ks) => {
            for k in ks {
                if !out.contains(k) {
                    out.push(*k);
                }
            }
        }
        Abs::And(v) | Abs::Or(v) => {
            for c in v {
                key_indices(c, out);
            }
        }
        _ => {}
    }
}

fn hash_atoms(p: &Abs, out: &mut Vec<String>) {
    match p {
        Abs::Sha256(h) => {
            let x = hex_of(h);
            if !out.contains(&x) {
                out.push(x);
            }
        }
        Abs::Hash160(h) => {
            let x = hex_of(h);
            if !out.contains(&x) {
                out.push(x);
            }
        }
        Abs::And(v) | Abs::Or(v) => {
            for c in v {
                hash_atoms(c, out);
            }
        }
        _ => {}
    }
}

/// Timelock breakpoints the policy is sensitive to, plus a point below
/// the lowest one so "too early" is always expressible.
fn height_points(p: &Abs) -> Vec<u32> {
    let mut v = Vec::new();
    collect_after(p, &mut v);
    v.sort_unstable();
    v.dedup();
    v
}

fn collect_after(p: &Abs, out: &mut Vec<u32>) {
    match p {
        Abs::After(t) => out.push(*t),
        Abs::And(v) | Abs::Or(v) => v.iter().for_each(|c| collect_after(c, out)),
        _ => {}
    }
}

fn collect_older(p: &Abs, out: &mut Vec<u32>) {
    match p {
        Abs::Older(t) => out.push(*t),
        Abs::And(v) | Abs::Or(v) => v.iter().for_each(|c| collect_older(c, out)),
        _ => {}
    }
}

fn label_of(keys: &[KeyVar], i: usize) -> String {
    keys.get(i).map(|k| k.label.clone()).unwrap_or_default()
}

fn name_list(keys: &[KeyVar], idx: &[usize]) -> String {
    let names: Vec<String> = idx.iter().map(|i| label_of(keys, *i)).collect();
    match names.len() {
        0 => "nobody".into(),
        1 => names[0].clone(),
        _ => format!(
            "{} and {}",
            names[..names.len() - 1].join(", "),
            names[names.len() - 1]
        ),
    }
}

fn describe(
    keys: &[KeyVar],
    signers: &[usize],
    hashes: &[String],
    height: u32,
    age: u32,
    ok: bool,
) -> String {
    let mut cond = format!("{} sign", name_list(keys, signers));
    if signers.len() == 1 {
        cond = format!("{} signs", name_list(keys, signers));
    }
    if signers.is_empty() {
        cond = "nobody signs".into();
    }
    if !hashes.is_empty() {
        cond.push_str(" and the preimage is revealed");
    }
    let when = match (height, age) {
        (0, 0) => "immediately".to_string(),
        (h, 0) => format!("at block height {h}"),
        (0, a) => format!("after {a} confirmations"),
        (h, a) => format!("at block height {h} with {a} confirmations"),
    };
    if ok {
        format!("{cond} — spendable {when}")
    } else {
        format!("{cond} — must NOT be spendable {when}")
    }
}

/// Derive the requirement set for a policy.
///
/// Positives are the policy's real spending paths; negatives are the
/// near-misses that matter — each signer alone, and every path tried
/// before its timelock. Points the requirements do not mention are
/// left free: that freedom is what makes this a design task.
pub fn requirements_for(p: &Abs, keys: &[KeyVar], rng: &mut SeededRng) -> Vec<Requirement> {
    let mut ks = Vec::new();
    key_indices(p, &mut ks);
    let mut hs = Vec::new();
    hash_atoms(p, &mut hs);

    let mut heights = height_points(p);
    let mut ages = Vec::new();
    collect_older(p, &mut ages);
    ages.sort_unstable();
    ages.dedup();
    let max_h = heights.last().copied().unwrap_or(0);
    let max_a = ages.last().copied().unwrap_or(0);
    heights.push(0);

    let mut out: Vec<Requirement> = Vec::new();
    let mut push = |signers: Vec<usize>, hashes: Vec<String>, height: u32, age: u32| {
        let ok = holds(p, &signers, &hashes, height, age);
        let desc = describe(keys, &signers, &hashes, height, age, ok);
        let r = Requirement {
            keys: signers
                .iter()
                .filter_map(|i| keys.get(*i).map(|k| k.pubkey.clone()))
                .collect(),
            hashes,
            height,
            age,
            spendable: ok,
            description: desc,
        };
        if !out.iter().any(|e| {
            e.keys == r.keys && e.hashes == r.hashes && e.height == r.height && e.age == r.age
        }) {
            out.push(r);
        }
    };

    // Everyone together, with everything known, past every timelock:
    // the policy's most permissive point. Almost always spendable, and
    // a design that fails it is unusable.
    push(ks.clone(), hs.clone(), max_h, max_a);
    // Nobody, before anything: must never be spendable.
    push(Vec::new(), Vec::new(), 0, 0);
    // Each signer alone, fully timed out: catches designs that hand one
    // party unilateral control.
    for i in &ks {
        push(vec![*i], hs.clone(), max_h, max_a);
    }
    // Everyone, but too early: catches missing or mis-set timelocks.
    if max_h > 0 {
        push(ks.clone(), hs.clone(), max_h.saturating_sub(1), max_a);
    }
    if max_a > 0 {
        push(ks.clone(), hs.clone(), max_h, max_a.saturating_sub(1));
    }
    // Everyone, without the preimage: catches dropped hashlocks.
    if !hs.is_empty() {
        push(ks.clone(), Vec::new(), max_h, max_a);
    }
    // A couple of seeded interior points, so the set is not purely
    // extremal and cannot be satisfied by pattern alone.
    for _ in 0..2 {
        if ks.len() >= 2 {
            let mut subset: Vec<usize> = ks.clone();
            let drop = rng.below(subset.len() as u64) as usize;
            subset.remove(drop);
            let h = if max_h > 0 { max_h } else { 0 };
            push(subset, hs.clone(), h, max_a);
        }
    }
    out
}

/// The request as a person would put it: the requirements, in prose,
/// with no policy tree and no canonical answer implied.
pub fn judgment_spec(reqs: &[Requirement], context: ContextKind) -> String {
    let noun = match context {
        ContextKind::Legacy => "P2SH redeem script",
        ContextKind::SegwitV0 => "P2WSH witness script",
        ContextKind::Tap => "tapscript leaf",
    };
    let mut lines = vec![format!(
        "Design a {noun} that satisfies all of the following. Any \
         design meeting them is acceptable; the encoding is yours to \
         choose."
    )];
    for r in reqs {
        lines.push(format!("- {}", r.description));
    }
    lines.join("\n")
}
