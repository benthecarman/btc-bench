//! Prompt assembly for the three task types. Deterministic from the
//! fixture; the runner sends these verbatim and the tool schema collects
//! the structured answer.
//!
//! Scripts embedded in prompts (the optimize baseline, the identify
//! scriptPubKey/inner script) are rendered per [`DisplayFormat`] — hex
//! or decoded Bitcoin Core asm. Answers are always accepted in either
//! notation.

use bench_core::task::{Fixture, OptimizeFixture, WriteFixture};
use bitcoin::ScriptBuf;

/// How embedded scripts are displayed in prompts.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum DisplayFormat {
    /// Script bytes as hex.
    Hex,
    /// Decoded Bitcoin Core asm (`OP_DUP OP_HASH160 ...`); the default.
    #[default]
    Asm,
}

impl DisplayFormat {
    fn render(self, hex: &str) -> String {
        match self {
            DisplayFormat::Hex => hex.to_string(),
            DisplayFormat::Asm => bench_core::human_asm::to_human_asm(
                ScriptBuf::from_hex(hex)
                    .expect("fixture hex is valid")
                    .as_script(),
            ),
        }
    }

    fn label(self) -> &'static str {
        match self {
            DisplayFormat::Hex => "(hex)",
            DisplayFormat::Asm => "(Bitcoin Core asm)",
        }
    }
}

fn key_block(keys: &[bench_core::task::KeyVar]) -> String {
    let lines: Vec<String> = keys
        .iter()
        .map(|k| format!("- {}'s public key: {}", k.label, k.pubkey))
        .collect();
    lines.join("\n")
}

pub fn write_prompt(f: &WriteFixture) -> String {
    format!(
        "Write a Bitcoin Script for the spending condition below.\n\
         \n\
         Script type: the {}.\n\
         \n\
         Keys:\n{}\n\
         \n\
         {}\n\
         \n\
         Rules:\n\
         - Use exactly the keys listed above; do not invent keys.\n\
         - The script must be a valid, consensus-enforceable script.\n\
         - Respond by calling the submit_answer tool with the script as a \
         hex string or Bitcoin Core asm. In asm, opcode names carry the \
         OP_ prefix (OP_CHECKMULTISIG, not CHECKMULTISIG) and data pushes \
         are raw hex, except that a timelock value written directly \
         before OP_CHECKLOCKTIMEVERIFY or OP_CHECKSEQUENCEVERIFY is \
         decimal (e.g. 744813 OP_CHECKLOCKTIMEVERIFY).",
        f.context.script_noun(),
        key_block(&f.keys),
        f.spec_en,
    )
}
pub fn optimize_prompt(f: &OptimizeFixture, display: DisplayFormat) -> String {
    format!(
        "The following Bitcoin Script (a {}) is correct but deliberately unoptimized {}:\n\
         \n\
         {}\n\
         \n\
         Write a semantically equivalent script with a lower input weight \
         (script plus witness, the quantity transaction fees are paid for). \
         Script byte size is a secondary metric. The spending semantics must \
         not change.\n\
         \n\
         Respond by calling the submit_answer tool with the script as a hex string or Bitcoin Core asm.",
        f.context.script_noun(),
        display.label(),
        display.render(&f.baseline_script_hex),
    )
}

pub fn tree_prompt(f: &bench_core::task::TreeFixture) -> String {
    format!(
        "Design a Taproot output for the spending condition below.\n\
         \n\
         Keys:\n{}\n\
         \n\
         Unspendable internal key (use it as the internal key only if \
         no listed key deserves the key path): {}\n\
         \n\
         {}\n\
         \n\
         Rules:\n\
         - Use exactly the keys listed above; do not invent keys.\n\
         - You choose the internal key and the script tree: put the \
         best spending path on the key path when one fits, and split \
         the rest into tapleaves.\n\
         - Correctness is the gate; among correct designs, a lower \
         worst-case input weight (script plus witness) scores higher.\n\
         - Respond by calling the submit_answer tool with a descriptor \
         of the form tr(INTERNAL_KEY,TREE), where TREE nests tapleaf \
         scripts in Miniscript notation with braces, e.g. \
         tr(KEY,{{pk(A),{{and_v(v:pk(B),older(144)),pk(C)}}}}).",
        key_block(&f.keys),
        f.unspendable_key,
        f.spec_en,
    )
}

pub fn identify_prompt(f: &bench_core::task::IdentifyFixture, display: DisplayFormat) -> String {
    let inner = match &f.inner_script_hex {
        Some(h) => format!(
            "\nRedeem script / witness script {}: {}\n",
            display.label(),
            display.render(h)
        ),
        None => String::new(),
    };
    format!(
        "Identify the following Bitcoin output.\n\
         \n\
         scriptPubKey {}: {}{}\
         \n\
         Call submit_answer with:\n\
         - label: one of {}",
        display.label(),
        display.render(&f.spk_hex),
        inner,
        crate::corpus::FAMILIES.join(", "),
    )
}

pub fn for_fixture(f: &Fixture) -> String {
    for_fixture_fmt(f, DisplayFormat::default())
}

pub fn for_fixture_fmt(f: &Fixture, display: DisplayFormat) -> String {
    match f {
        Fixture::Write(w) => write_prompt(w),
        Fixture::Optimize(o) => optimize_prompt(o, display),
        Fixture::Identify(i) => identify_prompt(i, display),
        Fixture::Tree(t) => tree_prompt(t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_contain_essentials() {
        // The recalibrated answer contract must stay in the prompts:
        // models lost whole tiers to not knowing the miniscript gate
        // or the OP_ prefix rule existed.

        let params = crate::fixtures::GenParams {
            seed: 3,
            write: 1,
            optimize: 1,
            identify: 1,
            tree: 1,
            ..crate::fixtures::GenParams::default()
        };
        for f in crate::fixtures::generate(&params) {
            let p = for_fixture(&f);
            match &f {
                Fixture::Write(_) => {
                    assert!(p.contains("submit_answer"));
                    assert!(p.contains("Alice's public key:"));
                    assert!(
                        !p.contains("Miniscript"),
                        "the decode gate stays implicit by design"
                    );
                    assert!(
                        p.contains("OP_ prefix"),
                        "the asm notation rule must be stated"
                    );
                }
                Fixture::Optimize(_) => {
                    assert!(p.contains("input weight"));
                    assert!(p.contains("OP_CHECKSIG"), "default display is asm");
                    assert!(
                        !p.contains("Miniscript"),
                        "the decode gate stays implicit by design"
                    );
                }
                Fixture::Identify(_) => assert!(p.contains("scriptPubKey")),
                Fixture::Tree(_) => {
                    assert!(p.contains("submit_answer"));
                    assert!(p.contains("tr(INTERNAL_KEY,TREE)"));
                    assert!(p.contains("Unspendable internal key"));
                }
            }
        }
    }

    #[test]
    fn asm_display_decodes_scripts() {
        let params = crate::fixtures::GenParams {
            seed: 3,
            write: 0,
            optimize: 1,
            identify: 1,
            tree: 1,
            ..crate::fixtures::GenParams::default()
        };
        let fixtures = crate::fixtures::generate(&params);
        for f in &fixtures {
            let hex_prompt = for_fixture_fmt(f, DisplayFormat::Hex);
            let asm_prompt = for_fixture_fmt(f, DisplayFormat::Asm);
            match f {
                Fixture::Optimize(_) => {
                    assert!(
                        asm_prompt.contains("OP_CHECKSIG"),
                        "asm not decoded: {asm_prompt}"
                    );
                    assert!(!hex_prompt.contains("OP_CHECKSIG"));
                }
                Fixture::Identify(i) => {
                    let _ = i;
                    assert_ne!(hex_prompt, asm_prompt);
                }
                Fixture::Write(_) => unreachable!("no write fixtures generated"),
                Fixture::Tree(_) => {
                    // Tree prompts embed no rendered script; the
                    // display toggle must not change them.
                    assert_eq!(hex_prompt, asm_prompt);
                }
            }
        }
    }

    #[test]
    fn write_prompt_is_display_independent() {
        // Write prompts embed no script; the toggle must not change them.
        let params = crate::fixtures::GenParams {
            seed: 3,
            write: 1,
            optimize: 0,
            identify: 0,
            ..crate::fixtures::GenParams::default()
        };
        let fixtures = crate::fixtures::generate(&params);
        let f = &fixtures[0];
        assert_eq!(
            for_fixture_fmt(f, DisplayFormat::Hex),
            for_fixture_fmt(f, DisplayFormat::Asm)
        );
    }
}
