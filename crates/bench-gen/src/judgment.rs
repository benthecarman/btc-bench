//! Judgment contracts state all spending rules in prose. Scripts are
//! compared by behavior, not compiler bytes. Sparse rows are examples only.

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
        Abs::Sha256(h) => hashes.contains(&format!("sha256:{}", hex_of(h))),
        Abs::Hash160(h) => hashes.contains(&format!("hash160:{}", hex_of(h))),
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
            let x = format!("sha256:{}", hex_of(h));
            if !out.contains(&x) {
                out.push(x);
            }
        }
        Abs::Hash160(h) => {
            let x = format!("hash160:{}", hex_of(h));
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
        cond.push_str(&format!(
            " and the 32-byte preimages for {} are revealed",
            hashes.join(", ")
        ));
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

/// Sample diagnostic examples. The full policy in the request defines
/// correctness, including all states omitted by these examples.
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

/// The prose states the entire contract. No extra spending paths are allowed.
pub fn judgment_spec(p: &Abs, keys: &[KeyVar], context: ContextKind) -> String {
    format!(
        "Design a {}. The encoding is yours to choose.\n\n{}\n\n\
         These are the complete spending rules: spending must be possible if and only if \
         these conditions hold. Do not add other spending paths or extra requirements.",
        context.script_noun(),
        crate::verbal::spec(p, keys),
    )
}

/// Build one witness script to prove the contract is implementable.
/// This is an audit witness, not a required encoding.
pub fn reference_script(
    f: &bench_core::task::JudgmentFixture,
) -> Result<bitcoin::ScriptBuf, String> {
    use miniscript::{policy::Concrete, Legacy, Segwitv0, Tap};
    macro_rules! compile {
        ($pk:ty, $ctx:ty) => {
            f.reference_policy
                .parse::<Concrete<$pk>>()
                .and_then(|p| Ok(p.compile::<$ctx>()?))
                .map(|ms| ms.encode())
                .map_err(|e| e.to_string())
        };
    }
    match f.context {
        ContextKind::Legacy => compile!(bitcoin::PublicKey, Legacy),
        ContextKind::SegwitV0 => compile!(bitcoin::PublicKey, Segwitv0),
        ContextKind::Tap => compile!(bitcoin::XOnlyPublicKey, Tap),
    }
}

pub fn validate(f: &bench_core::task::JudgmentFixture) -> Result<bitcoin::ScriptBuf, String> {
    if f.contract_version != 1 {
        return Err(
            "unsupported judgment contract; regenerate the dataset with the current generator"
                .into(),
        );
    }
    let script = reference_script(f)?;
    let result = bench_core::grade_judgment(f, &script.to_hex_string());
    if result.score != 1.0 {
        return Err(result
            .reason
            .unwrap_or_else(|| "contract witness did not pass".into()));
    }
    let preimages = bench_core::HashPreimages::from_hex_map(&f.hash_preimages)?;
    bench_core::execution_check(f.context, &script, &preimages)?;
    Ok(script)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{generate, GenParams};
    use bench_core::task::{Fixture, Tier};

    #[test]
    fn judgment_pool_is_complete_satisfiable_and_prompts_include_hashes() {
        let fixtures = generate(&GenParams {
            seed: 2026,
            write: 0,
            optimize: 0,
            identify: 0,
            judgment: 100,
            ..GenParams::default()
        });
        assert_eq!(fixtures.len(), 100);
        let mut hashes = 0;
        for fixture in fixtures {
            let Fixture::Judgment(j) = fixture else {
                unreachable!()
            };
            let reference = validate(&j).expect("satisfiable contract");
            let prompt = crate::prompt::judgment_prompt(&j);
            assert!(prompt.contains("if and only if"));
            for (digest, preimage) in &j.hash_preimages {
                hashes += 1;
                assert!(prompt.contains(digest));
                assert!(prompt.contains(if digest.len() == 64 {
                    "SHA-256"
                } else {
                    "HASH160"
                }));
                assert!(
                    !prompt.contains(preimage),
                    "do not reveal private preimages"
                );
            }
            assert_eq!(
                bench_core::grade_judgment(&j, &reference.to_hex_string()).score,
                1.0
            );
            // The old pool gave OP_0 negative-row credit and could panic.
            assert_eq!(bench_core::grade_judgment(&j, "OP_0").score, 0.0);
        }
        assert!(hashes > 0);
    }

    #[test]
    fn audit_rejects_inconsistent_examples_and_old_contracts() {
        let mut fixtures = generate(&GenParams {
            seed: 7,
            write: 0,
            optimize: 0,
            identify: 0,
            judgment: 1,
            tiers: vec![Tier::Hard],
            ..GenParams::default()
        });
        let Fixture::Judgment(j) = &mut fixtures[0] else {
            unreachable!()
        };
        j.requirements[0].spendable = !j.requirements[0].spendable;
        assert!(validate(j).unwrap_err().contains("inconsistent"));
        j.contract_version = 0;
        assert!(validate(j).unwrap_err().contains("regenerate"));
    }
}
