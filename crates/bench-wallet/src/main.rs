use anyhow::{ensure, Context, Result};
use bench_wallet::*;
use bip388::{ClearText, KeyInformation};
use bitcoin::{
    bip32::{Xpriv, Xpub},
    hashes::{sha256, Hash},
    secp256k1::Secp256k1,
    NetworkKind,
};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    name = "btc-wallet-bench",
    about = "BIP-388 template/concrete diagnostic pilot"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Build {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    BuildTraining {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    BuildEvaluation {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Audit {
        #[arg(long)]
        dataset: PathBuf,
    },
    GradeTraining {
        #[arg(long)]
        dataset: PathBuf,
        #[arg(long)]
        responses: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Grade {
        #[arg(long)]
        dataset: PathBuf,
        #[arg(long)]
        responses: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    suite: String,
    evaluation_only: bool,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    family: String,
    group: String,
    request: String,
    keys: Vec<String>,
    template: String,
    policy: String,
}

fn hash(bytes: &[u8]) -> String {
    sha256::Hash::hash(bytes).to_string()
}

fn write_json(path: impl AsRef<Path>, value: &impl serde::Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

fn load(dataset: &Path, training: bool) -> Result<Vec<WalletFixture>> {
    let bytes = fs::read(dataset.join("fixtures.jsonl"))?;
    let manifest: Value = serde_json::from_slice(&fs::read(dataset.join("manifest.json"))?)?;
    ensure!(
        manifest["evaluation_only"] == !training && manifest["schema"] == "btc-wallet-v1",
        "wrong wallet dataset manifest"
    );
    ensure!(
        manifest["fixtures_sha256"] == hash(&bytes),
        "fixture hash mismatch"
    );
    let fixtures: Vec<WalletFixture> = String::from_utf8(bytes)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    ensure!(!fixtures.is_empty(), "empty dataset");
    let ids: BTreeSet<_> = fixtures.iter().map(|f| &f.id).collect();
    ensure!(ids.len() == fixtures.len(), "duplicate fixture IDs");
    ensure!(
        fixtures
            .iter()
            .all(|f| f.split == if training { "training" } else { "evaluation" }),
        "fixture split mismatch"
    );
    Ok(fixtures)
}

fn build(path: &Path, out: &Path, training: bool, pilot: bool) -> Result<()> {
    ensure!(!out.exists(), "output exists; preserve the earlier dataset");
    let source = fs::read(path)?;
    let catalog: Catalog = serde_json::from_slice(&source)?;
    ensure!(
        catalog.evaluation_only != training && !catalog.cases.is_empty(),
        "catalog split does not match the requested build command"
    );
    ensure!(
        !pilot || catalog.cases.len() == 20,
        "expected 20 evaluation-only scenarios"
    );
    let scenario_count = catalog.cases.len();
    let mut fixtures = Vec::new();
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for case in catalog.cases {
        ensure!(seen.insert(case.id.clone()), "duplicate scenario ID");
        ensure!(
            !case.request.contains('@'),
            "human request must use names, not placeholder syntax"
        );
        let keys: Vec<_> = case
            .keys
            .iter()
            .enumerate()
            .map(|(i, label)| {
                // Public test keys only. No funded-wallet material enters this fixture.
                let seed =
                    sha256::Hash::hash(format!("btc-wallet-v1/{}/key/{i}", case.id).as_bytes())
                        .to_byte_array();
                let master = Xpriv::new_master(NetworkKind::Test, &seed).expect("32-byte seed");
                WalletKey {
                    label: label.clone(),
                    key_info: Xpub::from_priv(&Secp256k1::new(), &master).to_string(),
                }
            })
            .collect();
        let info: Vec<KeyInformation> = keys
            .iter()
            .map(|k| {
                KeyInformation::try_from(k.key_info.as_str()).map_err(|e| anyhow::anyhow!("{e:?}"))
            })
            .collect::<Result<_>>()?;
        let template = standard_template(&case.template)?;
        let (cleartext, supported) = template.to_cleartext();
        ensure!(
            supported || (!pilot && !training),
            "{}: no supported cleartext for reference",
            case.id
        );
        let roundtrip = cleartext_roundtrip(&template);
        ensure!(
            roundtrip || (!pilot && !training),
            "{}: cleartext round trip failed",
            case.id
        );
        let mut derivations = Vec::new();
        for (change, index) in [(false, 0), (false, 7), (true, 0), (true, 7)] {
            let public_keys = derive_keys(&info, case.template.starts_with("tr("), change, index)?;
            derivations.push(Derivation {
                is_change: change,
                address_index: index,
                descriptor: derive_descriptor(&case.template, &info, change, index)?,
                policy: instantiate_policy(&case.policy, &public_keys)?,
            });
        }
        for output_kind in [OutputKind::Template, OutputKind::Concrete] {
            let name = if output_kind == OutputKind::Template {
                "template"
            } else {
                "concrete"
            };
            let mut request = case.request.clone();
            if output_kind == OutputKind::Template {
                request.push_str("\n\nReturn a BIP-388 descriptor template. Use the standard receive/change branches <0;1>/* for every key (/** is the shorthand). The key list is managed separately; use these placeholders:\n");
                for (i, key) in keys.iter().enumerate() {
                    request.push_str(&format!("{}: @{i}\n", key.label));
                }
            } else {
                request.push_str("\n\nReturn a concrete descriptor using the public keys below. These are already derived for this address; no extended keys, placeholders, or wildcards are needed.\n");
                let derived = derive_keys(&info, case.template.starts_with("tr("), false, 0)?;
                for (key, public) in keys.iter().zip(derived) {
                    request.push_str(&format!("{}: {public}\n", key.label));
                }
            }
            let fixture = WalletFixture {
                id: format!("{}-{name}", case.id),
                group: case.group.clone(),
                family: case.family.clone(),
                split: if training { "training" } else { "evaluation" }.into(),
                output_kind,
                request,
                spec_en: case.request.clone(),
                keys: keys.clone(),
                reference_template: case.template.clone(),
                policy_template: case.policy.clone(),
                cleartext: cleartext.clone(),
                cleartext_supported: supported,
                cleartext_roundtrip: roundtrip,
                confusion_score: template.confusion_score(),
                derivations: derivations.clone(),
            };
            if training {
                audit_training(&fixture)
            } else {
                audit(&fixture)
            }
            .with_context(|| fixture.id.clone())?;
            groups
                .entry(fixture.group.clone())
                .or_default()
                .push(fixture.id.clone());
            fixtures.push(fixture);
        }
    }
    ensure!(
        !pilot || (groups.len() == 10 && groups.values().all(|g| g.len() == 4)),
        "expected ten intact family groups"
    );
    fs::create_dir_all(out)?;
    let text = fixtures
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()?
        .join("\n")
        + "\n";
    fs::write(out.join("fixtures.jsonl"), &text)?;
    fs::write(out.join("source.json"), &source)?;
    write_json(out.join("groups.json"), &groups)?;
    let references: String = fixtures
        .iter()
        .map(|f| {
            json!({"task_id":f.id,"answer":reference_answer(f),"finish_reason":"reference"})
                .to_string()
                + "\n"
        })
        .collect();
    fs::write(out.join("references.jsonl"), references)?;
    if training {
        let traces = fixtures
            .iter()
            .map(training_trace)
            .collect::<Result<Vec<_>>>()?;
        write_json(out.join("traces.json"), &traces)?;
    }
    write_json(
        out.join("manifest.json"),
        &json!({"schema":"btc-wallet-v1","suite":catalog.suite,"evaluation_only":!training,
        "purpose":"paired representation diagnostic; not an independent human transfer test", "questions":fixtures.len(),"scenarios":scenario_count,"groups":groups.len(),
        "fixtures_sha256":hash(text.as_bytes()),"source_sha256":hash(&source),"bip388_revision":BIP388_REVISION,
        "cargo_lock_sha256":hash(&fs::read("Cargo.lock")?),"pins":{"miniscript":"13.1.0","bitcoin":"0.32.102"}}),
    )?;
    println!(
        "Built and audited {} questions in {} family groups",
        fixtures.len(),
        groups.len()
    );
    Ok(())
}

fn grade_file(dataset: &Path, responses: &Path, out: &Path, training: bool) -> Result<()> {
    ensure!(!out.exists(), "output exists; preserve earlier grades");
    let fixtures = load(dataset, training)?;
    let ids: BTreeSet<_> = fixtures.iter().map(|f| f.id.as_str()).collect();
    let mut answers = BTreeMap::new();
    for line in fs::read_to_string(responses)?
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let row: Value = serde_json::from_str(line)?;
        let id = row["task_id"]
            .as_str()
            .context("response missing task_id")?
            .to_string();
        ensure!(ids.contains(id.as_str()), "unexpected response ID {id}");
        ensure!(answers.insert(id, row).is_none(), "duplicate response ID");
    }
    let mut scores = Vec::new();
    let mut extraction = Vec::new();
    for f in &fixtures {
        let answer = answers
            .get(&f.id)
            .context("missing response")
            .and_then(|r| {
                if let Some(a) = r["answer"].as_str() {
                    Ok(a.to_string())
                } else {
                    extract_chat(
                        f.output_kind,
                        r["text"].as_str().context("missing final answer")?,
                    )
                }
            });
        let score = match answer {
            Ok(answer) => {
                let s = grade(f, &answer);
                extraction.push(json!({"task_id":f.id,"answer":answer}));
                s
            }
            Err(e) => WalletScore {
                task_id: f.id.clone(),
                score: 0.0,
                failure: Some("missing or ambiguous answer".into()),
                reason: Some(e.to_string()),
            },
        };
        scores.push(score);
    }
    let mut summary = BTreeMap::new();
    for kind in [OutputKind::Template, OutputKind::Concrete] {
        let rows: Vec<_> = fixtures
            .iter()
            .zip(&scores)
            .filter(|(f, _)| f.output_kind == kind)
            .collect();
        summary.insert(
            format!("{kind:?}").to_lowercase(),
            json!({"correct":rows.iter().filter(|(_,s)|s.score==1.0).count(),"total":rows.len()}),
        );
    }
    fs::create_dir_all(out)?;
    write_json(out.join("results.json"), &scores)?;
    write_json(out.join("summary.json"), &summary)?;
    write_json(out.join("extracted.json"), &extraction)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

fn main() -> Result<()> {
    match Args::parse().command {
        Command::Build { catalog, out } => build(&catalog, &out, false, true),
        Command::BuildTraining { catalog, out } => build(&catalog, &out, true, false),
        Command::BuildEvaluation { catalog, out } => build(&catalog, &out, false, false),
        Command::Audit { dataset } => {
            let fixtures = load(&dataset, false)?;
            for f in &fixtures {
                audit(f).with_context(|| f.id.clone())?;
            }
            println!("Audited {} references", fixtures.len());
            Ok(())
        }
        Command::Grade {
            dataset,
            responses,
            out,
        } => grade_file(&dataset, &responses, &out, false),
        Command::GradeTraining {
            dataset,
            responses,
            out,
        } => grade_file(&dataset, &responses, &out, true),
    }
}
