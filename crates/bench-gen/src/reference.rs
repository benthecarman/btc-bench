//! Searched answer keys for the weight-scored tasks (optimize, tree).
//!
//! The compiler is a weak reference for a worst-case weight metric: it
//! minimizes expected satisfaction cost under the policy's odds, never
//! regroups keys into `multi`, and the old tree reference put one
//! compiled leaf per branch in a balanced tree. Models beat both
//! (bench-s42-lite, 2026-10-10: Opus 5.5 on 6/48 optimize and 17/50
//! tree tasks, by up to 32 WU), and the clamped score hid it. This
//! module searches a small space of equivalent encodings and keeps the
//! lightest that passes the same gates as any answer key: it decodes
//! and lifts (gradable), is oracle-equivalent to the plain compile,
//! and passes the execution oracle.
//!
//! The search is a heuristic, not a proof of optimality. Grade
//! summaries count answers lighter than the reference so the search
//! can be strengthened when models outdo it.
//!
//! Fixtures record which reference builder made their answer key in
//! `reference_search`: 0 = the compiler (optimize) or the balanced
//! one-leaf-per-branch tree (tree), [`SEARCH_VERSION`] = this module.
//! The audit re-derives keys with the builder the fixture names, so
//! existing datasets keep auditing and grading exactly as before.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::str::FromStr;
use std::sync::Arc;

use bench_core::task::{ContextKind, Fixture};
use bench_core::{check_equivalence, execution_check, weights_for, HashPreimages, Verdict};
use bitcoin::{PublicKey, ScriptBuf, XOnlyPublicKey};
use miniscript::policy::{Concrete, Liftable};
use miniscript::{
    Descriptor, Legacy, Miniscript, MiniscriptKey, ScriptContext, Segwitv0, Tap, Threshold,
};

/// The `reference_search` value of fixtures built by this module.
pub const SEARCH_VERSION: u32 = 1;

/// Upper bound on compiled policy variants per optimize task.
const MAX_VARIANTS: usize = 96;
/// Upper bound on leaf-set combinations per tree task.
const MAX_COMBOS: usize = 4096;
/// Upper bound on leaves a single split may produce.
const MAX_SPLIT: usize = 20;
/// Odds tried on each binary `or`: equal, then skewed either way. The
/// compiler optimizes expected cost; skewing the odds moves it toward
/// encodings that are cheaper in the worst case.
const ODDS: [(usize, usize); 3] = [(1, 1), (1, 10), (10, 1)];

/// A searched optimize reference.
#[derive(Clone, Debug)]
pub struct SearchedScript {
    pub miniscript: String,
    pub script: ScriptBuf,
    pub weight: usize,
    pub size: usize,
}

/// Number of rewrite sites in a policy: `or`/`and` nodes with at least
/// two bare-key children, which can become `thresh(1, ..)` /
/// `thresh(n, ..)` (and so `multi`/`multi_a`).
fn key_group_sites<Pk: MiniscriptKey>(p: &Concrete<Pk>) -> usize {
    let keys = |v: &mut dyn Iterator<Item = &Arc<Concrete<Pk>>>| {
        v.filter(|c| matches!(***c, Concrete::Key(_))).count()
    };
    match p {
        Concrete::Or(v) => {
            usize::from(keys(&mut v.iter().map(|(_, c)| c)) >= 2)
                + v.iter().map(|(_, c)| key_group_sites(c)).sum::<usize>()
        }
        Concrete::And(v) => {
            usize::from(keys(&mut v.iter()) >= 2)
                + v.iter().map(|c| key_group_sites(c)).sum::<usize>()
        }
        Concrete::Thresh(t) => t.iter().map(|c| key_group_sites(c)).sum(),
        _ => 0,
    }
}

/// Number of binary `or` nodes (odds sites).
fn or_sites<Pk: MiniscriptKey>(p: &Concrete<Pk>) -> usize {
    match p {
        Concrete::Or(v) => {
            usize::from(v.len() == 2) + v.iter().map(|(_, c)| or_sites(c)).sum::<usize>()
        }
        Concrete::And(v) => v.iter().map(|c| or_sites(c)).sum(),
        Concrete::Thresh(t) => t.iter().map(|c| or_sites(c)).sum(),
        _ => 0,
    }
}

/// Group the bare-key children of an `or`/`and` into one threshold.
fn group_keys<Pk: MiniscriptKey>(
    children: Vec<Arc<Concrete<Pk>>>,
    is_or: bool,
) -> Option<(Arc<Concrete<Pk>>, Vec<Arc<Concrete<Pk>>>)> {
    let (keys, rest): (Vec<_>, Vec<_>) = children
        .into_iter()
        .partition(|c| matches!(**c, Concrete::Key(_)));
    let k = if is_or { 1 } else { keys.len() };
    let t = Threshold::new(k, keys).ok()?;
    Some((Arc::new(Concrete::Thresh(t)), rest))
}

/// One policy variant: `groups` selects which key-group sites become
/// thresholds (bit i = site i, in walk order) and `odds` the odds
/// choice per binary `or` (base-3 digits, walk order). Semantics are
/// unchanged: or(keys) = thresh(1, keys), and(keys) = thresh(n, keys),
/// and odds carry no meaning.
fn variant<Pk: MiniscriptKey>(
    p: &Concrete<Pk>,
    groups: u64,
    gi: &mut u32,
    odds: &mut u64,
) -> Option<Concrete<Pk>> {
    Some(match p {
        Concrete::Or(v) => {
            let kids: Vec<Arc<Concrete<Pk>>> = v
                .iter()
                .map(|(_, c)| variant(c, groups, gi, odds).map(Arc::new))
                .collect::<Option<_>>()?;
            let site = kids
                .iter()
                .filter(|c| matches!(***c, Concrete::Key(_)))
                .count()
                >= 2;
            let regroup = site && {
                let on = groups >> *gi & 1 == 1;
                *gi += 1;
                on
            };
            let weights = if v.len() == 2 {
                let (a, b) = ODDS[(*odds % 3) as usize];
                *odds /= 3;
                vec![a, b]
            } else {
                vec![1; v.len()]
            };
            if regroup {
                let (t, rest) = group_keys(kids, true)?;
                if rest.is_empty() {
                    return Some((*t).clone());
                }
                let mut all = vec![(1, t)];
                all.extend(rest.into_iter().map(|c| (1, c)));
                Concrete::Or(all)
            } else {
                Concrete::Or(weights.into_iter().zip(kids).collect())
            }
        }
        Concrete::And(v) => {
            let kids: Vec<Arc<Concrete<Pk>>> = v
                .iter()
                .map(|c| variant(c, groups, gi, odds).map(Arc::new))
                .collect::<Option<_>>()?;
            let site = kids
                .iter()
                .filter(|c| matches!(***c, Concrete::Key(_)))
                .count()
                >= 2;
            let regroup = site && {
                let on = groups >> *gi & 1 == 1;
                *gi += 1;
                on
            };
            if regroup {
                let (t, rest) = group_keys(kids, false)?;
                if rest.is_empty() {
                    return Some((*t).clone());
                }
                let mut all = vec![t];
                all.extend(rest);
                Concrete::And(all)
            } else {
                Concrete::And(kids)
            }
        }
        Concrete::Thresh(t) => {
            let kids: Vec<Arc<Concrete<Pk>>> = t
                .iter()
                .map(|c| variant(c, groups, gi, odds).map(Arc::new))
                .collect::<Option<_>>()?;
            Concrete::Thresh(Threshold::new(t.k(), kids).ok()?)
        }
        other => other.clone(),
    })
}

/// Every policy variant to compile, plain policy first, capped at
/// [`MAX_VARIANTS`] in a fixed enumeration order.
fn variants<Pk: MiniscriptKey>(p: &Concrete<Pk>) -> Vec<Concrete<Pk>> {
    let g = key_group_sites(p).min(6) as u32;
    let o = or_sites(p).min(8) as u32;
    let mut out = Vec::new();
    'outer: for groups in 0..(1u64 << g) {
        for odds in 0..3u64.pow(o) {
            if out.len() >= MAX_VARIANTS {
                break 'outer;
            }
            let (mut gi, mut od) = (0, odds);
            if let Some(v) = variant(p, groups, &mut gi, &mut od) {
                out.push(v);
            }
        }
    }
    out
}

fn compiled<Ctx: ScriptContext>(variants: &[Concrete<Ctx::Key>]) -> Vec<(String, ScriptBuf)>
where
    Ctx::Key: MiniscriptKey,
{
    variants
        .iter()
        .filter_map(|v| v.compile::<Ctx>().ok())
        .map(|ms: Miniscript<Ctx::Key, Ctx>| (ms.to_string(), ms.encode()))
        .collect()
}

/// The lightest gradable, equivalent, executable encoding of an
/// optimize task's policy among the compiled variants. The plain
/// compile is always a candidate, so the result never weighs more
/// than it.
pub fn optimize_reference(
    context: ContextKind,
    policy: &str,
    preimages: &HashPreimages,
) -> Result<SearchedScript, String> {
    let candidates = match context {
        ContextKind::Legacy => {
            let p = Concrete::<PublicKey>::from_str(policy).map_err(|e| e.to_string())?;
            compiled::<Legacy>(&variants(&p))
        }
        ContextKind::SegwitV0 => {
            let p = Concrete::<PublicKey>::from_str(policy).map_err(|e| e.to_string())?;
            compiled::<Segwitv0>(&variants(&p))
        }
        ContextKind::Tap => {
            let p = Concrete::<XOnlyPublicKey>::from_str(policy).map_err(|e| e.to_string())?;
            compiled::<Tap>(&variants(&p))
        }
    };
    let Some((_, plain)) = candidates.first().cloned() else {
        return Err("policy does not compile".into());
    };
    let mut weighed: Vec<(usize, usize, String, ScriptBuf)> = candidates
        .into_iter()
        .filter_map(|(ms, s)| {
            let w = weights_for(context, &s).ok()?;
            Some((w.weight, w.size, ms, s))
        })
        .collect();
    weighed.sort_by(|a, b| (a.0, a.1, a.3.as_bytes()).cmp(&(b.0, b.1, b.3.as_bytes())));
    weighed.dedup_by(|a, b| a.3 == b.3);
    for (weight, size, ms, script) in weighed {
        // Same gates as the plain compile's: gradable (decode + lift),
        // equivalent to the policy as compiled, executable.
        if check_equivalence(context, &script, &script) != Verdict::Equivalent
            || check_equivalence(context, &plain, &script) != Verdict::Equivalent
            || execution_check(context, &script, preimages).is_err()
        {
            continue;
        }
        return Ok(SearchedScript {
            miniscript: ms,
            script,
            weight,
            size,
        });
    }
    Err("no gradable encoding".into())
}

type Pol = Concrete<XOnlyPublicKey>;

/// Flatten a root-level or-chain into branches.
pub(crate) fn flatten_or(p: &Pol, out: &mut Vec<Pol>) {
    if let Concrete::Or(subs) = p {
        for (_, sub) in subs {
            flatten_or(sub, out);
        }
    } else {
        out.push(p.clone());
    }
}

fn subsets(n: usize, k: usize) -> Vec<Vec<usize>> {
    (0u32..(1 << n))
        .filter(|m| m.count_ones() as usize == k)
        .map(|m| (0..n).filter(|i| m >> i & 1 == 1).collect())
        .collect()
}

fn and_of(parts: Vec<Arc<Pol>>) -> Pol {
    if parts.len() == 1 {
        (*parts[0]).clone()
    } else {
        Concrete::And(parts)
    }
}

/// Ways to spread one branch over leaves: as a single leaf; a k-of-n
/// threshold (alone or under an `and`) as one leaf per k-subset; an
/// `or` under an `and` distributed into one leaf per alternative.
fn leaf_sets(branch: &Pol) -> Vec<Vec<Pol>> {
    let mut out = vec![vec![branch.clone()]];
    let (split, rest): (Option<Pol>, Vec<Arc<Pol>>) = match branch {
        Concrete::Thresh(_) => (Some(branch.clone()), vec![]),
        Concrete::And(v) => {
            match v
                .iter()
                .position(|c| matches!(**c, Concrete::Thresh(_) | Concrete::Or(_)))
            {
                Some(i) => (
                    Some((*v[i]).clone()),
                    v.iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .map(|(_, c)| c.clone())
                        .collect(),
                ),
                None => (None, vec![]),
            }
        }
        _ => (None, vec![]),
    };
    let alternatives: Vec<Vec<Arc<Pol>>> = match &split {
        Some(Concrete::Thresh(t)) if t.k() > 1 && t.k() < t.n() && t.n() <= 16 => {
            let kids: Vec<Arc<Pol>> = t.iter().cloned().collect();
            subsets(kids.len(), t.k())
                .into_iter()
                .map(|s| s.into_iter().map(|i| kids[i].clone()).collect())
                .collect()
        }
        Some(Concrete::Or(v)) if !rest.is_empty() => {
            v.iter().map(|(_, c)| vec![c.clone()]).collect()
        }
        _ => vec![],
    };
    if (2..=MAX_SPLIT).contains(&alternatives.len()) {
        out.push(
            alternatives
                .into_iter()
                .map(|mut parts| {
                    parts.extend(rest.iter().cloned());
                    and_of(parts)
                })
                .collect(),
        );
    }
    out
}

/// A tap tree under construction: leaf index or a pair.
enum Node {
    Leaf(usize),
    Pair(Box<Node>, Box<Node>),
}

fn render(node: &Node, leaves: &[String]) -> String {
    match node {
        Node::Leaf(i) => leaves[*i].clone(),
        Node::Pair(a, b) => format!("{{{},{}}}", render(a, leaves), render(b, leaves)),
    }
}

/// The shape minimizing max(leaf cost + 32 × depth): repeatedly merge
/// the two cheapest subtrees into one costing max(a, b) + 32 (each
/// level adds one 32-byte hash to the control block). Ties break on
/// creation order, so the shape is deterministic.
fn minimax_tree(costs: &[usize]) -> Node {
    let mut nodes: Vec<Option<Node>> = (0..costs.len()).map(|i| Some(Node::Leaf(i))).collect();
    let mut heap: BinaryHeap<Reverse<(usize, usize)>> = costs
        .iter()
        .enumerate()
        .map(|(i, &c)| Reverse((c, i)))
        .collect();
    while heap.len() > 1 {
        let Reverse((ca, a)) = heap.pop().expect("len > 1");
        let Reverse((cb, b)) = heap.pop().expect("len > 1");
        let pair = Node::Pair(
            Box::new(nodes[a].take().expect("live")),
            Box::new(nodes[b].take().expect("live")),
        );
        nodes.push(Some(pair));
        heap.push(Reverse((ca.max(cb) + 32, nodes.len() - 1)));
    }
    let Reverse((_, root)) = heap.pop().expect("at least one leaf");
    nodes[root].take().expect("live")
}

fn descriptor_weight(text: &str) -> Option<usize> {
    text.parse::<Descriptor<XOnlyPublicKey>>()
        .ok()?
        .max_weight_to_satisfy()
        .ok()
        .map(|w| w.to_wu() as usize)
}

/// The lightest verified tr() descriptor for a tree task's policy among
/// the searched designs. `balanced` (the version-0 reference) is
/// always a candidate, so the result never weighs more than it.
pub fn tree_reference(
    policy: &str,
    unspendable_key: &str,
    balanced: &str,
    preimages: &HashPreimages,
) -> Result<String, String> {
    let concrete = Pol::from_str(policy).map_err(|e| e.to_string())?;
    let mut branches = Vec::new();
    flatten_or(&concrete, &mut branches);
    let key_at = branches
        .iter()
        .position(|b| matches!(b, Concrete::Key(_)))
        .ok_or("no bare-key branch for the key path")?;
    let Concrete::Key(internal) = branches.remove(key_at) else {
        unreachable!("position matched a key branch")
    };
    let expected = concrete.lift().map_err(|e| e.to_string())?;

    // Per-branch options, each a list of (leaf text, cost at depth 0).
    let leaf_cost = |p: &Pol| -> Option<(String, usize)> {
        let ms = p.compile::<Tap>().ok()?;
        let text = ms.to_string();
        let w = descriptor_weight(&format!("tr({unspendable_key},{text})"))?;
        Some((text, w))
    };
    let options: Vec<Vec<Vec<(String, usize)>>> = branches
        .iter()
        .map(|b| {
            leaf_sets(b)
                .iter()
                .filter_map(|set| set.iter().map(leaf_cost).collect::<Option<Vec<_>>>())
                .collect()
        })
        .collect();
    if options.iter().any(|o| o.is_empty()) {
        return Err("a branch does not compile".into());
    }
    let combos: usize = options.iter().map(|o| o.len()).product();

    let mut candidates = vec![balanced.to_string()];
    for c in 0..combos.min(MAX_COMBOS) {
        let mut x = c;
        let mut leaves: Vec<(String, usize)> = Vec::new();
        for o in &options {
            leaves.extend(o[x % o.len()].iter().cloned());
            x /= o.len();
        }
        let texts: Vec<String> = leaves.iter().map(|l| l.0.clone()).collect();
        let costs: Vec<usize> = leaves.iter().map(|l| l.1).collect();
        let tree = render(&minimax_tree(&costs), &texts);
        candidates.push(format!("tr({internal},{tree})"));
    }
    let mut weighed: Vec<(usize, String)> = candidates
        .into_iter()
        .filter_map(|d| descriptor_weight(&d).map(|w| (w, d)))
        .collect();
    weighed.sort();
    weighed.dedup();
    for (_, d) in weighed {
        let Ok(Descriptor::Tr(tr)) = d.parse::<Descriptor<XOnlyPublicKey>>() else {
            continue;
        };
        let Ok(lifted) = Descriptor::Tr(tr.clone()).lift() else {
            continue;
        };
        if !bench_core::check_semantic(&expected, &lifted, Some(unspendable_key)).is_equivalent() {
            continue;
        }
        if tr.leaves().any(|l| {
            execution_check(ContextKind::Tap, &l.miniscript().encode(), preimages).is_err()
        }) {
            continue;
        }
        return Ok(d);
    }
    Err("no verified tree design".into())
}

/// The same fixture with its optimize or tree answer key rebuilt by
/// this module. Other kinds, fixtures already at [`SEARCH_VERSION`],
/// and trees the search cannot read (authored policies without a
/// bare-key branch) come back unchanged.
pub fn rereference(f: &Fixture) -> Result<Fixture, String> {
    let mut f = f.clone();
    match &mut f {
        Fixture::Optimize(o) if o.reference_search == 0 => {
            let pre = HashPreimages::from_hex_map(&o.hash_preimages)?;
            let r = optimize_reference(o.context, &o.reference_policy, &pre)
                .map_err(|e| format!("{}: {e}", o.id))?;
            if r.weight > o.optimal_weight {
                return Err(format!("{}: searched reference is heavier", o.id));
            }
            o.optimal_script_hex = r.script.to_hex_string();
            o.optimal_weight = r.weight;
            o.optimal_size = r.size;
            o.reference_miniscript = r.miniscript;
            o.reference_search = SEARCH_VERSION;
        }
        Fixture::Tree(t) if t.reference_search == 0 => {
            let pre = HashPreimages::from_hex_map(&t.hash_preimages)?;
            let Ok(d) = tree_reference(
                &t.reference_policy,
                &t.unspendable_key,
                &t.reference_descriptor,
                &pre,
            ) else {
                return Ok(f);
            };
            let w = descriptor_weight(&d).ok_or_else(|| format!("{}: unweighable", t.id))?;
            if w > t.reference_weight {
                return Err(format!("{}: searched reference is heavier", t.id));
            }
            t.reference_descriptor = d;
            t.reference_weight = w;
            t.reference_search = SEARCH_VERSION;
        }
        _ => {}
    }
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn or_of_two_keys_finds_multi() {
        // The compiler gives or_b; regrouping into thresh(1, ..) gives
        // multi(1, ..), one byte shorter with the same witness.
        let ks = crate::keys::generate(&mut crate::rng::SeededRng::new(3), 2);
        let policy = format!("or(pk({}),pk({}))", ks.compressed[0], ks.compressed[1]);
        let plain = Concrete::<PublicKey>::from_str(&policy)
            .unwrap()
            .compile::<Segwitv0>()
            .unwrap();
        let plain_w = weights_for(ContextKind::SegwitV0, &plain.encode()).unwrap();
        let r =
            optimize_reference(ContextKind::SegwitV0, &policy, &HashPreimages::default()).unwrap();
        assert!(r.miniscript.starts_with("multi(1,"), "{}", r.miniscript);
        assert!(r.weight < plain_w.weight);
    }

    #[test]
    fn minimax_puts_heavy_leaf_shallow() {
        let t = minimax_tree(&[300, 100, 100, 100]);
        let leaves: Vec<String> = ["H", "a", "b", "c"].iter().map(|s| s.to_string()).collect();
        // The heavy leaf ends at depth 1; the light ones share a subtree.
        assert_eq!(render(&t, &leaves), "{{c,{a,b}},H}");
    }
}
