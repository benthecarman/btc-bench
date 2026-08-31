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
            (Some(e), _) => return format!("parse error: {e}"),
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

/// Full diagnostic pass over a candidate tr() descriptor string.
pub fn check_descriptor(text: &str) -> DescriptorCheck {
    let tr = match parse_tr_answer(text) {
        Ok(t) => t,
        Err(e) => {
            return DescriptorCheck {
                parsed: false,
                parse_error: Some(e),
                ..Default::default()
            }
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

    use std::str::FromStr as _;

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
