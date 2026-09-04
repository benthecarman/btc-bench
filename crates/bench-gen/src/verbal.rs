//! Deterministic English verbalization of abstract policies.
//!
//! Two independent axes keep training prose away from the frozen eval
//! prose without an LLM paraphraser:
//!
//! - **Template families**: hand-written vocabularies per AST node.
//!   Family 0 is the canonical benchmark phrasing; train on other
//!   families (and hold some out entirely) so word-level template
//!   recall never scores.
//! - **Structural variation** (seeded): `and`/`or`/`thresh` are
//!   commutative, so child order in the *prose* can be permuted
//!   freely — the policy, reference script, and oracle are untouched.
//!   The root list can also render in different shapes (inline,
//!   numbered, spending-path framing). This varies the clause tree the
//!   model must parse, not just the words, so the shared template
//!   skeleton is no longer a memorizable constant.
//!
//! Every variant is authored per node, so a paraphrase can never
//! drift from the policy semantics.
//!
//! Invariants across all families and structures (test-pinned):
//! - relative vs absolute timelocks use distinct vocabulary; the
//!   formal families (0-2) also name the opcode
//!   (OP_CHECKLOCKTIMEVERIFY / OP_CHECKSEQUENCEVERIFY), the informal
//!   family (3) disambiguates linguistically, and the jargon family
//!   (4) uses the community abbreviations (CLTV / CSV);
//! - SHA-256 and HASH160 atoms name their hash function;
//! - the same policy with the same style always yields the same prose.
//!
//! Keys are referenced by label.

use bench_core::task::KeyVar;

use crate::policy::Abs;
use crate::rng::SeededRng;

/// Number of template families. Family 0 is canonical. Family 3 is
/// the informal register: casual phrasing, relative timelocks in
/// human units (1 day = 144 blocks) when they divide cleanly, and no
/// opcode names — disambiguation is linguistic ("after this output
/// confirms" vs "the chain passes block N"). Training-only, like
/// every non-zero family; it teaches the "hey, write me a script
/// that..." register the formal families never exercise. Family 4 is
/// the jargon register — the words the community actually uses
/// ("2-of-3 multisig", "timelocked until block N", "hashlock") — and
/// is EVAL-ONLY like family 0: models must map real-world vocabulary
/// to script without ever training on it through this generator.
///
/// Families 5 (contractual) and 6 (declarative) are RL-training
/// registers, held back from SFT so reinforcement learning has
/// unsaturated phrasing to work on: a model that aced its SFT
/// families scored ~1.0 on them while scoring 0.43 on family 0 at
/// matched difficulty, leaving GRPO no reward variance to learn from.
pub const FAMILIES: u32 = 7;

/// Relative timelock in human units when the block count divides a
/// day (144 blocks) cleanly; raw blocks otherwise. Teaches the
/// canonical conversion instead of hiding it.
fn human_blocks(t: u32) -> String {
    if t % 1008 == 0 {
        let w = t / 1008;
        if w == 1 {
            "a week".into()
        } else {
            format!("{w} weeks")
        }
    } else if t % 144 == 0 {
        let d = t / 144;
        if d == 1 {
            "a day".into()
        } else {
            format!("{d} days")
        }
    } else {
        format!("{t} blocks")
    }
}

/// How a spec is rendered: a template family plus optional structural
/// variation. `structure_seed: None` is the canonical fixed structure
/// (byte-stable; the eval set uses this).
#[derive(Copy, Clone, Debug)]
pub struct Style {
    pub family: u32,
    pub structure_seed: Option<u64>,
}

impl Style {
    pub fn canonical(family: u32) -> Style {
        Style {
            family,
            structure_seed: None,
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Prose order of `n` children: identity when structural variation is
/// off, a seeded permutation when on. Commutativity of and/or/thresh
/// makes any order semantically identical.
fn order(n: usize, rng: &mut Option<SeededRng>) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..n).collect();
    if let Some(r) = rng {
        for i in (1..n).rev() {
            let j = r.below((i + 1) as u64) as usize;
            idx.swap(i, j);
        }
    }
    idx
}

fn and_intro(family: u32) -> &'static str {
    match family {
        1 => "every condition below is met",
        2 => "all of these are true",
        3 => "all of this happens",
        4 => "all of the following",
        5 => "each of these requirements is satisfied",
        6 => "these all hold",
        _ => "all of the following hold",
    }
}

fn or_intro(family: u32) -> &'static str {
    match family {
        1 => "any condition below is met",
        2 => "at least one of these is true",
        3 => "one of these happens",
        4 => "any of the following",
        5 => "at least one of these requirements is satisfied",
        6 => "any one of these holds",
        _ => "at least one of the following holds",
    }
}

/// Render a sub-policy as a prose clause; compound children get
/// parenthesized.
pub fn clause(p: &Abs, keys: &[KeyVar]) -> String {
    clause_in(p, keys, 0, &mut None)
}

fn clause_in(p: &Abs, keys: &[KeyVar], family: u32, rng: &mut Option<SeededRng>) -> String {
    let label = |i: usize| keys[i].label.as_str();
    match p {
        Abs::Key(i) => match family {
            1 => format!("a valid signature from {} is provided", label(*i)),
            2 => format!("{} provides their signature", label(*i)),
            3 => format!("{} signs off", label(*i)),
            4 => format!("a signature from {}", label(*i)),
            5 => format!("the spender must present {}'s signature", label(*i)),
            6 => format!("{}'s signature is required", label(*i)),
            _ => format!("{} signs the transaction", label(*i)),
        },
        Abs::After(t) => match family {
            1 => format!(
                "the blockchain has reached height {t} (absolute timelock, OP_CHECKLOCKTIMEVERIFY)"
            ),
            2 => format!(
                "the chain tip height is {t} or greater (absolute timelock, OP_CHECKLOCKTIMEVERIFY)"
            ),
            3 => format!("the chain has passed block {t}"),
            4 => format!("a timelock until block height {t} (CLTV)"),
            5 => format!(
                "the transaction must not be mined before block {t} (absolute timelock, OP_CHECKLOCKTIMEVERIFY)"
            ),
            6 => format!("spending is permitted once block {t} is reached"),
            _ => format!(
                "the chain has reached block height {t} (absolute timelock, OP_CHECKLOCKTIMEVERIFY)"
            ),
        },
        Abs::Older(t) => match family {
            1 => format!(
                "at least {t} blocks have passed since this output confirmed (relative timelock, OP_CHECKSEQUENCEVERIFY)"
            ),
            2 => format!(
                "at least {t} blocks have elapsed since this output was mined (relative timelock, OP_CHECKSEQUENCEVERIFY)"
            ),
            3 => format!(
                "{} have gone by since this output confirmed",
                human_blocks(*t)
            ),
            4 => format!("a relative timelock of {t} blocks (CSV)"),
            5 => format!(
                "the output must have been confirmed for {t} blocks (relative timelock, OP_CHECKSEQUENCEVERIFY)"
            ),
            6 => format!("spending is permitted {t} blocks after this output confirms"),
            _ => format!(
                "{t} blocks have been mined since this output confirmed (relative timelock, OP_CHECKSEQUENCEVERIFY)"
            ),
        },
        Abs::Sha256(h) => match family {
            1 => format!("someone reveals data whose SHA-256 hash is {}", hex(h)),
            2 => format!(
                "the spender presents a preimage matching the SHA-256 hash {}",
                hex(h)
            ),
            3 => format!(
                "whoever spends shows the secret behind this SHA-256 hash: {}",
                hex(h)
            ),
            4 => format!("a SHA-256 hashlock on {}", hex(h)),
            5 => format!(
                "the spender must disclose a value whose SHA-256 digest equals {}",
                hex(h)
            ),
            6 => format!("a SHA-256 preimage of {} is required", hex(h)),
            _ => format!("a preimage of the SHA-256 hash {} is revealed", hex(h)),
        },
        Abs::Hash160(h) => match family {
            1 => format!("someone reveals data whose HASH160 hash is {}", hex(h)),
            2 => format!(
                "the spender presents a preimage matching the HASH160 hash {}",
                hex(h)
            ),
            3 => format!(
                "whoever spends shows the secret behind this HASH160 hash: {}",
                hex(h)
            ),
            4 => format!("a HASH160 hashlock on {}", hex(h)),
            5 => format!(
                "the spender must disclose a value whose HASH160 digest equals {}",
                hex(h)
            ),
            6 => format!("a HASH160 preimage of {} is required", hex(h)),
            _ => format!("a preimage of the HASH160 hash {} is revealed", hex(h)),
        },
        Abs::And(v) => {
            let ord = order(v.len(), rng);
            let inner: Vec<String> = ord.iter().map(|&i| wrap(&v[i], keys, family, rng)).collect();
            format!("{}: {}", and_intro(family), inner.join("; and "))
        }
        Abs::Or(v) => {
            let ord = order(v.len(), rng);
            let inner: Vec<String> = ord.iter().map(|&i| wrap(&v[i], keys, family, rng)).collect();
            format!("{}: {}", or_intro(family), inner.join("; or "))
        }
        Abs::Thresh(k, ks) => {
            if family == 4 {
                let ord = order(ks.len(), rng);
                let names: Vec<&str> = ord.iter().map(|&i| label(ks[i])).collect();
                return format!(
                    "a {k}-of-{} multisig between {}",
                    ks.len(),
                    names.join(", ")
                );
            }
            let (intro, item): (String, fn(&str) -> String) = match family {
                1 => (
                    format!("signatures from at least {k} of these parties are provided"),
                    |l| l.to_string(),
                ),
                2 => (format!("any {k} of the following parties sign"), |l| {
                    l.to_string()
                }),
                3 => (format!("at least {k} of this group sign off"), |l| {
                    l.to_string()
                }),
                4 => (String::new(), |l| l.to_string()),
                5 => (
                    format!("signatures from no fewer than {k} of these parties are presented"),
                    |l| l.to_string(),
                ),
                6 => (format!("{k} or more of these signatures are required"), |l| {
                    l.to_string()
                }),
                _ => (format!("at least {k} of these parties sign"), |l| {
                    format!("{l} signs the transaction")
                }),
            };
            let ord = order(ks.len(), rng);
            let inner: Vec<String> = ord.iter().map(|&i| item(label(ks[i]))).collect();
            format!("{intro}: {}", inner.join("; "))
        }
    }
}

/// Compound children are parenthesized inside a parent's list.
fn wrap(p: &Abs, keys: &[KeyVar], family: u32, rng: &mut Option<SeededRng>) -> String {
    match p {
        Abs::Key(_) | Abs::After(_) | Abs::Older(_) | Abs::Sha256(_) | Abs::Hash160(_) => {
            clause_in(p, keys, family, rng)
        }
        Abs::And(_) | Abs::Or(_) | Abs::Thresh(_, _) => {
            format!("({})", clause_in(p, keys, family, rng))
        }
    }
}

/// Root-level list shapes under structural variation. The intro names
/// the combinator explicitly, so every shape stays unambiguous.
fn root_clause(p: &Abs, keys: &[KeyVar], family: u32, rng: &mut Option<SeededRng>) -> String {
    let (v, is_or) = match p {
        Abs::Or(v) => (v, true),
        Abs::And(v) => (v, false),
        _ => return clause_in(p, keys, family, rng),
    };
    // Draw the root shape first so child rendering draws stay stable.
    // Or: inline / numbered / spending-paths. And: inline / numbered.
    let shape = match rng {
        Some(r) => r.below(if is_or { 3 } else { 2 }),
        None => 0,
    };
    match shape {
        1 => {
            let ord = order(v.len(), rng);
            let items: Vec<String> = ord
                .iter()
                .enumerate()
                .map(|(n, &i)| format!("({}) {}", n + 1, wrap(&v[i], keys, family, rng)))
                .collect();
            let intro = if is_or {
                or_intro(family)
            } else {
                and_intro(family)
            };
            format!("{intro}: {}", items.join("; "))
        }
        2 => {
            let ord = order(v.len(), rng);
            let items: Vec<String> = ord
                .iter()
                .enumerate()
                .map(|(n, &i)| {
                    format!(
                        "Path {} — {}",
                        (b'A' + n as u8) as char,
                        wrap(&v[i], keys, family, rng)
                    )
                })
                .collect();
            format!(
                "at least one of the following spending paths is satisfied: {}",
                items.join("; ")
            )
        }
        _ => clause_in(p, keys, family, rng),
    }
}

/// Full spec sentence in the canonical family (0).
pub fn spec(p: &Abs, keys: &[KeyVar]) -> String {
    spec_styled(p, keys, Style::canonical(0))
}

/// Full spec sentence in a given template family, canonical structure.
pub fn spec_with(p: &Abs, keys: &[KeyVar], family: u32) -> String {
    spec_styled(p, keys, Style::canonical(family))
}

/// Full spec sentence in a given style. Families beyond the known set
/// fall back to canonical vocabulary.
pub fn spec_styled(p: &Abs, keys: &[KeyVar], style: Style) -> String {
    let mut rng = style.structure_seed.map(SeededRng::new);
    let c = root_clause(p, keys, style.family, &mut rng);
    match style.family {
        1 => format!("Spending this output requires that {c}."),
        2 => format!("This output can be spent only if {c}."),
        3 => format!("I want the coins spendable as soon as {c}."),
        4 => format!("The output requires {c}."),
        5 => format!("Funds may be moved only if {c}."),
        6 => format!("Spending is authorized when {c}."),
        _ => format!("The script can be spent when {c}."),
    }
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
    fn timelock_vocabulary_is_distinct_in_every_family() {
        let ks = keys();
        // Formal families name the opcodes.
        for family in 0..3 {
            let a = spec_with(&Abs::After(700_000), &ks, family);
            let o = spec_with(&Abs::Older(144), &ks, family);
            assert!(a.contains("absolute"), "family {family}: {a}");
            assert!(a.contains("OP_CHECKLOCKTIMEVERIFY"), "family {family}: {a}");
            assert!(o.contains("relative"), "family {family}: {o}");
            assert!(o.contains("OP_CHECKSEQUENCEVERIFY"), "family {family}: {o}");
            assert_ne!(a, o);
        }
        // The informal family disambiguates in words, never opcodes.
        let a = spec_with(&Abs::After(700_000), &ks, 3);
        let o = spec_with(&Abs::Older(144), &ks, 3);
        assert!(a.contains("passed block 700000"), "{a}");
        assert!(o.contains("since this output confirmed"), "{o}");
        assert!(!a.contains("OP_"), "{a}");
        assert!(!o.contains("OP_"), "{o}");
        assert_ne!(a, o);
        // The jargon family uses the community abbreviations.
        let a = spec_with(&Abs::After(700_000), &ks, 4);
        let o = spec_with(&Abs::Older(144), &ks, 4);
        assert!(a.contains("(CLTV)"), "{a}");
        assert!(o.contains("(CSV)"), "{o}");
        assert_ne!(a, o);
    }

    /// Every family must render the same policy distinctly: a
    /// collision means an "unseen" register is really a seen one, and
    /// the held-out-phrasing measurements silently stop meaning
    /// anything.
    #[test]
    fn all_families_render_distinctly() {
        let ks = keys();
        let p = Abs::Or(vec![
            Abs::And(vec![Abs::Key(0), Abs::Older(1008)]),
            Abs::Thresh(2, vec![0, 1, 2]),
            Abs::Sha256([7u8; 32]),
            Abs::After(700_000),
        ]);
        let rendered: Vec<String> = (0..FAMILIES).map(|f| spec_with(&p, &ks, f)).collect();
        for i in 0..rendered.len() {
            for j in (i + 1)..rendered.len() {
                assert_ne!(rendered[i], rendered[j], "families {i} and {j} collide");
            }
        }
    }

    /// The jargon family renders thresh the way the community says it,
    /// and never appears in training pools (eval-only, like family 0
    /// — enforced by convention in the pool gen commands and pinned
    /// here as vocabulary that must stay unique to family 4).
    #[test]
    fn jargon_family_says_multisig_and_hashlock() {
        let ks = keys();
        let m = spec_with(&Abs::Thresh(2, vec![0, 1, 2]), &ks, 4);
        assert!(m.contains("2-of-3 multisig between"), "{m}");
        let h = spec_with(&Abs::Sha256([1u8; 32]), &ks, 4);
        assert!(h.contains("SHA-256 hashlock"), "{h}");
        // No other family uses these words: they are the held-out
        // vocabulary the jargon eval measures.
        for family in (0..FAMILIES).filter(|f| *f != 4) {
            let m = spec_with(&Abs::Thresh(2, vec![0, 1, 2]), &ks, family);
            assert!(!m.contains("multisig"), "family {family}: {m}");
            let h = spec_with(&Abs::Sha256([1u8; 32]), &ks, family);
            assert!(!h.contains("hashlock"), "family {family}: {h}");
        }
    }

    #[test]
    fn informal_family_uses_human_time_units() {
        let ks = keys();
        // Clean multiples render as days/weeks (1 day = 144 blocks).
        let day = spec_with(&Abs::Older(144), &ks, 3);
        assert!(day.contains("a day"), "{day}");
        let month = spec_with(&Abs::Older(4320), &ks, 3);
        assert!(month.contains("30 days"), "{month}");
        let week = spec_with(&Abs::Older(1008), &ks, 3);
        assert!(week.contains("a week"), "{week}");
        let two_weeks = spec_with(&Abs::Older(2016), &ks, 3);
        assert!(two_weeks.contains("2 weeks"), "{two_weeks}");
        // Non-multiples stay in exact blocks: no lossy rounding ever.
        let odd = spec_with(&Abs::Older(145), &ks, 3);
        assert!(odd.contains("145 blocks"), "{odd}");
    }

    #[test]
    fn hash_vocabulary_names_the_function_in_every_family() {
        let ks = keys();
        for family in 0..FAMILIES {
            let s = spec_with(&Abs::Sha256([1u8; 32]), &ks, family);
            let h = spec_with(&Abs::Hash160([2u8; 20]), &ks, family);
            assert!(s.contains("SHA-256"), "family {family}: {s}");
            assert!(h.contains("HASH160"), "family {family}: {h}");
        }
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

    #[test]
    fn family_zero_is_canonical_and_families_differ() {
        // Family 0 must stay byte-identical to the historical
        // verbalizer: eval datasets pin these strings.
        let ks = keys();
        let p = Abs::And(vec![
            Abs::Key(0),
            Abs::Or(vec![
                Abs::Key(1),
                Abs::And(vec![Abs::Thresh(2, vec![0, 1, 2]), Abs::Older(300)]),
            ]),
        ]);
        assert_eq!(spec(&p, &ks), spec_with(&p, &ks, 0));
        assert!(spec(&p, &ks).starts_with("The script can be spent when"));
        // Each family renders distinct prose for the same policy.
        let rendered: Vec<String> = (0..FAMILIES).map(|f| spec_with(&p, &ks, f)).collect();
        for i in 0..rendered.len() {
            for j in (i + 1)..rendered.len() {
                assert_ne!(rendered[i], rendered[j], "families {i} and {j} collide");
            }
        }
        // Unknown family falls back to canonical.
        assert_eq!(spec_with(&p, &ks, 99), spec(&p, &ks));
    }

    /// Every key label the policy references, with its multiplicity.
    fn label_counts(s: &str, ks: &[KeyVar]) -> Vec<usize> {
        ks.iter().map(|k| s.matches(&k.label).count()).collect()
    }

    #[test]
    fn structural_variation_permutes_but_never_drops() {
        let ks = keys();
        let p = Abs::Or(vec![
            Abs::Key(0),
            Abs::And(vec![Abs::Key(1), Abs::Older(300)]),
            Abs::And(vec![Abs::Key(2), Abs::After(700_000)]),
        ]);
        let canonical = spec_with(&p, &ks, 0);
        let mut saw_reorder = false;
        let mut shapes = std::collections::BTreeSet::new();
        for seed in 0..32u64 {
            let s = spec_styled(
                &p,
                &ks,
                Style {
                    family: 0,
                    structure_seed: Some(seed),
                },
            );
            // No branch or atom is ever dropped or duplicated.
            assert_eq!(label_counts(&s, &ks), label_counts(&canonical, &ks), "{s}");
            assert!(s.contains("OP_CHECKSEQUENCEVERIFY"), "{s}");
            assert!(s.contains("OP_CHECKLOCKTIMEVERIFY"), "{s}");
            // Same seed, same prose.
            assert_eq!(
                s,
                spec_styled(
                    &p,
                    &ks,
                    Style {
                        family: 0,
                        structure_seed: Some(seed),
                    },
                )
            );
            if let Some(alice) = s.find("Alice") {
                if let Some(bob) = s.find("Bob") {
                    if bob < alice {
                        saw_reorder = true;
                    }
                }
            }
            for marker in ["(1)", "Path A —"] {
                if s.contains(marker) {
                    shapes.insert(marker);
                }
            }
        }
        assert!(saw_reorder, "no seed reordered the or-branches");
        assert_eq!(
            shapes.len(),
            2,
            "expected numbered and path shapes across seeds: {shapes:?}"
        );
        // structure_seed: None stays canonical.
        assert_eq!(
            spec_styled(&p, &ks, Style::canonical(0)),
            canonical,
            "canonical structure drifted"
        );
    }
}
