//! Fixture assembly: sample policy, materialize per context, compile,
//! de-optimize, verify everything with the oracle, and emit fixtures.

use std::collections::BTreeMap;
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
    render(p, &key)
}

/// Render any node: n-ary and/or right-fold into the binary concrete
/// grammar (binary nodes render byte-identically to the historical
/// two-child form), leaves verbatim.
fn render(p: &Abs, key: &dyn Fn(usize) -> String) -> String {
    match p {
        Abs::And(v) => fold_nary("and", v, key),
        Abs::Or(v) => fold_nary("or", v, key),
        leaf => policy_leaf(leaf, key),
    }
}

fn fold_nary(op: &str, v: &[Abs], key: &dyn Fn(usize) -> String) -> String {
    let mut iter = v.iter().rev();
    let last = iter.next().expect("nonempty node");
    let mut acc = render(last, key);
    for e in iter {
        acc = format!("{op}({},{})", render(e, key), acc);
    }
    acc
}

fn policy_leaf(p: &Abs, key: &dyn Fn(usize) -> String) -> String {
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
        Abs::Thresh(k, ksx) => {
            let inner: Vec<String> = ksx.iter().map(|i| format!("pk({})", key(*i))).collect();
            format!("thresh({},{})", k, inner.join(","))
        }
        Abs::And(_) | Abs::Or(_) => unreachable!("handled by render"),
    }
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
    preimages: BTreeMap<String, String>,
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
        let mut pre = policy::Preimages::default();
        let abs = if need_baseline {
            policy::sample_with_or_pre(rng, tier, &mut pre)
        } else {
            policy::sample_pre(rng, tier, &mut pre)
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
                let n_text = naive::sample_naive(rng, &abs, &ks, false);
                let nms = Miniscript::<PublicKey, Legacy>::from_str_insane(&n_text).ok()?;
                let nw = bench_core::weights_for(context, &nms.encode()).ok()?;
                (ms.to_string(), ms.encode(), w, nms.encode(), nw)
            }
            ContextKind::SegwitV0 => {
                let p = Concrete::<PublicKey>::from_str(&p_text).ok()?;
                let ms = p.compile::<Segwitv0>().ok()?;
                let w = bench_core::weights_for(context, &ms.encode()).ok()?;
                let n_text = naive::sample_naive(rng, &abs, &ks, false);
                let nms = Miniscript::<PublicKey, Segwitv0>::from_str_insane(&n_text).ok()?;
                let nw = bench_core::weights_for(context, &nms.encode()).ok()?;
                (ms.to_string(), ms.encode(), w, nms.encode(), nw)
            }
            ContextKind::Tap => {
                let p = Concrete::<XOnlyPublicKey>::from_str(&p_text).ok()?;
                let ms = p.compile::<Tap>().ok()?;
                let w = bench_core::weights_for(context, &ms.encode()).ok()?;
                let n_text = naive::sample_naive(rng, &abs, &ks, true);
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
        // Dual-oracle cross-check (after rust-miniscript's bitcoind
        // integration tests): both the reference and the baseline must
        // be *spendable* — a real witness, executed through the crate
        // interpreter under the output's natural wrapping. Catches
        // truth-table-walk bugs the lift oracle cannot.
        let typed = typed_preimages(&pre);
        if bench_core::execution_check(context, &script, &typed).is_err() {
            return None;
        }
        if need_baseline && bench_core::execution_check(context, &naive_script, &typed).is_err() {
            return None;
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
            preimages: preimage_hex_map(&pre),
        });
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hex map for the fixture schema (hex hash -> hex preimage).
fn preimage_hex_map(pre: &policy::Preimages) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for (h, p) in &pre.sha256 {
        m.insert(hex(h), hex(p));
    }
    for (h, p) in &pre.hash160 {
        m.insert(hex(h), hex(p));
    }
    m
}

/// Typed preimages for the execution oracle.
fn typed_preimages(pre: &policy::Preimages) -> bench_core::HashPreimages {
    let mut out = bench_core::HashPreimages::default();
    for (h, p) in &pre.sha256 {
        out.sha256.insert(*h, *p);
    }
    for (h, p) in &pre.hash160 {
        out.hash160.insert(*h, *p);
    }
    out
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
            hash_preimages: c.preimages,
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
            hash_preimages: c.preimages,
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

    #[test]
    fn boundary_shape_is_not_starved() {
        // Walk the boundary shape through the exact attempt() gates in
        // every context; it must not be starved (RawPkH, lift failure,
        // non-equivalence, or not-heavier would all silently drop it).
        let mut rng = SeededRng::new(55);
        for ctx in [ContextKind::Legacy, ContextKind::SegwitV0, ContextKind::Tap] {
            for trial in 0..8 {
                let mut prng = SeededRng::new(1000 + trial);
                let abs = policy::boundary_timelocks(&mut prng);
                let ks = keys::generate(&mut rng, 7);
                let p_text = policy_string(&abs, &ks, ctx);
                let (script, naive_script) = match ctx {
                    ContextKind::Legacy => {
                        let p = Concrete::<PublicKey>::from_str(&p_text).expect("parse");
                        let ms = p.compile::<Legacy>().expect("compile");
                        let n = naive::sample_naive(&mut prng, &abs, &ks, false);
                        (
                            ms.encode(),
                            Miniscript::<PublicKey, Legacy>::from_str_insane(&n)
                                .expect("naive parse")
                                .encode(),
                        )
                    }
                    ContextKind::SegwitV0 => {
                        let p = Concrete::<PublicKey>::from_str(&p_text).expect("parse");
                        let ms = p.compile::<Segwitv0>().expect("compile");
                        let n = naive::sample_naive(&mut prng, &abs, &ks, false);
                        (
                            ms.encode(),
                            Miniscript::<PublicKey, Segwitv0>::from_str_insane(&n)
                                .expect("naive parse")
                                .encode(),
                        )
                    }
                    ContextKind::Tap => {
                        let p = Concrete::<XOnlyPublicKey>::from_str(&p_text).expect("parse");
                        let ms = p.compile::<Tap>().expect("compile");
                        let n = naive::sample_naive(&mut prng, &abs, &ks, true);
                        (
                            ms.encode(),
                            Miniscript::<XOnlyPublicKey, Tap>::from_str_insane(&n)
                                .expect("naive parse")
                                .encode(),
                        )
                    }
                };
                assert_eq!(
                    check_equivalence(ctx, &script, &script),
                    Verdict::Equivalent,
                    "{ctx:?} trial {trial}: reference ungradable"
                );
                assert_eq!(
                    check_equivalence(ctx, &script, &naive_script),
                    Verdict::Equivalent,
                    "{ctx:?} trial {trial}: baseline not equivalent"
                );
                let ow = bench_core::weights_for(ctx, &script)
                    .expect("weights")
                    .weight;
                let nw = bench_core::weights_for(ctx, &naive_script)
                    .expect("weights")
                    .weight;
                assert!(
                    nw > ow,
                    "{ctx:?} trial {trial}: baseline not heavier ({nw} vs {ow})"
                );
                bench_core::execution_check(ctx, &script, &bench_core::HashPreimages::default())
                    .unwrap_or_else(|e| panic!("{ctx:?} trial {trial}: reference exec: {e}"));
                bench_core::execution_check(
                    ctx,
                    &naive_script,
                    &bench_core::HashPreimages::default(),
                )
                .unwrap_or_else(|e| panic!("{ctx:?} trial {trial}: baseline exec: {e}"));
            }
        }
    }

    #[test]
    fn shape_census() {
        // Every dispatched shape must ship at a healthy rate in at
        // least one context (parse -> compile -> lift self-check ->
        // execution oracle, with the shape's own sampled preimages).
        // Shapes that die at every gate are dead code: they never ship
        // and silently skew the tier distribution while burning
        // retries. The four MINT vault structures and the old
        // hard-synth sampler were removed for exactly this reason.
        use std::collections::BTreeMap;
        let mut rng = SeededRng::new(7);
        let mut ships: BTreeMap<&'static str, usize> = BTreeMap::new();
        for trial in 0..30 {
            let mut prng = SeededRng::new(3000 + trial);
            let mut pre = policy::Preimages::default();
            let shapes: Vec<(&'static str, Abs)> = vec![
                ("timelock_in_thresh", policy::timelock_in_thresh(&mut prng)),
                ("boundary_timelocks", policy::boundary_timelocks(&mut prng)),
                ("recovery_paths", policy::recovery_paths(&mut prng)),
                ("deep_nest", policy::deep_nest(&mut prng, &mut pre)),
                ("wide_thresh", policy::wide_thresh(&mut prng)),
                (
                    "medium_synth",
                    policy::sample_medium_synth(&mut prng, &mut pre),
                ),
            ];
            let mut typed = bench_core::HashPreimages::default();
            for (h, p) in &pre.sha256 {
                typed.sha256.insert(*h, *p);
            }
            for (h, p) in &pre.hash160 {
                typed.hash160.insert(*h, *p);
            }
            for (name, abs) in shapes {
                for ctx in [ContextKind::Legacy, ContextKind::SegwitV0, ContextKind::Tap] {
                    let ks = keys::generate(&mut rng, abs.key_count().max(1));
                    let p_text = policy_string(&abs, &ks, ctx);
                    let script = match ctx {
                        ContextKind::Legacy => Concrete::<PublicKey>::from_str(&p_text)
                            .ok()
                            .and_then(|p| p.compile::<Legacy>().ok())
                            .map(|ms| ms.encode()),
                        ContextKind::SegwitV0 => Concrete::<PublicKey>::from_str(&p_text)
                            .ok()
                            .and_then(|p| p.compile::<Segwitv0>().ok())
                            .map(|ms| ms.encode()),
                        ContextKind::Tap => Concrete::<XOnlyPublicKey>::from_str(&p_text)
                            .ok()
                            .and_then(|p| p.compile::<Tap>().ok())
                            .map(|ms| ms.encode()),
                    };
                    let Some(script) = script else { continue };
                    if check_equivalence(ctx, &script, &script) != Verdict::Equivalent {
                        continue;
                    }
                    if bench_core::execution_check(ctx, &script, &typed).is_err() {
                        continue;
                    }
                    *ships.entry(name).or_insert(0) += 1;
                    break;
                }
            }
        }
        for (name, n) in &ships {
            println!("{name:24} ships {n:2}/30 trials");
            assert!(
                *n >= 10,
                "shape {name} ships only {n}/30 — near-dead, remove or repair it"
            );
        }
        assert_eq!(
            ships.len(),
            6,
            "dispatched shape set changed; update the census"
        );
    }

    #[test]
    fn n_ary_policy_string_keeps_every_branch() {
        // Regression: the concrete-policy grammar is binary and the
        // renderer once dropped n-ary nodes' third-plus children,
        // shipping answer keys whose prompts promised branches the
        // script did not contain. N-ary nodes must right-fold and
        // round-trip through the parser with every branch intact.
        let mut rng = SeededRng::new(8);
        let ks = keys::generate(&mut rng, 7);
        let key = |i: usize| ks.compressed[i].clone();
        let p = Abs::Or(vec![
            Abs::And(vec![
                Abs::And(vec![Abs::Key(0), Abs::Key(1)]),
                Abs::After(499_999_998),
            ]),
            Abs::And(vec![
                Abs::And(vec![Abs::Key(2), Abs::Key(3)]),
                Abs::After(499_999_999),
            ]),
            Abs::And(vec![Abs::Thresh(2, vec![4, 5, 6]), Abs::Older(144)]),
        ]);
        let s = policy_string(&p, &ks, ContextKind::Legacy);
        // Every branch's keys survive into the rendered policy.
        for i in 0..7 {
            assert!(s.contains(&key(i)), "key {i} missing from: {s}");
        }
        assert!(s.contains("after(499999998)") && s.contains("after(499999999)"));
        assert!(s.contains("older(144)"));
        // And the rendered policy parses (the binary right-fold is
        // valid concrete-policy syntax).
        let parsed = Concrete::<PublicKey>::from_str(&s).expect("n-ary fold parses");
        let _ = parsed.to_string();
        // Binary nodes render byte-identically to the old form.
        let bin = Abs::And(vec![Abs::Key(0), Abs::Key(1)]);
        assert_eq!(
            policy_string(&bin, &ks, ContextKind::Legacy),
            format!("and(pk({}),pk({}))", key(0), key(1))
        );
    }
}
