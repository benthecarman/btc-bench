//! Audit only explicitly marked reasoning expressions; never repair syntax.
use anyhow::{Context, Result};
use bench_wallet::{grade, OutputKind, WalletFixture};
use miniscript::{
    descriptor::DescriptorPublicKey,
    policy::{Concrete, Liftable},
    Descriptor,
};
use serde_json::{json, Value};
use std::io::{self, BufRead};

fn main() -> Result<()> {
    for line in io::stdin().lock().lines() {
        let row: Value = serde_json::from_str(&line?)?;
        let mut f: WalletFixture = serde_json::from_value(row["fixture"].clone())?;
        f.output_kind = OutputKind::Concrete;
        let reference: Concrete<DescriptorPublicKey> = f.derivations[0].policy.parse()?;
        let policy = match row["policy"].as_str() {
            None => json!({"status":"missing or ambiguous"}),
            Some(text) => match text
                .parse::<Concrete<DescriptorPublicKey>>()
                .and_then(|p| p.lift())
            {
                Err(e) => json!({"status":"invalid","reason":e.to_string()}),
                Ok(candidate) => {
                    let verdict = bench_core::check_semantic(&reference.lift()?, &candidate, None);
                    json!({"status":if verdict.is_equivalent() {"equivalent"} else {"not equivalent"},"reason":verdict.to_string()})
                }
            },
        };
        let descriptor: Descriptor<DescriptorPublicKey> = f.derivations[0].descriptor.parse()?;
        let bodies = row["bodies"].as_array();
        let body = if matches!(&descriptor, Descriptor::Wpkh(_))
            || matches!(&descriptor, Descriptor::Tr(tr) if tr.tap_tree().is_none())
        {
            json!({"status":"not applicable"})
        } else if let Some(bodies) = bodies.filter(|b| !b.is_empty()) {
            let mut parts = bodies
                .iter()
                .map(|b| b.as_str().context("body is not a string").map(String::from))
                .collect::<Result<Vec<_>>>()?;
            let text = match &descriptor {
                Descriptor::Tr(tr) => {
                    let mut tree = parts.pop().unwrap();
                    while let Some(part) = parts.pop() {
                        tree = format!("{{{part},{tree}}}");
                    }
                    format!("tr({},{tree})", tr.internal_key())
                }
                Descriptor::Wsh(_) if parts.len() == 1 => format!("wsh({})", parts[0]),
                _ => "invalid trace body count".into(),
            };
            let score = grade(&f, &text);
            json!({"status":if score.score==1.0 {"equivalent with reference internal key"} else {"invalid or not equivalent"},"reason":score.reason})
        } else {
            json!({"status":"missing or ambiguous"})
        };
        println!("{}", json!({"id":row["id"],"policy":policy,"bodies":body}));
    }
    Ok(())
}
