//! The semantic equivalence oracle: decode gate + lift fast path +
//! exhaustive truth-table proof.

use bitcoin::ScriptBuf;
use miniscript::policy::Liftable;
use miniscript::{Legacy, Miniscript, ScriptContext, Segwitv0, Tap};

use crate::task::ContextKind;
use crate::truth::{exhaustive_equivalent, Atoms};

/// Defense-in-depth cap; the generator's tiers stay far below this.
const MAX_BOOLEAN_ATOMS: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Semantically equivalent to the reference.
    Equivalent,
    /// Decoded fine, but differs from the reference on some assignment.
    NotEquivalent,
    /// Not a valid, type-checked miniscript in this context.
    InvalidScript(String),
    /// Atom space exceeds the exhaustive bound (only reachable for
    /// out-of-distribution inputs; generator tiers cap at 12 atoms).
    TooLarge,
}

impl Verdict {
    pub fn is_equivalent(&self) -> bool {
        matches!(self, Verdict::Equivalent)
    }
}

/// Check `candidate` against `reference` in a specific script context.
///
/// Generic over the miniscript context; use [`check_equivalence`] for the
/// task-level dispatch by [`ContextKind`].
pub fn check_in_context<Ctx: ScriptContext>(reference: &ScriptBuf, candidate: &ScriptBuf) -> Verdict
where
    Ctx::Key: std::fmt::Display,
{
    let cand: Miniscript<Ctx::Key, Ctx> = match Miniscript::decode_consensus(candidate.as_script())
    {
        Ok(ms) => ms,
        Err(e) => return Verdict::InvalidScript(e.to_string()),
    };
    let refr: Miniscript<Ctx::Key, Ctx> = match Miniscript::decode_consensus(reference.as_script())
    {
        Ok(ms) => ms,
        Err(e) => return Verdict::InvalidScript(format!("reference failed to decode: {e}")),
    };

    let sem_cand = match cand.lift() {
        Ok(p) => p,
        Err(e) => return Verdict::InvalidScript(format!("candidate failed to lift: {e}")),
    };
    let sem_ref = match refr.lift() {
        Ok(p) => p,
        Err(e) => return Verdict::InvalidScript(format!("reference failed to lift: {e}")),
    };

    // Fast path: canonicalized structural equality.
    if sem_cand.clone().sorted() == sem_ref.clone().sorted() {
        return Verdict::Equivalent;
    }

    let mut atoms = Atoms::default();
    Atoms::collect(&sem_cand, &mut atoms);
    Atoms::collect(&sem_ref, &mut atoms);
    if atoms.boolean_count() > MAX_BOOLEAN_ATOMS {
        return Verdict::TooLarge;
    }

    if exhaustive_equivalent(&sem_cand, &sem_ref, &atoms) {
        Verdict::Equivalent
    } else {
        Verdict::NotEquivalent
    }
}

/// Task-level dispatch: check in the context named by the fixture.
pub fn check_equivalence(
    kind: ContextKind,
    reference: &ScriptBuf,
    candidate: &ScriptBuf,
) -> Verdict {
    match kind {
        ContextKind::Legacy => check_in_context::<Legacy>(reference, candidate),
        ContextKind::SegwitV0 => check_in_context::<Segwitv0>(reference, candidate),
        ContextKind::Tap => check_in_context::<Tap>(reference, candidate),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn script(hex: &str) -> ScriptBuf {
        ScriptBuf::from_hex(hex).expect("valid hex in test")
    }

    /// Real curve points: miniscript's string parser validates key
    /// encoding on-curve, so synthetic byte patterns will not parse.
    fn pk_hex(i: u8) -> String {
        [
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
            "02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5",
            "02f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9",
            "025601570cb47f238d2b0286db4a990fa0f3ba28d1a319f5e7cf55c2a2444da7cc",
            "03acd484e2f0c7f65309ad178a9f559abde09796974c57e714c35f110dfc27ccbe",
        ]
        .into_iter()
        .nth((i - 1) as usize)
        .expect("i in 1..=5")
        .to_string()
    }

    fn ms(s: &str) -> miniscript::Miniscript<bitcoin::PublicKey, Segwitv0> {
        use std::str::FromStr;
        miniscript::Miniscript::<bitcoin::PublicKey, Segwitv0>::from_str(s).unwrap()
    }

    fn ms_any(s: &str) -> miniscript::Miniscript<bitcoin::PublicKey, Segwitv0> {
        // from_str_insane: the reassociated candidate repeats a pubkey
        // across branches, which sane parsing rejects as a sanity error.
        // decode_consensus accepts it at the script level; here we only
        // need bytes to grade.
        miniscript::Miniscript::<bitcoin::PublicKey, Segwitv0>::from_str_insane(s).unwrap()
    }

    #[test]
    fn equivalent_structural_reassociation() {
        // and(A, or(B, C)) vs or(and(A, B), and(A, C)); the candidate's
        // and-arms use and_b so or_d's dissatisfiability requirement holds.
        let (a, b, c) = (pk_hex(1), pk_hex(2), pk_hex(3));
        let r = ms(&format!("and_v(v:pk({a}),or_d(pk({b}),pk({c})))"));
        let cand = ms_any(&format!(
            "or_d(and_b(pk({a}),s:pk({b})),and_b(pk({a}),s:pk({c})))"
        ));
        let verdict = check_in_context::<Segwitv0>(&r.encode(), &cand.encode());
        assert_eq!(verdict, Verdict::Equivalent);
    }

    #[test]
    fn different_key_not_equivalent() {
        let (a, b, c) = (pk_hex(1), pk_hex(2), pk_hex(3));
        let r = ms(&format!("and_v(v:pk({a}),pk({b}))"));
        let cand = ms(&format!("and_v(v:pk({a}),pk({c}))"));
        let verdict = check_in_context::<Segwitv0>(&r.encode(), &cand.encode());
        assert_eq!(verdict, Verdict::NotEquivalent);
    }

    #[test]
    fn timelock_boundary_differences_caught() {
        // after(500) vs after(501): differ exactly at height 500.
        let a = pk_hex(1);
        let r = ms(&format!("and_v(v:pk({a}),after(500))"));
        let cand = ms(&format!("and_v(v:pk({a}),after(501))"));
        let verdict = check_in_context::<Segwitv0>(&r.encode(), &cand.encode());
        assert_eq!(verdict, Verdict::NotEquivalent);
    }

    #[test]
    fn equivalent_timelock_reencoding() {
        // Same semantics, different fragments: and_v(v:pk, after) vs
        // and_v(v:after, pk) — both are A AND height >= t.
        let a = pk_hex(1);
        let r = ms(&format!("and_v(v:pk({a}),after(500))"));
        let cand = ms(&format!("and_v(v:after(500),pk({a}))"));
        let verdict = check_in_context::<Segwitv0>(&r.encode(), &cand.encode());
        assert_eq!(verdict, Verdict::Equivalent);
    }

    #[test]
    fn attacker_and_invalid_scripts() {
        let (a, b) = (pk_hex(1), pk_hex(2));
        let r = ms(&format!("and_v(v:pk({a}),pk({b}))"));
        // OP_PUSHNUM_1 is a valid miniscript (Trivial): decodes, then
        // correctly grades NotEquivalent — the always-true attack.
        let verdict = check_in_context::<Segwitv0>(&r.encode(), &script("51"));
        assert_eq!(verdict, Verdict::NotEquivalent);
        // OP_RETURN is outside the miniscript fragment set: invalid.
        let verdict = check_in_context::<Segwitv0>(&r.encode(), &script("6a"));
        assert!(matches!(verdict, Verdict::InvalidScript(_)));
    }
}
