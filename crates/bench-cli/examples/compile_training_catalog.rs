//! Compile the separate composed-training source, never a human eval catalog.
use anyhow::{ensure, Context, Result};
use bitcoin::hashes::{sha256, Hash};
use std::{fs, path::PathBuf};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 2,
        "usage: compile_training_catalog SOURCE OUT"
    );
    let source = fs::read(&args[0])?;
    let input: serde_json::Value = serde_json::from_slice(&source)?;
    ensure!(
        input["generator"] == "composed-training-v1" && input["purpose"] == "training",
        "not a composed training source"
    );
    let cases = input["cases"].as_array().context("missing cases")?;
    ensure!(!cases.is_empty(), "empty training source");
    for case in cases {
        ensure!(
            case["id"]
                .as_str()
                .is_some_and(|s| s.starts_with("composed-train-")),
            "reserved case ID"
        );
        ensure!(
            case["group"]
                .as_str()
                .is_some_and(|s| s.starts_with("composed-train-")),
            "reserved group ID"
        );
    }
    let out = PathBuf::from(&args[1]);
    ensure!(
        !out.exists(),
        "output already exists; preserve the earlier pool"
    );
    let data = bench_gen::human::compile_catalog(&serde_json::to_string(cases)?)
        .map_err(anyhow::Error::msg)?;
    bench_cli::write_dataset(
        &out,
        &data.fixtures,
        input["seed"].as_u64().context("missing seed")?,
        &bench_cli::build_stamp(),
    )?;
    let manifest_path = out.join("manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    manifest["suite"] = "composed-training-v1".into();
    manifest["evaluation_only"] = false.into();
    manifest["source_sha256"] = sha256::Hash::hash(&source).to_string().into();
    manifest["fixtures_sha256"] = sha256::Hash::hash(&fs::read(out.join("fixtures.jsonl"))?)
        .to_string()
        .into();
    manifest["exclusions"] = input["exclusions"].clone();
    fs::write(manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    fs::write(
        out.join("groups.json"),
        serde_json::to_vec_pretty(&data.groups)?,
    )?;
    fs::write(
        out.join("reference-notes.json"),
        serde_json::to_vec_pretty(&data.reference_notes)?,
    )?;
    fs::write(out.join("source.json"), source)?;
    println!(
        "compiled {} independent training fixtures to {}",
        data.fixtures.len(),
        out.display()
    );
    Ok(())
}
