//! Diagnostic tools for tool-assisted runs: the same mechanical
//! feedback a developer gets from a compiler and analyzer loop.
//!
//! The inviolable rule: every function here is a pure function of
//! *model-supplied* input (plus the task's script context). No
//! function takes a fixture, so no tool can leak a reference, an
//! answer key, or a distinguishing assignment — by construction, not
//! by discipline. The model still does the English-to-semantics
//! translation on its own; these tools only remove the mechanical
//! friction around it.

use bitcoin::{ScriptBuf, XOnlyPublicKey};
use miniscript::descriptor::{TapTree, Tr};
use miniscript::{Descriptor, Legacy, Miniscript, Segwitv0, Tap};
use serde::Serialize;

use crate::answer::parse_script_answer;
use crate::grade::{lint_report, parse_tr_answer};
use crate::human_asm::to_human_asm;
use crate::task::ContextKind;

/// Everything the toolbox can mechanically determine about a
/// candidate script: parse, decode gate, lint, weight/size.
#[derive(Clone, Debug, Default, Serialize)]
pub struct ScriptCheck {
    pub parsed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asm: Option<String>,
    pub decoded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub miniscript: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decode_error: Option<String>,
    pub lint: Vec<String>,
    /// Context-level consensus facts: constructs that make the script
    /// unconditionally invalid in this script context regardless of
    /// any analysis (e.g. OP_CHECKMULTISIG in tapscript). Reported
    /// even when decoding fails — especially then, since a
    /// decode-failing script often gets defended as
    /// "still consensus-valid" when it is not.
    pub consensus: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_error: Option<String>,
}

impl ScriptCheck {
    /// Compact report for a tool-response message.
    pub fn render(&self) -> String {
        let mut out = String::new();
        match (&self.parse_error, &self.hex) {
            (Some(e), _) => return format!("parse error: {e}"),
            (None, Some(hex)) => {
                out.push_str(&format!("parsed. hex: {hex}\n"));
                if let Some(asm) = &self.asm {
                    out.push_str(&format!("asm: {asm}\n"));
                }
            }
            _ => {}
        }
        for c in &self.consensus {
            out.push_str(&format!("consensus: {c}\n"));
        }
        match (&self.decode_error, &self.miniscript) {
            (Some(e), _) => {
                out.push_str(&format!("miniscript decode gate: FAIL — {e}\n"));
                // Compiler-style orientation note: the library parses
                // scripts from the END, so its "unexpected «X»" names
                // a position models otherwise repair from the wrong
                // side. Mechanical documentation, not a hint.
                out.push_str(
                    "note: Miniscript decodes scripts from the end; the reported \
                     token is where structure stopped matching, counting from \
                     the end of the script.\n",
                );
                return out.trim_end().to_string();
            }
            (None, Some(ms)) => out.push_str(&format!("miniscript: {ms}\n")),
            _ => {}
        }
        if self.lint.is_empty() {
            out.push_str("analysis: no findings\n");
        } else {
            out.push_str(&format!("analysis findings: {}\n", self.lint.join("; ")));
        }
        match (self.weight, self.size, &self.weight_error) {
            (Some(w), Some(s), _) => out.push_str(&format!(
                "max satisfaction weight: {w} WU, script size: {s} bytes"
            )),
            (_, _, Some(e)) => out.push_str(&format!("weight not computable: {e}")),
            _ => {}
        }
        out.trim_end().to_string()
    }
}

/// Non-panicking sibling of [`crate::weights_for`]: adversarial tool
/// input can be decodable yet fail weight computation, which must be
/// a report line, never a crash.
fn safe_weights(kind: ContextKind, script: &ScriptBuf) -> Result<(usize, usize), String> {
    let err = |e: &dyn std::fmt::Display| e.to_string();
    match kind {
        ContextKind::Legacy => {
            let ms: Miniscript<bitcoin::PublicKey, Legacy> =
                Miniscript::decode_consensus(script.as_script()).map_err(|e| err(&e))?;
            let size = ms.encode().len();
            let w = Descriptor::new_sh(ms)
                .map_err(|e| err(&e))?
                .max_weight_to_satisfy()
                .map_err(|e| err(&e))?;
            Ok((w.to_wu() as usize, size))
        }
        ContextKind::SegwitV0 => {
            let ms: Miniscript<bitcoin::PublicKey, Segwitv0> =
                Miniscript::decode_consensus(script.as_script()).map_err(|e| err(&e))?;
            let size = ms.encode().len();
            let w = Descriptor::new_wsh(ms)
                .map_err(|e| err(&e))?
                .max_weight_to_satisfy()
                .map_err(|e| err(&e))?;
            Ok((w.to_wu() as usize, size))
        }
        ContextKind::Tap => {
            let ms: Miniscript<XOnlyPublicKey, Tap> =
                Miniscript::decode_consensus(script.as_script()).map_err(|e| err(&e))?;
            let size = ms.encode().len();
            let dummy = XOnlyPublicKey::from_slice(&[0x51; 32]).expect("valid x-only key");
            let w = Tr::new(dummy, Some(TapTree::leaf(ms)))
                .map_err(|e| err(&e))?
                .max_weight_to_satisfy()
                .map_err(|e| err(&e))?;
            Ok((w.to_wu() as usize, size))
        }
    }
}

/// Static consensus scan: facts provable from the script bytes alone
/// that make the script invalid (or, for tapscript OP_SUCCESS,
/// trivially spendable) in its context, independent of any witness or
/// miniscript analysis. Reports violations only — it never certifies
/// validity, because full validity depends on execution, and a
/// "consensus: OK" line next to a decode failure would invite the
/// "my script is still consensus-valid" dismissal this scan exists
/// to defuse.
///
/// No implementation offers "validate these bytes" to call instead:
/// consensus validity is defined over (script, witness, transaction)
/// execution, so any static scan must enumerate the same structural
/// checks Core's EvalScript performs even for unexecuted branches
/// (interpreter.cpp: disabled-opcode and conditional-balance checks
/// run per-iterated-opcode regardless of fExec). Finding texts are
/// Core's own script_error.cpp strings, verbatim, with a context
/// clarifier appended.
pub fn consensus_notes(kind: ContextKind, script: &ScriptBuf) -> Vec<String> {
    use bitcoin::blockdata::opcodes::all;
    use bitcoin::script::Instruction;
    let mut out: Vec<String> = Vec::new();
    let mut push = |m: String| {
        if !out.contains(&m) {
            out.push(m);
        }
    };

    // Size limits.
    match kind {
        ContextKind::Legacy => {
            if script.len() > 520 {
                push(format!(
                    "Push value size limit exceeded: a {}-byte P2SH redeem \
                     script cannot be pushed in the scriptSig (520-byte \
                     limit); the output is unspendable",
                    script.len()
                ));
            }
        }
        ContextKind::SegwitV0 => {
            if script.len() > 10_000 {
                push(format!(
                    "Script is too big ({} bytes; 10,000-byte limit)",
                    script.len()
                ));
            }
        }
        ContextKind::Tap => {}
    }

    // Legacy/segwit v0: disabled opcodes make the script invalid by
    // their mere presence, even in unexecuted branches; tapscript
    // (BIP 342) instead turns most of them into OP_SUCCESS, which
    // makes the WHOLE script unconditionally spendable.
    const DISABLED_V0: &[u8] = &[
        0x7e, 0x7f, 0x80, 0x81, // CAT SUBSTR LEFT RIGHT
        0x83, 0x84, 0x85, 0x86, // INVERT AND OR XOR
        0x8d, 0x8e, // 2MUL 2DIV
        0x95, 0x96, 0x97, 0x98, 0x99, // MUL DIV MOD LSHIFT RSHIFT
    ];
    fn is_tap_success(b: u8) -> bool {
        matches!(b,
            0x50 | 0x62 | 0x7e..=0x81 | 0x83..=0x86 | 0x89 | 0x8a
            | 0x8d | 0x8e | 0x95..=0x99 | 0xbb..=0xfe)
    }

    let mut if_depth: i64 = 0;
    let mut unbalanced = false;
    let mut nonpush_ops: usize = 0;
    for ins in script.instructions() {
        match ins {
            Err(e) => {
                push(format!(
                    "malformed script: {e} (a push runs past the end of the script)"
                ));
                break;
            }
            Ok(Instruction::PushBytes(b)) => {
                if b.len() > 520 && kind != ContextKind::Tap {
                    push(format!(
                        "Push value size limit exceeded ({}-byte push; \
                         520-byte limit)",
                        b.len()
                    ));
                }
            }
            Ok(Instruction::Op(op)) => {
                let b = op.to_u8();
                if b > 0x60 {
                    nonpush_ops += 1;
                }
                if op == all::OP_IF || op == all::OP_NOTIF {
                    if_depth += 1;
                } else if op == all::OP_ELSE {
                    if if_depth == 0 {
                        unbalanced = true;
                    }
                } else if op == all::OP_ENDIF {
                    if if_depth == 0 {
                        unbalanced = true;
                    } else {
                        if_depth -= 1;
                    }
                }
                match kind {
                    ContextKind::Tap => {
                        if op == all::OP_CHECKMULTISIG || op == all::OP_CHECKMULTISIGVERIFY {
                            push(
                                "OP_CHECKMULTISIG(VERIFY) is not available in \
                                 tapscript (BIP 342); tapscript multisig is an \
                                 OP_CHECKSIG / OP_CHECKSIGADD accumulation"
                                    .to_string(),
                            );
                        } else if is_tap_success(b) {
                            push(format!(
                                "{op} is an OP_SUCCESS opcode in tapscript \
                                 (BIP 342): by consensus its presence makes \
                                 the ENTIRE script unconditionally spendable \
                                 by anyone (Core policy: \"OP_SUCCESSx \
                                 reserved for soft-fork upgrades\")"
                            ));
                        }
                    }
                    ContextKind::Legacy | ContextKind::SegwitV0 => {
                        if op == all::OP_VERIF || op == all::OP_VERNOTIF {
                            push(format!(
                                "Opcode missing or not understood ({op}): \
                                 invalid by its mere presence, even in an \
                                 unexecuted branch"
                            ));
                        } else if DISABLED_V0.contains(&b) {
                            push(format!(
                                "Attempted to use a disabled opcode ({op}): \
                                 its presence invalidates the script, even in \
                                 an unexecuted branch"
                            ));
                        } else if op == all::OP_CHECKSIGADD {
                            push(
                                "Opcode missing or not understood \
                                 (OP_CHECKSIGADD is defined only in tapscript, \
                                 BIP 342); any path executing it here fails"
                                    .to_string(),
                            );
                        }
                    }
                }
            }
        }
    }
    if unbalanced || if_depth > 0 {
        push(
            "Invalid OP_IF construction: every OP_IF/OP_NOTIF needs a \
             matching OP_ENDIF, and OP_ELSE/OP_ENDIF must sit inside one; \
             execution always fails"
                .to_string(),
        );
    }
    if nonpush_ops > 201 && kind != ContextKind::Tap {
        push(format!(
            "Operation limit exceeded ({nonpush_ops} non-push opcodes; \
             201-opcode limit)"
        ));
    }
    out
}

fn decoded_miniscript(kind: ContextKind, script: &ScriptBuf) -> Result<String, String> {
    match kind {
        ContextKind::Legacy => {
            Miniscript::<bitcoin::PublicKey, Legacy>::decode_consensus(script.as_script())
                .map(|ms| ms.to_string())
                .map_err(|e| e.to_string())
        }
        ContextKind::SegwitV0 => {
            Miniscript::<bitcoin::PublicKey, Segwitv0>::decode_consensus(script.as_script())
                .map(|ms| ms.to_string())
                .map_err(|e| e.to_string())
        }
        ContextKind::Tap => Miniscript::<XOnlyPublicKey, Tap>::decode_consensus(script.as_script())
            .map(|ms| ms.to_string())
            .map_err(|e| e.to_string()),
    }
}

/// Full diagnostic pass over a candidate script (hex or asm).
pub fn check_script(kind: ContextKind, text: &str) -> ScriptCheck {
    let script = match parse_script_answer(text) {
        Ok(s) => s,
        Err(e) => {
            return ScriptCheck {
                parsed: false,
                parse_error: Some(e.to_string()),
                ..Default::default()
            }
        }
    };
    let mut check = ScriptCheck {
        parsed: true,
        hex: Some(script.to_hex_string()),
        asm: Some(to_human_asm(script.as_script())),
        consensus: consensus_notes(kind, &script),
        ..Default::default()
    };
    match decoded_miniscript(kind, &script) {
        Ok(ms) => {
            check.decoded = true;
            check.miniscript = Some(ms);
        }
        Err(e) => {
            check.decode_error = Some(e);
            return check;
        }
    }
    check.lint = lint_report(kind, &script)
        .into_iter()
        .map(str::to_string)
        .collect();
    match safe_weights(kind, &script) {
        Ok((weight, size)) => {
            check.weight = Some(weight);
            check.size = Some(size);
        }
        Err(e) => check.weight_error = Some(e),
    }
    check
}

/// Diagnostic pass over a candidate tr() descriptor.
#[derive(Clone, Debug, Default, Serialize)]
pub struct DescriptorCheck {
    pub parsed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
    /// The input as parsed (echoed verbatim, trimmed). Not re-derived
    /// from Display: miniscript 13.1's TapTree Display is broken for
    /// depth-decreasing trees and would corrupt the echo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub descriptor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leaves: Option<usize>,
    /// Static syntax facts when parsing fails: unbalanced
    /// parens/braces with positions, brace groups holding more than
    /// two children, wrong-length hex in fragment arguments. Facts
    /// about the submitted text only.
    pub syntax: Vec<String>,
    /// Union of insanity findings across the leaves.
    pub lint: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_error: Option<String>,
}

impl DescriptorCheck {
    pub fn render(&self) -> String {
        let mut out = String::new();
        match (&self.parse_error, &self.descriptor) {
            (Some(e), _) => {
                out.push_str(&format!("parse error: {e}\n"));
                for n in &self.syntax {
                    out.push_str(&format!("syntax: {n}\n"));
                }
                return out.trim_end().to_string();
            }
            (None, Some(d)) => out.push_str(&format!("parsed: {d}\n")),
            _ => {}
        }
        if let Some(n) = self.leaves {
            out.push_str(&format!("tapleaves: {n}\n"));
        }
        if self.lint.is_empty() {
            out.push_str("leaf analysis: no findings\n");
        } else {
            out.push_str(&format!(
                "leaf analysis findings: {}\n",
                self.lint.join("; ")
            ));
        }
        match (self.weight, &self.weight_error) {
            (Some(w), _) => {
                out.push_str(&format!("max satisfaction weight: {w} WU"));
            }
            (_, Some(e)) => out.push_str(&format!("weight not computable: {e}")),
            _ => {}
        }
        out.trim_end().to_string()
    }
}

/// Static syntax facts about a descriptor string that failed to
/// parse: mechanical observations about the text itself, mirroring
/// what a human squints for — bracket balance, brace arity, argument
/// hex lengths.
pub fn descriptor_syntax_notes(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |m: String| {
        if !out.contains(&m) {
            out.push(m);
        }
    };
    let bytes = text.as_bytes();

    // Bracket balance, with the position of the first orphan.
    for (open, close, name) in [(b'(', b')', "parenthesis"), (b'{', b'}', "brace")] {
        let mut depth: i64 = 0;
        let mut orphan: Option<usize> = None;
        for (i, b) in bytes.iter().enumerate() {
            if *b == open {
                depth += 1;
            } else if *b == close {
                depth -= 1;
                if depth < 0 && orphan.is_none() {
                    orphan = Some(i);
                    depth = 0;
                }
            }
        }
        if let Some(i) = orphan {
            push(format!(
                "unbalanced {name}: '{}' at position {i} has no matching '{}'",
                close as char, open as char
            ));
        } else if depth > 0 {
            push(format!(
                "unbalanced {name}: {depth} unclosed '{}'",
                open as char
            ));
        }
    }

    // Brace arity: a taptree brace group pairs exactly two children.
    // (Singleton groups are auto-unwrapped by the parser; only
    // over-full groups are worth a note.)
    let mut paren: i64 = 0;
    let mut stack: Vec<(i64, usize)> = Vec::new(); // (paren depth at open, top-level commas)
    for b in bytes {
        match b {
            b'(' => paren += 1,
            b')' => paren -= 1,
            b'{' => stack.push((paren, 0)),
            b',' => {
                if let Some(top) = stack.last_mut() {
                    if paren == top.0 {
                        top.1 += 1;
                    }
                }
            }
            b'}' => {
                if let Some((_, commas)) = stack.pop() {
                    if commas >= 2 {
                        push(format!(
                            "a {{...}} group holds {} children; taptree braces \
                             pair exactly two — nest further groups for more \
                             leaves",
                            commas + 1
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    // Fragment argument lengths: x-only keys are 64 hex chars,
    // sha256/hash256 digests 64, hash160/ripemd160 digests 40.
    for (frag, want, what) in [
        ("pk(", 64usize, "x-only keys"),
        ("sha256(", 64, "SHA-256 digests"),
        ("hash256(", 64, "HASH256 digests"),
        ("hash160(", 40, "HASH160 digests"),
        ("ripemd160(", 40, "RIPEMD160 digests"),
    ] {
        let mut from = 0;
        while let Some(rel) = text[from..].find(frag) {
            let start = from + rel + frag.len();
            let Some(end_rel) = text[start..].find(')') else {
                break;
            };
            let arg = &text[start..start + end_rel];
            from = start + end_rel;
            if arg.contains('(') {
                continue; // nested expression, not a literal argument
            }
            let hexish = !arg.is_empty() && arg.bytes().all(|c| c.is_ascii_hexdigit());
            if hexish && arg.len() != want {
                push(format!(
                    "{frag}...) argument is {} hex characters; {what} are {want}",
                    arg.len()
                ));
            }
        }
    }
    out
}

/// Full diagnostic pass over a candidate tr() descriptor string.
pub fn check_descriptor(text: &str) -> DescriptorCheck {
    let tr = match parse_tr_answer(text) {
        Ok(t) => t,
        Err(e) => {
            let mut syntax = descriptor_syntax_notes(text);
            // Clarify the library's own taproot-multi rejection with
            // the documented taproot form.
            if e.contains("Multi node in taproot") {
                syntax.push(
                    "taproot descriptors use multi_a(k,...) in place of multi(k,...)".to_string(),
                );
            }
            return DescriptorCheck {
                parsed: false,
                parse_error: Some(e),
                syntax,
                ..Default::default()
            };
        }
    };
    let mut lint: Vec<String> = Vec::new();
    let mut leaves = 0usize;
    for leaf in tr.leaves() {
        leaves += 1;
        for l in lint_report(ContextKind::Tap, &leaf.miniscript().encode()) {
            let l = l.to_string();
            if !lint.contains(&l) {
                lint.push(l);
            }
        }
    }
    let mut check = DescriptorCheck {
        parsed: true,
        descriptor: Some(text.trim().trim_matches('`').trim().to_string()),
        leaves: Some(leaves),
        lint,
        ..Default::default()
    };
    match Descriptor::Tr(tr).max_weight_to_satisfy() {
        Ok(w) => check.weight = Some(w.to_wu() as usize),
        Err(e) => check.weight_error = Some(e.to_string()),
    }
    check
}

#[cfg(test)]
mod tests {
    use super::*;

    const K1: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
    const K2: &str = "02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";

    #[test]
    fn script_check_reports_each_gate() {
        // Garbage: parse error only.
        let c = check_script(ContextKind::SegwitV0, "zz not hex");
        assert!(!c.parsed && c.parse_error.is_some());
        assert!(c.render().starts_with("parse error:"));
        // OP_RETURN: parses, fails the decode gate.
        let c = check_script(ContextKind::SegwitV0, "6a");
        assert!(c.parsed && !c.decoded && c.decode_error.is_some());
        assert!(c.render().contains("decode gate: FAIL"));
        // OP_1: decodes, lints unsafe, weighs.
        let c = check_script(ContextKind::SegwitV0, "51");
        assert!(c.decoded);
        assert!(c
            .lint
            .iter()
            .any(|l| l.contains("All spend paths must require a signature")));
        assert!(c.weight.is_some());
        // A sane two-key script: clean report with all facts.
        let ms = miniscript::Miniscript::<bitcoin::PublicKey, Segwitv0>::from_str(&format!(
            "and_v(v:pk({K1}),pk({K2}))"
        ))
        .unwrap();
        let c = check_script(ContextKind::SegwitV0, &ms.encode().to_hex_string());
        assert!(c.decoded && c.lint.is_empty());
        let r = c.render();
        assert!(r.contains("miniscript: and_v"), "{r}");
        assert!(r.contains("no findings"), "{r}");
        assert!(r.contains("weight:"), "{r}");
        // Asm input is accepted like grading input.
        let c = check_script(
            ContextKind::SegwitV0,
            &to_human_asm(ms.encode().as_script()),
        );
        assert!(c.decoded);
    }

    #[test]
    fn consensus_scan_reports_static_violations_only() {
        let k = "32a9c1b6aa84caf9b6898e162f8967d618a2eba4f4e185481e5a373c874a6a14";
        // CHECKMULTISIG in tapscript: disabled.
        let ms = format!("2 {k} {k} {k} 3 OP_CHECKMULTISIG");
        let c = check_script(ContextKind::Tap, &ms);
        assert!(
            c.consensus
                .iter()
                .any(|n| n.contains("not available in tapscript")),
            "{:?}",
            c.consensus
        );
        assert!(c.render().contains("consensus:"), "{}", c.render());
        // Same bytes in segwit: legal, silent — and cleanliness is
        // silence, never a certification line.
        let c = check_script(ContextKind::SegwitV0, &ms);
        assert!(c.consensus.is_empty(), "{:?}", c.consensus);
        assert!(!c.render().contains("consensus"), "{}", c.render());
        // CHECKSIGADD outside tapscript.
        let c = check_script(
            ContextKind::SegwitV0,
            &format!("{k} OP_CHECKSIG {k} OP_CHECKSIGADD"),
        );
        assert!(
            c.consensus
                .iter()
                .any(|n| n.contains("defined only in tapscript")),
            "{:?}",
            c.consensus
        );
        // Unbalanced conditionals: the real failure shape from the
        // runs (ELSE after a closed ENDIF).
        let c = check_script(
            ContextKind::Legacy,
            &format!("OP_IF {k} OP_CHECKSIG OP_ENDIF OP_ELSE {k} OP_CHECKSIG OP_ENDIF"),
        );
        assert!(
            c.consensus
                .iter()
                .any(|n| n.contains("Invalid OP_IF construction")),
            "{:?}",
            c.consensus
        );
        // Unclosed IF.
        let c = check_script(ContextKind::SegwitV0, &format!("OP_IF {k} OP_CHECKSIG"));
        assert!(c
            .consensus
            .iter()
            .any(|n| n.contains("Invalid OP_IF construction")));
        // Disabled opcode in segwit (OP_CAT, via hex 7e).
        let c = check_script(ContextKind::SegwitV0, "7e");
        assert!(
            c.consensus
                .iter()
                .any(|n| n.contains("Attempted to use a disabled opcode")),
            "{:?}",
            c.consensus
        );
        // Same byte in tapscript: OP_SUCCESS — trivially spendable.
        let c = check_script(ContextKind::Tap, "7e");
        assert!(
            c.consensus.iter().any(|n| n.contains("OP_SUCCESS")),
            "{:?}",
            c.consensus
        );
        // Clean scripts in every context: silent.
        for ctx in [ContextKind::Legacy, ContextKind::SegwitV0, ContextKind::Tap] {
            let key = if ctx == ContextKind::Tap {
                k.to_string()
            } else {
                format!("02{k}")
            };
            let c = check_script(ctx, &format!("{key} OP_CHECKSIG"));
            assert!(c.consensus.is_empty(), "{ctx:?}: {:?}", c.consensus);
        }
    }

    use std::str::FromStr as _;

    #[test]
    fn descriptor_syntax_notes_name_the_defect() {
        let a = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let b = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";
        let c2 = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
        // Flat 3-child brace group: arity note.
        let r = check_descriptor(&format!("tr({a},{{pk({a}),pk({b}),pk({c2})}})"));
        assert!(!r.parsed);
        assert!(
            r.syntax.iter().any(|n| n.contains("holds 3 children")),
            "{:?}",
            r.syntax
        );
        assert!(r.render().contains("syntax:"), "{}", r.render());
        // Missing closing paren: unbalanced note.
        let r = check_descriptor(&format!("tr({a},and_v(v:pk({b}),older(16))"));
        assert!(
            r.syntax.iter().any(|n| n.contains("unclosed '('")),
            "{:?}",
            r.syntax
        );
        // Truncated key inside pk(): length note.
        let r = check_descriptor(&format!("tr({a},pk({}))", &b[..63]));
        assert!(
            r.syntax
                .iter()
                .any(|n| n.contains("63 hex characters") && n.contains("64")),
            "{:?}",
            r.syntax
        );
        // multi() in taproot: the library error gets the multi_a
        // clarifier.
        let r = check_descriptor(&format!("tr({a},multi(2,{b},{c2}))"));
        assert!(
            r.syntax.iter().any(|n| n.contains("multi_a")),
            "{:?} / {:?}",
            r.parse_error,
            r.syntax
        );
        // A valid descriptor gets no syntax section.
        let r = check_descriptor(&format!("tr({a},{{pk({b}),pk({c2})}})"));
        assert!(r.parsed && r.syntax.is_empty());
    }

    #[test]
    fn descriptor_check_reports() {
        let c = check_descriptor("nonsense");
        assert!(!c.parsed);
        let a = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let b = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";
        let c2 = "f9308a019258c31049344f85f89d5229b531c845836f99b08601f113bce036f9";
        let d = format!("tr({a},{{pk({b}),pk({c2})}})");
        let c = check_descriptor(&d);
        assert!(c.parsed, "{:?}", c.parse_error);
        assert_eq!(c.leaves, Some(2));
        assert!(c.weight.is_some());
        assert!(c.render().contains("tapleaves: 2"));
    }
}
