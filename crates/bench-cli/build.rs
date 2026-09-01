// Embed the git revision so graded artifacts and reports are
// self-identifying: a results file stamped with an old rev is visibly
// stale instead of silently misleading.
use std::process::Command;

fn main() {
    let rev = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=BTC_BENCH_GIT_REV={rev}");
    // Rebuild when HEAD moves so the stamp stays honest.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs");
}
