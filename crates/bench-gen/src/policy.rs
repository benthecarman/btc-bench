//! Abstract policy sampling. Policies are sampled context-free (key
//! indices, not keys) so the same abstract policy can be materialized
//! per script context and verbalized deterministically.

use bench_core::Tier;

use crate::rng::SeededRng;
use bitcoin::hashes::Hash as _;

/// Context-free policy AST. Key(i) references the i-th key of the task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Abs {
    Key(usize),
    After(u32),
    Older(u32),
    Sha256([u8; 32]),
    Hash160([u8; 20]),
    And(Vec<Abs>),
    Or(Vec<Abs>),
    /// k-of-n over keys; the compiler renders `multi`/`multi_a`.
    Thresh(usize, Vec<usize>),
}

impl Abs {
    /// Count of boolean atoms (keys + hash preimages).
    pub fn atom_count(&self) -> usize {
        match self {
            Abs::Key(_) | Abs::Sha256(_) | Abs::Hash160(_) => 1,
            Abs::After(_) | Abs::Older(_) => 0, // timelocks: not boolean atoms
            Abs::And(v) | Abs::Or(v) => v.iter().map(|a| a.atom_count()).sum(),
            Abs::Thresh(_, ks) => ks.len(),
        }
    }

    pub fn key_count(&self) -> usize {
        match self {
            Abs::Key(_) => 1,
            Abs::Sha256(_) | Abs::Hash160(_) | Abs::After(_) | Abs::Older(_) => 0,
            Abs::And(v) | Abs::Or(v) => v.iter().map(|a| a.key_count()).sum(),
            Abs::Thresh(_, ks) => ks.len(),
        }
    }
}

fn sample_after(rng: &mut SeededRng) -> u32 {
    rng.range(600_000, 900_000) as u32
}

fn sample_older(rng: &mut SeededRng) -> u32 {
    // Block-based CSV, 16..=1024, avoiding 0 and the low zero-value trap.
    rng.range(16, 1024) as u32
}

/// Preimages sampled alongside hash atoms, so the execution oracle can
/// witness hash leaves: sha256(preimage) == the atom's hash.
#[derive(Clone, Debug, Default)]
pub struct Preimages {
    pub sha256: Vec<([u8; 32], [u8; 32])>,
    pub hash160: Vec<([u8; 20], [u8; 32])>,
}

fn sha(rng: &mut SeededRng, pre: &mut Preimages) -> Abs {
    let mut preimage = [0u8; 32];
    rng.bytes(&mut preimage);
    let hash = bitcoin::hashes::sha256::Hash::hash(&preimage).to_byte_array();
    pre.sha256.push((hash, preimage));
    Abs::Sha256(hash)
}

fn h160(rng: &mut SeededRng, pre: &mut Preimages) -> Abs {
    let mut preimage = [0u8; 32];
    rng.bytes(&mut preimage);
    let hash = bitcoin::hashes::hash160::Hash::hash(&preimage).to_byte_array();
    pre.hash160.push((hash, preimage));
    Abs::Hash160(hash)
}

fn key(_rng: &mut SeededRng, used: &mut Vec<usize>) -> Abs {
    // Always a fresh key index. An earlier "controlled reuse" path
    // produced duplicate-key policies, which the sane compiler rejects
    // in every context — every reused-key draw was dead rng burn.
    let next = used.len();
    used.push(next);
    Abs::Key(next)
}

/// Sample one task policy for a tier, recording hash preimages. Atom
/// budgets per DESIGN.md: easy <= 2 atoms no timelocks; medium 2..=6
/// atoms with a timelock or hash always present (the 2-atom band is
/// the calibration step above easy); hard 7..=12 with timelocks +
/// hashes + thresh.
pub fn sample_pre(rng: &mut SeededRng, tier: Tier, pre: &mut Preimages) -> Abs {
    match tier {
        Tier::Easy => sample_easy(rng),
        Tier::Medium => sample_medium(rng, pre),
        Tier::Hard => sample_hard(rng, pre),
    }
}

/// Sample one task policy (preimages discarded).
pub fn sample(rng: &mut SeededRng, tier: Tier) -> Abs {
    sample_pre(rng, tier, &mut Preimages::default())
}

/// Sample until the policy contains an `or` or a `thresh` — shapes where
/// the naive encoder is guaranteed non-optimal (or_d chains vs or_c/multi
/// carry real weight). Bounded to keep determinism cheap.
pub fn sample_with_or_pre(rng: &mut SeededRng, tier: Tier, pre: &mut Preimages) -> Abs {
    for _ in 0..32 {
        let p = sample_pre(rng, tier, pre);
        let ok = match &p {
            Abs::Or(_) | Abs::Thresh(..) => true,
            Abs::And(v) => v.iter().any(|a| matches!(a, Abs::Or(_) | Abs::Thresh(..))),
            _ => false,
        };
        if ok {
            return p;
        }
    }
    // Fallback: medium shapes nearly always contain an or.
    sample_medium(rng, pre)
}

/// [`sample_with_or_pre`] with preimages discarded.
pub fn sample_with_or(rng: &mut SeededRng, tier: Tier) -> Abs {
    sample_with_or_pre(rng, tier, &mut Preimages::default())
}
fn sample_easy(rng: &mut SeededRng) -> Abs {
    let mut used = Vec::new();
    let a = key(rng, &mut used);
    let b = key(rng, &mut used);
    if rng.bool() {
        Abs::And(vec![a, b])
    } else {
        Abs::Or(vec![a, b])
    }
}

fn sample_medium(rng: &mut SeededRng, pre: &mut Preimages) -> Abs {
    // Gradient band: synth shapes (2-4 keys + one timelock/hash leaf)
    // plus the surviving MINT structure (timelock_in_thresh, 3..5
    // atoms) at one draw in four. The 2-key+leaf synth variant is the
    // deliberate step between easy (2 keys, nothing else) and the
    // 3+-key shapes: calibration showed a cliff, not a gradient,
    // without it.
    match rng.below(4) {
        2 => timelock_in_thresh(rng),
        _ => sample_medium_synth(rng, pre),
    }
}

pub(crate) fn sample_medium_synth(rng: &mut SeededRng, pre: &mut Preimages) -> Abs {
    let mut used = Vec::new();
    // 2..=4 keys: the 2-key draws are the gradient step above easy
    // (a non-key atom is always present, so the task is never just
    // "and/or of two keys" — that is the easy tier).
    let n_keys = rng.range(2, 4) as usize;
    let mut leaves: Vec<Abs> = (0..n_keys).map(|_| key(rng, &mut used)).collect();
    match rng.below(3) {
        0 => leaves.push(Abs::After(sample_after(rng))),
        1 => leaves.push(Abs::Older(sample_older(rng))),
        _ => leaves.push(if rng.bool() {
            sha(rng, pre)
        } else {
            h160(rng, pre)
        }),
    }
    // Shuffle so the non-key leaf is not always last, then combine into a
    // two-level and/or shape.
    shuffle(rng, &mut leaves);
    combine(rng, &leaves, 1)
}

fn sample_hard(rng: &mut SeededRng, pre: &mut Preimages) -> Abs {
    // Every shape here is census-verified to ship in at least one
    // context (see the shape_census test): the four MINT vault
    // structures (vault_full, vault_simplified, timelock_gated_recovery,
    // vault_single_principal) and the old synthetic sampler were removed
    // — all died at the pk_h/RawPkH gate in every context (thresh groups
    // under or-branches make the compiler emit pk_h, which cannot lift),
    // so they never shipped and only burned retries. timelock_in_thresh
    // (MINT-001/002) survives, dispatched from sample_medium.
    match rng.below(4) {
        0 => boundary_timelocks(rng),
        1 => recovery_paths(rng),
        2 => deep_nest(rng, pre),
        _ => wide_thresh(rng),
    }
}

/// Custody-style recovery structure: an instant path gated by a
/// relative timelock and a delayed path through a 2-of-3 committee,
/// itself behind an absolute timelock. Thresh only under and-branches
/// (never directly under an or) — the pattern the compiler compiles
/// without pk_h. Eight atoms.
pub(crate) fn recovery_paths(rng: &mut SeededRng) -> Abs {
    let mut used = Vec::new();
    let fresh = |used: &mut Vec<usize>| -> usize {
        let next = used.len();
        used.push(next);
        next
    };
    let (k0, k1) = (fresh(&mut used), fresh(&mut used));
    let committee = Abs::Thresh(
        2,
        vec![fresh(&mut used), fresh(&mut used), fresh(&mut used)],
    );
    let (k5, k6, k7) = (fresh(&mut used), fresh(&mut used), fresh(&mut used));
    Abs::Or(vec![
        Abs::And(vec![
            Abs::And(vec![Abs::Key(k0), Abs::Key(k1)]),
            Abs::Older(sample_older(rng)),
        ]),
        Abs::And(vec![
            Abs::And(vec![Abs::Key(k5), Abs::Key(k6)]),
            Abs::And(vec![
                Abs::Key(k7),
                Abs::And(vec![committee, Abs::After(sample_after(rng))]),
            ]),
        ]),
    ])
}

/// Edge shape: absolute timelocks hugging the height/time consensus
/// boundary from below. 499999999 is the LAST height-encoded CLTV
/// value (the BIP65 threshold 500000000 is inclusive: values >= it are
/// UNIX timestamps), so every value here stays height-typed while
/// sitting one block from the boundary. Seven atoms to meet the hard
/// tier's 7..=12 budget.
pub(crate) fn boundary_timelocks(rng: &mut SeededRng) -> Abs {
    let mut used = Vec::new();
    let boundaries = [499_999_996u32, 499_999_997, 499_999_998, 499_999_999];
    let i = rng.below(4) as usize;
    let j = (i + 1 + rng.below(3) as usize) % 4;
    // Binary or with seven key atoms. Three-branch ors push the
    // compiler toward pk_h left arms (cheaper dissatisfaction), which
    // decode as RawPkH and cannot lift — the shape would be silently
    // starved by the gradability resample.
    let fresh = |used: &mut Vec<usize>| -> usize {
        let next = used.len();
        used.push(next);
        next
    };
    let kp0 = Abs::And(vec![Abs::Key(fresh(&mut used)), Abs::Key(fresh(&mut used))]);
    let branch1 = Abs::And(vec![kp0, Abs::After(boundaries[i])]);
    let mut ks = Vec::new();
    for _ in 0..3 {
        ks.push(fresh(&mut used));
    }
    let kp1 = Abs::And(vec![
        Abs::Thresh(2, ks),
        Abs::And(vec![Abs::Key(fresh(&mut used)), Abs::Key(fresh(&mut used))]),
    ]);
    Abs::Or(vec![
        branch1,
        Abs::And(vec![kp1, Abs::After(boundaries[j])]),
    ])
}

/// Edge shape: deep and-nesting over mixed atom kinds with a single
/// embedded key-or — four levels, the deepest the tier budgets allow.
/// One timelock kind per policy (height+relative in a single path
/// cannot compile). Structured rather than randomly combined: random
/// or-heavy trees push the compiler into pk_h (RawPkH cannot lift),
/// which starved the shape to a 5/30 ship rate. Seven atoms (six keys
/// + one hash) to meet the hard tier's floor.
pub(crate) fn deep_nest(rng: &mut SeededRng, pre: &mut Preimages) -> Abs {
    let mut used = Vec::new();
    for _ in 0..6 {
        let _ = key(rng, &mut used);
    }
    let tl = if rng.bool() {
        Abs::After(sample_after(rng))
    } else {
        Abs::Older(sample_older(rng))
    };
    let hash = if rng.bool() {
        sha(rng, pre)
    } else {
        h160(rng, pre)
    };
    Abs::And(vec![
        Abs::And(vec![
            Abs::Or(vec![Abs::Key(0), Abs::Key(1)]),
            Abs::And(vec![Abs::Key(2), Abs::Key(3)]),
        ]),
        Abs::And(vec![
            tl,
            Abs::And(vec![Abs::Key(4), Abs::And(vec![Abs::Key(5), hash])]),
        ]),
    ])
}

/// Edge shape: k-of-n at the subset-expansion cap, so the naive
/// baseline's k-subset enumeration runs at its widest. Fresh keys only
/// (repeated keys across branches fail the sane compiler); the
/// timelock branch carries two keys so every variant clears the hard
/// tier's 7-atom floor.
pub(crate) fn wide_thresh(rng: &mut SeededRng) -> Abs {
    let mut used = Vec::new();
    // (n, k) pairs with C(n, k) <= MAX_SUBSETS (12).
    let (n, k) = [(5usize, 2usize), (6, 5), (7, 6), (5, 3)][rng.below(4) as usize];
    let mut ks = Vec::with_capacity(n);
    for _ in 0..n {
        ks.push(used.len());
        used.push(used.len());
    }
    let tl = if rng.bool() {
        Abs::After(sample_after(rng))
    } else {
        Abs::Older(sample_older(rng))
    };
    Abs::Or(vec![
        Abs::Thresh(k, ks),
        Abs::And(vec![key(rng, &mut used), key(rng, &mut used), tl]),
    ])
}

/// MINT-001/002: thresh(k, pk...pk, after(N)) — timelock counts toward k.
pub(crate) fn timelock_in_thresh(rng: &mut SeededRng) -> Abs {
    let mut used = Vec::new();
    let n_keys = rng.range(3, 5) as usize;
    let mut ks = Vec::new();
    for _ in 0..n_keys {
        ks.push(used.len());
        used.push(used.len());
    }
    let k = rng.range(2, n_keys as u64) as usize;
    // Timelock counts as one of the k conditions
    Abs::Thresh(k, ks).combine_with(
        if rng.bool() {
            Abs::After(sample_after(rng))
        } else {
            Abs::Older(sample_older(rng))
        },
        rng,
    )
}

/// Sample a taproot tree-task policy: a root-level disjunction whose
/// branches become the key path and tapleaves. Every branch carries at
/// least one key (compile_tr rejects signature-free paths), exactly
/// one branch is a bare key (the key-path candidate), and branch
/// counts scale with tier while staying inside the oracle's boolean
/// atom budget (<= 12; the NUMS internal key is pinned out).
pub fn sample_tree(rng: &mut SeededRng, tier: Tier, pre: &mut Preimages) -> Abs {
    let mut used = Vec::new();
    let branch =
        |rng: &mut SeededRng, used: &mut Vec<usize>, pre: &mut Preimages, allow_thresh: bool| {
            match rng.below(if allow_thresh { 5 } else { 4 }) {
                0 => Abs::And(vec![key(rng, used), key(rng, used)]),
                1 => Abs::And(vec![key(rng, used), Abs::Older(sample_older(rng))]),
                2 => Abs::And(vec![key(rng, used), Abs::After(sample_after(rng))]),
                3 => {
                    let h = if rng.bool() {
                        sha(rng, pre)
                    } else {
                        h160(rng, pre)
                    };
                    Abs::And(vec![key(rng, used), h])
                }
                _ => {
                    let n = 3;
                    let k = rng.range(2, 3) as usize;
                    let mut ks = Vec::with_capacity(n);
                    for _ in 0..n {
                        ks.push(used.len());
                        used.push(used.len());
                    }
                    Abs::Thresh(k, ks)
                }
            }
        };
    let n_branches = match tier {
        Tier::Easy => 2,
        Tier::Medium => rng.range(3, 4) as usize,
        Tier::Hard => rng.range(5, 6) as usize,
    };
    let mut branches = Vec::with_capacity(n_branches);
    // One bare-key branch: the key-path candidate compile_tr extracts.
    branches.push(key(rng, &mut used));
    let mut thresh_used = false;
    for _ in 1..n_branches {
        // At most one thresh branch keeps hard policies inside the
        // atom budget (thresh carries 3 keys; other branches <= 2).
        let allow = !thresh_used && tier != Tier::Easy;
        let b = branch(rng, &mut used, pre, allow);
        thresh_used |= matches!(b, Abs::Thresh(..));
        branches.push(b);
    }
    shuffle(rng, &mut branches);
    Abs::Or(branches)
}

// Helper trait for fluent pattern building
trait AbsExt {
    fn combine_with(self, other: Abs, rng: &mut SeededRng) -> Abs;
}

impl AbsExt for Abs {
    fn combine_with(self, other: Abs, _rng: &mut SeededRng) -> Abs {
        // Rebuild thresh with the timelock as an additional condition
        if let Abs::Thresh(k, ref ks) = self {
            // For MINT-001: the policy is thresh(k, keys...) where the
            // timelock is one of the k conditions. We model this as
            // and(thresh(k, keys), timelock) since the miniscript
            // concrete-policy parser rejects mixed thresh with timelocks.
            Abs::And(vec![Abs::Thresh(k, ks.clone()), other])
        } else {
            Abs::And(vec![self, other])
        }
    }
}

/// Combine leaves into nested and/or shapes up to `depth`.
fn combine(rng: &mut SeededRng, leaves: &[Abs], depth: u32) -> Abs {
    if leaves.len() == 1 {
        return leaves[0].clone();
    }
    if depth == 0 {
        // Binary right fold: the policy grammar's and/or are binary.
        let f: fn(Vec<Abs>) -> Abs = if rng.bool() { Abs::And } else { Abs::Or };
        let (last, rest) = leaves.split_last().expect("nonempty");
        return f(vec![combine(rng, rest, 0), last.clone()]);
    }
    let mid = 1 + rng.below((leaves.len() - 1) as u64) as usize;
    let (l, r) = leaves.split_at(mid);
    let a = combine(rng, l, depth - 1);
    let b = combine(rng, r, depth - 1);
    if rng.bool() {
        Abs::And(vec![a, b])
    } else {
        Abs::Or(vec![a, b])
    }
}

fn shuffle<T>(rng: &mut SeededRng, v: &mut [T]) {
    for i in (1..v.len()).rev() {
        let j = rng.below((i + 1) as u64) as usize;
        v.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_respected() {
        let mut rng = SeededRng::new(99);
        for _ in 0..300 {
            let p = sample(&mut rng, Tier::Easy);
            assert!(p.atom_count() <= 2, "easy too big: {p:?}");
            let p = sample(&mut rng, Tier::Medium);
            assert!(
                (2..=6).contains(&p.atom_count()),
                "medium out of range: {p:?}"
            );
            let p = sample(&mut rng, Tier::Hard);
            assert!(
                (7..=12).contains(&p.atom_count()),
                "hard out of range: {p:?}"
            );
        }
    }

    #[test]
    fn edge_shapes_meet_hard_budget() {
        let mut rng = SeededRng::new(12);
        let mut pre = Preimages::default();
        for _ in 0..50 {
            for p in [
                boundary_timelocks(&mut rng),
                deep_nest(&mut rng, &mut pre),
                wide_thresh(&mut rng),
            ] {
                assert!(
                    (7..=12).contains(&p.atom_count()),
                    "edge shape out of hard budget: {p:?}"
                );
                // Boundary values must all be height-encoded (< 500000000).
                assert_after_heights_only(&p);
            }
        }
    }

    fn assert_after_heights_only(p: &Abs) {
        match p {
            Abs::After(t) => assert!(*t < 500_000_000, "time-typed after({t}) in boundary shape"),
            Abs::And(v) | Abs::Or(v) => v.iter().for_each(assert_after_heights_only),
            _ => {}
        }
    }

    #[test]
    fn deterministic() {
        let mut a = SeededRng::new(5);
        let mut b = SeededRng::new(5);
        assert_eq!(sample(&mut a, Tier::Hard), sample(&mut b, Tier::Hard));
    }

    #[test]
    fn tree_policies_shape_and_budget() {
        let mut rng = SeededRng::new(41);
        let mut pre = Preimages::default();
        for _ in 0..100 {
            for (tier, branches) in [
                (Tier::Easy, 2..=2),
                (Tier::Medium, 3..=4),
                (Tier::Hard, 5..=6),
            ] {
                let p = sample_tree(&mut rng, tier, &mut pre);
                let Abs::Or(v) = &p else {
                    panic!("tree policy must be a root or: {p:?}")
                };
                assert!(
                    branches.contains(&v.len()),
                    "{tier:?}: {} branches",
                    v.len()
                );
                assert!(p.atom_count() <= 12, "over budget: {p:?}");
                // Exactly one bare-key branch (the key-path candidate);
                // every branch requires a signature.
                let bare = v.iter().filter(|b| matches!(b, Abs::Key(_))).count();
                assert_eq!(bare, 1, "{p:?}");
                for b in v {
                    assert!(b.key_count() >= 1, "signature-free branch: {b:?}");
                }
            }
        }
    }
}
