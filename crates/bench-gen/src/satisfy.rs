//! Satisfy tasks (t6), derived from an existing dataset's write and
//! optimize fixtures: the compiled reference scripts and the bloated
//! optimize baselines become scripts to spend.
//!
//! Each source script is re-keyed to keys derived from the new task id
//! ([`bench_core::satisfy::secret_key`]) so the grader can sign. A
//! situation is then sampled (each key signs or not, each secret known
//! or not, the transaction's nLockTime and nSequence at or around the
//! timelock values) until the policy allows the spend, and the
//! reference witness is built with rust-miniscript's satisfier using
//! real signatures and checked against Bitcoin Core.

use std::collections::BTreeMap;

use bench_core::satisfy::{grade_satisfy, locked, public_key, signatures, to_template};
use bench_core::task::{ContextKind, Fixture, KeyVar, SatisfyFixture, WitnessAnswer, PROMPT_V2};
use bench_core::truth::{eval, Atoms, TruthContext};
use bitcoin::hashes::{hash160, Hash};
use bitcoin::hex::FromHex;
use bitcoin::locktime::{absolute, relative};
use bitcoin::script::{Builder, Instruction, PushBytesBuf};
use bitcoin::{ScriptBuf, Sequence, TapLeafHash, XOnlyPublicKey};
use miniscript::policy::Liftable;
use miniscript::{Legacy, Miniscript, Satisfier, Segwitv0, Tap};

use crate::rng::SeededRng;

/// Real signatures, known preimages and the stated transaction fields.
struct RealSat {
    ecdsa: BTreeMap<bitcoin::PublicKey, bitcoin::ecdsa::Signature>,
    schnorr: BTreeMap<XOnlyPublicKey, bitcoin::taproot::Signature>,
    pkh: BTreeMap<hash160::Hash, bitcoin::PublicKey>,
    xpkh: BTreeMap<hash160::Hash, XOnlyPublicKey>,
    /// Digest bytes -> preimage, for the secrets the spender knows.
    preimages: BTreeMap<Vec<u8>, [u8; 32]>,
    lock_time: absolute::LockTime,
    sequence: Sequence,
}

macro_rules! impl_real_sat {
    ($pk:ty) => {
        impl Satisfier<$pk> for RealSat {
            fn lookup_ecdsa_sig(&self, pk: &$pk) -> Option<bitcoin::ecdsa::Signature> {
                use miniscript::ToPublicKey;
                self.ecdsa.get(&pk.to_public_key()).copied()
            }
            fn lookup_tap_leaf_script_sig(
                &self,
                pk: &$pk,
                _: &TapLeafHash,
            ) -> Option<bitcoin::taproot::Signature> {
                use miniscript::ToPublicKey;
                self.schnorr.get(&pk.to_x_only_pubkey()).copied()
            }
            fn lookup_raw_pkh_pk(&self, h: &hash160::Hash) -> Option<bitcoin::PublicKey> {
                self.pkh.get(h).copied()
            }
            fn lookup_raw_pkh_x_only_pk(&self, h: &hash160::Hash) -> Option<XOnlyPublicKey> {
                self.xpkh.get(h).copied()
            }
            fn lookup_raw_pkh_ecdsa_sig(
                &self,
                h: &hash160::Hash,
            ) -> Option<(bitcoin::PublicKey, bitcoin::ecdsa::Signature)> {
                let pk = self.pkh.get(h)?;
                Some((*pk, *self.ecdsa.get(pk)?))
            }
            fn lookup_raw_pkh_tap_leaf_script_sig(
                &self,
                (h, _): &(hash160::Hash, TapLeafHash),
            ) -> Option<(XOnlyPublicKey, bitcoin::taproot::Signature)> {
                let pk = self.xpkh.get(h)?;
                Some((*pk, *self.schnorr.get(pk)?))
            }
            fn lookup_sha256(&self, h: &bitcoin::hashes::sha256::Hash) -> Option<[u8; 32]> {
                self.preimages.get(h.as_byte_array().as_slice()).copied()
            }
            fn lookup_hash160(&self, h: &hash160::Hash) -> Option<[u8; 32]> {
                self.preimages.get(h.as_byte_array().as_slice()).copied()
            }
            fn check_after(&self, t: absolute::LockTime) -> bool {
                t.is_implied_by(self.lock_time)
            }
            fn check_older(&self, t: relative::LockTime) -> bool {
                self.sequence
                    .to_relative_lock_time()
                    .is_some_and(|s| t.is_implied_by(s))
            }
        }
    };
}
impl_real_sat!(bitcoin::PublicKey);
impl_real_sat!(XOnlyPublicKey);

/// Replace each source key (and its HASH160, for pkh fragments) with
/// the task's derived key. Every other byte is kept.
fn rekey(script: &ScriptBuf, map: &BTreeMap<Vec<u8>, Vec<u8>>) -> Option<ScriptBuf> {
    let mut b = Builder::new();
    for ins in script.instructions() {
        b = match ins.ok()? {
            Instruction::PushBytes(p) => {
                let bytes = map
                    .get(p.as_bytes())
                    .cloned()
                    .unwrap_or(p.as_bytes().to_vec());
                b.push_slice(PushBytesBuf::try_from(bytes).ok()?)
            }
            Instruction::Op(op) => b.push_opcode(op),
        };
    }
    let out = b.into_script();
    // Builder re-encodes pushes minimally; the source scripts are
    // minimal, so anything else means the rewrite changed structure.
    (out.len() == script.len()).then_some(out)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

struct Source<'a> {
    id: &'a str,
    tier: bench_core::Tier,
    context: ContextKind,
    script_hex: &'a str,
    keys: &'a [KeyVar],
    preimages: &'a BTreeMap<String, String>,
}

/// Write references and optimize baselines, alternating, so any prefix
/// mixes compiled and hand-bloated scripts.
fn sources(fixtures: &[Fixture]) -> Vec<Source<'_>> {
    let (mut writes, mut baselines) = (Vec::new(), Vec::new());
    for f in fixtures {
        match f {
            Fixture::Write(w) if !w.choose_context && w.request.is_none() => writes.push(Source {
                id: &w.id,
                tier: w.tier,
                context: w.context,
                script_hex: &w.reference_script_hex,
                keys: &w.keys,
                preimages: &w.hash_preimages,
            }),
            Fixture::Optimize(o) => baselines.push(Source {
                id: &o.id,
                tier: o.tier,
                context: o.context,
                script_hex: &o.baseline_script_hex,
                keys: &o.keys,
                preimages: &o.hash_preimages,
            }),
            _ => {}
        }
    }
    let mut out = Vec::new();
    let (mut w, mut b) = (writes.into_iter(), baselines.into_iter());
    loop {
        match (w.next(), b.next()) {
            (None, None) => break,
            (x, y) => out.extend(x.into_iter().chain(y)),
        }
    }
    out
}

/// One satisfy task from one source, or None when no situation with a
/// Core-verified reference witness turns up.
fn derive_one(
    src: &Source<'_>,
    id: &str,
    want_spendable: bool,
    rng: &mut SeededRng,
) -> Option<SatisfyFixture> {
    let ctx = src.context;
    let secp = bitcoin::secp256k1::Secp256k1::new();
    // Re-key.
    let mut map = BTreeMap::new();
    let mut keys = Vec::new();
    for k in src.keys {
        let old = Vec::from_hex(&k.pubkey).ok()?;
        let new = public_key(ctx, id, &k.label);
        map.insert(hash160::Hash::hash(&old).to_byte_array().to_vec(), {
            hash160::Hash::hash(&new).to_byte_array().to_vec()
        });
        map.insert(old, new.clone());
        keys.push(KeyVar {
            label: k.label.clone(),
            pubkey: hex(&new),
        });
    }
    let script = rekey(&ScriptBuf::from_hex(src.script_hex).ok()?, &map)?;

    // The policy, for sampling situations the spend is allowed in.
    macro_rules! policy {
        ($pk:ty, $c:ty) => {{
            let ms = Miniscript::<$pk, $c>::decode_consensus(script.as_script()).ok()?;
            ms.lift().ok()?
        }};
    }
    let mut atoms = Atoms::default();
    let (semantic_eval, key_names): (Box<dyn Fn(&TruthContext) -> bool>, Vec<String>) = match ctx {
        ContextKind::Legacy | ContextKind::SegwitV0 => {
            let p = if ctx == ContextKind::Legacy {
                policy!(bitcoin::PublicKey, Legacy)
            } else {
                policy!(bitcoin::PublicKey, Segwitv0)
            };
            Atoms::collect(&p, &mut atoms);
            let names = keys
                .iter()
                .map(|k| bitcoin::PublicKey::from_slice(&Vec::from_hex(&k.pubkey).unwrap()))
                .map(|pk| pk.map(|pk| pk.to_string()))
                .collect::<Result<_, _>>()
                .ok()?;
            (Box::new(move |c| eval(&p, c)), names)
        }
        ContextKind::Tap => {
            let p = policy!(XOnlyPublicKey, Tap);
            Atoms::collect(&p, &mut atoms);
            (
                Box::new(move |c| eval(&p, c)),
                keys.iter().map(|k| k.pubkey.clone()).collect(),
            )
        }
    };
    let digest_of = |h: &str| h.split_once(':').map(|(_, d)| d.to_string());
    let known_secret = |digest: &str| src.preimages.get(digest).cloned();
    let allows = |sit: &Situation| {
        semantic_eval(&TruthContext {
            keys: key_names
                .iter()
                .enumerate()
                .map(|(i, k)| (k.clone(), sit.signers.contains(&i)))
                .collect(),
            hashes: sit.known.clone(),
            height: sit.lock_time,
            age: sit.sequence,
        })
    };
    let heights = atoms.heights();
    let ages = atoms.ages();

    for _ in 0..200 {
        let mut sit = Situation {
            signers: (0..keys.len()).filter(|_| rng.bool()).collect(),
            known: atoms
                .hashes
                .iter()
                .map(|h| (h.clone(), rng.bool()))
                .collect(),
            lock_time: if atoms.afters.is_empty() {
                0
            } else {
                heights[rng.below(heights.len() as u64) as usize]
            },
            sequence: if atoms.olders.is_empty() {
                0xffff_fffe
            } else {
                ages[rng.below(ages.len() as u64) as usize]
            },
        };
        if !allows(&sit) {
            continue;
        }
        if !want_spendable {
            // Trim to a minimal allowed situation, then take away one
            // thing it relies on: the spend is a near miss.
            for i in shuffled(rng, sit.signers.clone()) {
                let mut t = sit.clone();
                t.signers.retain(|&s| s != i);
                if allows(&t) {
                    sit = t;
                }
            }
            for h in shuffled(rng, sit.known.keys().cloned().collect()) {
                let mut t = sit.clone();
                t.known.insert(h, false);
                if allows(&t) {
                    sit = t;
                }
            }
            let mut misses: Vec<Situation> = Vec::new();
            for &i in &sit.signers {
                let mut t = sit.clone();
                t.signers.retain(|&s| s != i);
                misses.push(t);
            }
            for (h, k) in &sit.known {
                if *k {
                    let mut t = sit.clone();
                    t.known.insert(h.clone(), false);
                    misses.push(t);
                }
            }
            for &a in &atoms.afters {
                if a > 0 && sit.lock_time >= a {
                    misses.push(Situation {
                        lock_time: a - 1,
                        ..sit.clone()
                    });
                }
            }
            for &o in &atoms.olders {
                if o & 0xffff > 0 && (sit.sequence & 0xffff) >= (o & 0xffff) {
                    misses.push(Situation {
                        sequence: o - 1,
                        ..sit.clone()
                    });
                }
            }
            let Some(miss) = shuffled(rng, misses).into_iter().find(|t| !allows(t)) else {
                continue;
            };
            sit = miss;
            // Distractors: extra signers and secrets that do not make
            // the spend possible.
            for i in shuffled(rng, (0..keys.len()).collect()) {
                if !sit.signers.contains(&i) && rng.bool() {
                    let mut t = sit.clone();
                    t.signers.push(i);
                    t.signers.sort();
                    if !allows(&t) {
                        sit = t;
                    }
                }
            }
        }
        let mut preimages = Vec::new();
        for (h, known) in &sit.known {
            if *known {
                preimages.push(known_secret(&digest_of(h)?)?);
            }
        }
        let mut f = SatisfyFixture {
            prompt_version: PROMPT_V2,
            id: id.to_string(),
            tier: src.tier,
            context: ctx,
            script_hex: script.to_hex_string(),
            keys: keys.clone(),
            signers: sit.signers.iter().map(|&i| keys[i].label.clone()).collect(),
            preimages,
            lock_time: sit.lock_time,
            sequence: sit.sequence,
            spendable: want_spendable,
            reference_witness: Vec::new(),
            source: src.id.to_string(),
        };
        // The satisfier with real signatures: the reference witness, or,
        // for a near miss, an independent check that none exists.
        let l = locked(&f).ok()?;
        let sigs = signatures(&f, &l).ok()?;
        let mut sat = RealSat {
            ecdsa: BTreeMap::new(),
            schnorr: BTreeMap::new(),
            pkh: BTreeMap::new(),
            xpkh: BTreeMap::new(),
            preimages: BTreeMap::new(),
            lock_time: absolute::LockTime::from_consensus(sit.lock_time),
            sequence: Sequence(sit.sequence),
        };
        for &i in &sit.signers {
            let label = &keys[i].label;
            let sk = bench_core::satisfy::secret_key(id, label);
            let bytes = &sigs[label];
            match ctx {
                ContextKind::Tap => {
                    let x = sk.x_only_public_key(&secp).0;
                    sat.schnorr
                        .insert(x, bitcoin::taproot::Signature::from_slice(bytes).ok()?);
                    sat.xpkh.insert(hash160::Hash::hash(&x.serialize()), x);
                }
                _ => {
                    let pk = bitcoin::PublicKey::new(sk.public_key(&secp));
                    sat.ecdsa
                        .insert(pk, bitcoin::ecdsa::Signature::from_slice(bytes).ok()?);
                    sat.pkh.insert(hash160::Hash::hash(&pk.to_bytes()), pk);
                }
            }
        }
        for (h, known) in &sit.known {
            if *known {
                let d = digest_of(h)?;
                let pre: [u8; 32] = Vec::from_hex(&known_secret(&d)?).ok()?.try_into().ok()?;
                sat.preimages.insert(Vec::from_hex(&d).ok()?, pre);
            }
        }
        let items = match ctx {
            ContextKind::Legacy => {
                Miniscript::<bitcoin::PublicKey, Legacy>::decode_consensus(&script)
                    .ok()?
                    .satisfy_malleable(&sat)
            }
            ContextKind::SegwitV0 => {
                Miniscript::<bitcoin::PublicKey, Segwitv0>::decode_consensus(&script)
                    .ok()?
                    .satisfy_malleable(&sat)
            }
            ContextKind::Tap => Miniscript::<XOnlyPublicKey, Tap>::decode_consensus(&script)
                .ok()?
                .satisfy_malleable(&sat),
        };
        match (want_spendable, items) {
            (true, Ok(items)) => {
                f.reference_witness = to_template(&items, &sigs);
                let answer = WitnessAnswer {
                    witness: f.reference_witness.clone(),
                    unspendable: false,
                };
                if grade_satisfy(&f, &answer).score == 1.0 {
                    return Some(f);
                }
            }
            // The policy and the satisfier agree nothing spends.
            (false, Err(_)) => return Some(f),
            _ => {}
        }
    }
    None
}

/// Who signs, which secrets are known (by hash atom), and the
/// transaction's nLockTime and nSequence.
#[derive(Clone)]
struct Situation {
    signers: Vec<usize>,
    known: BTreeMap<String, bool>,
    lock_time: u32,
    sequence: u32,
}

fn shuffled<T>(rng: &mut SeededRng, mut v: Vec<T>) -> Vec<T> {
    for i in (1..v.len()).rev() {
        v.swap(i, rng.below(i as u64 + 1) as usize);
    }
    v
}

/// Derive up to `count` satisfy tasks from a dataset, cycling through
/// its write and optimize fixtures in order; every other task is a near
/// miss the policy forbids. Returns the tasks and how many sources were
/// skipped.
pub fn derive(fixtures: &[Fixture], count: usize, seed: u64) -> (Vec<Fixture>, usize) {
    let mut rng = SeededRng::new(seed);
    let srcs = sources(fixtures);
    let mut out = Vec::new();
    let mut skipped = 0;
    for src in srcs.iter() {
        if out.len() == count {
            break;
        }
        let id = format!("t6-{:04}", out.len());
        match derive_one(src, &id, out.len() % 2 == 0, &mut rng) {
            Some(f) => out.push(Fixture::Satisfy(f)),
            None => skipped += 1,
        }
    }
    (out, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{generate, GenParams};

    #[test]
    fn derived_tasks_spend_under_core_and_are_deterministic() {
        let source = generate(&GenParams {
            seed: 7,
            write: 6,
            optimize: 6,
            ..GenParams::default()
        });
        let (a, skipped) = derive(&source, 10, 1);
        assert!(a.len() >= 8, "{} derived, {skipped} skipped", a.len());
        let spendable = a
            .iter()
            .filter(|f| matches!(f, Fixture::Satisfy(s) if s.spendable))
            .count();
        assert!(spendable > 0 && spendable < a.len(), "both kinds appear");
        for f in &a {
            let Fixture::Satisfy(s) = f else {
                panic!("not a satisfy task")
            };
            let reference = WitnessAnswer {
                witness: s.reference_witness.clone(),
                unspendable: !s.spendable,
            };
            assert_eq!(grade_satisfy(s, &reference).score, 1.0, "{}", s.id);
            // Claiming the opposite never scores.
            let wrong = WitnessAnswer {
                witness: vec![],
                unspendable: s.spendable,
            };
            assert_eq!(grade_satisfy(s, &wrong).score, 0.0, "{}", s.id);
            // The keys are the derived ones the grader signs with.
            for k in &s.keys {
                assert_eq!(k.pubkey, hex(&public_key(s.context, &s.id, &k.label)));
            }
        }
        let (b, _) = derive(&source, 10, 1);
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );
    }
}
