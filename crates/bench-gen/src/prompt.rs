//! Prompt assembly for the three task types. Deterministic from the
//! fixture; the runner sends these verbatim and the tool schema collects
//! the structured answer.
//!
//! Scripts embedded in prompts (the optimize baseline, the identify
//! scriptPubKey/inner script) are rendered per [`DisplayFormat`] — hex
//! or decoded Bitcoin Core asm. Answers are always accepted in either
//! notation.
//!
//! Each fixture's `prompt_version` picks the surface. Version 0/1 is
//! the original wording, kept byte-stable so old runs stay comparable.
//! Version 2 removes scaffolding: no strategy hints, no grading
//! language, no opcode names in specs (see
//! [`crate::verbal::without_opcode_names`]), and real Bitcoin Core asm
//! (`bitcoin-cli decodescript`) for displayed and answered scripts. It
//! keeps what the task needs: keys, script type, the optimization
//! objective, and the answer notation.

use bench_core::task::{AsmDialect, Fixture, OptimizeFixture, WriteFixture, PROMPT_V2};
use bitcoin::ScriptBuf;

/// The answer-notation line of v2 prompts.
const V2_ANSWER: &str = "Answer with the script as hex or as Bitcoin Core asm (the notation \
                         `bitcoin-cli decodescript` prints).";

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
        self.render_in(hex, AsmDialect::Legacy)
    }

    fn render_in(self, hex: &str, dialect: AsmDialect) -> String {
        let script = || ScriptBuf::from_hex(hex).expect("fixture hex is valid");
        match (self, dialect) {
            (DisplayFormat::Hex, _) => hex.to_string(),
            (DisplayFormat::Asm, AsmDialect::Legacy) => {
                bench_core::human_asm::to_human_asm(script().as_script())
            }
            (DisplayFormat::Asm, AsmDialect::Core) => {
                bench_core::human_asm::to_core_asm(script().as_script())
            }
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
    if let Some(request) = &f.request {
        return request.clone();
    }
    if f.prompt_version >= PROMPT_V2 {
        return format!(
            "Write a Bitcoin Script for the spending condition below.\n\
             \n\
             Script type: {}.\n\
             \n\
             Keys:\n{}\n\
             \n\
             {}\n\
             \n\
             {V2_ANSWER}",
            f.context.script_noun(),
            key_block(&f.keys),
            f.spec_en,
        );
    }
    format!(
        "Write a Bitcoin Script for the spending condition below.\n\
         \n\
         Script type: {}.\n\
         \n\
         Keys:\n{}\n\
         \n\
         {}\n\
         \n\
         Rules:\n\
         - Use exactly the keys listed above; do not invent keys.\n\
         - The script must be a valid, consensus-enforceable, and \
         mechanically verifiable script.\n\
         - Answer with the script as a hex string or \
         Bitcoin Core asm. In asm, opcode names carry the OP_ prefix \
         (OP_CHECKMULTISIG, not CHECKMULTISIG) and data pushes are raw \
         hex, except that a timelock value written directly before \
         OP_CHECKLOCKTIMEVERIFY or OP_CHECKSEQUENCEVERIFY is decimal \
         (e.g. 744813 OP_CHECKLOCKTIMEVERIFY). Write pushes as bare \
         hex, never placeholder names (0279be66...8798 OP_CHECKSIG, \
         not <Alice's key> OP_CHECKSIG).",
        f.context.script_noun(),
        key_block(&f.keys),
        f.spec_en,
    )
}
pub fn optimize_prompt(f: &OptimizeFixture, display: DisplayFormat) -> String {
    if f.prompt_version >= PROMPT_V2 {
        return format!(
            "The following Bitcoin Script (a {}) is correct but unoptimized {}:\n\
             \n\
             {}\n\
             \n\
             Write a script with the same spending conditions and a lower \
             input weight (script plus witness, the quantity transaction \
             fees are paid for).\n\
             \n\
             {V2_ANSWER}",
            f.context.script_noun(),
            display.label(),
            display.render_in(&f.baseline_script_hex, AsmDialect::Core),
        );
    }
    format!(
        "The following Bitcoin Script (a {}) is correct but unoptimized {}:\n\
         \n\
         {}\n\
         \n\
         Write a semantically equivalent script with a lower input weight \
         (script plus witness, the quantity transaction fees are paid for). \
         The spending semantics must not change, and the script must be \
         a valid, consensus-enforceable, and mechanically verifiable \
         script.\n\
         \n\
         Answer with the script as a hex string or \
         Bitcoin Core asm. In asm, opcode names carry the OP_ prefix \
         (OP_CHECKMULTISIG, not CHECKMULTISIG) and data pushes are raw \
         hex, except that a timelock value written directly before \
         OP_CHECKLOCKTIMEVERIFY or OP_CHECKSEQUENCEVERIFY is decimal \
         (e.g. 744813 OP_CHECKLOCKTIMEVERIFY). Write pushes as bare \
         hex, never placeholder names (0279be66...8798 OP_CHECKSIG, \
         not <Alice's key> OP_CHECKSIG).",
        f.context.script_noun(),
        display.label(),
        display.render(&f.baseline_script_hex),
    )
}

pub fn tree_prompt(f: &bench_core::task::TreeFixture) -> String {
    if let Some(request) = &f.request {
        return request.clone();
    }
    if f.prompt_version >= PROMPT_V2 {
        return format!(
            "Design a Taproot output for the spending condition below, \
             minimizing the worst-case input weight (script plus witness).\n\
             \n\
             Keys:\n{}\n\
             - NUMS (provably unspendable): {}\n\
             \n\
             {}\n\
             \n\
             Answer with a tr() descriptor (BIP 386) whose tapleaves are \
             Miniscript, e.g. tr(KEY,{{pk(A),{{and_v(v:pk(B),older(144)),pk(C)}}}}).",
            key_block(&f.keys),
            f.unspendable_key,
            f.spec_en,
        );
    }
    format!(
        "Design a Taproot output for the spending condition below.\n\
         \n\
         Keys:\n{}\n\
         - NUMS (provably unspendable): {}\n\
         \n\
         {}\n\
         \n\
         Rules:\n\
         - Use exactly the keys listed above; do not invent keys.\n\
         - You choose the internal key and the script tree: put the \
         best spending path on the key path when one fits, or use NUMS \
         as the internal key when none fits, and split the rest into \
         tapleaves.\n\
         - Correctness is the gate; among correct designs, a lower \
         worst-case input weight (script plus witness) scores higher.\n\
         - Answer with a descriptor of the form tr(INTERNAL_KEY,TREE), \
         where TREE nests tapleaf scripts in Miniscript notation with \
         braces, e.g. tr(KEY,{{pk(A),{{and_v(v:pk(B),older(144)),pk(C)}}}}). \
         A single tapleaf takes no braces; braces always pair exactly \
         two children, nested for more than two leaves. Hash \
         conditions use the fragment matching the hash function named \
         in the spec: sha256, hash256, ripemd160, or hash160.",
        key_block(&f.keys),
        f.unspendable_key,
        f.spec_en,
    )
}

/// Address form of a scriptPubKey, when it has one. Bare multisig and
/// OP_RETURN outputs have no address; those lines are simply omitted.
/// Real outputs are met as addresses far more often than as raw
/// scripts, and some shapes are identified by the address alone —
/// P2A is exactly `OP_1 <0x4e73>`, universally known as
/// bc1pfeessrawgf.
fn address_line(spk_hex: &str) -> String {
    let script = match ScriptBuf::from_hex(spk_hex) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    match bitcoin::Address::from_script(script.as_script(), bitcoin::Network::Bitcoin) {
        Ok(addr) => format!("Address: {addr}\n"),
        Err(_) => String::new(),
    }
}

pub fn identify_prompt(f: &bench_core::task::IdentifyFixture, display: DisplayFormat) -> String {
    let dialect = bench_core::task::dialect_for(f.prompt_version);
    let inner = match &f.inner_script_hex {
        Some(h) => format!(
            "\nRedeem script / witness script {}: {}\n",
            display.label(),
            display.render_in(h, dialect)
        ),
        // A lone newline keeps the blank line before the call
        // instruction when there is no inner script.
        None => "\n".to_string(),
    };
    format!(
        "Identify the following Bitcoin output.\n\
         \n\
         scriptPubKey {}: {}\n\
         {}{}\
         \n\
         Call the submit_identify tool with one of the following labels:\n\
         - {}",
        display.label(),
        display.render_in(&f.spk_hex, dialect),
        address_line(&f.spk_hex),
        inner,
        crate::corpus::FAMILIES.join(", "),
    )
}

/// Judgment prompts state the requirements and the available keys.
/// No policy tree, no notation lecture beyond the answer format: the
/// point is to see whether the model can design against a brief.
pub fn judgment_prompt(f: &bench_core::task::JudgmentFixture) -> String {
    if f.prompt_version >= PROMPT_V2 {
        return format!(
            "{}\n\nKeys:\n{}\n\n{V2_ANSWER}",
            f.spec_en,
            key_block(&f.keys)
        );
    }
    format!(
        "{}\n\nKeys:\n{}\n\n\
         Answer with the script as a hex string or Bitcoin Core asm. \
         In asm, opcode names carry the OP_ prefix (OP_CHECKMULTISIG, \
         not CHECKMULTISIG) and data pushes are raw hex, except that a \
         timelock value written directly before OP_CHECKLOCKTIMEVERIFY \
         or OP_CHECKSEQUENCEVERIFY is decimal.",
        f.spec_en,
        key_block(&f.keys),
    )
}

/// The same fixture posed at another prompt version: version 2 also
/// drops opcode names from the spec. Answer keys are untouched.
pub fn at_prompt_version(f: &Fixture, version: u32) -> Fixture {
    let spec = |s: &str| {
        if version >= PROMPT_V2 {
            crate::verbal::without_opcode_names(s)
        } else {
            s.to_string()
        }
    };
    let mut f = f.clone();
    match &mut f {
        Fixture::Write(w) => {
            w.prompt_version = version;
            w.spec_en = spec(&w.spec_en);
        }
        Fixture::Optimize(o) => {
            o.prompt_version = version;
            o.spec_en = spec(&o.spec_en);
        }
        Fixture::Identify(i) => i.prompt_version = version,
        Fixture::Tree(t) => {
            t.prompt_version = version;
            t.spec_en = spec(&t.spec_en);
        }
        Fixture::Judgment(j) => {
            j.prompt_version = version;
            j.spec_en = spec(&j.spec_en);
        }
    }
    f
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
        Fixture::Judgment(j) => judgment_prompt(j),
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
                    assert!(
                        !p.contains("Respond by calling"),
                        "tool mechanics live in the system prompt and tool docs"
                    );
                    assert!(p.contains("Alice's public key:"));
                    assert!(
                        !p.contains("Miniscript"),
                        "the decode gate stays implicit by design"
                    );
                    assert!(
                        p.contains("OP_ prefix"),
                        "the asm notation rule must be stated"
                    );
                    assert!(
                        p.contains("mechanically verifiable"),
                        "consensus-validity must not read as the whole bar"
                    );
                }
                Fixture::Optimize(_) => {
                    assert!(p.contains("input weight"));
                    assert!(p.contains("OP_CHECKSIG"), "default display is asm");
                    assert!(
                        p.contains("OP_ prefix"),
                        "optimize states the same notation rule as write"
                    );
                    assert!(
                        !p.contains("byte size"),
                        "script size is folded into weight, not a stated metric"
                    );
                    assert!(
                        p.contains("mechanically verifiable"),
                        "consensus-validity must not read as the whole bar"
                    );
                    assert!(
                        !p.contains("deliberately"),
                        "no constructed-artifact framing"
                    );
                    assert!(
                        !p.contains("Miniscript"),
                        "the decode gate stays implicit by design"
                    );
                }
                Fixture::Identify(_) => {
                    assert!(p.contains("scriptPubKey"));
                    assert!(p.contains("submit_identify"), "prompt names the real tool");
                    assert!(!p.contains("params"), "identify is label-only");
                }
                Fixture::Judgment(_) => {
                    assert!(
                        p.contains("The encoding is yours to choose"),
                        "judgment prompts must state that the encoding is free"
                    );
                    assert!(
                        !p.contains("reference"),
                        "a judgment brief must never mention a reference answer"
                    );
                }
                Fixture::Tree(_) => {
                    assert!(p.contains("tr(INTERNAL_KEY,TREE)"));
                    assert!(
                        p.contains("- NUMS (provably unspendable):"),
                        "NUMS is listed with the keys"
                    );
                    assert!(
                        !p.contains("Respond by calling"),
                        "tool mechanics live in the system prompt and tool docs"
                    );
                }
            }
        }
    }

    /// Version 2 removes scaffolding and keeps what the task needs.
    #[test]
    fn v2_prompts_drop_scaffolding() {
        let params = crate::fixtures::GenParams {
            seed: 3,
            write: 6,
            optimize: 6,
            identify: 1,
            tree: 6,
            judgment: 2,
            ..crate::fixtures::GenParams::default()
        };
        for f in crate::fixtures::generate(&params) {
            let p = for_fixture(&at_prompt_version(&f, PROMPT_V2));
            for hint in [
                "mechanically verifiable",
                "Correctness is the gate",
                "scores higher",
                "put the best spending path",
                "do not invent keys",
                "OP_CHECKLOCKTIMEVERIFY)",
                "OP_CHECKSEQUENCEVERIFY)",
                "Hash conditions use the fragment",
            ] {
                assert!(!p.contains(hint), "{}: {hint:?} in {p}", f.id());
            }
            match &f {
                Fixture::Write(_) | Fixture::Optimize(_) | Fixture::Judgment(_) => {
                    assert!(p.contains("`bitcoin-cli decodescript`"), "{p}");
                    assert!(
                        !p.contains("Miniscript"),
                        "the decode gate stays implicit: {p}"
                    );
                }
                Fixture::Tree(_) => {
                    assert!(p.contains("minimizing the worst-case input weight"), "{p}");
                    assert!(p.contains("- NUMS (provably unspendable):"), "{p}");
                }
                Fixture::Identify(_) => assert!(p.contains("submit_identify")),
            }
            if let Fixture::Optimize(o) = &f {
                // Displayed in Core asm: OP_1..OP_16 as numbers, no
                // rust-bitcoin names.
                assert!(!p.contains("OP_PUSHNUM"), "{p}");
                assert!(!p.contains("OP_CLTV ") && !p.contains("OP_CSV "), "{p}");
                let shown = bench_core::human_asm::to_core_asm(
                    ScriptBuf::from_hex(&o.baseline_script_hex)
                        .unwrap()
                        .as_script(),
                );
                assert!(p.contains(&shown));
            }
        }
    }

    /// Version 0 prompts are byte-stable: re-versioning to 0 is a no-op.
    #[test]
    fn v1_prompts_unchanged_by_versioning() {
        let params = crate::fixtures::GenParams {
            seed: 3,
            write: 2,
            optimize: 2,
            tree: 2,
            ..crate::fixtures::GenParams::default()
        };
        for f in crate::fixtures::generate(&params) {
            assert_eq!(for_fixture(&f), for_fixture(&at_prompt_version(&f, 0)));
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
                Fixture::Optimize(o) => {
                    assert!(
                        asm_prompt.contains("OP_CHECKSIG"),
                        "asm not decoded: {asm_prompt}"
                    );
                    // The hex display embeds the raw baseline hex; the
                    // asm display must not. (The notation rule's
                    // example contains "OP_CHECKSIG" in both, so the
                    // baseline hex is the display marker.)
                    assert!(hex_prompt.contains(&o.baseline_script_hex));
                    assert!(!asm_prompt.contains(&o.baseline_script_hex));
                }
                Fixture::Identify(i) => {
                    let _ = i;
                    assert_ne!(hex_prompt, asm_prompt);
                }
                Fixture::Write(_) => unreachable!("no write fixtures generated"),
                Fixture::Judgment(_) => unreachable!("no judgment fixtures generated"),
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
