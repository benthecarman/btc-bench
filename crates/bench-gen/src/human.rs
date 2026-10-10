//! Fixed, authored requests for transfer evaluation. The seed changes neither
//! wording nor policy: keys and secrets are deterministic per scenario group.
//! These are synthetic requests, not transcripts collected from people.

use crate::{fixtures, keys, rng::SeededRng};
use bench_core::task::{ContextKind, Fixture, KeyVar, Tier, TreeFixture, WriteFixture};
use bitcoin::hashes::{hash160, sha256, Hash};
use bitcoin::{PublicKey, XOnlyPublicKey};
use miniscript::{policy::Concrete, Descriptor, Legacy, Segwitv0, Tap};
use miniscript::{policy::Liftable, FromStrKey, Miniscript, MiniscriptKey, ScriptContext};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub const CATALOG: &str = include_str!("../../../evals/human-v1.json");
pub const SUITE: &str = "human-v1";

pub fn catalog(suite: &str) -> Result<&'static str, String> {
    match suite {
        "human-v1" => Ok(CATALOG),
        "human-v2" => Ok(include_str!("../../../evals/human-v2.json")),
        "composition-transfer-v1" => {
            Ok(include_str!("../../../evals/composition-transfer-v1.json"))
        }
        _ => Err(format!("unknown human request suite {suite}")),
    }
}

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
    pub reference_notes: BTreeMap<String, Vec<String>>,
}

// Deliberately simple encoding for policies the optimizing compiler cannot
// handle. Signature reuse across branches is allowed. No policy is simplified
// here: decode the result and prove agreement against the original policy.
fn plain_miniscript<Pk: MiniscriptKey>(policy: &Concrete<Pk>) -> String {
    fn unit<Pk: MiniscriptKey>(p: &Concrete<Pk>) -> bool {
        match p {
            Concrete::After(_) | Concrete::Older(_) => false,
            Concrete::And(children) => unit(children.last().expect("nonempty AND")),
            Concrete::Or(children) => children.iter().all(|(_, p)| unit(p)),
            _ => true,
        }
    }
    match policy {
        Concrete::Key(k) => format!("pk({k})"),
        Concrete::After(t) => format!("after({})", t.to_consensus_u32()),
        Concrete::Older(t) => format!("older({})", t.to_consensus_u32()),
        Concrete::Sha256(h) => format!("sha256({h})"),
        Concrete::Hash160(h) => format!("hash160({h})"),
        Concrete::Hash256(h) => format!("hash256({h})"),
        Concrete::Ripemd160(h) => format!("ripemd160({h})"),
        Concrete::Trivial => "1".into(),
        Concrete::Unsatisfiable => "0".into(),
        Concrete::And(children) => children
            .iter()
            .rev()
            .map(|c| plain_miniscript(c))
            .reduce(|right, left| format!("and_v(v:{left},{right})"))
            .expect("nonempty AND"),
        Concrete::Or(children) => children
            .iter()
            .rev()
            .map(|(_, c)| plain_miniscript(c))
            .reduce(|right, left| format!("or_i({left},{right})"))
            .expect("nonempty OR"),
        Concrete::Thresh(t) => {
            // Signatures already have a canonical dissatisfaction. Adding an
            // optional branch creates a second one and makes the threshold
            // malleable even when all its keys are independent.
            if t.data()
                .iter()
                .all(|c| matches!(c.as_ref(), Concrete::Key(_)))
            {
                let children = t
                    .data()
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let prefix = if i == 0 { "" } else { "a:" };
                        format!("{prefix}{}", plain_miniscript(c))
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                return format!("thresh({},{children})", t.k());
            }
            // An explicit false branch gives every child a dissatisfaction.
            // a: carries the sum past children with multiple witness items.
            let children = t
                .data()
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let prefix = if i == 0 { "" } else { "a:" };
                    // Timelocks leave their numeric operand on the stack.
                    // A threshold must count each satisfied child as one.
                    let normalize = if unit(c) { "" } else { "n:" };
                    format!("{prefix}or_i({normalize}{},0)", plain_miniscript(c))
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("thresh({},{children})", t.k())
        }
    }
}

fn checked_reference<Pk: FromStrKey + miniscript::ToPublicKey, Ctx: ScriptContext<Key = Pk>>(
    policy: &Concrete<Pk>,
    notes: &mut Vec<String>,
) -> Result<Miniscript<Pk, Ctx>, String> {
    let expected = policy.lift().map_err(|e| e.to_string())?;
    let verify = |ms: &Miniscript<Pk, Ctx>| -> Result<(), String> {
        let decoded =
            Miniscript::<Pk, Ctx>::decode_consensus(&ms.encode()).map_err(|e| e.to_string())?;
        let actual = decoded.lift().map_err(|e| e.to_string())?;
        let verdict = bench_core::check_semantic(&expected, &actual, None);
        if verdict.is_equivalent() {
            Ok(())
        } else {
            Err(verdict.to_string())
        }
    };
    let optimized = policy
        .compile::<Ctx>()
        .map_err(|e| e.to_string())
        .and_then(|ms| {
            verify(&ms)?;
            Ok(ms)
        });
    match optimized {
        Ok(ms) => Ok(ms),
        Err(reason) => {
            let ms = Miniscript::<Pk, Ctx>::from_str_insane(&plain_miniscript(policy))
                .map_err(|e| e.to_string())?;
            verify(&ms)?;
            notes.push(format!("Used a plain reference encoding: {reason}. It is not an optimization or witness-malleability target."));
            Ok(ms)
        }
    }
}

fn checked_tree(policy: &str, notes: &mut Vec<String>) -> Result<(String, String), String> {
    let concrete: Concrete<XOnlyPublicKey> = policy
        .parse()
        .map_err(|e: miniscript::Error| e.to_string())?;
    let verify = |pair: &(String, String)| -> Result<(), String> {
        let expected = concrete.lift().map_err(|e| e.to_string())?;
        for (i, s) in [&pair.0, &pair.1].into_iter().enumerate() {
            let descriptor: Descriptor<XOnlyPublicKey> =
                s.parse().map_err(|e: miniscript::Error| e.to_string())?;
            let actual = descriptor.lift().map_err(|e| e.to_string())?;
            // Only the target's leaves are used by the execution checker.
            // Baselines are lifted as descriptors, where key hashes retain
            // their public keys. Preserve existing baseline weight targets.
            if i == 0 {
                let Descriptor::Tr(tr) = &descriptor else {
                    return Err("tree reference must use tr()".into());
                };
                for leaf in tr.leaves() {
                    Miniscript::<XOnlyPublicKey, Tap>::decode_consensus(
                        &leaf.miniscript().encode(),
                    )
                    .map_err(|e| e.to_string())?
                    .lift()
                    .map_err(|e| e.to_string())?;
                }
            }
            let verdict =
                bench_core::check_semantic(&expected, &actual, Some(fixtures::UNSPENDABLE_KEY));
            if !verdict.is_equivalent() {
                return Err(verdict.to_string());
            }
        }
        Ok(())
    };
    let optimized = fixtures::tree_descriptors_for_policy(policy, fixtures::UNSPENDABLE_KEY)
        .and_then(|pair| {
            verify(&pair)?;
            Ok(pair)
        });
    if let Ok(pair) = optimized {
        return Ok(pair);
    }
    notes.push(format!(
        "Used a tree with separately checked leaves: {}",
        optimized.unwrap_err()
    ));
    fn routes(p: &Concrete<XOnlyPublicKey>) -> Result<Vec<Concrete<XOnlyPublicKey>>, String> {
        match p {
            Concrete::Or(children) => {
                let mut out = Vec::new();
                for (_, child) in children {
                    out.extend(routes(child)?);
                }
                if out.len() > 64 {
                    return Err("too many tree routes".into());
                }
                Ok(out)
            }
            Concrete::And(children) => {
                let mut out = routes(&children[0])?;
                for child in &children[1..] {
                    let right = routes(child)?;
                    if out.len() * right.len() > 64 {
                        return Err("too many tree routes".into());
                    }
                    out = out
                        .into_iter()
                        .flat_map(|a| {
                            right.iter().map(move |b| {
                                Concrete::And(vec![
                                    std::sync::Arc::new(a.clone()),
                                    std::sync::Arc::new(b.clone()),
                                ])
                            })
                        })
                        .collect();
                }
                Ok(out)
            }
            _ => Ok(vec![p.clone()]),
        }
    }
    fn balanced(leaves: &[String]) -> String {
        if leaves.len() == 1 {
            leaves[0].clone()
        } else {
            let (a, b) = leaves.split_at(leaves.len() / 2);
            format!("{{{},{}}}", balanced(a), balanced(b))
        }
    }
    let mut branches = routes(&concrete)?;
    let at = branches
        .iter()
        .position(|p| matches!(p, Concrete::Key(_)))
        .ok_or("tree needs an immediate key route")?;
    let Concrete::Key(internal) = branches.remove(at) else {
        unreachable!()
    };
    let leaves = branches
        .iter()
        .map(|p| checked_reference::<XOnlyPublicKey, Tap>(p, notes).map(|ms| ms.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let reference = if leaves.is_empty() {
        format!("tr({internal})")
    } else {
        format!("tr({internal},{})", balanced(&leaves))
    };
    // A realistic baseline keeps the owner in a leaf under a NUMS key,
    // instead of forcing repeated keys into one large Miniscript leaf.
    let baseline = if leaves.is_empty() {
        format!("tr({},pk({internal}))", fixtures::UNSPENDABLE_KEY)
    } else {
        format!(
            "tr({},{{pk({internal}),{}}})",
            fixtures::UNSPENDABLE_KEY,
            balanced(&leaves)
        )
    };
    notes.push("Tree baseline places the owner in a leaf under a NUMS key; the reference promotes it to the key path. Alternatives inside conjunctions are split into separate leaves.".into());
    let pair = (reference, baseline);
    verify(&pair)?;
    Ok(pair)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn build(case: &Case, notes: &mut Vec<String>) -> Result<Fixture, String> {
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
                    let ms = checked_reference::<$key, $ctx>(&policy, notes)?;
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
                prompt_version: 0,
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
            let (reference, baseline) = checked_tree(&policy, notes)?;
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
                prompt_version: 0,
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
    generate_suite(SUITE)
}

pub fn generate_suite(suite: &str) -> Result<HumanDataset, String> {
    compile_catalog(catalog(suite)?)
}

/// Compile external synthetic requests with the same reference checks.
/// This function does not assign a split; the caller must preserve source
/// provenance and prevent evaluation catalogs from entering training.
pub fn compile_catalog(input: &str) -> Result<HumanDataset, String> {
    let cases: Vec<Case> = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let mut out = HumanDataset {
        fixtures: Vec::new(),
        groups: BTreeMap::new(),
        reference_notes: BTreeMap::new(),
    };
    let mut ids = BTreeSet::new();
    let mut errors = Vec::new();
    for case in cases {
        if !ids.insert(case.id.clone()) {
            return Err(format!("duplicate case ID {}", case.id));
        }
        let mut notes = Vec::new();
        match build(&case, &mut notes) {
            Ok(f) => {
                if !notes.is_empty() {
                    out.reference_notes.insert(f.id().into(), notes);
                }
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
    #[test]
    fn fallback_signature_quorum_is_equivalent_and_non_malleable() {
        use super::*;
        let material = keys::generate(&mut SeededRng::new(20260907), 4);
        macro_rules! check {
            ($keys:expr, $pk:ty, $ctx:ty, $kind:expr) => {{
                let keys = $keys;
                let policy: Concrete<$pk> = format!(
                    "or(thresh(2,pk({}),pk({}),pk({})),and(pk({}),older(144)))",
                    keys[0], keys[1], keys[2], keys[3]
                )
                .parse()
                .unwrap();
                let ms =
                    Miniscript::<$pk, $ctx>::from_str_insane(&plain_miniscript(&policy)).unwrap();
                let script = ms.encode();
                let decoded = Miniscript::<$pk, $ctx>::decode_consensus(&script).unwrap();
                assert!(decoded.is_non_malleable());
                assert!(bench_core::lint_report($kind, &script).is_empty());
                assert!(bench_core::check_semantic(
                    &policy.lift().unwrap(),
                    &decoded.lift().unwrap(),
                    None
                )
                .is_equivalent());
            }};
        }
        check!(
            &material.compressed,
            PublicKey,
            Segwitv0,
            ContextKind::SegwitV0
        );
        check!(&material.xonly, XOnlyPublicKey, Tap, ContextKind::Tap);
    }

    use super::*;
    use bench_core::truth::{eval, TruthContext};
    use miniscript::policy::Liftable;

    #[test]
    fn compound_votes_count_approvals_and_preserve_lock_boundaries() {
        let ds = generate_suite("composition-transfer-v1").unwrap();
        assert_eq!(ds.fixtures.len(), 24);
        let spends = |threshold, signers: &[usize], age| {
            let id = format!("t1-human-compound-vote-either-representative-{threshold}");
            let Fixture::Write(w) = ds.fixtures.iter().find(|f| f.id() == id).unwrap() else {
                unreachable!()
            };
            let script = bitcoin::ScriptBuf::from_hex(&w.reference_script_hex).unwrap();
            let policy = Miniscript::<PublicKey, Segwitv0>::decode_consensus(&script)
                .unwrap()
                .lift()
                .unwrap();
            eval(
                &policy,
                &TruthContext {
                    keys: signers
                        .iter()
                        .map(|i| (w.keys[*i].pubkey.clone(), true))
                        .collect(),
                    age,
                    ..Default::default()
                },
            )
        };
        assert!(!spends(2, &[0, 1], 0)); // Two people in one department count once.
        assert!(spends(2, &[0, 2], 0));
        assert!(!spends(2, &[0, 4], 255));
        assert!(spends(2, &[0, 4], 256));
        assert!(!spends(2, &[0, 6], 256)); // The fourth approval still needs its secret.
        assert!(!spends(3, &[0, 2], 256));
        assert!(!spends(3, &[0, 2, 4], 255));
        assert!(spends(3, &[0, 2, 4], 256));
    }

    #[test]
    fn expanded_suite_preserves_pilot_and_checks_new_spending_boundaries() {
        let old = generate().unwrap();
        let ds = generate_suite("human-v2").unwrap();
        assert_eq!(ds.fixtures.len(), 160);
        assert_eq!(
            ds.fixtures
                .iter()
                .filter(|f| matches!(f, Fixture::Write(_)))
                .count(),
            120
        );
        assert_eq!(
            serde_json::to_value(&old.fixtures).unwrap(),
            serde_json::to_value(&ds.fixtures[..32]).unwrap()
        );
        assert!(old.reference_notes.is_empty());
        assert!(!ds.reference_notes.is_empty());
        for (id, group) in &old.groups {
            assert_eq!(ds.groups.get(id), Some(group));
        }
        let spends = |name: &str, signers: &[usize], age| {
            let Fixture::Write(w) = ds
                .fixtures
                .iter()
                .find(|f| f.id() == format!("t1-human-{name}"))
                .unwrap()
            else {
                unreachable!()
            };
            let ctx = TruthContext {
                keys: signers
                    .iter()
                    .map(|i| (w.keys[*i].pubkey.clone(), true))
                    .collect(),
                age,
                ..Default::default()
            };
            let script = bitcoin::ScriptBuf::from_hex(&w.reference_script_hex).unwrap();
            match w.context {
                ContextKind::Tap => eval(
                    &Miniscript::<XOnlyPublicKey, Tap>::decode_consensus(&script)
                        .unwrap()
                        .lift()
                        .unwrap(),
                    &ctx,
                ),
                ContextKind::SegwitV0 => eval(
                    &Miniscript::<PublicKey, Segwitv0>::decode_consensus(&script)
                        .unwrap()
                        .lift()
                        .unwrap(),
                    &ctx,
                ),
                _ => unreachable!(),
            }
        };
        assert!(!spends("two-committee-quorums-both", &[0, 1, 2], 0));
        assert!(spends("two-committee-quorums-both", &[0, 1, 3, 4], 0));
        assert!(spends("two-committee-quorums-either", &[0, 1], 0));
        assert!(!spends("two-committee-quorums-either", &[0, 3], 0));
        assert!(spends("three-stage-quorum-single-late", &[0, 1, 2], 0));
        assert!(!spends("three-stage-quorum-single-late", &[0, 1], 143));
        assert!(spends("three-stage-quorum-single-late", &[0, 1], 144));
        assert!(!spends("three-stage-quorum-single-late", &[2], 1007));
        assert!(spends("three-stage-quorum-single-late", &[2], 1008));
        assert!(!spends("three-stage-quorum-single-late", &[], 65535));
        assert!(spends("owner-scope-independent-recovery", &[3], 2016));
        assert!(!spends("owner-scope-owner-stays", &[3], 2016));
        assert!(spends("owner-scope-owner-stays", &[0, 3], 2016));
        assert!(!spends("relative-lock-domain-seconds", &[0], 1024));
        assert!(!spends("relative-lock-domain-seconds", &[0], 4194305));
        assert!(spends("relative-lock-domain-seconds", &[0], 4194306));
        assert!(!spends("relative-lock-domain-blocks", &[0], 4194306));
        assert!(spends("relative-lock-domain-blocks", &[0], 1024));
    }

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
