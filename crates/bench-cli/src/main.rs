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
        /// Score for a correct label with wrong params on identify tasks.
        #[arg(long, default_value_t = 0.5)]
        partial_credit: f64,
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
        } => {
            let params = GenParams {
                seed,
                write,
                optimize,
                identify,
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
            let stats = tokio::runtime::Runtime::new()
                .context("build tokio runtime")?
                .block_on(bench_cli::runner::run(
                    fixtures,
                    entry,
                    &out,
                    concurrency,
                    display_fmt,
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
                ))?;
            println!(
                "rerun: {} recovered, {} still failing; merged into {}",
                stats.recovered,
                stats.still_failed,
                run_dir.display()
            );
            Ok(())
        }
        Command::Grade {
            dataset,
            responses,
            out,
            partial_credit,
        } => {
            let fixtures = load_dataset(&dataset)?;
            let records = load_responses(&responses)?;
            let (scores, summary) = grade(&fixtures, &records, partial_credit)?;
            fs::create_dir_all(&out)?;
            fs::write(
                out.join("results.json"),
                serde_json::to_string_pretty(&scores)?,
            )?;
            fs::write(out.join("summary.md"), summary_markdown(&summary))?;
            println!("{}", summary_markdown(&summary));
            Ok(())
        }
    }
}
