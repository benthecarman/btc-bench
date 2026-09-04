//! Fixed, authored requests for transfer evaluation. The seed changes neither
//! wording nor policy: keys and secrets are deterministic per scenario group.
//! These are synthetic requests, not transcripts collected from people.

use crate::{fixtures, keys, rng::SeededRng};
use bench_core::task::{ContextKind, Fixture, KeyVar, Tier, TreeFixture, WriteFixture};
use bitcoin::hashes::{hash160, sha256, Hash};
use bitcoin::{PublicKey, XOnlyPublicKey};
use miniscript::{policy::Concrete, Descriptor, Legacy, Segwitv0, Tap};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub const CATALOG: &str = include_str!("../../../evals/human-v1.json");
pub const SUITE: &str = "human-v1";

#[derive(Deserialize)]
struct Case {
    id: String,
    group: String,
    kind: String,
    context: ContextKind,
    #[serde(default)]
    choose_context: bool,
    keys: Vec<String>,
    tier: Tier,
    policy: String,
    prompt: String,
}

pub struct HumanDataset {
    pub fixtures: Vec<Fixture>,
    pub groups: BTreeMap<String, String>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn build(case: &Case) -> Result<Fixture, String> {
    if case.choose_context && (case.kind != "write" || case.context != ContextKind::SegwitV0) {
        return Err("context-free requests use a segwit reference with full public keys".into());
    }
    let salt = case.group.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    let mut rng = SeededRng::new(salt);
    if case.keys.is_empty() || case.keys.len() > 12 {
        return Err("expected 1..12 named keys".into());
    }
    let material = keys::generate(&mut rng, case.keys.len());
    let key_values = if case.context == ContextKind::Tap {
        &material.xonly
    } else {
        &material.compressed
    };
    let key_vars: Vec<_> = case
        .keys
        .iter()
        .zip(key_values)
        .map(|(label, pk)| KeyVar {
            label: label.clone(),
            pubkey: pk.clone(),
        })
        .collect();
    let mut policy = case.policy.clone();
    for (i, pk) in key_values.iter().enumerate().rev() {
        policy = policy.replace(&format!("${i}"), pk);
    }
    let mut prompt = case.prompt.clone();
    let mut preimages = BTreeMap::new();
    for (slot, which) in [("$sha256", 0), ("$hash160", 1)] {
        if policy.contains(slot) {
            if !prompt.contains(slot) {
                return Err(format!("prompt omits {slot}"));
            }
            let mut preimage = [0; 32];
            rng.bytes(&mut preimage);
            let digest = if which == 0 {
                sha256::Hash::hash(&preimage).to_string()
            } else {
                hash160::Hash::hash(&preimage).to_string()
            };
            policy = policy.replace(slot, &digest);
            prompt = prompt.replace(slot, &digest);
            preimages.insert(digest, hex(&preimage));
        }
    }
    if policy.contains('$') || prompt.contains('$') {
        return Err("unresolved placeholder".into());
    }
    let spec_en = prompt.clone();
    prompt.push_str("\n\n");
    for k in &key_vars {
        prompt.push_str(&format!("{}: {}\n", k.label, k.pubkey));
    }
    let typed = bench_core::HashPreimages::from_hex_map(&preimages)?;
    // The names and prose are authored independently of the policy renderer.
    // The internal policy exists only to verify outputs, never in the request.
    match case.kind.as_str() {
        "write" => {
            macro_rules! compile {
                ($key:ty, $ctx:ty) => {{
                    let policy: Concrete<$key> = policy
                        .parse()
                        .map_err(|e: miniscript::Error| e.to_string())?;
                    let ms = policy.compile::<$ctx>().map_err(|e| e.to_string())?;
                    (ms.to_string(), ms.encode())
                }};
            }
            let (ms, script) = match case.context {
                ContextKind::Legacy => compile!(PublicKey, Legacy),
                ContextKind::SegwitV0 => compile!(PublicKey, Segwitv0),
                ContextKind::Tap => compile!(XOnlyPublicKey, Tap),
            };
            if !bench_core::check_equivalence(case.context, &script, &script).is_equivalent() {
                return Err("reference cannot be decoded and lifted by the oracle".into());
            }
            bench_core::execution_check(case.context, &script, &typed)?;
            let fixture = WriteFixture {
                choose_context: case.choose_context,
                id: format!("t1-human-{}", case.id),
                request: Some(prompt),
                tier: case.tier,
                context: case.context,
                spec_en,
                spec_family: 0,
                atoms: key_vars.len() + preimages.len(),
                keys: key_vars,
                reference_policy: policy,
                reference_miniscript: ms,
                reference_script_hex: script.to_hex_string(),
                hash_preimages: preimages,
            };
            if bench_core::grade_write(&fixture, &fixture.reference_script_hex).score != 1.0 {
                return Err("reference did not earn full credit".into());
            }
            Ok(Fixture::Write(fixture))
        }
        "tree" => {
            if case.context != ContextKind::Tap {
                return Err("tree requires tap context".into());
            }
            let (reference, baseline) =
                fixtures::tree_descriptors_for_policy(&policy, fixtures::UNSPENDABLE_KEY)?;
            let weight = |s: &str| -> Result<usize, String> {
                s.parse::<Descriptor<XOnlyPublicKey>>()
                    .map_err(|e| e.to_string())?
                    .max_weight_to_satisfy()
                    .map(|w| w.to_wu() as usize)
                    .map_err(|e| e.to_string())
            };
            let rw = weight(&reference)?;
            let bw = weight(&baseline)?;
            if bw <= rw {
                return Err("tree weight objective has no improvement over baseline".into());
            }
            for s in [&reference] {
                let Descriptor::Tr(tr) = s
                    .parse::<Descriptor<XOnlyPublicKey>>()
                    .map_err(|e| e.to_string())?
                else {
                    unreachable!()
                };
                for leaf in tr.leaves() {
                    bench_core::execution_check(
                        ContextKind::Tap,
                        &leaf.miniscript().encode(),
                        &typed,
                    )?;
                }
            }
            let fixture = TreeFixture {
                id: format!("t4-human-{}", case.id),
                request: Some(prompt),
                tier: case.tier,
                spec_en,
                spec_family: 0,
                atoms: key_vars.len() + preimages.len(),
                keys: key_vars,
                unspendable_key: fixtures::UNSPENDABLE_KEY.into(),
                reference_policy: policy,
                reference_descriptor: reference,
                reference_weight: rw,
                baseline_descriptor: baseline,
                baseline_weight: bw,
                hash_preimages: preimages,
            };
            if bench_core::grade_tree(&fixture, &fixture.reference_descriptor).weight_score != 1.0 {
                return Err("tree reference did not earn full credit".into());
            }
            Ok(Fixture::Tree(fixture))
        }
        other => Err(format!("unsupported kind {other}")),
    }
}

pub fn generate() -> Result<HumanDataset, String> {
    let cases: Vec<Case> = serde_json::from_str(CATALOG).map_err(|e| e.to_string())?;
    let mut out = HumanDataset {
        fixtures: Vec::new(),
        groups: BTreeMap::new(),
    };
    let mut ids = BTreeSet::new();
    let mut errors = Vec::new();
    for case in cases {
        if !ids.insert(case.id.clone()) {
            return Err(format!("duplicate case ID {}", case.id));
        }
        match build(&case) {
            Ok(f) => {
                out.groups.insert(f.id().into(), case.group);
                out.fixtures.push(f);
            }
            Err(e) => errors.push(format!("{}: {e}", case.id)),
        }
    }
    // No resampling: an unsupported brief must be fixed explicitly, never
    // silently removed from the measured distribution.
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bench_core::truth::{eval, TruthContext};
    use miniscript::policy::Liftable;

    #[test]
    fn generic_requests_accept_context_choices_but_reject_wrong_spends() {
        let ds = generate().unwrap();
        let Fixture::Write(w) = ds
            .fixtures
            .iter()
            .find(|f| f.id() == "t1-human-shared-wallet")
            .unwrap()
        else {
            unreachable!()
        };
        let prompt = crate::prompt::write_prompt(w);
        assert!(prompt.contains("Bitcoin script"));
        assert!(!prompt.contains("P2WSH"));
        assert!(w.choose_context);
        let mut policy = w.reference_policy.clone();
        for k in &w.keys {
            let xonly = XOnlyPublicKey::from(k.pubkey.parse::<PublicKey>().unwrap());
            policy = policy.replace(&k.pubkey, &xonly.to_string());
        }
        let tap = policy
            .parse::<Concrete<XOnlyPublicKey>>()
            .unwrap()
            .compile::<Tap>()
            .unwrap()
            .encode()
            .to_hex_string();
        let legacy = w
            .reference_policy
            .parse::<Concrete<PublicKey>>()
            .unwrap()
            .compile::<Legacy>()
            .unwrap()
            .encode()
            .to_hex_string();
        assert_eq!(bench_core::grade_write(w, &legacy).score, 1.0);
        assert_eq!(
            bench_core::grade_write(w, &w.reference_script_hex).score,
            1.0
        );
        assert_eq!(bench_core::grade_write(w, &tap).score, 1.0);
        let mut fixed = w.clone();
        fixed.choose_context = false;
        assert_eq!(bench_core::grade_write(&fixed, &tap).score, 0.0);
        let key = XOnlyPublicKey::from(w.keys[0].pubkey.parse::<PublicKey>().unwrap());
        assert_eq!(
            bench_core::grade_write(w, &format!("{key} OP_CHECKSIG")).score,
            0.0
        );
        assert_eq!(bench_core::grade_write(w, "OP_1").score, 0.0);
        let stranger = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
        let with_extra_path = format!(
            "or_i({},pk({stranger}))",
            policy
                .parse::<Concrete<XOnlyPublicKey>>()
                .unwrap()
                .compile::<Tap>()
                .unwrap()
        );
        let extra = with_extra_path
            .parse::<miniscript::Miniscript<XOnlyPublicKey, Tap>>()
            .unwrap()
            .encode()
            .to_hex_string();
        assert_eq!(bench_core::grade_write(w, &extra).score, 0.0);
    }

    #[test]
    fn delay_scope_and_department_pairs_match_the_briefs() {
        let ds = generate().unwrap();
        let spends = |name: &str, signers: &[usize], height, age| {
            let Fixture::Write(w) = ds
                .fixtures
                .iter()
                .find(|f| f.id() == format!("t1-human-{name}"))
                .unwrap()
            else {
                unreachable!()
            };
            let ms: miniscript::Miniscript<PublicKey, Segwitv0> =
                w.reference_miniscript.parse().unwrap();
            eval(
                &ms.lift().unwrap(),
                &TruthContext {
                    keys: signers
                        .iter()
                        .map(|i| (w.keys[*i].pubkey.clone(), true))
                        .collect(),
                    height,
                    age,
                    ..Default::default()
                },
            )
        };
        assert!(spends("cooperative-recovery", &[0, 1], 0, 0));
        assert!(!spends("all-paths-delayed", &[0, 1], 0, 0));
        for name in ["cooperative-recovery", "all-paths-delayed"] {
            assert!(!spends(name, &[2], 0, 1007));
            assert!(spends(name, &[2], 0, 1008));
            assert!(spends(name, &[0, 1], 0, 1008));
        }
        assert!(!spends("department-approval", &[0, 1], 0, 0));
        assert!(!spends("department-approval", &[2, 3], 0, 0));
        assert!(spends("department-approval", &[0, 3], 0, 0));
        assert!(!spends("two-clocks", &[0], 919999, 144));
        assert!(!spends("two-clocks", &[0], 920000, 143));
        assert!(spends("two-clocks", &[0], 920000, 144));
    }
    #[test]
    fn authored_cases_are_complete_and_verified() {
        let ds = generate().expect("all authored cases must be gradable");
        assert_eq!(ds.fixtures.len(), 32);
        for f in &ds.fixtures {
            let request = crate::prompt::for_fixture(f);
            assert!(!request.contains('$'));
            assert!(!request.contains("Rules:"));
            assert!(!request.contains("submit_"));
            let keys = match f {
                Fixture::Write(w) => &w.keys,
                Fixture::Tree(t) => &t.keys,
                _ => unreachable!(),
            };
            for k in keys {
                assert!(request.contains(&k.pubkey));
            }
        }
        assert_eq!(
            serde_json::to_string(&ds.fixtures).unwrap(),
            serde_json::to_string(&generate().unwrap().fixtures).unwrap()
        );
    }
}
