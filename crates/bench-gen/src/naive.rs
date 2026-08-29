//! The systematic de-optimizer: renders an abstract policy as a
//! deliberately naive miniscript string — right-nested `and_v` chains of
//! `v:`-wrapped leaves, left-folded `or_d`, and `thresh` expanded into an
//! or over k-subsets with no `multi`. The output is parsed with
//! `from_str_insane` (subset expansion repeats pubkeys across branches,
//! which sane parsing rejects) and verified equivalent by the oracle at
//! fixture-build time.

use crate::keys::KeySet;
use crate::policy::Abs;

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
}
