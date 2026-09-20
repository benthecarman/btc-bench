//! Check writing targets in their requested Script context.
use anyhow::{ensure, Context, Result};
use bench_core::{task::Fixture, ContextKind, HashPreimages, TaskAnswer};
use bitcoin::{PublicKey, XOnlyPublicKey};
use miniscript::{Legacy, Miniscript, Segwitv0, Tap};
use serde_json::{json, Value};
use std::io::{self, BufRead};

fn main() -> Result<()> {
    for line in io::stdin().lock().lines() {
        let row: Value = serde_json::from_str(&line?)?;
        let fixture: Fixture = serde_json::from_value(row["fixture"].clone())?;
        let Fixture::Write(ref f) = fixture else {
            anyhow::bail!("expected a script-writing fixture")
        };
        let text = row["miniscript"]
            .as_str()
            .unwrap_or(&f.reference_miniscript);
        macro_rules! compile {
            ($key:ty, $context:ty) => {{
                let ms = Miniscript::<$key, $context>::from_str_insane(text)?;
                (ms.to_string(), ms.encode())
            }};
        }
        let (miniscript, script) = match f.context {
            ContextKind::Legacy => compile!(PublicKey, Legacy),
            ContextKind::SegwitV0 => compile!(PublicKey, Segwitv0),
            ContextKind::Tap => compile!(XOnlyPublicKey, Tap),
        };
        let asm = bench_core::human_asm::to_human_asm(&script);
        ensure!(bench_core::answer::parse_script_answer(&asm)? == script);
        let mut fixed = f.clone();
        fixed.choose_context = false;
        let grade = bench_core::grade_write(&fixed, &asm);
        ensure!(
            grade.score == 1.0 && grade.lint.is_empty(),
            "target rejected: {grade:?}"
        );
        let preimages =
            HashPreimages::from_hex_map(&f.hash_preimages).map_err(anyhow::Error::msg)?;
        bench_core::execution_check(f.context, &script, &preimages).map_err(anyhow::Error::msg)?;
        let context = match f.context {
            ContextKind::Legacy => "P2SH redeem script",
            ContextKind::SegwitV0 => "P2WSH witness script",
            ContextKind::Tap => "Tapscript leaf",
        };
        let final_text = format!("{context}:\n\n```text\n{asm}\n```");
        let answer = bench_cli::runner::reextract_chat_answer(&fixture, &final_text)
            .context("chat extractor did not find the script")?;
        let TaskAnswer::Script(answer) = answer else {
            anyhow::bail!("expected a script answer")
        };
        ensure!(bench_core::grade_write(&fixed, &answer.script).score == 1.0);
        println!(
            "{}",
            json!({"id":f.id,"miniscript":miniscript,"asm":asm,
            "hex":script.to_hex_string(),"context":f.context,"context_label":context,
            "final_chat":final_text,"strict_equivalence":true,"execution_check":true,
            "asm_round_trip":true,"chat_extraction_check":true})
        );
    }
    Ok(())
}
