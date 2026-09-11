//! Check explicit policy and Segwit v0 Miniscript trace lines independently.
use anyhow::{ensure, Context, Result};
use bitcoin::PublicKey;
use miniscript::{policy::Concrete, policy::Liftable, Miniscript, Segwitv0};
use serde_json::{json, Value};
use std::io::{self, BufRead};

fn check(reference: &Concrete<PublicKey>, text: Option<&str>, is_miniscript: bool) -> Value {
    let Some(text) = text else {
        return json!({"status": "missing"});
    };
    let lifted = if is_miniscript {
        Miniscript::<PublicKey, Segwitv0>::from_str_insane(text)
            .and_then(|p| p.lift())
            .map_err(|e| e.to_string())
    } else {
        text.parse::<Concrete<PublicKey>>()
            .and_then(|p| p.lift())
            .map_err(|e| e.to_string())
    };
    match lifted {
        Err(error) => json!({"status": "invalid", "reason": error}),
        Ok(candidate) => {
            let verdict = bench_core::check_semantic(&reference.lift().unwrap(), &candidate, None);
            json!({"status": if verdict.is_equivalent() { "equivalent" } else { "not equivalent" },
                   "reason": verdict.to_string()})
        }
    }
}

fn main() -> Result<()> {
    for line in io::stdin().lock().lines() {
        let row: Value = serde_json::from_str(&line?)?;
        let reference: Concrete<PublicKey> = row["reference_policy"]
            .as_str()
            .context("missing reference_policy")?
            .parse()?;
        ensure!(reference.lift().is_ok(), "invalid reference policy");
        println!(
            "{}",
            json!({"id": row["id"],
            "policy": check(&reference, row["policy"].as_str(), false),
            "miniscript": check(&reference, row["miniscript"].as_str(), true)})
        );
    }
    Ok(())
}
