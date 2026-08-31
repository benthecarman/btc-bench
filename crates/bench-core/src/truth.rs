//! Exhaustive truth-table evaluation over rust-miniscript's semantic
//! policy. Complete for our task distribution because the atom set is
//! closed and finite: keys and hash preimages are boolean; timelocks are
//! monotone, so testing at each distinct atom value and one below it
//! (plus zero) covers every point where the function can change.

use miniscript::policy::semantic::Policy as Semantic;
use miniscript::MiniscriptKey;
use std::collections::{BTreeMap, BTreeSet};

/// One point of the truth table: which boolean atoms are satisfied, and
/// the transaction context the timelocks evaluate against.
#[derive(Clone, Debug, Default)]
pub struct TruthContext {
    /// Canonical key string (Display of the pubkey) -> signature present?
    pub keys: BTreeMap<String, bool>,
    /// Hash Display string -> preimage revealed? Covers sha256/hash160
    /// (and hash256/ripemd160 if a script ever uses them; distinct hex
    /// lengths keep the namespaces apart).
    pub hashes: BTreeMap<String, bool>,
    /// Absolute chain height the CLTV atoms compare against.
    pub height: u32,
    /// Relative age (sequence value) the CSV atoms compare against.
    pub age: u32,
}

/// The closed atom set of a (reference, candidate) pair.
#[derive(Clone, Debug, Default)]
pub struct Atoms {
    pub keys: BTreeSet<String>,
    pub hashes: BTreeSet<String>,
    pub afters: BTreeSet<u32>,
    pub olders: BTreeSet<u32>,
}

impl Atoms {
    /// Collect atoms from a semantic policy; call for both sides.
    pub fn collect<Pk: MiniscriptKey>(p: &Semantic<Pk>, out: &mut Atoms) {
        match p {
            Semantic::Unsatisfiable | Semantic::Trivial => {}
            Semantic::Key(pk) => {
                out.keys.insert(pk.to_string());
            }
            Semantic::After(t) => {
                out.afters.insert(t.to_consensus_u32());
            }
            Semantic::Older(t) => {
                out.olders.insert(t.to_consensus_u32());
            }
            Semantic::Sha256(h) => {
                out.hashes.insert(h.to_string());
            }
            Semantic::Hash256(h) => {
                out.hashes.insert(h.to_string());
            }
            Semantic::Ripemd160(h) => {
                out.hashes.insert(h.to_string());
            }
            Semantic::Hash160(h) => {
                out.hashes.insert(h.to_string());
            }
            Semantic::Thresh(th) => {
                for sub in th.data() {
                    Atoms::collect(sub, out);
                }
            }
        }
    }

    /// Test heights: each distinct absolute value, one below it, and zero.
    /// Miniscript locktimes are nonzero, so `v - 1` never underflows a
    /// real atom; saturate defensively anyway.
    pub fn heights(&self) -> Vec<u32> {
        let mut v: BTreeSet<u32> = BTreeSet::new();
        v.insert(0);
        for t in &self.afters {
            v.insert(t.saturating_sub(1));
            v.insert(*t);
        }
        v.into_iter().collect()
    }

    /// Test ages: each distinct relative value, one below it, and zero.
    pub fn ages(&self) -> Vec<u32> {
        let mut v: BTreeSet<u32> = BTreeSet::new();
        v.insert(0);
        for t in &self.olders {
            v.insert(t.saturating_sub(1));
            v.insert(*t);
        }
        v.into_iter().collect()
    }

    /// Boolean atom count (keys + preimages).
    pub fn boolean_count(&self) -> usize {
        self.keys.len() + self.hashes.len()
    }
}

/// Evaluate a semantic policy at one truth-table point.
pub fn eval<Pk: MiniscriptKey>(p: &Semantic<Pk>, ctx: &TruthContext) -> bool {
    match p {
        Semantic::Unsatisfiable => false,
        Semantic::Trivial => true,
        Semantic::Key(pk) => ctx.keys.get(&pk.to_string()).copied().unwrap_or(false),
        Semantic::After(t) => ctx.height >= t.to_consensus_u32(),
        Semantic::Older(t) => ctx.age >= t.to_consensus_u32(),
        Semantic::Sha256(h) => ctx.hashes.get(&h.to_string()).copied().unwrap_or(false),
        Semantic::Hash256(h) => ctx.hashes.get(&h.to_string()).copied().unwrap_or(false),
        Semantic::Ripemd160(h) => ctx.hashes.get(&h.to_string()).copied().unwrap_or(false),
        Semantic::Hash160(h) => ctx.hashes.get(&h.to_string()).copied().unwrap_or(false),
        Semantic::Thresh(th) => {
            let k = th.k();
            th.data().iter().filter(|sub| eval(sub, ctx)).count() >= k
        }
    }
}

/// Default (unset) atoms evaluate unsatisfied, so a candidate using an
/// atom absent from the reference is caught as a mismatch, never silently
/// passed. The `eval` lookups above implement that: `.unwrap_or(false)`.

/// Exhaustive equivalence over the combined atom space.
///
/// Soundness of the timelock point set: after/older atoms are pure
/// functions of height/age respectively, and and/or/thresh preserve
/// monotonicity, so each policy is a monotone step function per axis with
/// steps only at its own atom values. Equal output at every union
/// breakpoint (and zero) implies equality everywhere.
///
/// Returns `true` only when every point agrees. Panics never; the caller
/// bounds `boolean_count` before calling.
pub fn exhaustive_equivalent<Pk: MiniscriptKey>(
    a: &Semantic<Pk>,
    b: &Semantic<Pk>,
    atoms: &Atoms,
) -> bool {
    let bools: Vec<String> = atoms
        .keys
        .iter()
        .chain(atoms.hashes.iter())
        .cloned()
        .collect();
    let n = bools.len();
    debug_assert!(n <= 20, "atom space must be bounded by the generator");
    for height in atoms.heights() {
        for age in atoms.ages() {
            for mask in 0u64..(1u64 << n) {
                let ctx = TruthContext {
                    keys: atoms
                        .keys
                        .iter()
                        .enumerate()
                        .map(|(i, k)| (k.clone(), mask >> i & 1 == 1))
                        .collect(),
                    hashes: atoms
                        .hashes
                        .iter()
                        .enumerate()
                        .map(|(i, h)| (h.clone(), mask >> (atoms.keys.len() + i) & 1 == 1))
                        .collect(),
                    height,
                    age,
                };
                if eval(a, &ctx) != eval(b, &ctx) {
                    return false;
                }
            }
        }
    }
    true
}

/// Row-level agreement between a reference and a candidate over the
/// combined atom space, split by the reference's own value. The split
/// is what makes the derived score hack-resistant: a constant
/// candidate (always-true `OP_1`, or always-false) agrees perfectly on
/// one side and never on the other, so it can never exceed 0.5 no
/// matter how skewed the table is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Agreement {
    pub ref_true_agree: u64,
    pub ref_true_total: u64,
    pub ref_false_agree: u64,
    pub ref_false_total: u64,
}

impl Agreement {
    /// Balanced agreement in [0, 1]: the mean of the agreement rates
    /// on reference-true and reference-false rows. 1.0 exactly when
    /// the two policies are equivalent; constant candidates cap at
    /// 0.5. An empty side (unsatisfiable or trivial reference) counts
    /// as fully agreeing, so the 1.0-iff-equivalent property holds.
    pub fn balanced(&self) -> f64 {
        let rate = |agree: u64, total: u64| {
            if total == 0 {
                1.0
            } else {
                agree as f64 / total as f64
            }
        };
        (rate(self.ref_true_agree, self.ref_true_total)
            + rate(self.ref_false_agree, self.ref_false_total))
            / 2.0
    }
}

/// Walk the same truth table as [`exhaustive_equivalent`] but count
/// agreement per row instead of stopping at the first divergence.
/// `a` is the reference (its value picks the side each row counts
/// toward); `b` is the candidate.
pub fn exhaustive_agreement<Pk: MiniscriptKey>(
    a: &Semantic<Pk>,
    b: &Semantic<Pk>,
    atoms: &Atoms,
) -> Agreement {
    let n = atoms.boolean_count();
    debug_assert!(n <= 20, "atom space must be bounded by the caller");
    let mut out = Agreement::default();
    for height in atoms.heights() {
        for age in atoms.ages() {
            for mask in 0u64..(1u64 << n) {
                let ctx = TruthContext {
                    keys: atoms
                        .keys
                        .iter()
                        .enumerate()
                        .map(|(i, k)| (k.clone(), mask >> i & 1 == 1))
                        .collect(),
                    hashes: atoms
                        .hashes
                        .iter()
                        .enumerate()
                        .map(|(i, h)| (h.clone(), mask >> (atoms.keys.len() + i) & 1 == 1))
                        .collect(),
                    height,
                    age,
                };
                let (ra, rb) = (eval(a, &ctx), eval(b, &ctx));
                if ra {
                    out.ref_true_total += 1;
                    out.ref_true_agree += (ra == rb) as u64;
                } else {
                    out.ref_false_total += 1;
                    out.ref_false_agree += (ra == rb) as u64;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(keys: &[(&str, bool)], hashes: &[(&str, bool)], height: u32, age: u32) -> TruthContext {
        TruthContext {
            keys: keys.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            hashes: hashes.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            height,
            age,
        }
    }

    #[test]
    fn heights_cover_breakpoints() {
        let mut a = Atoms::default();
        a.afters.insert(500);
        a.afters.insert(1000);
        assert_eq!(a.heights(), vec![0, 499, 500, 999, 1000]);
        let mut o = Atoms::default();
        o.olders.insert(16);
        assert_eq!(o.ages(), vec![0, 15, 16]);
    }

    #[test]
    fn monotone_breakpoints_are_complete() {
        // f = after(500); g = after(501): differ only at height 500.
        let mut fa = Atoms::default();
        fa.afters.insert(500);
        let mut ga = Atoms::default();
        ga.afters.insert(501);
        let mut both = Atoms::default();
        both.afters.insert(500);
        both.afters.insert(501);
        // Simulate: eval functions of height.
        let f = |h: u32| h >= 500;
        let g = |h: u32| h >= 501;
        for h in both.heights() {
            assert_eq!(f(h) != g(h), h == 500, "difference must be caught at {h}");
        }
    }
}
