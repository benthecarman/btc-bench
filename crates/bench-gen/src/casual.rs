//! Casual prompt wrappers: the "hey, write me a script that..."
//! register. The bench's formal scaffold (Rules block, notation
//! rules) never varies, so a model can bind the skill to the
//! scaffold; these wrappers strip it down to how a person actually
//! asks, while the reference answers stay the same oracle-verified
//! canonical forms.
//!
//! Templates are split: `Split::Train` templates go into SFT
//! exports, `Split::Eval` templates are held out for measuring the
//! register (the same discipline as verbal family 0 vs training
//! families). The template id is drawn from the seed, so wrapping is
//! deterministic per task.
//!
//! Only write and tree tasks get casual wrappers — those are the
//! "ask for a script" demos. Optimize and identify keep the formal
//! prompt.

use bench_core::task::{Fixture, TreeFixture, WriteFixture};

use crate::rng::SeededRng;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Split {
    Train,
    Eval,
}

/// Casual context nouns; the script kind stays stated (a request
/// without it is genuinely ambiguous).
fn noun(f: &WriteFixture) -> &'static str {
    match f.context {
        bench_core::task::ContextKind::Legacy => "P2SH redeem script",
        bench_core::task::ContextKind::SegwitV0 => "P2WSH witness script",
        bench_core::task::ContextKind::Tap => "tapscript (taproot leaf script)",
    }
}

fn key_lines(keys: &[bench_core::task::KeyVar], style: u64) -> String {
    match style {
        0 => keys
            .iter()
            .map(|k| format!("{} = {}", k.label, k.pubkey))
            .collect::<Vec<_>>()
            .join(", "),
        1 => keys
            .iter()
            .map(|k| format!("{}'s key is {}.", k.label, k.pubkey))
            .collect::<Vec<_>>()
            .join(" "),
        _ => keys
            .iter()
            .map(|k| format!("- {}: {}", k.label, k.pubkey))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

pub fn write_prompt(f: &WriteFixture, seed: u64, split: Split) -> String {
    let mut rng = SeededRng::new(seed ^ 0xCA5A_CA5A);
    let template = match split {
        Split::Train => rng.below(3),
        Split::Eval => 3 + rng.below(2),
    };
    let keys = key_lines(&f.keys, rng.below(3));
    let spec = &f.spec_en;
    let n = noun(f);
    match template {
        0 => format!(
            "hey, can you write me a bitcoin script for this? it should be a {n}.\n\n\
             {spec}\n\nkeys: {keys}\n\nhex or asm is fine"
        ),
        1 => format!(
            "need a {n} that does the following. {spec}\n\n{keys}\n\njust give me the script"
        ),
        2 => format!("write a bitcoin script ({n}):\n\n{spec}\n\nthe keys:\n{keys}"),
        3 => format!(
            "help me lock up some coins. i want a {n}. here's the deal: {spec}\n\n\
             keys are {keys}"
        ),
        _ => {
            format!("can you put together a {n} for me?\n\n{spec}\n\n{keys}\n\nscript only please")
        }
    }
}

pub fn tree_prompt(f: &TreeFixture, seed: u64, split: Split) -> String {
    let mut rng = SeededRng::new(seed ^ 0x7AB7_7AB7);
    let template = match split {
        Split::Train => rng.below(3),
        Split::Eval => 3 + rng.below(2),
    };
    let keys = f
        .keys
        .iter()
        .map(|k| format!("{} = {}", k.label, k.pubkey))
        .collect::<Vec<_>>()
        .join(", ");
    let spec = &f.spec_en;
    let nums = &f.unspendable_key;
    match template {
        0 => format!(
            "hey, design me a taproot output. {spec}\n\nkeys: {keys}\n\
             (if no path fits the key path, the NUMS key is {nums})\n\n\
             answer with a tr(...) descriptor"
        ),
        1 => format!(
            "i need a taproot setup for this: {spec}\n\n{keys}\n\
             NUMS if you need it: {nums}\n\ngive me the tr() descriptor"
        ),
        2 => format!(
            "put together a taproot output for me. {spec}\n\nkeys are {keys}; \
             unspendable key {nums} if nothing belongs on the key path. \
             tr(...) descriptor please"
        ),
        3 => format!(
            "taproot design question. {spec}\n\nkeys: {keys}. NUMS: {nums}.\n\
             what's the tr() descriptor?"
        ),
        _ => format!(
            "can you sketch the taproot output for this? {spec}\n\n{keys}\n\
             (NUMS = {nums})\n\nanswer as a tr(...) descriptor"
        ),
    }
}

/// Casual prompt for a fixture, when the kind has one.
pub fn prompt_for(f: &Fixture, seed: u64, split: Split) -> Option<String> {
    match f {
        Fixture::Write(w) => Some(write_prompt(w, seed, split)),
        Fixture::Tree(t) => Some(tree_prompt(t, seed, split)),
        Fixture::Optimize(_) | Fixture::Identify(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{generate, GenParams};

    fn sample_fixtures() -> Vec<Fixture> {
        generate(&GenParams {
            seed: 21,
            write: 3,
            optimize: 0,
            identify: 0,
            tree: 2,
            verbal_families: vec![3],
            ..GenParams::default()
        })
    }

    #[test]
    fn casual_prompts_are_deterministic_and_complete() {
        for (i, f) in sample_fixtures().iter().enumerate() {
            let seed = i as u64;
            let p1 = prompt_for(f, seed, Split::Train).expect("write/tree");
            let p2 = prompt_for(f, seed, Split::Train).expect("write/tree");
            assert_eq!(p1, p2, "same seed must wrap identically");
            // The spec and every key must survive the wrapper.
            match f {
                Fixture::Write(w) => {
                    assert!(p1.contains(&w.spec_en), "{p1}");
                    for k in &w.keys {
                        assert!(p1.contains(&k.pubkey), "{p1}");
                    }
                }
                Fixture::Tree(t) => {
                    assert!(p1.contains(&t.spec_en), "{p1}");
                    assert!(p1.contains(&t.unspendable_key), "{p1}");
                    assert!(
                        p1.contains("tr("),
                        "tree asks must name the answer form: {p1}"
                    );
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn train_and_eval_templates_never_collide() {
        // Across many seeds, no train wrapping equals any eval
        // wrapping for the same fixture: held-out scaffolds stay
        // held out.
        for f in sample_fixtures() {
            for seed in 0..24u64 {
                let train = prompt_for(&f, seed, Split::Train).unwrap();
                for eval_seed in 0..24u64 {
                    let eval = prompt_for(&f, eval_seed, Split::Eval).unwrap();
                    assert_ne!(train, eval);
                }
            }
        }
    }

    #[test]
    fn formal_kinds_get_no_casual_prompt() {
        let fixtures = generate(&GenParams {
            seed: 21,
            write: 0,
            optimize: 1,
            identify: 1,
            ..GenParams::default()
        });
        for f in fixtures {
            assert!(prompt_for(&f, 0, Split::Train).is_none(), "{}", f.id());
        }
    }
}
