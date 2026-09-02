//! btc-bench binary. Subcommands:
//! - gen: generate a fixture dataset (fixtures.jsonl + manifest.json)
//! - prompts: emit the prompt for every fixture, one JSONL line
//! - grade: grade a responses JSONL against a dataset

use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context as _, Result};
use bench_cli::{gen_dataset, grade, load_dataset, load_responses, summary_markdown};
use bench_gen::fixtures::GenParams;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "btc-bench",
    version,
    about = "Bitcoin Script benchmark for AI models"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a fixture dataset.
    Gen {
        /// Output directory for fixtures.jsonl and manifest.json.
        #[arg(long)]
        out: PathBuf,
        /// Generator seed.
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Number of write tasks.
        #[arg(long, default_value_t = 300)]
        write: usize,
        /// Number of optimize tasks.
        #[arg(long, default_value_t = 300)]
        optimize: usize,
        /// Number of identify task groups (each emits one item per family).
        #[arg(long, default_value_t = 25)]
        identify: usize,
        /// Number of taproot tree-design tasks (t4). Appended after
        /// the other kinds, so adding trees to an existing seed never
        /// disturbs t1-t3.
        #[arg(long, default_value_t = 0)]
        tree: usize,
        /// Verbalizer template family ids to draw from,
        /// comma-separated (default: 0, the canonical benchmark
        /// phrasing). List only non-eval families (e.g. 1,2) when
        /// generating training sets, so bench-only families never
        /// appear in training data.
        #[arg(long, value_delimiter = ',')]
        verbal_families: Vec<u32>,
        /// Vary the prose structure (seeded): permute commutative
        /// and/or/thresh children and vary the root list shape.
        /// Training-set setting; the eval structure is canonical.
        #[arg(long, default_value_t = false)]
        vary_structure: bool,
        /// Tier cycle for write/optimize tasks, comma-separated
        /// (easy,medium,hard). Repeat a tier to weight it. Default:
        /// the 40/40/20 easy/medium/hard split.
        #[arg(long, value_delimiter = ',')]
        tiers: Vec<String>,
        /// Datasets whose answer keys must not reappear here: any
        /// sampled task whose reference script matches is resampled.
        /// Point this at the eval set when generating training data.
        #[arg(long)]
        exclude: Vec<PathBuf>,
        /// Script-context cycle for write/optimize tasks,
        /// comma-separated (legacy,segwit,tap). Repeat a context to
        /// weight it. Default: even legacy/segwit/tap rotation.
        #[arg(long, value_delimiter = ',')]
        contexts: Vec<String>,
    },
    /// Emit one JSONL line per fixture: {id, kind, prompt}.
    Prompts {
        #[arg(long)]
        dataset: PathBuf,
        /// Output file; defaults to <dataset>/prompts.jsonl.
        #[arg(long)]
        out: Option<PathBuf>,
        /// How embedded scripts are rendered: hex or asm.
        #[arg(long, default_value = "asm")]
        display: String,
    },
    /// Run a live benchmark against one configured model.
    Run {
        #[arg(long)]
        dataset: PathBuf,
        /// Path to models.toml.
        #[arg(long)]
        config: PathBuf,
        /// Name of the [[model.<name>]] entry to run.
        #[arg(long)]
        model: String,
        /// Output directory; defaults to runs/<timestamp>-<model>/.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Only run the first N tasks (debugging).
        #[arg(long)]
        limit: Option<usize>,
        /// Parallel in-flight requests. SGLang-style servers batch
        /// continuously, so this is nearly free throughput.
        #[arg(long, default_value_t = 1)]
        concurrency: usize,
        /// How embedded scripts are shown in prompts.
        #[arg(long, default_value = "asm")]
        display: String,
        /// Graded attempts per task with mechanical feedback between
        /// turns (the benchmark's default mode); pass 1 for single-shot.
        #[arg(long, default_value_t = 3)]
        attempts: u32,
        /// Resume an interrupted run: completed tasks are skipped,
        /// failed tasks retried, output files appended.
        #[arg(long, default_value_t = false)]
        resume: bool,
        /// Diagnostic tools offered beside the submit tool: "none"
        /// (headline benchmark) or "basic" (check_script /
        /// check_descriptor — the compiler-and-lint loop; reference-
        /// free by construction).
        #[arg(long, default_value = "none")]
        tools: bench_cli::runner::ToolMode,
        /// Wrap write/tree prompts in held-out casual eval templates
        /// (the "hey, write me a script" register); optimize/identify
        /// keep the formal prompt. Measures the informal register the
        /// casual SFT exports train.
        #[arg(long, default_value_t = false)]
        casual: bool,
    },
    /// Re-attempt the failed tasks in a run directory.
    Rerun {
        #[arg(long)]
        dataset: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        model: String,
        /// The run directory (contains responses.jsonl / failures.jsonl).
        #[arg(long)]
        run_dir: PathBuf,
        #[arg(long, default_value_t = 4)]
        concurrency: usize,
        #[arg(long, default_value = "asm")]
        display: String,
        /// Diagnostic tools (see run --tools).
        #[arg(long, default_value = "none")]
        tools: bench_cli::runner::ToolMode,
    },
    /// Serve the graders over HTTP for RL training loops.
    RewardServe {
        /// Address to bind (use 0.0.0.0:PORT to serve a remote trainer).
        #[arg(long, default_value = "127.0.0.1:9900")]
        bind: String,
        /// Worker threads handling requests.
        #[arg(long, default_value_t = 8)]
        threads: usize,
        /// Shaping rung for a parseable answer.
        #[arg(long, default_value_t = 0.0)]
        shape_parse: f64,
        /// Shaping rung for clearing the miniscript decode gate.
        #[arg(long, default_value_t = 0.0)]
        shape_decode: f64,
        /// Shaping band scaled by balanced truth-table agreement
        /// (constant scripts earn none of it).
        #[arg(long, default_value_t = 0.0)]
        shape_agreement: f64,
        /// Floor for an equivalent-but-unimproved optimize answer.
        #[arg(long, default_value_t = 0.0)]
        shape_equivalent_floor: f64,
        /// Penalty per lint finding on the shaped score.
        #[arg(long, default_value_t = 0.0)]
        lint_penalty: f64,
        /// Zero the shaped score of linted answers (training analog of
        /// grade --standard-mode).
        #[arg(long, default_value_t = false)]
        lint_gate: bool,
    },
    /// Export SFT pairs: one JSONL line per task with the exact
    /// runner prompt and the reference answer (hex + asm for scripts,
    /// the family label for identify). Format completions to taste.
    SftExport {
        #[arg(long)]
        dataset: PathBuf,
        /// Output file; defaults to <dataset>/sft.jsonl.
        #[arg(long)]
        out: Option<PathBuf>,
        /// How embedded scripts are rendered in prompts: hex or asm.
        #[arg(long, default_value = "asm")]
        display: String,
        /// Wrap write/tree prompts in casual training templates (the
        /// "hey, write me a script" register); optimize/identify are
        /// skipped. Default output becomes <dataset>/sft-casual.jsonl.
        /// Eval-split casual templates are reserved for `run --casual`.
        #[arg(long, default_value_t = false)]
        casual: bool,
    },
    /// Audit a committed dataset: re-verify every answer key.
    Audit {
        /// Dataset directory (fixtures.jsonl + manifest.json).
        #[arg(long)]
        dataset: PathBuf,
    },
    /// Grade responses against a dataset.
    Grade {
        #[arg(long)]
        dataset: PathBuf,
        /// Responses JSONL: {task_id, answer, raw?}.
        #[arg(long)]
        responses: PathBuf,
        /// Output directory for results.json and summary.md.
        #[arg(long)]
        out: PathBuf,
        /// attempts.jsonl from a multi-turn run; enables turn-discounted
        /// scoring and the first-try/solved breakdown.
        #[arg(long)]
        attempts: Option<PathBuf>,
        /// Turn-discount floor: solving on the final attempt scores this
        /// fraction of the graded score; first try scores it in full.
        #[arg(long, default_value_t = 0.5)]
        mt_base: f64,
        /// Gate write/optimize scores on miniscript sanity: answers
        /// with lint findings (malleable, unsafe, repeated keys, ...)
        /// score 0 with the findings as the reason.
        #[arg(long, default_value_t = false)]
        standard_mode: bool,
    },
    /// Cross-model sweep report (tier/context cuts, failure taxonomy,
    /// lint rates, optimize curve).
    Report {
        /// Dataset the runs were graded against.
        #[arg(long)]
        dataset: PathBuf,
        /// Run dirs (must contain graded/results.json), comma-separated.
        #[arg(long, value_delimiter = ',')]
        runs: Vec<PathBuf>,
        /// Labels for the runs, comma-separated (defaults to dir names).
        #[arg(long, value_delimiter = ',')]
        labels: Vec<String>,
        /// Output markdown file.
        #[arg(long)]
        out: PathBuf,
    },
    /// pass@k / pass^k across N sampled runs of one model.
    Passk {
        #[arg(long)]
        dataset: PathBuf,
        /// Two or more run dirs (responses.jsonl), comma-separated.
        #[arg(long, value_delimiter = ',')]
        runs: Vec<PathBuf>,
        /// Labels, comma-separated (defaults to dir names).
        #[arg(long, value_delimiter = ',')]
        labels: Vec<String>,
        #[arg(long)]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Gen {
            out,
            seed,
            write,
            optimize,
            identify,
            tree,
            verbal_families,
            vary_structure,
            tiers,
            exclude,
            contexts,
        } => {
            if let Some(f) = verbal_families
                .iter()
                .find(|f| **f >= bench_gen::verbal::FAMILIES)
            {
                bail!(
                    "unknown verbal family {f}; known families are 0..{}",
                    bench_gen::verbal::FAMILIES - 1
                );
            }
            let tiers = tiers
                .iter()
                .map(|t| match t.trim().to_ascii_lowercase().as_str() {
                    "easy" => Ok(bench_core::Tier::Easy),
                    "medium" => Ok(bench_core::Tier::Medium),
                    "hard" => Ok(bench_core::Tier::Hard),
                    other => Err(anyhow::anyhow!(
                        "unknown tier {other:?}; use easy, medium, or hard"
                    )),
                })
                .collect::<Result<Vec<_>>>()?;
            let contexts = contexts
                .iter()
                .map(|c| match c.trim().to_ascii_lowercase().as_str() {
                    "legacy" => Ok(bench_core::task::ContextKind::Legacy),
                    "segwit" => Ok(bench_core::task::ContextKind::SegwitV0),
                    "tap" => Ok(bench_core::task::ContextKind::Tap),
                    other => Err(anyhow::anyhow!(
                        "unknown context {other:?}; use legacy, segwit, or tap"
                    )),
                })
                .collect::<Result<Vec<_>>>()?;
            let mut excluded = std::collections::BTreeSet::new();
            for dir in &exclude {
                for f in load_dataset(dir)? {
                    match f {
                        bench_core::task::Fixture::Write(w) => {
                            excluded.insert(w.reference_script_hex);
                        }
                        bench_core::task::Fixture::Optimize(o) => {
                            excluded.insert(o.optimal_script_hex);
                            // The baseline is embedded verbatim in the
                            // prompt; a shared baseline leaks an eval
                            // prompt into training data.
                            excluded.insert(o.baseline_script_hex);
                        }
                        bench_core::task::Fixture::Tree(t) => {
                            excluded.insert(t.reference_descriptor);
                        }
                        bench_core::task::Fixture::Identify(i) => {
                            excluded.insert(i.spk_hex);
                            if let Some(inner) = i.inner_script_hex {
                                excluded.insert(inner);
                            }
                        }
                    }
                }
            }
            if !excluded.is_empty() {
                println!(
                    "excluding {} answer keys from {} dataset(s)",
                    excluded.len(),
                    exclude.len()
                );
            }
            let params = GenParams {
                seed,
                write,
                optimize,
                identify,
                tree,
                verbal_families,
                vary_structure,
                tiers,
                exclude: excluded,
                contexts,
            };
            let n = gen_dataset(&out, &params, env!("CARGO_PKG_VERSION"))?;
            println!("wrote {n} fixtures to {}", out.display());
            Ok(())
        }
        Command::Prompts {
            dataset,
            out,
            display,
        } => {
            let fixtures = load_dataset(&dataset)?;
            let display_fmt = match display.as_str() {
                "hex" => bench_gen::prompt::DisplayFormat::Hex,
                "asm" => bench_gen::prompt::DisplayFormat::Asm,
                other => bail!("unknown --display {other:?}; use hex or asm"),
            };
            let out = out.unwrap_or_else(|| dataset.join("prompts.jsonl"));
            let mut text = String::new();
            for f in &fixtures {
                let kind = match f {
                    bench_core::task::Fixture::Write(_) => "write",
                    bench_core::task::Fixture::Optimize(_) => "optimize",
                    bench_core::task::Fixture::Identify(_) => "identify",
                    bench_core::task::Fixture::Tree(_) => "tree",
                };
                let line = serde_json::json!({
                    "id": f.id(),
                    "kind": kind,
                    "prompt": bench_gen::prompt::for_fixture_fmt(f, display_fmt),
                });
                text.push_str(&line.to_string());
                text.push('\n');
            }
            fs::write(&out, text).with_context(|| format!("write {}", out.display()))?;
            println!("wrote prompts to {}", out.display());
            Ok(())
        }
        Command::Run {
            dataset,
            config,
            model,
            out,
            limit,
            concurrency,
            display,
            attempts,
            resume,
            tools,
            casual,
        } => {
            let all = load_dataset(&dataset)?;
            let fixtures = match limit {
                Some(n) => &all[..n.min(all.len())],
                None => &all[..],
            };
            let display_fmt = match display.as_str() {
                "hex" => bench_gen::prompt::DisplayFormat::Hex,
                "asm" => bench_gen::prompt::DisplayFormat::Asm,
                other => bail!("unknown --display {other:?}; use hex or asm"),
            };
            let models = bench_cli::runner::load_models_config(&config)?;
            let entry = models.get(&model).with_context(|| {
                format!(
                    "model {model:?} not in {}; have: {:?}",
                    config.display(),
                    models.keys()
                )
            })?;
            let out = out.unwrap_or_else(|| {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                PathBuf::from(format!("runs/{ts}-{model}"))
            });
            // Provenance: everything needed to interpret this run
            // years later without the original shell history — which
            // dataset (manifest embedded, since datasets/ is not in
            // git), which model entry, which knobs.
            fs::create_dir_all(&out)?;
            let manifest: serde_json::Value = fs::read_to_string(dataset.join("manifest.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or(serde_json::Value::Null);
            let run_meta = serde_json::json!({
                "model": model,
                "dataset": dataset.display().to_string(),
                "dataset_manifest": manifest,
                "attempts": attempts,
                "concurrency": concurrency,
                "display": display,
                "limit": limit,
                "resume": resume,
                "tools": tools.to_string(),
                "casual": casual,
                "bench_version": env!("CARGO_PKG_VERSION"),
                "started_unix": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            });
            fs::write(
                out.join("run.json"),
                serde_json::to_string_pretty(&run_meta)?,
            )?;
            let stats = tokio::runtime::Runtime::new()
                .context("build tokio runtime")?
                .block_on(bench_cli::runner::run_resume(
                    fixtures,
                    entry,
                    &out,
                    concurrency,
                    display_fmt,
                    attempts,
                    tools,
                    resume,
                    casual,
                ))?;
            println!(
                "ran {} tasks: {} answered, {} failed; responses in {}/responses.jsonl",
                fixtures.len(),
                stats.answered,
                stats.failed,
                out.display()
            );
            Ok(())
        }
        Command::Rerun {
            dataset,
            config,
            model,
            run_dir,
            concurrency,
            display,
            tools,
        } => {
            let fixtures = load_dataset(&dataset)?;
            let display_fmt = match display.as_str() {
                "hex" => bench_gen::prompt::DisplayFormat::Hex,
                "asm" => bench_gen::prompt::DisplayFormat::Asm,
                other => bail!("unknown --display {other:?}; use hex or asm"),
            };
            let models = bench_cli::runner::load_models_config(&config)?;
            let entry = models.get(&model).with_context(|| {
                format!(
                    "model {model:?} not in {}; have: {:?}",
                    config.display(),
                    models.keys()
                )
            })?;
            let stats = tokio::runtime::Runtime::new()
                .context("build tokio runtime")?
                .block_on(bench_cli::runner::rerun(
                    &fixtures,
                    entry,
                    &run_dir,
                    concurrency,
                    display_fmt,
                    tools,
                ))?;
            println!(
                "rerun: {} recovered, {} still failing; merged into {}",
                stats.recovered,
                stats.still_failed,
                run_dir.display()
            );
            Ok(())
        }
        Command::RewardServe {
            bind,
            threads,
            shape_parse,
            shape_decode,
            shape_agreement,
            shape_equivalent_floor,
            lint_penalty,
            lint_gate,
        } => bench_cli::reward::serve(
            &bind,
            threads,
            bench_cli::reward::Shaping {
                parse: shape_parse,
                decode: shape_decode,
                agreement: shape_agreement,
                equivalent_floor: shape_equivalent_floor,
                lint_penalty,
                lint_gate,
            },
        ),
        Command::SftExport {
            dataset,
            out,
            display,
            casual,
        } => {
            let fixtures = load_dataset(&dataset)?;
            let display_fmt = match display.as_str() {
                "hex" => bench_gen::prompt::DisplayFormat::Hex,
                "asm" => bench_gen::prompt::DisplayFormat::Asm,
                other => bail!("unknown --display {other:?}; use hex or asm"),
            };
            let default_name = if casual {
                "sft-casual.jsonl"
            } else {
                "sft.jsonl"
            };
            let out = out.unwrap_or_else(|| dataset.join(default_name));
            let mut text = String::new();
            let mut written = 0usize;
            for (idx, f) in fixtures.iter().enumerate() {
                let prompt = if casual {
                    match bench_gen::casual::prompt_for(
                        f,
                        idx as u64,
                        bench_gen::casual::Split::Train,
                    ) {
                        Some(p) => p,
                        None => continue,
                    }
                } else {
                    bench_gen::prompt::for_fixture_fmt(f, display_fmt)
                };
                let asm = |hex: &str| {
                    bench_core::human_asm::to_human_asm(
                        bitcoin::ScriptBuf::from_hex(hex)
                            .expect("fixture hex is valid")
                            .as_script(),
                    )
                };
                let mut line = match f {
                    bench_core::task::Fixture::Write(w) => serde_json::json!({
                        "task_id": w.id, "kind": "write", "prompt": prompt,
                        "target_hex": w.reference_script_hex,
                        "target_asm": asm(&w.reference_script_hex),
                    }),
                    bench_core::task::Fixture::Optimize(o) => serde_json::json!({
                        "task_id": o.id, "kind": "optimize", "prompt": prompt,
                        "target_hex": o.optimal_script_hex,
                        "target_asm": asm(&o.optimal_script_hex),
                    }),
                    bench_core::task::Fixture::Identify(i) => serde_json::json!({
                        "task_id": i.id, "kind": "identify", "prompt": prompt,
                        "target_label": i.family,
                    }),
                    bench_core::task::Fixture::Tree(t) => serde_json::json!({
                        "task_id": t.id, "kind": "tree", "prompt": prompt,
                        "target_descriptor": t.reference_descriptor,
                    }),
                };
                if casual {
                    line.as_object_mut()
                        .expect("json object")
                        .insert("style".into(), serde_json::json!("casual"));
                }
                text.push_str(&line.to_string());
                text.push('\n');
                written += 1;
            }
            fs::write(&out, text).with_context(|| format!("write {}", out.display()))?;
            println!("wrote {written} SFT pairs to {}", out.display());
            Ok(())
        }
        Command::Audit { dataset } => {
            let report = bench_cli::audit::audit_dataset(&dataset)?;
            for w in &report.warnings {
                println!("warning: {w}");
            }
            for f in &report.failures {
                println!("FAIL: {f}");
            }
            println!(
                "audited {} fixtures: {} failures, {} warnings",
                report.fixtures_checked,
                report.failures.len(),
                report.warnings.len()
            );
            if bench_cli::audit::report_ok(&report) {
                Ok(())
            } else {
                bail!(
                    "dataset {} failed audit ({} failures)",
                    dataset.display(),
                    report.failures.len()
                )
            }
        }
        Command::Grade {
            dataset,
            responses,
            out,
            attempts,
            mt_base,
            standard_mode,
        } => {
            let fixtures = load_dataset(&dataset)?;
            let records = load_responses(&responses)?;
            let attempts_map = attempts
                .as_ref()
                .map(|p| bench_cli::load_attempts(p))
                .transpose()?;
            let (scores, summary) = grade(
                &fixtures,
                &records,
                attempts_map.as_ref(),
                mt_base,
                standard_mode,
            )?;
            fs::create_dir_all(&out)?;
            fs::write(
                out.join("results.json"),
                serde_json::to_string_pretty(&scores)?,
            )?;
            fs::write(out.join("summary.md"), summary_markdown(&summary))?;
            fs::write(
                out.join("meta.json"),
                serde_json::to_string_pretty(&serde_json::json!({
                    "graded_by": bench_cli::build_stamp(),
                }))?,
            )?;
            println!("{}", summary_markdown(&summary));
            Ok(())
        }
        Command::Report {
            dataset,
            runs,
            labels,
            out,
        } => {
            let pairs = label_pairs(runs, labels)?;
            bench_cli::report::report(&dataset, &pairs, &out)?;
            println!("wrote {}", out.display());
            Ok(())
        }
        Command::Passk {
            dataset,
            runs,
            labels,
            out,
        } => {
            let pairs = label_pairs(runs, labels)?;
            bench_cli::report::passk(&dataset, &pairs, &out)?;
            println!("wrote {}", out.display());
            Ok(())
        }
    }
}

/// Pair run dirs with labels: explicit labels if given, else dir names.
fn label_pairs(runs: Vec<PathBuf>, labels: Vec<String>) -> Result<Vec<(String, PathBuf)>> {
    if !labels.is_empty() && labels.len() != runs.len() {
        bail!(
            "--labels count ({}) must match --runs count ({})",
            labels.len(),
            runs.len()
        );
    }
    Ok(runs
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            let label = labels.get(i).cloned().unwrap_or_else(|| {
                p.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| format!("run{i}"))
            });
            (label, p)
        })
        .collect())
}
