//! The systematic de-optimizer: renders an abstract policy as a
//! deliberately naive miniscript string — right-nested `and_v` chains of
//! `v:`-wrapped leaves, left-folded `or_d`, and `thresh` expanded into an
//! or over k-subsets with no `multi`. The output is parsed with
//! `from_str_insane` (subset expansion repeats pubkeys across branches,
//! which sane parsing rejects) and verified equivalent by the oracle at
//! fixture-build time.

use crate::keys::KeySet;
use crate::policy::Abs;
use crate::rng::SeededRng;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// B-typed leaf fragment.
fn b_leaf(p: &Abs, keys: &KeySet, xonly: bool) -> String {
    let k = |i: usize| {
        if xonly {
            &keys.xonly[i]
        } else {
            &keys.compressed[i]
        }
    };
    match p {
        Abs::Key(i) => format!("pk({})", k(*i)),
        Abs::After(t) => format!("after({t})"),
        Abs::Older(t) => format!("older({t})"),
        Abs::Sha256(h) => format!("sha256({})", hex(h)),
        Abs::Hash160(h) => format!("hash160({})", hex(h)),
        Abs::And(v) => and_naive(v, keys, xonly),
        Abs::Or(v) => or_naive(v, keys, xonly),
        Abs::Thresh(kq, ks) => thresh_naive(*kq, ks, keys, xonly),
    }
}

/// V-typed leaf fragment (every leaf kind has a v: form).
fn v_leaf(p: &Abs, keys: &KeySet, xonly: bool) -> String {
    format!("v:{}", b_leaf(p, keys, xonly))
}

/// and(e1,..,en) => and_v(v:e1, and_v(v:e2, ... b(en)))
/// Right-nested; every inner and_v is B-typed, the last child stays B.
fn and_naive(v: &[Abs], keys: &KeySet, xonly: bool) -> String {
    let mut iter = v.iter().rev();
    let last = iter.next().expect("nonempty and");
    let mut acc = b_leaf(last, keys, xonly);
    for e in iter {
        acc = format!("and_v({},{})", v_form(e, keys, xonly), acc);
    }
    acc
}

/// V-typed rendering of any policy: and_v chains of v: leaves (V),
/// or_i folds (V,V -> V), thresh subsets as V chains. Used wherever a
/// V operand is required (or_i arms).
fn v_form(p: &Abs, keys: &KeySet, xonly: bool) -> String {
    match p {
        Abs::And(v) => {
            let mut iter = v.iter().rev();
            let last = iter.next().expect("nonempty and");
            let mut acc = v_form(last, keys, xonly);
            for e in iter {
                acc = format!("and_v({},{})", v_form(e, keys, xonly), acc);
            }
            acc
        }
        Abs::Or(v) => {
            let mut iter = v.iter();
            let first = iter.next().expect("nonempty or");
            let mut acc = v_form(first, keys, xonly);
            for e in iter {
                acc = format!("or_i({},{})", acc, v_form(e, keys, xonly));
            }
            acc
        }
        Abs::Thresh(k, ks) => {
            let subsets = k_subsets(ks, *k);
            let mut iter = subsets.iter();
            let first = iter.next().expect("k >= 1");
            let mut acc = subset_v(first, keys, xonly);
            for s in iter {
                acc = format!("or_i({},{})", acc, subset_v(s, keys, xonly));
            }
            acc
        }
        leaf => v_leaf(leaf, keys, xonly),
    }
}

/// or(e1,..,en): left-fold `or_d` (left child must be dissatisfiable)
/// with d-capable children first; if none is d-capable, fall back to
/// `t:or_i(...)` which is B-typed at top.
fn or_naive(v: &[Abs], keys: &KeySet, xonly: bool) -> String {
    let mut d_first: Vec<&Abs> = Vec::new();
    let mut rest: Vec<&Abs> = Vec::new();
    for e in v {
        if d_capable(e) {
            d_first.push(e);
        } else {
            rest.push(e);
        }
    }
    if !d_first.is_empty() {
        let mut iter = d_first.into_iter().chain(rest.iter().copied());
        let first = iter.next().expect("nonempty or");
        let mut acc = b_leaf(first, keys, xonly);
        for e in iter {
            acc = format!("or_d({},{})", acc, b_leaf(e, keys, xonly));
        }
        acc
    } else {
        format!("t:{}", v_form(&Abs::Or(v.to_vec()), keys, xonly))
    }
}

/// A leaf whose B-form is dissatisfiable (usable as or_d's left child).
fn d_capable(p: &Abs) -> bool {
    matches!(p, Abs::Key(_) | Abs::Sha256(_) | Abs::Hash160(_))
}

/// thresh(k, K1..Kn) => t:or_i over every k-subset, each an and_v chain
/// of v:pk leaves. Caller bounds C(n,k); see [`crate::fixtures`].
fn thresh_naive(k: usize, ks: &[usize], keys: &KeySet, xonly: bool) -> String {
    format!("t:{}", v_form(&Abs::Thresh(k, ks.to_vec()), keys, xonly))
}

/// and_v chain of v:pk leaves for one key subset (V-typed).
fn subset_v(subset: &[usize], keys: &KeySet, xonly: bool) -> String {
    let leaves: Vec<Abs> = subset.iter().map(|i| Abs::Key(*i)).collect();
    v_form(&Abs::And(leaves), keys, xonly)
}

/// All k-subsets of `ks` in lexicographic index order.
pub fn k_subsets(ks: &[usize], k: usize) -> Vec<Vec<usize>> {
    let n = ks.len();
    assert!(k >= 1 && k <= n);
    let mut out = Vec::new();
    let mut idx: Vec<usize> = (0..k).collect();
    loop {
        out.push(idx.iter().map(|i| ks[*i]).collect());
        // Next combination.
        let mut i = k;
        loop {
            if i == 0 {
                return out;
            }
            i -= 1;
            if idx[i] < n - (k - i) {
                idx[i] += 1;
                for j in i + 1..k {
                    idx[j] = idx[j - 1] + 1;
                }
                break;
            }
        }
    }
}

/// Render the whole policy as a top-level B-typed naive miniscript.
pub fn naive_string(p: &Abs, keys: &KeySet, xonly: bool) -> String {
    b_leaf(p, keys, xonly)
}

// The deterministic encoder above produces one fixed "style" of naivety;
// a model can learn that single pattern. The sampler below instead picks
// among every type-valid encoding of each policy node — and_v chains vs
// and_b vs andor, or_d vs or_b vs or_i vs or_c vs andor, thresh subsets
// vs a thresh node — so baselines present the full space of hand-written
// bloat. Candidates are validated by parsing (from_str_insane) before
// being returned; the fixture oracle remains the final arbiter.
// ---------------------------------------------------------------------------

/// W-typed (altstack) form of any policy. `a:` (TOALTSTACK) wraps any
/// B-typed form; `s:` (SWAP) additionally requires its child to take
/// exactly one stack input, so it is only used on key/hash leaves.
/// Timelock leaves and composites take `a:` around their B form.
fn s_form(p: &Abs, keys: &KeySet, xonly: bool) -> String {
    match p {
        Abs::Key(_) | Abs::Sha256(_) | Abs::Hash160(_) => {
            format!("s:{}", b_leaf(p, keys, xonly))
        }
        _ => format!("a:{}", b_leaf(p, keys, xonly)),
    }
}

fn parses_ok(s: &str, xonly: bool) -> bool {
    if xonly {
        miniscript::Miniscript::<bitcoin::XOnlyPublicKey, miniscript::Tap>::from_str_insane(s)
            .is_ok()
    } else {
        // Legacy and segwit share the PublicKey context; validity of the
        // fragment set used here is identical in both.
        miniscript::Miniscript::<bitcoin::PublicKey, miniscript::Segwitv0>::from_str_insane(s)
            .is_ok()
    }
}

/// Sample a random naive encoding of `p`. Retries with fresh structural
/// randomness until a variant parses; falls back to the deterministic
/// encoding (always valid) after `ATTEMPTS` misses.
pub fn sample_naive(rng: &mut SeededRng, p: &Abs, keys: &KeySet, xonly: bool) -> String {
    const ATTEMPTS: usize = 12;
    for _ in 0..ATTEMPTS {
        let candidate = b_sampled(rng, p, keys, xonly);
        if parses_ok(&candidate, xonly) {
            return candidate;
        }
    }
    naive_string(p, keys, xonly)
}

/// B-typed sampled rendering of any policy.
fn b_sampled(rng: &mut SeededRng, p: &Abs, keys: &KeySet, xonly: bool) -> String {
    match p {
        Abs::And(v) => and_sampled(rng, v, keys, xonly),
        Abs::Or(v) => or_sampled(rng, v, keys, xonly),
        Abs::Thresh(k, ks) => thresh_sampled(rng, *k, ks, keys, xonly),
        leaf => b_leaf(leaf, keys, xonly),
    }
}

/// and(e1,..,en): right fold with a random type-valid encoding per step.
///   0: and_v(v:e1, B(rest))        (deterministic style)
///   1: and_b(B(e1), sv:e2)         altstack conjunction
///   2: andor(B(e1), B(e2), 0)      and_n shape, needs e1 dissatisfiable
fn and_sampled(rng: &mut SeededRng, v: &[Abs], keys: &KeySet, xonly: bool) -> String {
    if v.len() < 2 {
        return v
            .first()
            .map(|e| b_sampled(rng, e, keys, xonly))
            .unwrap_or_else(|| "1".into());
    }
    let (head, rest) = v.split_first().expect("len >= 2");
    let rest_b = if rest.len() == 1 {
        b_sampled(rng, &rest[0], keys, xonly)
    } else {
        and_sampled(rng, rest, keys, xonly)
    };
    match rng.below(3) {
        0 => format!("and_v({},{})", v_form(head, keys, xonly), rest_b),
        1 => format!(
            "and_b({},{})",
            b_sampled(rng, head, keys, xonly),
            s_form(&Abs::And(rest.to_vec()), keys, xonly)
        ),
        _ => format!("andor({},{},0)", b_sampled(rng, head, keys, xonly), rest_b),
    }
}

/// or(e1,..,en): right fold with a random type-valid encoding per step.
///   0: or_d(B(e1), B(rest))         (deterministic style, e1 d-capable)
///   1: or_b(B(e1), sv:rest)         altstack disjunction
///   2: t:or_i(v:e1, v:rest)         IF/ELSE disjunction
///   3: andor(B(e1), 1, B(rest))     or via andor
///   4: t:or_c(B(e1), v:rest)        NOTIF disjunction
fn or_sampled(rng: &mut SeededRng, v: &[Abs], keys: &KeySet, xonly: bool) -> String {
    if v.len() < 2 {
        return v
            .first()
            .map(|e| b_sampled(rng, e, keys, xonly))
            .unwrap_or_else(|| "0".into());
    }
    let (head, rest) = v.split_first().expect("len >= 2");
    let rest_b = if rest.len() == 1 {
        b_sampled(rng, &rest[0], keys, xonly)
    } else {
        or_sampled(rng, rest, keys, xonly)
    };
    match rng.below(5) {
        0 => format!("or_d({},{})", b_sampled(rng, head, keys, xonly), rest_b),
        1 => format!(
            "or_b({},{})",
            b_sampled(rng, head, keys, xonly),
            s_form(&Abs::Or(rest.to_vec()), keys, xonly)
        ),
        2 => format!(
            "t:or_i({},{})",
            v_form(head, keys, xonly),
            v_or_form(rng, rest, keys, xonly)
        ),
        3 => format!("andor({},1,{})", b_sampled(rng, head, keys, xonly), rest_b),
        _ => format!(
            "t:or_c({},{})",
            b_sampled(rng, head, keys, xonly),
            v_form(&Abs::Or(rest.to_vec()), keys, xonly)
        ),
    }
}

/// V-typed rendering of an or-chain's remainder, as or_i folds.
fn v_or_form(_rng: &mut SeededRng, v: &[Abs], keys: &KeySet, xonly: bool) -> String {
    if v.len() == 1 {
        return v_form(&v[0], keys, xonly);
    }
    let (head, rest) = v.split_first().expect("nonempty");
    format!(
        "or_i({},{})",
        v_form(head, keys, xonly),
        v_or_form(_rng, rest, keys, xonly)
    )
}

/// thresh(k, K1..Kn): random naive encoding.
///   0: t:or_i over every k-subset          (deterministic style)
///   1: or_d over every k-subset, and_b arms
///   2: thresh(k, pk, a:pk, ...) node        no multi
///   3: andor fold over every k-subset
fn thresh_sampled(
    rng: &mut SeededRng,
    k: usize,
    ks: &[usize],
    keys: &KeySet,
    xonly: bool,
) -> String {
    match rng.below(4) {
        0 => thresh_naive(k, ks, keys, xonly),
        1 => {
            let subsets = k_subsets(ks, k);
            let arms: Vec<String> = subsets.iter().map(|s| subset_d(s, keys, xonly)).collect();
            arms.into_iter()
                .reduce(|a, b| format!("or_d({a},{b})"))
                .unwrap_or_else(|| "0".into())
        }
        2 => {
            let key = |i: usize| {
                if xonly {
                    &keys.xonly[i]
                } else {
                    &keys.compressed[i]
                }
            };
            let mut parts = Vec::with_capacity(ks.len());
            for (idx, i) in ks.iter().enumerate() {
                if idx == 0 {
                    parts.push(format!("pk({})", key(*i)));
                } else {
                    parts.push(format!("a:pk({})", key(*i)));
                }
            }
            format!("thresh({k},{})", parts.join(","))
        }
        _ => {
            let subsets = k_subsets(ks, k);
            let arms: Vec<String> = subsets.iter().map(|s| subset_d(s, keys, xonly)).collect();
            arms.into_iter()
                .reduce(|a, b| format!("andor({a},1,{b})"))
                .unwrap_or_else(|| "0".into())
        }
    }
}

/// B-typed, dissatisfiable conjunction for one key subset:
/// and_b(pk(k1), a:pk(k2), ...) nested — the naive k-of-n arm whose
/// dissatisfaction is expressible (usable as or_d/andor operands).
fn subset_d(subset: &[usize], keys: &KeySet, xonly: bool) -> String {
    let key = |i: usize| {
        if xonly {
            &keys.xonly[i]
        } else {
            &keys.compressed[i]
        }
    };
    let mut iter = subset.iter();
    let first = iter.next().expect("k >= 1");
    let mut acc = format!("pk({})", key(*first));
    for k in iter {
        acc = format!("and_b({},a:pk({}))", acc, key(*k));
    }
    acc
}

/// Cap on subset expansion: C(n,k) above this skips the policy.
pub const MAX_SUBSETS: usize = 12;

pub fn subset_count(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    let mut c = 1u64;
    for i in 0..k {
        c = c * (n - i) as u64 / (i + 1) as u64;
    }
    c as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsets_enumerate_exactly() {
        let s = k_subsets(&[0, 1, 2], 2);
        assert_eq!(s, vec![vec![0, 1], vec![0, 2], vec![1, 2]]);
        assert_eq!(subset_count(5, 2), 10);
        assert_eq!(subset_count(4, 4), 1);
    }

    #[test]
    fn naive_shapes() {
        use bench_core::task::KeyVar;
        let keys = KeySet {
            labels: vec!["Alice".into(), "Bob".into()],
            compressed: vec!["02aa".into(), "02bb".into()],
            xonly: vec!["aa".into(), "bb".into()],
        };
        let kv = vec![
            KeyVar {
                label: "Alice".into(),
                pubkey: "02aa".into(),
            },
            KeyVar {
                label: "Bob".into(),
                pubkey: "02bb".into(),
            },
        ];
        let _ = kv;
        let p = Abs::And(vec![Abs::Key(0), Abs::Key(1), Abs::Older(144)]);
        assert_eq!(
            naive_string(&p, &keys, false),
            "and_v(v:pk(02aa),and_v(v:pk(02bb),older(144)))"
        );
        let p = Abs::Or(vec![Abs::Key(0), Abs::Key(1)]);
        assert_eq!(naive_string(&p, &keys, false), "or_d(pk(02aa),pk(02bb))");
    }

    #[test]
    fn sampled_naive_valid_equivalent_and_diverse() {
        use crate::keys;
        use crate::rng::SeededRng;
        use bench_core::task::ContextKind;
        use bench_core::{check_equivalence, Verdict};
        use bitcoin::hex::FromHex;
        use std::collections::BTreeSet;
        use std::str::FromStr;

        let policies = [
            Abs::Or(vec![Abs::Key(0), Abs::Key(1)]),
            Abs::And(vec![Abs::Key(0), Abs::Key(1), Abs::Older(144)]),
            Abs::Or(vec![
                Abs::And(vec![Abs::Key(0), Abs::After(600000)]),
                Abs::And(vec![Abs::Key(1), Abs::Key(2)]),
            ]),
            Abs::Thresh(2, vec![0, 1, 2]),
            Abs::And(vec![Abs::Or(vec![Abs::Key(0), Abs::Key(1)]), Abs::Key(2)]),
        ];
        let mut rng = SeededRng::new(4242);
        let mut total_distinct = 0usize;
        for xonly in [false, true] {
            for p in &policies {
                let ks = keys::generate(&mut rng, 3);
                // Reference: deterministic encoding, compiled-context
                // oracle target.
                let reference = naive_string(p, &ks, xonly);
                let ref_script =
                    bitcoin::ScriptBuf::from_hex(&render_hex(&reference, xonly)).expect("ref");
                let mut seen: BTreeSet<String> = BTreeSet::new();
                for _ in 0..24 {
                    let s = sample_naive(&mut rng, p, &ks, xonly);
                    // 1. Parses as insane miniscript (guaranteed by the
                    //    sampler, checked here as a contract).
                    let hex = render_hex(&s, xonly);
                    let script = bitcoin::ScriptBuf::from_hex(&hex).expect("encodes");
                    // 2. Oracle-proven equivalent to the reference.
                    let kind = if xonly {
                        ContextKind::Tap
                    } else {
                        ContextKind::SegwitV0
                    };
                    assert_eq!(
                        check_equivalence(kind, &ref_script, &script),
                        Verdict::Equivalent,
                        "not equivalent: {s}"
                    );
                    seen.insert(s);
                }
                // 3. Diversity: shapes whose arms are all dissatisfiable
                //    (keys/hashes) admit or_d/or_b/andor/or_c encodings;
                //    timelock-carrying arms legitimately force t:or_i
                //    only. Assert variety where the type system allows
                //    it, and aggregate variety across the batch.
                let has_timelock_arm = contains_timelock(p);
                let floor = if has_timelock_arm { 1 } else { 3 };
                assert!(
                    seen.len() >= floor,
                    "too little variety ({}) for {p:?}",
                    seen.len()
                );
                total_distinct += seen.len();
            }
        }
        assert!(
            total_distinct >= 24,
            "aggregate variety too low: {total_distinct}"
        );
    }

    fn contains_timelock(p: &Abs) -> bool {
        match p {
            Abs::After(_) | Abs::Older(_) => true,
            Abs::And(v) | Abs::Or(v) => v.iter().any(contains_timelock),
            _ => false,
        }
    }

    #[test]
    fn sampled_naive_deterministic_per_seed() {
        let ks = crate::keys::generate(&mut SeededRng::new(9), 3);
        let p = Abs::Or(vec![Abs::And(vec![Abs::Key(0), Abs::Key(1)]), Abs::Key(2)]);
        let a = sample_naive(&mut SeededRng::new(77), &p, &ks, false);
        let b = sample_naive(&mut SeededRng::new(77), &p, &ks, false);
        assert_eq!(a, b);
    }

    /// Encode a miniscript string to script hex via the crate itself.
    fn render_hex(ms: &str, xonly: bool) -> String {
        use std::str::FromStr;
        if xonly {
            miniscript::Miniscript::<bitcoin::XOnlyPublicKey, miniscript::Tap>::from_str_insane(ms)
                .expect("valid xonly ms")
                .encode()
                .to_hex_string()
        } else {
            miniscript::Miniscript::<bitcoin::PublicKey, miniscript::Segwitv0>::from_str_insane(ms)
                .expect("valid ms")
                .encode()
                .to_hex_string()
        }
    }
}
