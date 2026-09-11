//! Wallet-policy pilot. BIP-388 handles templates; rust-miniscript and
//! bench-core check the independently stated spending contract.
use anyhow::{bail, ensure, Context, Result};
use bip388::{
    ClearText, DescriptorTemplate, KeyExpressionType, KeyInformation, ToDescriptor, WalletPolicy,
};
use bitcoin::{bip32::Xpub, secp256k1::Secp256k1, PublicKey};
use miniscript::{
    descriptor::DescriptorPublicKey,
    policy::{Concrete, Liftable},
    Descriptor, ForEachKey,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

fn default_true() -> bool {
    true
}

pub const BIP388_REVISION: &str = "b80f6288afc7b7c2de5e3b307db35b250c21e941";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputKind {
    Template,
    Concrete,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletKey {
    pub label: String,
    pub key_info: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Derivation {
    pub is_change: bool,
    pub address_index: u32,
    pub descriptor: String,
    pub policy: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletFixture {
    pub id: String,
    pub group: String,
    pub family: String,
    pub split: String,
    pub output_kind: OutputKind,
    pub request: String,
    pub spec_en: String,
    pub keys: Vec<WalletKey>,
    pub reference_template: String,
    /// Separately authored policy over @0, @1, ...; not lifted from the template.
    pub policy_template: String,
    pub cleartext: Vec<String>,
    #[serde(default = "default_true")]
    pub cleartext_supported: bool,
    #[serde(default = "default_true")]
    pub cleartext_roundtrip: bool,
    pub confusion_score: u64,
    /// The first entry supplies the concrete-output request. Other entries
    /// check template expansion across receive/change and address indices.
    pub derivations: Vec<Derivation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletScore {
    pub task_id: String,
    pub score: f64,
    pub failure: Option<String>,
    pub reason: Option<String>,
}

fn keys(f: &WalletFixture) -> Result<Vec<KeyInformation>> {
    f.keys
        .iter()
        .map(|k| {
            KeyInformation::try_from(k.key_info.as_str()).map_err(|e| anyhow::anyhow!("{e:?}"))
        })
        .collect()
}

pub fn parse_template(text: &str) -> Result<DescriptorTemplate> {
    text.parse().map_err(|e| anyhow::anyhow!("{e:?}"))
}

/// This pilot supports ordinary individual keys with the standard account
/// branches. Enforce this structurally, rather than sampling arbitrary paths.
pub fn standard_template(text: &str) -> Result<DescriptorTemplate> {
    let template = parse_template(text)?;
    for (key, _) in template.placeholders() {
        ensure!(
            matches!(key.key_type, KeyExpressionType::PlainKey(_)),
            "MuSig is outside this pilot"
        );
        ensure!(
            (key.num1, key.num2) == (0, 1),
            "use receive/change branches <0;1>/*"
        );
    }
    Ok(template)
}

fn parse_descriptor(text: &str) -> Result<Descriptor<DescriptorPublicKey>> {
    Ok(text.parse::<Descriptor<DescriptorPublicKey>>()?)
}

fn outer<P: miniscript::MiniscriptKey>(d: &Descriptor<P>) -> Result<&'static str> {
    match d {
        Descriptor::Wpkh(_) => Ok("wpkh"),
        Descriptor::Wsh(_) => Ok("wsh"),
        Descriptor::Tr(_) => Ok("tr"),
        _ => bail!("this pilot requires wpkh, wsh, or tr"),
    }
}

/// Normalize Taproot parity only after real key parsing and type checking.
fn normalize(d: &Descriptor<PublicKey>) -> Result<Descriptor<String>> {
    let mut text = format!("{d:#}");
    if matches!(d, Descriptor::Tr(_)) {
        d.for_each_key(|key| {
            text = text.replace(
                &key.to_string(),
                &key.inner.x_only_public_key().0.to_string(),
            );
            true
        });
    }
    Ok(text.parse()?)
}

pub fn derive_descriptor(
    template: &str,
    key_info: &[KeyInformation],
    change: bool,
    index: u32,
) -> Result<String> {
    let policy =
        WalletPolicy::new(template, key_info.to_vec()).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let text = policy
        .descriptor_template()
        .to_descriptor(policy.key_information(), change, index)
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let descriptor = parse_descriptor(&text)?
        .at_derivation_index(0)?
        .derived_descriptor(&Secp256k1::verification_only());
    descriptor.sanity_check()?;
    Ok(format!("{:#}", normalize(&descriptor)?))
}

pub fn derive_keys(
    key_info: &[KeyInformation],
    taproot: bool,
    change: bool,
    index: u32,
) -> Result<Vec<String>> {
    let secp = Secp256k1::verification_only();
    key_info
        .iter()
        .map(|key| {
            let child: Xpub = key.pubkey.derive_pub(
                &secp,
                &[
                    bitcoin::bip32::ChildNumber::from_normal_idx(u32::from(change))?,
                    bitcoin::bip32::ChildNumber::from_normal_idx(index)?,
                ],
            )?;
            Ok(if taproot {
                child.public_key.x_only_public_key().0.to_string()
            } else {
                child.public_key.to_string()
            })
        })
        .collect()
}

pub fn instantiate_policy(policy: &str, public_keys: &[String]) -> Result<String> {
    let mut text = policy.to_string();
    // Replace longer indices first (@1 must not alter @10).
    for (i, key) in public_keys.iter().enumerate().rev() {
        text = text.replace(&format!("@{i}"), key);
    }
    ensure!(!text.contains('@'), "policy references an unknown key");
    let _: Concrete<String> = text.parse()?;
    Ok(text)
}

fn compare_contract(candidate: &str, reference: &Derivation) -> Result<()> {
    let parsed = parse_descriptor(candidate)?;
    ensure!(
        parsed.for_each_key(|key| matches!(key, DescriptorPublicKey::Single(_))),
        "a concrete answer must contain public keys, not extended keys or wildcards"
    );
    let reference_desc = parse_descriptor(&reference.descriptor)?;
    ensure!(
        outer(&parsed)? == outer(&reference_desc)?,
        "wrong descriptor type for this request"
    );
    parsed.sanity_check()?;
    let derived = parsed
        .at_derivation_index(0)?
        .derived_descriptor(&Secp256k1::verification_only());
    let normalized = normalize(&derived)?;
    let required = reference.policy.parse::<Concrete<String>>()?.lift()?;
    let verdict = bench_core::check_semantic(&required, &normalized.lift()?, None);
    ensure!(verdict.is_equivalent(), "{verdict}");
    Ok(())
}

pub fn reference_answer(f: &WalletFixture) -> &str {
    match f.output_kind {
        OutputKind::Template => &f.reference_template,
        OutputKind::Concrete => &f.derivations[0].descriptor,
    }
}

pub fn grade(f: &WalletFixture, answer: &str) -> WalletScore {
    let checked = (|| -> Result<()> {
        ensure!(!f.derivations.is_empty(), "invalid fixture: no derivations");
        ensure!(answer.len() <= 65536, "descriptor too large to verify");
        match f.output_kind {
            OutputKind::Template => {
                standard_template(answer)?;
                let info = keys(f)?;
                for reference in &f.derivations {
                    let candidate = derive_descriptor(
                        answer,
                        &info,
                        reference.is_change,
                        reference.address_index,
                    )?;
                    compare_contract(&candidate, reference)?;
                }
            }
            OutputKind::Concrete => compare_contract(answer, &f.derivations[0])?,
        }
        Ok(())
    })();
    match checked {
        Ok(()) => WalletScore {
            task_id: f.id.clone(),
            score: 1.0,
            failure: None,
            reason: None,
        },
        Err(e) => WalletScore {
            task_id: f.id.clone(),
            score: 0.0,
            failure: Some("invalid or wrong contract".into()),
            reason: Some(e.to_string()),
        },
    }
}

/// Audit reference consistency and canonical-English round trips. This checks
/// the English generated by bip388, not a proof about authored human prose.
pub fn audit(f: &WalletFixture) -> Result<()> {
    ensure!(
        f.split == "evaluation",
        "pilot fixtures must remain evaluation-only"
    );
    audit_contract(f)
}

/// Upstream rendering support and successful reverse decoding are distinct.
pub fn cleartext_roundtrip(template: &DescriptorTemplate) -> bool {
    let (text, supported) = template.to_cleartext();
    if !supported {
        return false;
    }
    let refs: Vec<_> = text.iter().map(String::as_str).collect();
    DescriptorTemplate::from_cleartext(&refs)
        .map(|mut candidates| candidates.any(|candidate| candidate == *template))
        .unwrap_or(false)
}

/// Training uses the same contract checks but a separate, explicit split gate.
pub fn audit_training(f: &WalletFixture) -> Result<()> {
    ensure!(
        f.split == "training",
        "cannot export evaluation fixtures for training"
    );
    audit_contract(f)
}

fn audit_contract(f: &WalletFixture) -> Result<()> {
    ensure!(f.derivations.len() == 4, "expected four derivation checks");
    let positions: BTreeSet<_> = f
        .derivations
        .iter()
        .map(|d| (d.is_change, d.address_index))
        .collect();
    ensure!(
        positions == BTreeSet::from([(false, 0), (false, 7), (true, 0), (true, 7)]),
        "wrong derivation checks"
    );
    ensure!(
        !f.derivations[0].is_change && f.derivations[0].address_index == 0,
        "concrete request must use receive index zero"
    );
    let template = standard_template(&f.reference_template)?;
    let (text, supported) = template.to_cleartext();
    ensure!(
        supported == f.cleartext_supported && text == f.cleartext,
        "cleartext is unsupported or has drifted"
    );
    ensure!(
        template.confusion_score() == f.confusion_score,
        "confusion score has drifted"
    );
    let roundtrip = cleartext_roundtrip(&template);
    ensure!(
        roundtrip == f.cleartext_roundtrip,
        "cleartext round trip has drifted"
    );
    let info = keys(f)?;
    let taproot = f.reference_template.starts_with("tr(");
    for d in &f.derivations {
        let actual = derive_descriptor(&f.reference_template, &info, d.is_change, d.address_index)?;
        ensure!(actual == d.descriptor, "reference descriptor has drifted");
        let public_keys = derive_keys(&info, taproot, d.is_change, d.address_index)?;
        ensure!(
            instantiate_policy(&f.policy_template, &public_keys)? == d.policy,
            "reference policy has drifted"
        );
        compare_contract(&actual, d)
            .context("reference does not match separately authored policy")?;
    }
    ensure!(
        grade(f, reference_answer(f)).score == 1.0,
        "reference answer did not pass"
    );
    Ok(())
}

/// A checked teaching trace. Bodies come from the typed, sane descriptor whose
/// complete spending policy was checked above, including the Taproot key path.
pub fn training_trace(f: &WalletFixture) -> Result<serde_json::Value> {
    audit_training(f)?;
    let d: Descriptor<String> = f.derivations[0].descriptor.parse()?;
    let (context, mut bodies) = match d {
        Descriptor::Tr(tr) => (
            "Taproot leaf Miniscript",
            tr.leaves()
                .map(|leaf| leaf.miniscript().to_string())
                .collect::<Vec<_>>(),
        ),
        Descriptor::Wsh(wsh) => (
            "P2WSH descriptor body",
            vec![match wsh.as_inner() {
                miniscript::descriptor::WshInner::Ms(ms) => ms.to_string(),
                miniscript::descriptor::WshInner::SortedMulti(multi) => multi.to_string(),
            }],
        ),
        Descriptor::Wpkh(_) => ("P2WPKH; no custom Miniscript", vec![]),
        _ => bail!("unsupported teaching context"),
    };
    let policy = if f.output_kind == OutputKind::Template {
        let public = derive_keys(&keys(f)?, f.reference_template.starts_with("tr("), false, 0)?;
        for body in &mut bodies {
            for (i, key) in public.iter().enumerate() {
                *body = body.replace(key, &format!("@{i}/**"));
            }
        }
        f.policy_template.clone()
    } else {
        f.derivations[0].policy.clone()
    };
    Ok(
        serde_json::json!({"task_id":f.id,"policy":policy,"context":context,"bodies":bodies,"answer":reference_answer(f)}),
    )
}

/// Extract only by the requested syntax. Never select an answer by its grade.
pub fn extract_chat(kind: OutputKind, text: &str) -> Result<String> {
    let final_text = text.rsplit_once("</think>").map_or(text, |(_, tail)| tail);
    ensure!(!final_text.contains("<think>"), "unfinished thinking block");
    let parts: Vec<_> = final_text.split("```").collect();
    ensure!(parts.len() % 2 == 1, "unfinished code fence");
    let unfenced = parts.len() == 1;
    let mut blocks: Vec<String> = if unfenced {
        let lines: Vec<_> = final_text.lines().collect();
        let mut found = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            if let Some((label, rest)) = lines[i].split_once(':') {
                let lower = label.to_ascii_lowercase();
                if lower.contains("descriptor") || lower.contains("template") {
                    let mut body = rest.trim().to_string();
                    i += 1;
                    while body.is_empty() && i < lines.len() && lines[i].trim().is_empty() {
                        i += 1;
                    }
                    while i < lines.len() && !lines[i].trim().is_empty() {
                        if !body.is_empty() {
                            body.push('\n');
                        }
                        body.push_str(lines[i].trim());
                        i += 1;
                    }
                    if !body.is_empty() {
                        found.push(body);
                    }
                    continue;
                }
            }
            i += 1;
        }
        if found.is_empty() {
            vec![final_text.trim().trim_matches('`').to_string()]
        } else {
            found
        }
    } else {
        parts
            .iter()
            .skip(1)
            .step_by(2)
            .filter_map(|p| p.split_once('\n').map(|(_, code)| code.trim().to_string()))
            .collect()
    };
    blocks.sort();
    blocks.dedup();
    ensure!(!blocks.is_empty(), "no answer");
    if blocks.len() == 1 {
        return Ok(blocks[0].to_string());
    }
    let valid: Vec<_> = blocks
        .iter()
        .filter(|block| match kind {
            OutputKind::Template => parse_template(block).is_ok(),
            OutputKind::Concrete => parse_descriptor(block).is_ok(),
        })
        .collect();
    ensure!(
        !unfenced || valid.len() == blocks.len(),
        "malformed labelled alternative"
    );
    ensure!(valid.len() == 1, "ambiguous final answers");
    Ok(valid[0].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::{bip32::Xpriv, NetworkKind};

    fn fixture(kind: OutputKind) -> WalletFixture {
        let keys: Vec<_> = [1u8, 2]
            .iter()
            .enumerate()
            .map(|(i, byte)| {
                let master = Xpriv::new_master(NetworkKind::Test, &[*byte; 32]).unwrap();
                WalletKey {
                    label: format!("person{i}"),
                    key_info: Xpub::from_priv(&Secp256k1::new(), &master).to_string(),
                }
            })
            .collect();
        let mut f = WalletFixture {
            id: "test".into(),
            group: "recovery".into(),
            family: "recovery".into(),
            split: "evaluation".into(),
            output_kind: kind,
            request: String::new(),
            spec_en: String::new(),
            keys,
            reference_template: "tr(@0/**,and_v(v:pk(@1/**),older(144)))".into(),
            policy_template: "or(pk(@0),and(pk(@1),older(144)))".into(),
            cleartext: vec![],
            cleartext_supported: true,
            cleartext_roundtrip: true,
            confusion_score: 0,
            derivations: vec![],
        };
        let t = standard_template(&f.reference_template).unwrap();
        f.cleartext = t.to_cleartext().0;
        f.confusion_score = t.confusion_score();
        let info = super::keys(&f).unwrap();
        for (change, index) in [(false, 0), (false, 7), (true, 0), (true, 7)] {
            let public = derive_keys(&info, true, change, index).unwrap();
            f.derivations.push(Derivation {
                is_change: change,
                address_index: index,
                descriptor: derive_descriptor(&f.reference_template, &info, change, index).unwrap(),
                policy: instantiate_policy(&f.policy_template, &public).unwrap(),
            });
        }
        f
    }

    #[test]
    fn equivalent_encodings_and_explicit_standard_paths_pass() {
        let f = fixture(OutputKind::Template);
        audit(&f).unwrap();
        assert_eq!(
            grade(&f, "tr(@0/**,and_v(v:older(144),pk(@1/**)))").score,
            1.0
        );
        assert_eq!(
            grade(&f, &f.reference_template.replace("/**", "/<0;1>/*")).score,
            1.0
        );
        let concrete = fixture(OutputKind::Concrete);
        audit(&concrete).unwrap();
    }

    #[test]
    fn roles_clock_boundaries_and_derivation_changes_fail() {
        let f = fixture(OutputKind::Template);
        for answer in [
            "tr(@1/**,and_v(v:pk(@0/**),older(144)))",
            "tr(@0/**,and_v(v:pk(@1/**),older(143)))",
            "tr(@0/**,and_v(v:pk(@1/**),older(145)))",
            "tr(@0/**,and_v(v:pk(@1/**),after(144)))",
            "tr(@0/**,pk(@1/**))",
            "tr(@0/**)",
            "tr(@0/<0;2>/*,and_v(v:pk(@1/**),older(144)))",
            "tr(@0/**,{and_v(v:pk(@1/**),older(144)),pk(@2/**)})",
        ] {
            assert_eq!(grade(&f, answer).score, 0.0, "{answer}");
        }
    }

    #[test]
    fn extra_spending_path_and_wrong_output_kind_fail() {
        let f = fixture(OutputKind::Concrete);
        let pubkeys = derive_keys(&super::keys(&f).unwrap(), true, false, 0).unwrap();
        let outsider = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let answer = format!(
            "tr({},{{and_v(v:pk({}),older(144)),pk({outsider})}})",
            pubkeys[0], pubkeys[1]
        );
        let result = grade(&f, &answer);
        assert_eq!(result.score, 0.0);
        assert!(result
            .reason
            .unwrap()
            .contains("not semantically equivalent"));
        assert_eq!(grade(&f, &f.reference_template).score, 0.0);
        let template = fixture(OutputKind::Template);
        assert_eq!(
            grade(&template, &template.derivations[0].descriptor).score,
            0.0
        );
        let wrong_context = format!(
            "wsh(or_i(pk(02{}),and_v(v:pk(02{}),older(144))))",
            pubkeys[0], pubkeys[1]
        );
        assert_eq!(grade(&f, &wrong_context).score, 0.0);
    }

    #[test]
    fn malformed_and_ill_typed_answers_fail_without_panicking() {
        for kind in [OutputKind::Template, OutputKind::Concrete] {
            let f = fixture(kind);
            for answer in [
                "",
                "OP_0",
                "tr()",
                "tr(@0/**,c:older(144))",
                "tr(@0/**,multi(1,@1/**))",
                "tr(@0/**,and_v(v:pk(@1/**),older(144)))junk",
            ] {
                assert_eq!(grade(&f, answer).score, 0.0, "{answer}");
            }
        }
    }

    #[test]
    fn reference_audit_detects_policy_or_cleartext_drift() {
        let mut f = fixture(OutputKind::Template);
        f.policy_template = "or(pk(@0),pk(@1))".into();
        assert!(audit(&f).is_err());
        let mut f = fixture(OutputKind::Template);
        f.cleartext[0].push_str(" invented route");
        assert!(audit(&f).is_err());
    }

    #[test]
    fn extraction_does_not_choose_between_distinct_answers() {
        let f = fixture(OutputKind::Template);
        let a = &f.reference_template;
        assert_eq!(
            extract_chat(
                OutputKind::Template,
                &format!("Here it is:\n```text\n{a}\n```")
            )
            .unwrap(),
            *a
        );
        assert!(extract_chat(
            OutputKind::Template,
            &format!("```text\n{a}\n```\n```text\ntr(@0/**)\n```")
        )
        .is_err());
        assert!(extract_chat(OutputKind::Template, "<think>tr(@0/**)").is_err());
        assert!(extract_chat(OutputKind::Template, "```text\ntr(@0/**)").is_err());
        assert_eq!(
            extract_chat(OutputKind::Template, "```text\nOP_0\n```").unwrap(),
            "OP_0"
        );
        assert_eq!(
            extract_chat(
                OutputKind::Template,
                &format!("Descriptor template:\n\n{a}\n\nExplanation follows.")
            )
            .unwrap(),
            *a
        );
        assert!(extract_chat(
            OutputKind::Template,
            &format!("Template:\n{a}\n\nAlternative descriptor:\nOP_0")
        )
        .is_err());
    }
}
