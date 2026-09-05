//! Re-extract saved final assistant text without model calls.
//! cargo run -p bench-cli --example reextract_chat -- DATASET CHAT_TEXT OUT
use std::{collections::BTreeMap, io::Write, path::Path};

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    anyhow::ensure!(
        args.len() == 3,
        "usage: reextract_chat DATASET CHAT_TEXT OUT"
    );
    let fixtures: BTreeMap<_, _> = bench_cli::load_dataset(Path::new(&args[0]))?
        .into_iter()
        .map(|f| (f.id().to_owned(), f))
        .collect();
    let out = Path::new(&args[2]);
    std::fs::create_dir_all(out)?;
    let mut responses = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out.join("responses.jsonl"))?;
    let mut failures = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out.join("failures.jsonl"))?;
    for line in std::fs::read_to_string(&args[1])?
        .lines()
        .filter(|s| !s.trim().is_empty())
    {
        let row: serde_json::Value = serde_json::from_str(line)?;
        let id = row["task_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing task_id"))?;
        let text = row["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing final text"))?;
        let fixture = fixtures
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("unknown task {id}"))?;
        let mut result = serde_json::json!({
            "task_id": id, "raw": row["raw"], "finish_reason": row["finish_reason"],
            "output_tokens": row["output_tokens"],
        });
        if let Some(answer) = bench_cli::runner::reextract_chat_answer(fixture, text) {
            result["answer"] = serde_json::to_value(answer)?;
            writeln!(responses, "{result}")?;
        } else {
            result["error"] = "no unambiguous final answer".into();
            writeln!(failures, "{result}")?;
        }
    }
    Ok(())
}
