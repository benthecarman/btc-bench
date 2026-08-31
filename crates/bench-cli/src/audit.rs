//! Dataset self-audit (the differential/regression methodology from
//! rust-miniscript's `regression_compiler` fuzz target, applied to our
//! committed fixtures): re-derive every answer key from first principles
//! and compare against what is stored. Catches dependency drift
//! (miniscript/bitcoin bumps that change compiler output or weight
//! computation) and silent dataset corruption before either can poison
//! a benchmark run.
//!
//! Per fixture:
//! - stored script bytes must decode and oracle-verify against
//!   themselves (the gradability invariant held at generation time);
//! - the stored policy must recompile to a byte-identical script
//!   (byte drift is a warning — the compiler is version-pinned — while
//!   non-equivalence is a hard failure);
//! - stored weights/sizes must match freshly computed values;
//! - optimize baselines must stay oracle-equivalent to the reference
//!   and strictly heavier;
//! - the execution oracle must still prove both scripts spendable;
//! - the manifest must match: schema version, per-kind fixture counts,
//!   and dependency pins (against the versions this build declares —
//!   a unit test ties those declarations to the workspace lockfile).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, Result};
use bench_core::task::{ContextKind, Fixture, OptimizeFixture, WriteFixture};
use bench_core::{check_equivalence, execution_check, weights_for, HashPreimages, Verdict};

use bitcoin::{PublicKey, ScriptBuf, XOnlyPublicKey};
use miniscript::{policy::Concrete, Legacy, Segwitv0, Tap};

use crate::{load_dataset, Manifest, BITCOIN_VERSION, MINISCRIPT_VERSION};

#[derive(serde::Serialize, Clone, Debug, Default)]
pub struct AuditReport {
    pub fixtures_checked: usize,
    /// Hard failures: the dataset is unusable as an answer key.
    pub failures: Vec<String>,
    /// Byte drift between stored and recompiled reference scripts that
    /// the oracle still proves equivalent (dependency drift).
    pub warnings: Vec<String>,
}

fn recompile(context: ContextKind, policy: &str) -> Result<(String, String)> {
    // Returns (miniscript text, script hex).
    match context {
        ContextKind::Legacy => {
            let p = Concrete::<PublicKey>::from_str(policy)?;
            let ms = p.compile::<Legacy>()?;
            Ok((ms.to_string(), ms.encode().to_hex_string()))
        }
        ContextKind::SegwitV0 => {
            let p = Concrete::<PublicKey>::from_str(policy)?;
            let ms = p.compile::<Segwitv0>()?;
            Ok((ms.to_string(), ms.encode().to_hex_string()))
        }
        ContextKind::Tap => {
            let p = Concrete::<XOnlyPublicKey>::from_str(policy)?;
            let ms = p.compile::<Tap>()?;
            Ok((ms.to_string(), ms.encode().to_hex_string()))
        }
    }
}

impl AuditReport {
    fn fail(&mut self, msg: String) {
        self.failures.push(msg);
    }

    fn warn(&mut self, msg: String) {
        self.warnings.push(msg);
    }

    fn check_script(
        &mut self,
        id: &str,
        context: ContextKind,
        script_hex: &str,
        preimages: &HashPreimages,
        what: &str,
    ) -> Option<ScriptBuf> {
        let script = match ScriptBuf::from_hex(script_hex) {
            Ok(s) => s,
            Err(e) => {
                self.fail(format!("{id}: {what} hex invalid: {e}"));
                return None;
            }
        };
        if check_equivalence(context, &script, &script) != Verdict::Equivalent {
            self.fail(format!(
                "{id}: {what} no longer oracle-verifies against itself"
            ));
            return None;
        }
        if let Err(e) = execution_check(context, &script, preimages) {
            self.fail(format!("{id}: {what} failed the execution oracle: {e}"));
            return None;
        }
        Some(script)
    }

    fn check_weights(
        &mut self,
        id: &str,
        context: ContextKind,
        script: &ScriptBuf,
        stored_weight: usize,
        stored_size: usize,
        what: &str,
    ) {
        match weights_for(context, script) {
            Ok(w) => {
                if w.weight != stored_weight {
                    self.fail(format!(
                        "{id}: {what} weight drift: stored {stored_weight}, computed {}",
                        w.weight
                    ));
                }
                if w.size != stored_size {
                    self.fail(format!(
                        "{id}: {what} size drift: stored {stored_size}, computed {}",
                        w.size
                    ));
                }
            }
            Err(e) => self.fail(format!("{id}: {what} weights no longer computable: {e}")),
        }
    }

    fn check_preimages(&mut self, id: &str, map: &BTreeMap<String, String>) -> HashPreimages {
        match HashPreimages::from_hex_map(map) {
            Ok(p) => p,
            Err(e) => {
                self.fail(format!("{id}: stored preimages invalid: {e}"));
                HashPreimages::default()
            }
        }
    }

    fn check_write(&mut self, w: &WriteFixture) {
        let pre = self.check_preimages(&w.id, &w.hash_preimages);
        // Write fixtures store no weights; nothing to drift-check there.
        let Some(reference) =
            self.check_script(&w.id, w.context, &w.reference_script_hex, &pre, "reference")
        else {
            return;
        };
        match recompile(w.context, &w.reference_policy) {
            Ok((ms_text, script_hex)) => {
                if ms_text != w.reference_miniscript {
                    self.warn(format!(
                        "{}: recompiled miniscript text differs from stored (dependency drift)",
                        w.id
                    ));
                }
                if script_hex != w.reference_script_hex {
                    let recompiled = ScriptBuf::from_hex(&script_hex).expect("just encoded");
                    if check_equivalence(w.context, &reference, &recompiled) == Verdict::Equivalent
                    {
                        self.warn(format!(
                            "{}: recompiled reference bytes differ but stay equivalent (dependency drift)",
                            w.id
                        ));
                    } else {
                        self.fail(format!(
                            "{}: recompiled policy is NOT equivalent to the stored answer key",
                            w.id
                        ));
                    }
                }
            }
            Err(e) => self.fail(format!("{}: policy no longer compiles: {e}", w.id)),
        }
    }

    fn check_optimize(&mut self, o: &OptimizeFixture) {
        let pre = self.check_preimages(&o.id, &o.hash_preimages);
        let Some(reference) =
            self.check_script(&o.id, o.context, &o.optimal_script_hex, &pre, "optimal")
        else {
            return;
        };
        self.check_weights(
            &o.id,
            o.context,
            &reference,
            o.optimal_weight,
            o.optimal_size,
            "optimal",
        );
        let Some(baseline) =
            self.check_script(&o.id, o.context, &o.baseline_script_hex, &pre, "baseline")
        else {
            return;
        };
        self.check_weights(
            &o.id,
            o.context,
            &baseline,
            o.baseline_weight,
            o.baseline_size,
            "baseline",
        );
        if check_equivalence(o.context, &reference, &baseline) != Verdict::Equivalent {
            self.fail(format!(
                "{}: baseline no longer equivalent to optimal",
                o.id
            ));
        }
        match (
            weights_for(o.context, &reference),
            weights_for(o.context, &baseline),
        ) {
            (Ok(r), Ok(b)) => {
                if b.weight <= r.weight {
                    self.fail(format!(
                        "{}: baseline no longer strictly heavier (optimize would be vacuous)",
                        o.id
                    ));
                }
            }
            _ => {} // already reported by check_weights
        }
        // Recompile drift on the answer key, same policy as write tasks.
        match recompile(o.context, &o.reference_policy) {
            Ok((_, script_hex)) => {
                if script_hex != o.optimal_script_hex {
                    let recompiled = ScriptBuf::from_hex(&script_hex).expect("just encoded");
                    if check_equivalence(o.context, &reference, &recompiled) == Verdict::Equivalent
                    {
                        self.warn(format!(
                            "{}: recompiled optimal bytes differ but stay equivalent (dependency drift)",
                            o.id
                        ));
                    } else {
                        self.fail(format!(
                            "{}: recompiled policy is NOT equivalent to the stored optimal",
                            o.id
                        ));
                    }
                }
            }
            Err(e) => self.fail(format!("{}: policy no longer compiles: {e}", o.id)),
        }
    }
}

use std::str::FromStr as _;

/// Audit a committed dataset directory. Returns the report; the caller
/// decides exit behavior (main exits nonzero on failures).
pub fn audit_dataset(dir: &Path) -> Result<AuditReport> {
    let fixtures = load_dataset(dir)?;
    let manifest_path = dir.join("manifest.json");
    let manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parse {}", manifest_path.display()))?;

    let mut report = AuditReport::default();
    if manifest.schema_version != crate::SCHEMA_VERSION {
        report.fail(format!(
            "manifest schema_version {} != this build's {} (regenerate the dataset)",
            manifest.schema_version,
            crate::SCHEMA_VERSION
        ));
    }
    if manifest.pins.get("miniscript").map(String::as_str) != Some(MINISCRIPT_VERSION) {
        report.fail(format!(
            "manifest pins miniscript {:?} but this build declares {MINISCRIPT_VERSION} (dependency drift)",
            manifest.pins.get("miniscript"),
        ));
    }
    if manifest.pins.get("bitcoin").map(String::as_str) != Some(BITCOIN_VERSION) {
        report.fail(format!(
            "manifest pins bitcoin {:?} but this build declares {BITCOIN_VERSION} (dependency drift)",
            manifest.pins.get("bitcoin"),
        ));
    }

    // Fixture counts per kind: catches line-level corruption (appended,
    // dropped, or duplicated fixtures.jsonl lines) that per-fixture
    // re-derivation alone cannot see.
    let mut seen_counts: BTreeMap<String, usize> = BTreeMap::new();
    for f in &fixtures {
        let kind = match f {
            Fixture::Write(_) => "t1",
            Fixture::Optimize(_) => "t2",
            Fixture::Identify(_) => "t3",
        };
        *seen_counts.entry(kind.to_string()).or_insert(0) += 1;
    }
    for (kind, want) in &manifest.counts {
        let got = seen_counts.get(kind).copied().unwrap_or(0);
        if &got != want {
            report.fail(format!(
                "manifest counts {kind}={want} but fixtures.jsonl holds {got}"
            ));
        }
    }
    for kind in seen_counts.keys() {
        if !manifest.counts.contains_key(kind) {
            report.fail(format!(
                "fixtures.jsonl has {kind} fixtures absent from manifest counts"
            ));
        }
    }
    for f in &fixtures {
        report.fixtures_checked += 1;
        match f {
            Fixture::Write(w) => report.check_write(w),
            Fixture::Optimize(o) => report.check_optimize(o),
            Fixture::Identify(_) => {
                // Identify items carry no compiled answer key; params are
                // static. Nothing to re-derive.
            }
        }
    }
    Ok(report)
}

/// True when the report is clean for CI gating.
pub fn report_ok(report: &AuditReport) -> bool {
    report.failures.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bench_gen::fixtures::GenParams;

    fn tmpdir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("btc-bench-audit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn fresh_dataset_audits_clean() {
        let dir = tmpdir("clean");
        let n = crate::gen_dataset(
            &dir,
            &GenParams {
                seed: 3,
                write: 4,
                optimize: 4,
                identify: 1,
            },
            "audit-test",
        )
        .expect("gen");
        assert!(n > 0);
        let report = audit_dataset(&dir).expect("audit");
        assert!(
            report_ok(&report),
            "failures: {:?}\nwarnings: {:?}",
            report.failures,
            report.warnings
        );
        // Some fixtures must have gone through the heavy checks.
        assert!(report.fixtures_checked >= 8);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tampered_dataset_fails_audit() {
        let dir = tmpdir("tampered");
        crate::gen_dataset(
            &dir,
            &GenParams {
                seed: 3,
                write: 2,
                optimize: 2,
                identify: 1,
            },
            "audit-test",
        )
        .expect("gen");
        // Tamper: bump one stored optimal weight.
        let path = dir.join("fixtures.jsonl");
        let text = std::fs::read_to_string(&path).unwrap();
        let mut out = String::new();
        let mut tampered = false;
        for line in text.lines() {
            if !tampered && line.contains("\"task\":\"optimize\"") {
                let mut v: serde_json::Value = serde_json::from_str(line).unwrap();
                if let Some(w) = v.get_mut("optimal_weight").and_then(|w| w.as_u64()) {
                    v["optimal_weight"] = serde_json::json!(w + 1);
                    tampered = true;
                }
                out.push_str(&v.to_string());
            } else {
                out.push_str(line);
            }
            out.push('\n');
        }
        std::fs::write(&path, out).unwrap();
        assert!(tampered, "test must tamper something");
        let report = audit_dataset(&dir).expect("audit");
        assert!(!report_ok(&report), "tampered dataset must fail");
        assert!(
            report.failures.iter().any(|f| f.contains("weight drift")),
            "expected weight drift failure, got {:?}",
            report.failures
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dropped_fixture_line_fails_audit() {
        let dir = tmpdir("dropped");
        crate::gen_dataset(
            &dir,
            &GenParams {
                seed: 3,
                write: 2,
                optimize: 2,
                identify: 1,
            },
            "audit-test",
        )
        .expect("gen");
        let path = dir.join("fixtures.jsonl");
        let text = std::fs::read_to_string(&path).unwrap();
        // Drop one write fixture line.
        let mut lines: Vec<&str> = text.lines().collect();
        let idx = lines
            .iter()
            .position(|l| l.contains("\"task\":\"write\""))
            .expect("a write fixture");
        lines.remove(idx);
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        let report = audit_dataset(&dir).expect("audit");
        assert!(!report_ok(&report), "dropped line must fail");
        assert!(
            report
                .failures
                .iter()
                .any(|f| f.contains("manifest counts t1")),
            "expected count mismatch, got {:?}",
            report.failures
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn schema_version_mismatch_fails_audit() {
        let dir = tmpdir("schema");
        crate::gen_dataset(
            &dir,
            &GenParams {
                seed: 3,
                write: 1,
                optimize: 1,
                identify: 1,
            },
            "audit-test",
        )
        .expect("gen");
        let path = dir.join("manifest.json");
        let mut m: crate::Manifest =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        m.schema_version += 1;
        std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap()).unwrap();
        let report = audit_dataset(&dir).expect("audit");
        assert!(
            !report_ok(&report),
            "schema mismatch must fail: {:?}",
            report.failures
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The manifest pin check is only meaningful if the declared pin
    /// constants match what the workspace actually resolved: tie them
    /// to Cargo.lock so a dependency bump without updating the pins
    /// fails here rather than silently passing audits.
    #[test]
    fn pin_constants_match_lockfile() {
        let lock =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock"))
                .expect("workspace Cargo.lock");
        let mut last_name = String::new();
        let mut found: Vec<(String, String)> = Vec::new();
        for line in lock.lines() {
            let t = line.trim();
            if let Some(name) = t.strip_prefix("name = ") {
                last_name = name.trim_matches('"').to_string();
            } else if let Some(v) = t.strip_prefix("version = ") {
                if matches!(last_name.as_str(), "miniscript" | "bitcoin")
                    && !found.iter().any(|(n, _)| *n == last_name)
                {
                    found.push((last_name.clone(), v.trim_matches('"').to_string()));
                }
            }
        }
        let get = |n: &str| {
            found
                .iter()
                .find(|(name, _)| name == n)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        assert_eq!(
            get("miniscript"),
            crate::MINISCRIPT_VERSION,
            "MINISCRIPT_VERSION constant is stale vs Cargo.lock"
        );
        assert_eq!(
            get("bitcoin"),
            crate::BITCOIN_VERSION,
            "BITCOIN_VERSION constant is stale vs Cargo.lock"
        );
    }
}
