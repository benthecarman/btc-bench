//! Compile a separate validation catalog and mark it evaluation-only.
use anyhow::{ensure, Context, Result};
use bitcoin::hashes::{sha256, Hash};
use std::{fs, path::PathBuf};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 2,
        "usage: compile_validation_catalog SOURCE OUT"
    );
    let source = fs::read(&args[0])?;
    let input: serde_json::Value = serde_json::from_slice(&source)?;
    ensure!(
        input["generator"] == "compound-validation-v1" && input["purpose"] == "validation",
        "not a compound validation source"
    );
    let cases = input["cases"].as_array().context("missing cases")?;
    ensure!(!cases.is_empty(), "empty validation source");
    for case in cases {
        for field in ["id", "group"] {
            ensure!(
                case[field]
                    .as_str()
                    .is_some_and(|s| s.starts_with("composed-val-")),
                "reserved validation ID"
            );
        }
    }
    let out = PathBuf::from(&args[1]);
    ensure!(
        !out.exists(),
        "output exists; preserve the previous validation set"
    );
    let data = bench_gen::human::compile_catalog(&serde_json::to_string(cases)?)
        .map_err(anyhow::Error::msg)?;
    bench_cli::write_dataset(
        &out,
        &data.fixtures,
        input["seed"].as_u64().context("missing seed")?,
        &bench_cli::build_stamp(),
    )?;
    let path = out.join("manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    manifest["suite"] = input["suite"]
        .as_str()
        .unwrap_or("compound-v2-validation")
        .into();
    manifest["evaluation_only"] = true.into();
    manifest["purpose"] = input["evaluation_role"]
        .as_str()
        .unwrap_or("checkpoint selection; not a final test")
        .into();
    manifest["source_sha256"] = sha256::Hash::hash(&source).to_string().into();
    manifest["fixtures_sha256"] = sha256::Hash::hash(&fs::read(out.join("fixtures.jsonl"))?)
        .to_string()
        .into();
    fs::write(path, serde_json::to_vec_pretty(&manifest)?)?;
    fs::write(out.join("source.json"), source)?;
    fs::write(
        out.join("groups.json"),
        serde_json::to_vec_pretty(&data.groups)?,
    )?;
    fs::write(
        out.join("reference-notes.json"),
        serde_json::to_vec_pretty(&data.reference_notes)?,
    )?;
    println!(
        "Compiled {} evaluation-only validation fixtures",
        data.fixtures.len()
    );
    Ok(())
}
