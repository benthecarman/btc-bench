//! Display/parse round-trip property: every script the bench ever
//! shows a model (write references via tool echo, optimize baselines
//! and optima in prompts, identify spk/inner scripts, tree leaves)
//! must survive to_human_asm -> parse_script_answer byte-identically.
//!
//! This is the guard for the dialect-asymmetry bug class: the display
//! renderer and the answer parser are two dialects of the same
//! notation, and any token one side reads differently than the other
//! writes silently changes script semantics for a model that echoes
//! what it was shown. The concrete instance this pins: even-length
//! decimal timelocks (`36 OP_CSV`) once parsed as hex 0x36 = 54 and
//! broke 21% of displayed scripts.

use bench_core::answer::parse_script_answer;
use bench_core::human_asm::to_human_asm;
use bench_core::task::Fixture;
use bench_gen::fixtures::{generate, GenParams};
use bitcoin::ScriptBuf;

fn displayed_scripts(f: &Fixture) -> Vec<(String, String)> {
    match f {
        Fixture::Write(w) => vec![(w.id.clone(), w.reference_script_hex.clone())],
        Fixture::Optimize(o) => vec![
            (format!("{}-baseline", o.id), o.baseline_script_hex.clone()),
            (format!("{}-optimal", o.id), o.optimal_script_hex.clone()),
        ],
        Fixture::Identify(i) => {
            let mut out = vec![(format!("{}-spk", i.id), i.spk_hex.clone())];
            if let Some(inner) = &i.inner_script_hex {
                out.push((format!("{}-inner", i.id), inner.clone()));
            }
            out
        }
        Fixture::Tree(t) => {
            let d: miniscript::Descriptor<bitcoin::XOnlyPublicKey> =
                t.reference_descriptor.parse().expect("fixture descriptor");
            let miniscript::Descriptor::Tr(tr) = d else {
                panic!("tree fixture is not tr()")
            };
            tr.leaves()
                .enumerate()
                .map(|(j, leaf)| {
                    (
                        format!("{}-leaf{j}", t.id),
                        leaf.miniscript().encode().to_hex_string(),
                    )
                })
                .collect()
        }
    }
}

#[test]
fn every_displayed_script_roundtrips_through_the_answer_parser() {
    // Several seeds so the sweep covers all tiers, contexts, and
    // identify families, not one lucky draw.
    let mut checked = 0usize;
    for seed in [7, 2026] {
        let fixtures = generate(&GenParams {
            seed,
            write: 20,
            optimize: 20,
            identify: 3,
            tree: 10,
            ..GenParams::default()
        });
        for f in &fixtures {
            for (id, hex) in displayed_scripts(f) {
                checked += 1;
                let script = ScriptBuf::from_hex(&hex).expect("fixture hex");
                let asm = to_human_asm(script.as_script());
                let parsed = parse_script_answer(&asm)
                    .unwrap_or_else(|e| panic!("{id}: displayed asm does not parse: {e}\n{asm}"));
                assert_eq!(
                    parsed, script,
                    "{id}: displayed asm changes bytes on re-parse\n asm: {asm}"
                );
            }
        }
    }
    assert!(checked > 150, "sweep too small: {checked}");
}

#[test]
fn displayed_reference_asm_grades_full_marks() {
    // The end-to-end version of the property: submitting the answer
    // key in the exact notation the bench itself displays must earn
    // the same grade as submitting its hex.
    let fixtures = generate(&GenParams {
        seed: 2026,
        write: 10,
        optimize: 10,
        identify: 0,
        tree: 0,
        ..GenParams::default()
    });
    for f in &fixtures {
        match f {
            Fixture::Write(w) => {
                let asm = to_human_asm(
                    ScriptBuf::from_hex(&w.reference_script_hex)
                        .unwrap()
                        .as_script(),
                );
                let r = bench_core::grade_write(w, &asm);
                assert_eq!(r.score, 1.0, "{}: {:?}", w.id, r.reason);
            }
            Fixture::Optimize(o) => {
                let opt_asm = to_human_asm(
                    ScriptBuf::from_hex(&o.optimal_script_hex)
                        .unwrap()
                        .as_script(),
                );
                let r = bench_core::grade_optimize(o, &opt_asm);
                assert_eq!(r.weight_score, 1.0, "{}: {:?}", o.id, r.reason);
                // The baseline as displayed in the prompt: equivalent,
                // zero improvement — never a parse or semantics error.
                let base_asm = to_human_asm(
                    ScriptBuf::from_hex(&o.baseline_script_hex)
                        .unwrap()
                        .as_script(),
                );
                let r = bench_core::grade_optimize(o, &base_asm);
                assert!(r.verdict.is_equivalent(), "{}: {:?}", o.id, r.reason);
                assert_eq!(r.weight_score, 0.0, "{}", o.id);
            }
            _ => unreachable!(),
        }
    }
}
