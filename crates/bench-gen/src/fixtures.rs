//! Fixture assembly: sample policy, materialize per context, compile,
//! de-optimize, verify everything with the oracle, and emit fixtures.

use std::str::FromStr;

use bench_core::task::{ContextKind, Fixture, KeyVar, OptimizeFixture, Tier, WriteFixture};
use bench_core::{check_equivalence, Verdict};
use bitcoin::{PublicKey, XOnlyPublicKey};
use miniscript::{policy::Concrete, Legacy, Miniscript, Segwitv0, Tap};

use crate::keys::{self, KeySet};
use crate::naive;
use crate::policy::{self, Abs};
use crate::rng::SeededRng;
use crate::verbal;

#[derive(Clone, Debug)]
pub struct GenParams {
    pub seed: u64,
    pub write: usize,
    pub optimize: usize,
    pub identify: usize,
}

impl Default for GenParams {
    fn default() -> Self {
        GenParams {
            seed: 0,
            write: 300,
            optimize: 300,
            identify: 250,
        }
    }
}

fn tier_for(i: usize) -> Tier {
    // 40/40/20 split per DESIGN.md, cycling every 5: 2 easy, 2 medium, 1 hard.
    match i % 5 {
        0 | 1 => Tier::Easy,
        2 | 3 => Tier::Medium,
        _ => Tier::Hard,
    }
}

fn context_for(i: usize) -> ContextKind {
    match i % 3 {
        0 => ContextKind::Legacy,
        1 => ContextKind::SegwitV0,
        _ => ContextKind::Tap,
    }
}

fn key_vars(ks: &KeySet, ctx: ContextKind) -> Vec<KeyVar> {
    (0..ks.len())
        .map(|i| KeyVar {
            label: ks.label(i).to_string(),
            pubkey: match ctx {
                ContextKind::Tap => ks.xonly[i].clone(),
                _ => ks.compressed[i].clone(),
            },
        })
        .collect()
}

/// Concrete policy string from the abstract policy in this context.
fn policy_string(p: &Abs, ks: &KeySet, ctx: ContextKind) -> String {
    let key = |i: usize| -> String {
        match ctx {
            ContextKind::Tap => ks.xonly[i].clone(),
            _ => ks.compressed[i].clone(),
        }
    };
    fn rec(p: &Abs, key: &dyn Fn(usize) -> String) -> String {
        match p {
            Abs::Key(i) => format!("pk({})", key(*i)),
            Abs::After(t) => format!("after({t})"),
            Abs::Older(t) => format!("older({t})"),
            Abs::Sha256(h) => format!(
                "sha256({})",
                h.iter().map(|b| format!("{b:02x}")).collect::<String>()
            ),
            Abs::Hash160(h) => format!(
                "hash160({})",
                h.iter().map(|b| format!("{b:02x}")).collect::<String>()
            ),
            Abs::And(v) => format!("and({},{})", rec(&v[0], key), rec(&v[1], key)),
            Abs::Or(v) => format!("or({},{})", rec(&v[0], key), rec(&v[1], key)),
            Abs::Thresh(k, ksx) => {
                let inner: Vec<String> = ksx.iter().map(|i| format!("pk({})", key(*i))).collect();
                format!("thresh({},{})", k, inner.join(","))
            }
        }
    }
    rec(p, &key)
}

/// One write/optimize task's compiled artifacts, verified.
struct Compiled {
    #[allow(dead_code)] // kept for diagnostics; fixtures carry context
    context: ContextKind,
    policy_text: String,
    ms_text: String,
    script_hex: String,
    naive_hex: String,
    naive_weight: usize,
    naive_size: usize,
    opt_weight: usize,
    opt_size: usize,
    keys: Vec<KeyVar>,
    spec_en: String,
}

fn compile_task(
    rng: &mut SeededRng,
    tier: Tier,
    context: ContextKind,
    need_baseline: bool,
) -> Option<Compiled> {
    // Bounded deterministic retries; a fresh policy each attempt.
    for _ in 0..64 {
        if let Some(c) = attempt(rng, tier, context, need_baseline) {
            return Some(c);
        }
    }
    None
}

fn attempt(
    rng: &mut SeededRng,
    tier: Tier,
    context: ContextKind,
    need_baseline: bool,
) -> Option<Compiled> {
    {
        let abs = if need_baseline {
            policy::sample_with_or(rng, tier)
        } else {
            policy::sample(rng, tier)
        };
        let key_count = abs.key_count().max(1).min(12);
        let ks = keys::generate(rng, key_count);
        if let Some(t) = thresh_of(&abs) {
            let (k, n) = t;
            if naive::subset_count(n, k) > naive::MAX_SUBSETS {
                return None;
            }
        }
        let p_text = policy_string(&abs, &ks, context);
        let kvars = key_vars(&ks, context);
        let spec = verbal::spec(&abs, &kvars);

        let (ms_text, script, opt_w, naive_script, naive_w) = match context {
            ContextKind::Legacy => {
                let p = Concrete::<PublicKey>::from_str(&p_text).ok()?;
                let ms = p.compile::<Legacy>().ok()?;
                let w = bench_core::weights_for(context, &ms.encode()).ok()?;
                let n_text = naive::naive_string(&abs, &ks, false);
                let nms = Miniscript::<PublicKey, Legacy>::from_str_insane(&n_text).ok()?;
                let nw = bench_core::weights_for(context, &nms.encode()).ok()?;
                (ms.to_string(), ms.encode(), w, nms.encode(), nw)
            }
            ContextKind::SegwitV0 => {
                let p = Concrete::<PublicKey>::from_str(&p_text).ok()?;
                let ms = p.compile::<Segwitv0>().ok()?;
                let w = bench_core::weights_for(context, &ms.encode()).ok()?;
                let n_text = naive::naive_string(&abs, &ks, false);
                let nms = Miniscript::<PublicKey, Segwitv0>::from_str_insane(&n_text).ok()?;
                let nw = bench_core::weights_for(context, &nms.encode()).ok()?;
                (ms.to_string(), ms.encode(), w, nms.encode(), nw)
            }
            ContextKind::Tap => {
                let p = Concrete::<XOnlyPublicKey>::from_str(&p_text).ok()?;
                let ms = p.compile::<Tap>().ok()?;
                let w = bench_core::weights_for(context, &ms.encode()).ok()?;
                let n_text = naive::naive_string(&abs, &ks, true);
                let nms = Miniscript::<XOnlyPublicKey, Tap>::from_str_insane(&n_text).ok()?;
                let nw = bench_core::weights_for(context, &nms.encode()).ok()?;
                (ms.to_string(), ms.encode(), w, nms.encode(), nw)
            }
        };
        // Self-check: the reference must be gradable — decode + lift must
        // succeed (the Legacy compiler sometimes emits pk_h, whose script
        // bytes carry only a hash and cannot lift; mixed height/relative
        // timelocks in one path are also ungradable). Resample those.
        if check_equivalence(context, &script, &script) != Verdict::Equivalent {
            return None;
        }
        let (opt_weight, opt_size) = (opt_w.weight, opt_w.size);
        let (naive_weight, naive_size) = (naive_w.weight, naive_w.size);

        // Verify the naive baseline decodes in-context and is equivalent
        // to the compiled reference; for optimize tasks it must also be
        // strictly heavier (or_d chains vs or_c/multi carry real weight).
        if need_baseline {
            if check_equivalence(context, &script, &naive_script) != Verdict::Equivalent {
                return None;
            }
            if naive_weight <= opt_weight {
                return None;
            }
        }
        return Some(Compiled {
            context,
            policy_text: p_text,
            ms_text,
            script_hex: script.to_hex_string(),
            naive_hex: naive_script.to_hex_string(),
            naive_weight,
            naive_size,
            opt_weight,
            opt_size,
            keys: kvars,
            spec_en: spec,
        });
    }
}

fn thresh_of(p: &Abs) -> Option<(usize, usize)> {
    match p {
        Abs::Thresh(k, v) => Some((*k, v.len())),
        Abs::And(v) | Abs::Or(v) => v.iter().find_map(thresh_of),
        _ => None,
    }
}
/// Generate the full fixture set. Panics if a task cannot be generated
/// after retries — the distributions are bounded, so a panic indicates a
/// generator bug, not bad luck.
pub fn generate(params: &GenParams) -> Vec<Fixture> {
    let mut out = Vec::new();
    let mut rng = SeededRng::new(params.seed);
    for i in 0..params.write {
        let tier = tier_for(i);
        let ctx = context_for(i);
        let c = compile_task(&mut rng, tier, ctx, false)
            .unwrap_or_else(|| panic!("write task {i} ({tier:?}/{ctx:?}) failed to generate"));
        out.push(Fixture::Write(WriteFixture {
            id: format!("t1-{i:04}"),
            tier,
            context: ctx,
            spec_en: c.spec_en,
            keys: c.keys,
            reference_policy: c.policy_text,
            reference_miniscript: c.ms_text,
            reference_script_hex: c.script_hex,
        }));
    }
    for i in 0..params.optimize {
        let tier = tier_for(i);
        let ctx = context_for(i);
        let c = compile_task(&mut rng, tier, ctx, true)
            .unwrap_or_else(|| panic!("optimize task {i} ({tier:?}/{ctx:?}) failed to generate"));
        out.push(Fixture::Optimize(OptimizeFixture {
            id: format!("t2-{i:04}"),
            tier,
            context: ctx,
            spec_en: c.spec_en,
            keys: c.keys,
            baseline_script_hex: c.naive_hex,
            baseline_size: c.naive_size,
            baseline_weight: c.naive_weight,
            optimal_script_hex: c.script_hex,
            optimal_size: c.opt_size,
            optimal_weight: c.opt_weight,
            reference_policy: c.policy_text,
            reference_miniscript: c.ms_text,
        }));
    }
    for i in 0..params.identify {
        let ks = keys::generate(&mut rng, 3);
        out.extend(
            crate::corpus::standards(&mut rng, &ks, i)
                .into_iter()
                .map(Fixture::Identify),
        );
        // Protocol rotation: 4 of the 11 protocol families per group,
        // cycling so every family appears across the dataset (~70/30
        // standard/protocol split overall).
        let all = crate::protocol::protocol_items(&mut rng, i);
        for j in 0..4 {
            let idx = (i * 4 + j) % all.len();
            out.push(Fixture::Identify(all[idx].clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::ScriptBuf;

    #[test]
    fn generate_and_verify() {
        let params = GenParams {
            seed: 7,
            write: 10,
            optimize: 10,
            identify: 3,
        };
        let fixtures = generate(&params);
        assert_eq!(
            fixtures.len(),
            10 + 10 + 3 * (10 + 4),
            "10 standards + 4 protocol per identify group"
        );
        // Every write fixture's answer key verifies against itself.
        for f in &fixtures {
            if let Fixture::Write(w) = f {
                let r = ScriptBuf::from_hex(&w.reference_script_hex).unwrap();
                assert_eq!(check_equivalence(w.context, &r, &r), Verdict::Equivalent);
            }
        }
    }

    #[test]
    fn deterministic() {
        let p = GenParams {
            seed: 21,
            write: 4,
            optimize: 4,
            identify: 2,
        };
        let a = serde_json::to_string(&generate(&p)).unwrap();
        let b = serde_json::to_string(&generate(&p)).unwrap();
        assert_eq!(a, b);
    }
}
