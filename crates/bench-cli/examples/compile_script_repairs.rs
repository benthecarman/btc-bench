//! Compile authored raw-script teaching targets and reject their faulty drafts.
use anyhow::{ensure, Context, Result};
use bench_core::{task::Fixture, ContextKind, HashPreimages, TaskAnswer};
use bitcoin::PublicKey;
use miniscript::{Miniscript, Segwitv0};
use serde_json::{json, Value};
use std::io::{self, BufRead};

fn main() -> Result<()> {
    for line in io::stdin().lock().lines() {
        let row: Value = serde_json::from_str(&line?)?;
        let fixture: Fixture = serde_json::from_value(row["fixture"].clone())?;
        let Fixture::Write(ref f) = fixture else {
            anyhow::bail!("only raw-script write fixtures are supported")
        };
        ensure!(f.context == ContextKind::SegwitV0 && f.choose_context);
        let ms: Miniscript<PublicKey, Segwitv0> = row["miniscript"]
            .as_str()
            .context("missing teaching Miniscript")?
            .parse()?;
        ms.sanity_check()?;
        let script = ms.encode();
        let asm = bench_core::human_asm::to_human_asm(&script);
        ensure!(bench_core::answer::parse_script_answer(&asm)? == script);
        let grade = bench_core::grade_write(f, &asm);
        ensure!(
            grade.score == 1.0 && grade.lint.is_empty(),
            "incorrect target: {grade:?}"
        );
        let mut fixed_context = f.clone();
        fixed_context.choose_context = false;
        ensure!(bench_core::grade_write(&fixed_context, &asm).score == 1.0);
        let preimages =
            HashPreimages::from_hex_map(&f.hash_preimages).map_err(anyhow::Error::msg)?;
        bench_core::execution_check(ContextKind::SegwitV0, &script, &preimages)
            .map_err(anyhow::Error::msg)?;
        let draft = if let Some(text) = row["bad_miniscript"].as_str() {
            let bad: Miniscript<PublicKey, Segwitv0> = text.parse()?;
            bench_core::human_asm::to_human_asm(&bad.encode())
        } else {
            let from = row["replace_from"].as_str().context("missing draft edit")?;
            let to = row["replace_to"].as_str().context("missing replacement")?;
            ensure!(asm.matches(from).count() == 1, "draft edit must match once");
            asm.replacen(from, to, 1)
        };
        let bad_grade = bench_core::grade_write(f, &draft);
        ensure!(bad_grade.score == 0.0, "faulty draft was accepted");
        let note = row["note"].as_str().context("missing explanation")?;
        let final_text = format!(
            "{note}\n\nI choose a P2WSH witness script.\n\nBitcoin script:\n\n```text\n{asm}\n```"
        );
        let answer = bench_cli::runner::reextract_chat_answer(&fixture, &final_text)
            .context("runner did not extract the raw-script answer")?;
        let TaskAnswer::Script(answer) = answer else {
            anyhow::bail!("chat answer was not a script")
        };
        ensure!(bench_core::grade_write(f, &answer.script).score == 1.0);
        println!(
            "{}",
            json!({"id":f.id,"miniscript":ms.to_string(),"asm":asm,
            "hex":script.to_hex_string(),"draft":draft,"draft_reason":bad_grade.reason,
            "final_chat":final_text,"score":grade.score,"lint":grade.lint,
            "chosen_context":"segwitv0","execution_check":true,"chat_extraction_check":true})
        );
    }
    Ok(())
}
