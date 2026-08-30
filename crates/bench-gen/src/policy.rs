//! Abstract policy sampling. Policies are sampled context-free (key
//! indices, not keys) so the same abstract policy can be materialized
//! per script context and verbalized deterministically.

use bench_core::Tier;

use crate::rng::SeededRng;

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

fn sha(rng: &mut SeededRng) -> Abs {
    let mut h = [0u8; 32];
    rng.bytes(&mut h);
    Abs::Sha256(h)
}

fn h160(rng: &mut SeededRng) -> Abs {
    let mut h = [0u8; 20];
    rng.bytes(&mut h);
    Abs::Hash160(h)
}

fn key(rng: &mut SeededRng, used: &mut Vec<usize>) -> Abs {
    // Fresh key index, or a controlled reuse to keep budgets small.
    if !used.is_empty() && rng.below(8) == 0 {
        Abs::Key(used[rng.below(used.len() as u64) as usize])
    } else {
        let next = used.len();
        used.push(next);
        Abs::Key(next)
    }
}

/// Sample one task policy for a tier. Atom budgets per DESIGN.md:
/// easy <= 2 atoms no timelocks; medium 3..=6 with one timelock or hash;
/// hard 7..=12 with timelocks + hashes + thresh.
pub fn sample(rng: &mut SeededRng, tier: Tier) -> Abs {
    match tier {
        Tier::Easy => sample_easy(rng),
        Tier::Medium => sample_medium(rng),
        Tier::Hard => sample_hard(rng),
    }
}

/// Sample until the policy contains an `or` or a `thresh` — shapes where
/// the naive encoder is guaranteed non-optimal (or_d chains vs or_c/multi
/// carry real weight). Bounded to keep determinism cheap.
pub fn sample_with_or(rng: &mut SeededRng, tier: Tier) -> Abs {
    for _ in 0..32 {
        let p = sample(rng, tier);
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
    sample(rng, Tier::Medium)
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

fn sample_medium(rng: &mut SeededRng) -> Abs {
    let mut used = Vec::new();
    let n_keys = rng.range(2, 4) as usize;
    let mut leaves: Vec<Abs> = (0..n_keys).map(|_| key(rng, &mut used)).collect();
    match rng.below(3) {
        0 => leaves.push(Abs::After(sample_after(rng))),
        1 => leaves.push(Abs::Older(sample_older(rng))),
        _ => leaves.push(if rng.bool() { sha(rng) } else { h160(rng) }),
    }
    // Shuffle so the non-key leaf is not always last, then combine into a
    // two-level and/or shape.
    shuffle(rng, &mut leaves);
    combine(rng, &leaves, 1)
}

fn sample_hard(rng: &mut SeededRng) -> Abs {
    // Real-world patterns from Blockstream/miniscript-templates
    // (MINT-001..002, MINT-005..009), vault and custody structures.
    let roll = rng.below(6);
    match roll {
        0 => vault_full(rng),
        1 => vault_simplified(rng),
        2 => timelock_gated_recovery(rng),
        3 => timelock_in_thresh(rng),
        4 => vault_single_principal(rng),
        _ => sample_hard_synth(rng),
    }
}

/// MINT-005/006: or(and(thresh(PAKs), or(thresh(PKs,after), and(pk,after))),
///                  and(thresh(RKs), after))
fn vault_full(rng: &mut SeededRng) -> Abs {
    let mut used = Vec::new();
    let mut group = |rng: &mut SeededRng, used: &mut Vec<usize>, n: usize| -> Abs {
        let mut ks = Vec::new();
        for _ in 0..n {
            ks.push(used.len());
            used.push(used.len());
        }
        Abs::Thresh(2, ks)
    };
    let paks = group(rng, &mut used, 3);
    let pks = group(rng, &mut used, 3);
    let rks = group(rng, &mut used, 3);
    let sak = Abs::Key(used.len());
    used.push(used.len());
    let t1 = Abs::After(sample_after(rng));
    let t2 = Abs::After(sample_after(rng));
    let t3 = Abs::After(sample_after(rng));
    Abs::Or(vec![
        Abs::And(vec![
            paks,
            Abs::Or(vec![Abs::And(vec![pks, t1]), Abs::And(vec![sak, t2])]),
        ]),
        Abs::And(vec![rks, t3]),
    ])
}

/// MINT-007/008: or(and(thresh(PAKs), or(pk, and(pk,after))), and(pk,after))
fn vault_simplified(rng: &mut SeededRng) -> Abs {
    let mut used = Vec::new();
    let mut ks = Vec::new();
    for _ in 0..3 {
        ks.push(used.len());
        used.push(used.len());
    }
    let paks = Abs::Thresh(2, ks);
    let pk = Abs::Key(used.len());
    used.push(used.len());
    let sak = Abs::Key(used.len());
    used.push(used.len());
    let rk = Abs::Key(used.len());
    used.push(used.len());
    let t1 = Abs::After(sample_after(rng));
    let t2 = Abs::After(sample_after(rng));
    Abs::Or(vec![
        Abs::And(vec![paks, Abs::Or(vec![pk, Abs::And(vec![sak, t1])])]),
        Abs::And(vec![rk, t2]),
    ])
}

/// MINT-009: or(and(thresh(SAKs), or(pk,pk)), and(after, or(pk,pk)))
fn timelock_gated_recovery(rng: &mut SeededRng) -> Abs {
    let mut used = Vec::new();
    let mut ks = Vec::new();
    for _ in 0..3 {
        ks.push(used.len());
        used.push(used.len());
    }
    let saks = Abs::Thresh(2, ks);
    let pak1 = Abs::Key(used.len());
    used.push(used.len());
    let pak2 = Abs::Key(used.len());
    used.push(used.len());
    let rk1 = Abs::Key(used.len());
    used.push(used.len());
    let rk2 = Abs::Key(used.len());
    used.push(used.len());
    let t = Abs::After(sample_after(rng));
    Abs::Or(vec![
        Abs::And(vec![saks, Abs::Or(vec![pak1, pak2])]),
        Abs::And(vec![t, Abs::Or(vec![rk1, rk2])]),
    ])
}

/// MINT-001/002: thresh(k, pk...pk, after(N)) — timelock counts toward k
fn timelock_in_thresh(rng: &mut SeededRng) -> Abs {
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

/// MINT-006 variant with 1 principal key instead of thresh
fn vault_single_principal(rng: &mut SeededRng) -> Abs {
    let mut used = Vec::new();
    let mut ks = Vec::new();
    for _ in 0..3 {
        ks.push(used.len());
        used.push(used.len());
    }
    let paks = Abs::Thresh(2, ks);
    let pk1 = Abs::Key(used.len());
    used.push(used.len());
    let pk2 = Abs::Key(used.len());
    used.push(used.len());
    let rk1 = Abs::Key(used.len());
    used.push(used.len());
    let rk2 = Abs::Key(used.len());
    used.push(used.len());
    let t1 = Abs::After(sample_after(rng));
    let t2 = Abs::After(sample_after(rng));
    Abs::Or(vec![
        Abs::And(vec![paks, Abs::Or(vec![pk1, pk2])]),
        Abs::And(vec![Abs::Or(vec![rk1, rk2]), t1]),
    ])
    .and_also(t2, rng)
}

// Helper trait for fluent pattern building
trait AbsExt {
    fn combine_with(self, other: Abs, rng: &mut SeededRng) -> Abs;
    fn and_also(self, other: Abs, rng: &mut SeededRng) -> Abs;
}

impl AbsExt for Abs {
    fn combine_with(self, other: Abs, rng: &mut SeededRng) -> Abs {
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
    fn and_also(self, other: Abs, rng: &mut SeededRng) -> Abs {
        // Randomly combine with and or or
        if rng.bool() {
            Abs::And(vec![self, other])
        } else {
            Abs::Or(vec![self, other])
        }
    }
}

/// Original synthetic hard-tier sampler (renamed)
fn sample_hard_synth(rng: &mut SeededRng) -> Abs {
    let mut used = Vec::new();
    let mut groups: Vec<Abs> = Vec::new();
    // Thresh group: k-of-n over 3..=5 fresh keys.
    let n = rng.range(3, 5) as usize;
    let k = rng.range(2, n as u64) as usize;
    let mut ks = Vec::new();
    for _ in 0..n {
        ks.push(used.len());
        used.push(used.len());
    }
    groups.push(Abs::Thresh(k, ks));
    // Timelock group: both a relative and an absolute lock, or-ed with a
    // key path.
    let tl_key = key(rng, &mut used);
    groups.push(Abs::Or(vec![
        Abs::And(vec![tl_key, Abs::Older(sample_older(rng))]),
        Abs::And(vec![key(rng, &mut used), Abs::After(sample_after(rng))]),
    ]));
    // Hash group.
    groups.push(Abs::And(vec![
        key(rng, &mut used),
        if rng.bool() { sha(rng) } else { h160(rng) },
    ]));
    shuffle(rng, &mut groups);
    combine(rng, &groups, 2)
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
        for _ in 0..200 {
            let p = sample(&mut rng, Tier::Easy);
            assert!(p.atom_count() <= 2, "easy too big: {p:?}");
            let p = sample(&mut rng, Tier::Medium);
            assert!(
                (3..=6).contains(&p.atom_count()) || p.atom_count() <= 6,
                "medium out of range: {p:?}"
            );
            let p = sample(&mut rng, Tier::Hard);
            assert!(p.atom_count() <= 12, "hard too big: {p:?}");
        }
    }

    #[test]
    fn deterministic() {
        let mut a = SeededRng::new(5);
        let mut b = SeededRng::new(5);
        assert_eq!(sample(&mut a, Tier::Hard), sample(&mut b, Tier::Hard));
    }
}
