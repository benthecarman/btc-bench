//! Deterministic English verbalization of abstract policies.
//!
//! Fixed, distinct vocabulary for relative vs absolute timelocks; same
//! policy always yields the same prose. Keys are referenced by label.

use bench_core::task::KeyVar;

use crate::policy::Abs;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Render a sub-policy as a prose clause; compound children get
/// parenthesized.
pub fn clause(p: &Abs, keys: &[KeyVar]) -> String {
    let label = |i: usize| keys[i].label.as_str();
    match p {
        Abs::Key(i) => format!("{} signs the transaction", label(*i)),
        Abs::After(t) => format!(
            "the chain has reached block height {t} (absolute timelock, OP_CHECKLOCKTIMEVERIFY)"
        ),
        Abs::Older(t) => format!(
            "{t} blocks have been mined since this output confirmed (relative timelock, OP_CHECKSEQUENCEVERIFY)"
        ),
        Abs::Sha256(h) => format!(
            "a preimage of the SHA-256 hash {} is revealed",
            hex(h)
        ),
        Abs::Hash160(h) => format!("a preimage of the HASH160 hash {} is revealed", hex(h)),
        Abs::And(v) => {
            let inner: Vec<String> =
                v.iter().map(|a| wrap(a, keys)).collect();
            format!("all of the following hold: {}", inner.join("; and "))
        }
        Abs::Or(v) => {
            let inner: Vec<String> =
                v.iter().map(|a| wrap(a, keys)).collect();
            format!("at least one of the following holds: {}", inner.join("; or "))
        }
        Abs::Thresh(k, ks) => {
            let inner: Vec<String> =
                ks.iter().map(|i| format!("{} signs the transaction", label(*i))).collect();
            format!("at least {k} of these parties sign: {}", inner.join("; "))
        }
    }
}

/// Compound children are parenthesized inside a parent's list.
fn wrap(p: &Abs, keys: &[KeyVar]) -> String {
    match p {
        Abs::Key(_) | Abs::After(_) | Abs::Older(_) | Abs::Sha256(_) | Abs::Hash160(_) => {
            clause(p, keys)
        }
        Abs::And(_) | Abs::Or(_) | Abs::Thresh(_, _) => format!("({})", clause(p, keys)),
    }
}

/// Full spec sentence for a prompt.
pub fn spec(p: &Abs, keys: &[KeyVar]) -> String {
    format!("The script can be spent when {}.", clause(p, keys))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Vec<KeyVar> {
        vec![
            KeyVar {
                label: "Alice".into(),
                pubkey: "aa".into(),
            },
            KeyVar {
                label: "Bob".into(),
                pubkey: "bb".into(),
            },
            KeyVar {
                label: "Carol".into(),
                pubkey: "cc".into(),
            },
        ]
    }

    #[test]
    fn timelock_vocabulary_is_distinct() {
        let ks = keys();
        let a = clause(&Abs::After(700_000), &ks);
        let o = clause(&Abs::Older(144), &ks);
        assert!(a.contains("absolute"));
        assert!(a.contains("OP_CHECKLOCKTIMEVERIFY"));
        assert!(o.contains("relative"));
        assert!(o.contains("OP_CHECKSEQUENCEVERIFY"));
        assert_ne!(a, o);
    }

    #[test]
    fn deterministic_and_nested() {
        let ks = keys();
        let p = Abs::And(vec![
            Abs::Key(0),
            Abs::Or(vec![Abs::Key(1), Abs::Older(300)]),
        ]);
        let s1 = spec(&p, &ks);
        let s2 = spec(&p, &ks);
        assert_eq!(s1, s2);
        assert!(s1.contains("Alice signs the transaction"));
        assert!(s1.contains("(at least one of the following holds:"));
    }
}
