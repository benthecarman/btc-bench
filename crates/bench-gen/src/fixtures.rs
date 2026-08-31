//! Fixture assembly: sample policy, materialize per context, compile,
//! de-optimize, verify everything with the oracle, and emit fixtures.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use bench_core::task::{
    ContextKind, Fixture, KeyVar, OptimizeFixture, Tier, TreeFixture, WriteFixture,
};
use bench_core::{check_equivalence, Verdict};
use bitcoin::{PublicKey, XOnlyPublicKey};
use miniscript::{policy::Concrete, Descriptor, Legacy, Miniscript, Segwitv0, Tap};

/// BIP-341 NUMS point: SHA-256 of the generator's compressed encoding,
/// lifted to a curve point — provably no one knows its discrete log.
/// Offered in tree prompts as the internal key for policies with no
/// key-path-worthy branch.
pub const UNSPENDABLE_KEY: &str =
    "50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0";

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
    /// Number of taproot tree-design tasks (t4). Appended after the
    /// other kinds, so adding trees never disturbs t1-t3 for a seed.
    pub tree: usize,
    /// Verbalizer template family ids to draw from. Empty = family 0
    /// only (canonical, byte-identical to historical datasets). List
    /// only non-eval families (e.g. [1, 2]) when generating training
    /// sets, so bench-only families never appear in training data.
    pub verbal_families: Vec<u32>,
    /// Structural prose variation: seeded permutation of commutative
    /// children and varied root list shapes. Off = canonical structure
    /// (the eval setting).
    pub vary_structure: bool,
    /// Tier cycle. Empty = the default 40/40/20 easy/medium/hard
    /// split. Non-empty = round-robin through exactly these tiers
    /// (repeat a tier to weight it), for curriculum generation.
    pub tiers: Vec<Tier>,
    /// Reference script hexes to exclude: any sampled task whose
    /// answer key lands in this set is resampled. Feed it the answer
    /// keys of the eval set so training data never contains an eval
    /// task (same-seed reuse is the realistic contamination path).
    pub exclude: BTreeSet<String>,
}

impl Default for GenParams {
    fn default() -> Self {
        GenParams {
            seed: 0,
            write: 300,
            optimize: 300,
            identify: 250,
            tree: 0,
            verbal_families: Vec::new(),
            vary_structure: false,
            tiers: Vec::new(),
            exclude: BTreeSet::new(),
        }
    }
}

fn tier_for(i: usize, tiers: &[Tier]) -> Tier {
    if !tiers.is_empty() {
        return tiers[i % tiers.len()];
    }
    // 40/40/20 split per DESIGN.md, cycling every 5: 2 easy, 2 medium, 1 hard.
    match i % 5 {
        0 | 1 => Tier::Easy,
        2 | 3 => Tier::Medium,
        _ => Tier::Hard,
    }
}

/// Prose style for task `i`: family and structure seed derived from a
/// per-task salt, not the main rng stream, so style choice never
/// perturbs policy sampling and retries never shift it.
fn style_for(params: &GenParams, kind_salt: u64, i: usize) -> verbal::Style {
    let salt = params.seed ^ kind_salt ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let family = match params.verbal_families.as_slice() {
        [] => 0,
        [f] => *f,
        fs => fs[SeededRng::new(salt).below(fs.len() as u64) as usize],
    };
    verbal::Style {
        family,
        structure_seed: params
            .vary_structure
            .then_some(salt ^ 0xA5A5_A5A5_A5A5_A5A5),
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
    atoms: usize,
    preimages: BTreeMap<String, String>,
}

fn compile_task(
    rng: &mut SeededRng,
    tier: Tier,
    context: ContextKind,
    need_baseline: bool,
    style: verbal::Style,
    exclude: &BTreeSet<String>,
) -> Option<Compiled> {
    // Bounded deterministic retries; a fresh policy each attempt.
    for _ in 0..64 {
        if let Some(c) = attempt(rng, tier, context, need_baseline, style, exclude) {
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
    style: verbal::Style,
    exclude: &BTreeSet<String>,
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
        let spec = verbal::spec_styled(&abs, &kvars, style);

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
        // Contamination dedup: never ship a task whose answer key is in
        // the excluded set (typically the eval set's answer keys).
        if !exclude.is_empty() && exclude.contains(&script.to_hex_string()) {
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
            atoms: abs.atom_count(),
            preimages: preimage_hex_map(&pre),
        });
    }
}

/// Flatten a concrete policy's root-level or-chain into branches.
fn flatten_or(p: &Concrete<XOnlyPublicKey>, out: &mut Vec<Concrete<XOnlyPublicKey>>) {
    if let Concrete::Or(subs) = p {
        for (_odds, sub) in subs {
            flatten_or(sub, out);
        }
    } else {
        out.push(p.clone());
    }
}

/// Balanced binary tap tree *string* over compiled leaves. With equal
/// branch odds this is the shape a Huffman tree would give, and it
/// minimizes the worst-case control-block depth — the metric tree
/// tasks score.
///
/// The string is built by hand because miniscript 13.1's `TapTree`
/// Display is broken: it emits closing braces after the next leaf
/// instead of before it, so any tree with a depth decrease between
/// consecutive leaves (e.g. `{{A,B},C}`) prints as a malformed string
/// its own parser rejects. The answer key must be a parseable string
/// — models answer in text — so we serialize the tree ourselves and
/// re-parse it as the source of truth.
fn balanced_tree_string(leaves: &[Miniscript<XOnlyPublicKey, Tap>]) -> String {
    match leaves.len() {
        0 => unreachable!("caller guarantees leaves"),
        1 => leaves[0].to_string(),
        n => {
            let (l, r) = leaves.split_at(n.div_ceil(2));
            format!(
                "{{{},{}}}",
                balanced_tree_string(l),
                balanced_tree_string(r)
            )
        }
    }
}

/// Re-derive a tree task's reference and baseline descriptors from its
/// concrete policy string. Used by generation and by the dataset
/// audit, so the answer key is always reconstructible from first
/// principles.
///
/// NOT `compile_tr`: its Huffman `TapTree` hits the same broken
/// Display (see [`balanced_tree_string`]), so its output cannot
/// round-trip through a string. Instead: the policy's single bare-key
/// branch becomes the internal key, every other branch compiles to
/// its own leaf, and the leaves form a balanced binary tree. The
/// baseline is the whole policy as one leaf under the unspendable
/// key. Returns (reference, baseline) descriptor strings, both
/// verified to parse.
pub fn tree_descriptors_for_policy(
    policy: &str,
    unspendable_key: &str,
) -> Result<(String, String), String> {
    let concrete = Concrete::<XOnlyPublicKey>::from_str(policy).map_err(|e| e.to_string())?;
    let mut branches = Vec::new();
    flatten_or(&concrete, &mut branches);
    let key_at = branches
        .iter()
        .position(|b| matches!(b, Concrete::Key(_)))
        .ok_or("no bare-key branch for the key path")?;
    let Concrete::Key(internal) = branches.remove(key_at) else {
        unreachable!("position matched a key branch")
    };
    let leaves = branches
        .iter()
        .map(|b| b.compile::<Tap>().map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let reference = if leaves.is_empty() {
        format!("tr({internal})")
    } else {
        format!("tr({internal},{})", balanced_tree_string(&leaves))
    };
    let single = concrete.compile::<Tap>().map_err(|e| e.to_string())?;
    let baseline = format!("tr({unspendable_key},{single})");
    for s in [&reference, &baseline] {
        s.parse::<Descriptor<XOnlyPublicKey>>()
            .map_err(|e| format!("built descriptor does not parse ({s}): {e}"))?;
    }
    Ok((reference, baseline))
}

/// One tree task's verified artifacts.
struct TreeCompiled {
    policy_text: String,
    reference_descriptor: String,
    reference_weight: usize,
    baseline_descriptor: String,
    baseline_weight: usize,
    keys: Vec<KeyVar>,
    spec_en: String,
    atoms: usize,
    preimages: BTreeMap<String, String>,
}

fn compile_tree_task(
    rng: &mut SeededRng,
    tier: Tier,
    style: verbal::Style,
    exclude: &BTreeSet<String>,
) -> Option<TreeCompiled> {
    for _ in 0..64 {
        if let Some(c) = tree_attempt(rng, tier, style, exclude) {
            return Some(c);
        }
    }
    None
}

fn tree_attempt(
    rng: &mut SeededRng,
    tier: Tier,
    style: verbal::Style,
    exclude: &BTreeSet<String>,
) -> Option<TreeCompiled> {
    let mut pre = policy::Preimages::default();
    let abs = policy::sample_tree(rng, tier, &mut pre);
    let ks = keys::generate(rng, abs.key_count().max(1));
    let p_text = policy_string(&abs, &ks, ContextKind::Tap);
    let kvars = key_vars(&ks, ContextKind::Tap);
    let spec = verbal::spec_styled(&abs, &kvars, style);

    let (reference, baseline) = tree_descriptors_for_policy(&p_text, UNSPENDABLE_KEY).ok()?;
    let weight_of = |s: &str| -> Option<usize> {
        s.parse::<Descriptor<XOnlyPublicKey>>()
            .ok()?
            .max_weight_to_satisfy()
            .ok()
            .map(|w| w.to_wu() as usize)
    };
    let reference_weight = weight_of(&reference)?;
    let baseline_weight = weight_of(&baseline)?;
    // The task must have something to design: the tree must strictly
    // beat the single leaf on the metric.
    if baseline_weight <= reference_weight {
        return None;
    }
    if !exclude.is_empty() && exclude.contains(&reference.to_string()) {
        return None;
    }
    // Self-check: the fixture must grade its own answer key at full
    // marks, and every reference leaf must be executable (the same
    // dual-oracle discipline as write/optimize, applied per leaf).
    let fixture = TreeFixture {
        id: String::new(),
        tier,
        spec_en: spec.clone(),
        spec_family: style.family,
        atoms: abs.atom_count(),
        keys: kvars.clone(),
        unspendable_key: UNSPENDABLE_KEY.to_string(),
        reference_policy: p_text.clone(),
        reference_descriptor: reference,
        reference_weight,
        baseline_descriptor: baseline,
        baseline_weight,
        hash_preimages: preimage_hex_map(&pre),
    };
    let self_grade = bench_core::grade_tree(&fixture, &fixture.reference_descriptor);
    if !self_grade.verdict.is_equivalent() || self_grade.weight_score < 1.0 {
        return None;
    }
    let typed = typed_preimages(&pre);
    let Ok(Descriptor::Tr(tr)) = fixture
        .reference_descriptor
        .parse::<Descriptor<XOnlyPublicKey>>()
    else {
        return None;
    };
    for leaf in tr.leaves() {
        if bench_core::execution_check(ContextKind::Tap, &leaf.miniscript().encode(), &typed)
            .is_err()
        {
            return None;
        }
    }
    Some(TreeCompiled {
        policy_text: p_text,
        reference_descriptor: fixture.reference_descriptor,
        reference_weight,
        baseline_descriptor: fixture.baseline_descriptor,
        baseline_weight,
        keys: kvars,
        spec_en: spec,
        atoms: abs.atom_count(),
        preimages: preimage_hex_map(&pre),
    })
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
        let tier = tier_for(i, &params.tiers);
        let ctx = context_for(i);
        let style = style_for(params, 0x51, i);
        let c = compile_task(&mut rng, tier, ctx, false, style, &params.exclude)
            .unwrap_or_else(|| panic!("write task {i} ({tier:?}/{ctx:?}) failed to generate"));
        out.push(Fixture::Write(WriteFixture {
            id: format!("t1-{i:04}"),
            tier,
            context: ctx,
            spec_en: c.spec_en,
            spec_family: style.family,
            atoms: c.atoms,
            keys: c.keys,
            reference_policy: c.policy_text,
            reference_miniscript: c.ms_text,
            reference_script_hex: c.script_hex,
            hash_preimages: c.preimages,
        }));
    }
    for i in 0..params.optimize {
        let tier = tier_for(i, &params.tiers);
        let ctx = context_for(i);
        let style = style_for(params, 0x52, i);
        let c = compile_task(&mut rng, tier, ctx, true, style, &params.exclude)
            .unwrap_or_else(|| panic!("optimize task {i} ({tier:?}/{ctx:?}) failed to generate"));
        out.push(Fixture::Optimize(OptimizeFixture {
            id: format!("t2-{i:04}"),
            tier,
            context: ctx,
            spec_en: c.spec_en,
            spec_family: style.family,
            atoms: c.atoms,
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
    for i in 0..params.tree {
        let tier = tier_for(i, &params.tiers);
        let style = style_for(params, 0x54, i);
        let c = compile_tree_task(&mut rng, tier, style, &params.exclude)
            .unwrap_or_else(|| panic!("tree task {i} ({tier:?}) failed to generate"));
        out.push(Fixture::Tree(TreeFixture {
            id: format!("t4-{i:04}"),
            tier,
            spec_en: c.spec_en,
            spec_family: style.family,
            atoms: c.atoms,
            keys: c.keys,
            unspendable_key: UNSPENDABLE_KEY.to_string(),
            reference_policy: c.policy_text,
            reference_descriptor: c.reference_descriptor,
            reference_weight: c.reference_weight,
            baseline_descriptor: c.baseline_descriptor,
            baseline_weight: c.baseline_weight,
            hash_preimages: c.preimages,
        }));
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
            ..GenParams::default()
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
            ..GenParams::default()
        };
        let a = serde_json::to_string(&generate(&p)).unwrap();
        let b = serde_json::to_string(&generate(&p)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn tier_subset_atoms_and_families() {
        let params = GenParams {
            seed: 31,
            write: 6,
            optimize: 0,
            identify: 0,
            verbal_families: vec![1, 2],
            vary_structure: true,
            tiers: vec![Tier::Medium, Tier::Hard],
            ..GenParams::default()
        };
        let fixtures = generate(&params);
        let mut families = std::collections::BTreeSet::new();
        for (i, f) in fixtures.iter().enumerate() {
            let Fixture::Write(w) = f else {
                panic!("write only")
            };
            // Tier cycle round-robins through exactly the listed tiers.
            let expect = [Tier::Medium, Tier::Hard][i % 2];
            assert_eq!(w.tier, expect, "task {i}");
            // Atom count is recorded and within the tier budget.
            let budget = match w.tier {
                Tier::Easy => 1..=2,
                Tier::Medium => 2..=6,
                Tier::Hard => 7..=12,
            };
            assert!(budget.contains(&w.atoms), "task {i}: atoms {}", w.atoms);
            // Only the listed (non-eval) families may appear.
            assert!([1, 2].contains(&w.spec_family), "family {}", w.spec_family);
            families.insert(w.spec_family);
        }
        // With 3 families over 6 tasks, at least two distinct families
        // should appear (seed-pinned).
        assert!(families.len() >= 2, "family draw collapsed: {families:?}");
    }

    #[test]
    fn exclusion_resamples_answer_keys() {
        let base = GenParams {
            seed: 13,
            write: 4,
            optimize: 2,
            identify: 0,
            tree: 2,
            ..GenParams::default()
        };
        let eval = generate(&base);
        let keys: BTreeSet<String> = eval
            .iter()
            .filter_map(|f| match f {
                Fixture::Write(w) => Some(w.reference_script_hex.clone()),
                Fixture::Optimize(o) => Some(o.optimal_script_hex.clone()),
                Fixture::Tree(t) => Some(t.reference_descriptor.clone()),
                Fixture::Identify(_) => None,
            })
            .collect();
        // Same seed + exclusion: every colliding task must be resampled.
        let train = generate(&GenParams {
            exclude: keys.clone(),
            ..base
        });
        for f in &train {
            let hex = match f {
                Fixture::Write(w) => &w.reference_script_hex,
                Fixture::Optimize(o) => &o.optimal_script_hex,
                Fixture::Tree(t) => &t.reference_descriptor,
                Fixture::Identify(_) => continue,
            };
            assert!(
                !keys.contains(hex),
                "task {} shipped an excluded answer key",
                f.id()
            );
        }
    }

    #[test]
    fn trees_append_without_disturbing_other_kinds() {
        let base = GenParams {
            seed: 9,
            write: 3,
            optimize: 2,
            identify: 1,
            tree: 0,
            ..GenParams::default()
        };
        let without = generate(&base);
        let with = generate(&GenParams { tree: 2, ..base });
        // t1-t3 fixtures are byte-identical; trees are appended.
        assert_eq!(with.len(), without.len() + 2);
        for (a, b) in without.iter().zip(with.iter()) {
            assert_eq!(
                serde_json::to_string(a).unwrap(),
                serde_json::to_string(b).unwrap(),
                "adding trees disturbed {}",
                a.id()
            );
        }
        for (i, f) in with[without.len()..].iter().enumerate() {
            let Fixture::Tree(t) = f else {
                panic!("appended fixture is not a tree")
            };
            assert_eq!(t.id, format!("t4-{i:04}"));
            // The answer key self-grades at full marks and the
            // baseline is strictly heavier (the task is non-vacuous).
            let r = bench_core::grade_tree(t, &t.reference_descriptor);
            assert_eq!(r.weight_score, 1.0, "{:?}", r.reason);
            assert!(t.baseline_weight > t.reference_weight);
            let b = bench_core::grade_tree(t, &t.baseline_descriptor);
            assert!(b.verdict.is_equivalent());
            assert_eq!(b.weight_score, 0.0);
        }
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
