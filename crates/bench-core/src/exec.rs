//! Execution cross-check oracle (dual-oracle methodology, after
//! rust-miniscript's bitcoind integration tests): prove a reference or
//! baseline script is not merely semantically equivalent but actually
//! *spendable* — produce a witness with the crate satisfier, then run it
//! through the crate interpreter. Signature checks are assumed (dummy
//! signatures); hash preimages and timelocks are checked for real.
//! Shares no evaluation code with the truth-table oracle, so the two
//! cross-check each other.

use std::collections::{BTreeMap, BTreeSet};

use bitcoin::hashes::{hash160, sha256, Hash};
use bitcoin::hex::FromHex;
use bitcoin::locktime::{absolute, relative};
use bitcoin::secp256k1::Secp256k1;
use bitcoin::taproot::{LeafVersion, TaprootBuilder};
use bitcoin::{
    absolute::LockTime, ecdsa, script::Builder, Script, ScriptBuf, Sequence, Witness,
    XOnlyPublicKey,
};
use miniscript::interpreter::Interpreter;
use miniscript::policy::semantic::Policy as Semantic;
use miniscript::policy::Liftable;
use miniscript::{
    Legacy, Miniscript, MiniscriptKey, Satisfier, ScriptContext, Segwitv0, Tap, ToPublicKey,
};

use crate::task::ContextKind;

/// Known hash preimages for a task: hex hash -> hex preimage.
pub type PreimageMap = BTreeMap<String, String>;

/// Typed view of [`PreimageMap`].
#[derive(Default)]
pub struct HashPreimages {
    pub sha256: BTreeMap<[u8; 32], [u8; 32]>,
    pub hash160: BTreeMap<[u8; 20], [u8; 32]>,
}

impl HashPreimages {
    pub fn from_hex_map(map: &PreimageMap) -> Result<Self, String> {
        let mut out = HashPreimages::default();
        for (h, p) in map {
            let hv = Vec::from_hex(h).map_err(|e| format!("bad hash hex {h}: {e}"))?;
            let pv = Vec::from_hex(p).map_err(|e| format!("bad preimage hex: {e}"))?;
            let pv: [u8; 32] = pv
                .try_into()
                .map_err(|_| format!("preimage for {h} is not 32 bytes"))?;
            match hv.len() {
                32 => {
                    out.sha256.insert(hv.try_into().expect("32 bytes"), pv);
                }
                20 => {
                    out.hash160.insert(hv.try_into().expect("20 bytes"), pv);
                }
                n => return Err(format!("hash {h} is {n} bytes, want 20 or 32")),
            }
        }
        Ok(out)
    }
}

fn dummy_ecdsa() -> ecdsa::Signature {
    ecdsa::Signature {
        signature: bitcoin::secp256k1::ecdsa::Signature::from_compact(&[0x11; 64])
            .expect("0x11.. is a valid (r, s) pair"),
        sighash_type: bitcoin::sighash::EcdsaSighashType::All,
    }
}

fn dummy_schnorr() -> bitcoin::taproot::Signature {
    bitcoin::taproot::Signature {
        signature: bitcoin::secp256k1::schnorr::Signature::from_slice(&[0xab; 64])
            .expect("64 bytes parses as a schnorr signature"),
        sighash_type: bitcoin::sighash::TapSighashType::Default,
    }
}

/// A chosen satisfying assignment of atoms.
#[derive(Clone)]
struct Assignment<Pk: MiniscriptKey> {
    keys: BTreeSet<Pk>,
    sha256: BTreeSet<Pk::Sha256>,
    hash160: BTreeSet<Pk::Hash160>,
    after: Vec<absolute::LockTime>,
    older: Vec<relative::LockTime>,
}

impl<Pk: MiniscriptKey> Assignment<Pk> {
    fn new() -> Self {
        Assignment {
            keys: BTreeSet::new(),
            sha256: BTreeSet::new(),
            hash160: BTreeSet::new(),
            after: Vec::new(),
            older: Vec::new(),
        }
    }
}

/// All absolute timelocks of one spending path must share a unit: the
/// interpreter errors on Blocks-vs-Seconds comparisons, and no single
/// nLockTime satisfies both under miniscript's typed semantics.
fn same_unit(l: &absolute::LockTime, r: &absolute::LockTime) -> bool {
    matches!(l, absolute::LockTime::Seconds(_)) == matches!(r, absolute::LockTime::Seconds(_))
}

/// Greedily construct one satisfying assignment over the semantic
/// policy. `Thresh(k, ..)` covers both `and` (k = n) and `or` (k = 1)
/// after the lifter's normalization. Returns None when no
/// single-timelock-unit witness exists.
fn find_sat<Pk: MiniscriptKey>(p: &Semantic<Pk>) -> Option<Assignment<Pk>> {
    match p {
        Semantic::Unsatisfiable => None,
        Semantic::Trivial => Some(Assignment::new()),
        Semantic::Key(k) => {
            let mut a = Assignment::new();
            a.keys.insert(k.clone());
            Some(a)
        }
        Semantic::After(t) => {
            let mut a = Assignment::new();
            a.after.push(absolute::LockTime::from(*t));
            Some(a)
        }
        Semantic::Older(t) => {
            let mut a = Assignment::new();
            a.older.push(relative::LockTime::from(*t));
            Some(a)
        }
        Semantic::Sha256(h) => {
            let mut a = Assignment::new();
            a.sha256.insert(h.clone());
            Some(a)
        }
        Semantic::Hash160(h) => {
            let mut a = Assignment::new();
            a.hash160.insert(h.clone());
            Some(a)
        }
        // Not produced by our task distribution; refusing keeps the
        // oracle honest rather than silently asserting satisfiability.
        Semantic::Hash256(_) | Semantic::Ripemd160(_) => None,
        Semantic::Thresh(th) => {
            let k = th.k();
            let mut acc = Assignment::<Pk>::new();
            let mut chosen = 0usize;
            for child in th.data() {
                if chosen == k {
                    break;
                }
                let Some(t) = find_sat(child) else { continue };
                // Children are independent: an assignment for one never
                // blocks another, so greedy selection is exact up to the
                // shared-unit rule on absolute timelocks.
                let units_ok = match (acc.after.first(), t.after.first()) {
                    (Some(a), Some(b)) => same_unit(a, b),
                    _ => true,
                };
                if units_ok {
                    acc.keys.extend(t.keys);
                    acc.sha256.extend(t.sha256);
                    acc.hash160.extend(t.hash160);
                    acc.after.extend(t.after);
                    acc.older.extend(t.older);
                    chosen += 1;
                }
            }
            if chosen == k {
                Some(acc)
            } else {
                None
            }
        }
    }
}

/// Satisfier exposing exactly the chosen assignment. Dummy signatures:
/// the interpreter run assumes signature validity; hashes and timelocks
/// are evaluated for real.
struct FullSat<'a, Pk: MiniscriptKey + ToPublicKey> {
    keys: BTreeSet<Pk>,
    sha256: &'a BTreeMap<[u8; 32], [u8; 32]>,
    hash160: &'a BTreeMap<[u8; 20], [u8; 32]>,
    after: Vec<absolute::LockTime>,
    older: Vec<relative::LockTime>,
}

impl<Pk: MiniscriptKey + ToPublicKey> Satisfier<Pk> for FullSat<'_, Pk> {
    fn lookup_ecdsa_sig(&self, pk: &Pk) -> Option<ecdsa::Signature> {
        self.keys.contains(pk).then(dummy_ecdsa)
    }

    fn lookup_tap_key_spend_sig(&self, pk: &Pk) -> Option<bitcoin::taproot::Signature> {
        self.keys.contains(pk).then(dummy_schnorr)
    }

    fn lookup_tap_leaf_script_sig(
        &self,
        pk: &Pk,
        _: &bitcoin::TapLeafHash,
    ) -> Option<bitcoin::taproot::Signature> {
        self.keys.contains(pk).then(dummy_schnorr)
    }

    fn lookup_sha256(&self, h: &Pk::Sha256) -> Option<[u8; 32]> {
        // Generic hash atoms carry no byte API; their Display is hex.
        let key = Vec::from_hex(&h.to_string()).ok()?;
        let key: [u8; 32] = key.try_into().ok()?;
        self.sha256.get(&key).copied()
    }

    fn lookup_hash160(&self, h: &Pk::Hash160) -> Option<[u8; 32]> {
        let key = Vec::from_hex(&h.to_string()).ok()?;
        let key: [u8; 20] = key.try_into().ok()?;
        self.hash160.get(&key).copied()
    }

    fn check_after(&self, t: absolute::LockTime) -> bool {
        self.after.contains(&t)
    }

    fn check_older(&self, t: relative::LockTime) -> bool {
        self.older.contains(&t)
    }
}

/// Transaction context that satisfies the assignment's timelocks.
fn tx_context<Pk: MiniscriptKey>(a: &Assignment<Pk>) -> (Sequence, LockTime) {
    let seq = a
        .older
        .iter()
        .map(|t| t.to_consensus_u32())
        .max()
        .map(Sequence::from_consensus)
        .unwrap_or_else(|| Sequence::from_consensus(1));
    let lt_consensus = a
        .after
        .iter()
        .map(|t| t.to_consensus_u32())
        .max()
        .unwrap_or(0);
    (seq, LockTime::from_consensus(lt_consensus))
}

/// Drive the interpreter to completion; every constraint must evaluate
/// without error and at least one must be satisfied.
fn interpret(
    spk: &ScriptBuf,
    script_sig: &Script,
    witness: &Witness,
    sequence: Sequence,
    lock_time: LockTime,
) -> Result<(), String> {
    let itp = Interpreter::from_txdata(spk, script_sig, witness, sequence, lock_time)
        .map_err(|e| format!("interpreter setup: {e}"))?;
    let mut satisfied = 0usize;
    for c in itp.iter_assume_sigs() {
        c.map_err(|e| format!("execution failed: {e}"))?;
        satisfied += 1;
    }
    if satisfied == 0 {
        return Err("execution proved nothing: no satisfied constraints".into());
    }
    Ok(())
}

fn interpret_legacy(
    script: &ScriptBuf,
    stack: &[Vec<u8>],
    sequence: Sequence,
    lock_time: LockTime,
) -> Result<(), String> {
    let script_hash: bitcoin::ScriptHash = hash160::Hash::hash(script.as_bytes()).into();
    let spk = ScriptBuf::new_p2sh(&script_hash);
    let mut b = Builder::new();
    for elem in stack {
        // Miniscript witnesses use [1]/[] as or_i branch selectors. The
        // interpreter parses the scriptSig with minimal-if rules, where
        // a 1-byte data push of 0x01 is rejected as non-minimal: the
        // canonical encoding is OP_PUSHNUM_1, which from_instruction
        // maps to Satisfied.
        if elem.as_slice() == [1] {
            b = b.push_opcode(bitcoin::blockdata::opcodes::all::OP_PUSHNUM_1);
        } else {
            b = b.push_slice(
                <&bitcoin::script::PushBytes>::try_from(elem.as_slice())
                    .expect("witness element fits push limits"),
            );
        }
    }
    let script_sig = b
        .push_slice(
            <&bitcoin::script::PushBytes>::try_from(script.as_bytes())
                .expect("script fits push limits"),
        )
        .into_script();
    interpret(
        &spk,
        script_sig.as_script(),
        &Witness::new(),
        sequence,
        lock_time,
    )
}

fn interpret_segwit(
    script: &ScriptBuf,
    stack: &[Vec<u8>],
    sequence: Sequence,
    lock_time: LockTime,
) -> Result<(), String> {
    let wsh: bitcoin::WScriptHash = sha256::Hash::hash(script.as_bytes()).into();
    let spk = ScriptBuf::new_p2wsh(&wsh);
    let mut elems: Vec<Vec<u8>> = stack.to_vec();
    elems.push(script.to_bytes());
    interpret(
        &spk,
        Script::new(),
        &Witness::from(elems),
        sequence,
        lock_time,
    )
}
fn interpret_tap(
    script: &ScriptBuf,
    stack: &[Vec<u8>],
    sequence: Sequence,
    lock_time: LockTime,
) -> Result<(), String> {
    let secp = Secp256k1::verification_only();
    let internal = XOnlyPublicKey::from_slice(&[0x51; 32]).expect("32 bytes is a valid x-only key");
    let spend_info = TaprootBuilder::new()
        .add_leaf(0, script.clone())
        .map_err(|e| format!("taproot build: {e}"))?
        .finalize(&secp, internal)
        .map_err(|_| "taproot finalize: tree not complete".to_string())?;
    let ctrl = spend_info
        .control_block(&(script.clone(), LeafVersion::TapScript))
        .ok_or("no control block for the sole leaf")?;
    let program = bitcoin::WitnessProgram::new(
        bitcoin::WitnessVersion::V1,
        &spend_info.output_key().serialize(),
    )
    .map_err(|e| format!("witness program: {e}"))?;
    let spk = ScriptBuf::new_witness_program(&program);
    let mut elems: Vec<Vec<u8>> = stack.to_vec();
    elems.push(script.to_bytes());
    elems.push(ctrl.serialize());
    interpret(
        &spk,
        Script::new(),
        &Witness::from(elems),
        sequence,
        lock_time,
    )
}

/// Prove `script` spendable in the `kind` context: decode, lift, find a
/// satisfying assignment, satisfy, and execute the witness through the
/// interpreter under the output's natural wrapping (P2SH / P2WSH /
/// P2TR script-path with a real commitment). Preimages must be supplied
/// for every hash atom on the chosen path.
pub fn execution_check(
    kind: ContextKind,
    script: &ScriptBuf,
    preimages: &HashPreimages,
) -> Result<(), String> {
    match kind {
        ContextKind::Legacy => exec_in_context::<Legacy>(script, preimages, interpret_legacy),
        ContextKind::SegwitV0 => exec_in_context::<Segwitv0>(script, preimages, interpret_segwit),
        ContextKind::Tap => exec_in_context::<Tap>(script, preimages, interpret_tap),
    }
}

fn exec_in_context<Ctx>(
    script: &ScriptBuf,
    preimages: &HashPreimages,
    run: fn(&ScriptBuf, &[Vec<u8>], Sequence, LockTime) -> Result<(), String>,
) -> Result<(), String>
where
    Ctx: ScriptContext,
    Ctx::Key: ToPublicKey,
{
    let ms = Miniscript::<Ctx::Key, Ctx>::decode_consensus(script.as_script())
        .map_err(|e| format!("decode: {e}"))?;
    let sem = ms.lift().map_err(|e| format!("lift: {e}"))?;
    let assignment = find_sat(&sem).ok_or(
        "no satisfiable assignment (needs a hash256/ripemd160 atom or mixed-unit absolute timelocks)",
    )?;
    for h in &assignment.sha256 {
        if !preimages.sha256.contains_key(&h.to_byte_array()) {
            return Err("sha256 atom on the chosen path has no known preimage".into());
        }
    }
    for h in &assignment.hash160 {
        if !preimages.hash160.contains_key(&h.to_byte_array()) {
            return Err("hash160 atom on the chosen path has no known preimage".into());
        }
    }
    let sat = FullSat {
        keys: assignment.keys.clone(),
        sha256: &preimages.sha256,
        hash160: &preimages.hash160,
        after: assignment.after.clone(),
        older: assignment.older.clone(),
    };
    let (sequence, lock_time) = tx_context(&assignment);
    let stack = ms
        .satisfy(&sat)
        .or_else(|_| ms.satisfy_malleable(&sat))
        .map_err(|e| format!("satisfier found no witness: {e}"))?;
    run(script, &stack, sequence, lock_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha_pre() -> HashPreimages {
        let pre = [7u8; 32];
        let mut m = HashPreimages::default();
        m.sha256
            .insert(sha256::Hash::hash(&pre).to_byte_array(), pre);
        m
    }

    fn keys(n: usize) -> (Vec<bitcoin::PublicKey>, Vec<XOnlyPublicKey>) {
        let secp = Secp256k1::new();
        let mut sk = [0u8; 32];
        let mut pks = Vec::new();
        let mut xpks = Vec::new();
        for i in 1..=n {
            sk[0] = i as u8;
            let sk = bitcoin::secp256k1::SecretKey::from_slice(&sk).unwrap();
            pks.push(bitcoin::PublicKey {
                inner: bitcoin::secp256k1::PublicKey::from_secret_key(&secp, &sk),
                compressed: true,
            });
            let kp = bitcoin::secp256k1::Keypair::from_secret_key(&secp, &sk);
            let (x, _) = XOnlyPublicKey::from_keypair(&kp);
            xpks.push(x);
        }
        (pks, xpks)
    }

    #[test]
    fn multi_segwit_executes() {
        let (pks, _) = keys(3);
        let ms = miniscript::Miniscript::<bitcoin::PublicKey, Segwitv0>::from_str_insane(&format!(
            "multi(2,{},{},{})",
            pks[0], pks[1], pks[2]
        ))
        .unwrap();
        execution_check(
            ContextKind::SegwitV0,
            &ms.encode(),
            &HashPreimages::default(),
        )
        .expect("2-of-3 must be spendable");
    }

    #[test]
    fn timelock_and_hash_legacy_executes() {
        let (pks, _) = keys(2);
        let pre = sha_pre();
        let h = sha256::Hash::hash(&[7u8; 32]);
        let ms = miniscript::Miniscript::<bitcoin::PublicKey, Legacy>::from_str_insane(&format!(
            "and_v(v:pk({}),and_v(v:sha256({}),after(700000)))",
            pks[0], h
        ))
        .unwrap();
        execution_check(ContextKind::Legacy, &ms.encode(), &pre)
            .expect("key+preimage+cltv must be spendable");
    }

    #[test]
    fn csv_segwit_executes() {
        let (pks, _) = keys(1);
        let ms = miniscript::Miniscript::<bitcoin::PublicKey, Segwitv0>::from_str_insane(&format!(
            "and_v(v:pk({}),older(144))",
            pks[0]
        ))
        .unwrap();
        execution_check(
            ContextKind::SegwitV0,
            &ms.encode(),
            &HashPreimages::default(),
        )
        .expect("key+csv must be spendable");
    }

    #[test]
    fn tap_thresh_executes() {
        let (_, xpks) = keys(3);
        let ms = miniscript::Miniscript::<XOnlyPublicKey, Tap>::from_str_insane(&format!(
            "multi_a(2,{},{},{})",
            xpks[0], xpks[1], xpks[2]
        ))
        .unwrap();
        execution_check(ContextKind::Tap, &ms.encode(), &HashPreimages::default())
            .expect("2-of-3 taproot thresh must be spendable");
    }

    #[test]
    fn or_d_malleable_baseline_executes() {
        // Malleable shape like the optimize baselines: satisfy() rejects,
        // satisfy_malleable must carry it.
        let (pks, _) = keys(2);
        let ms = miniscript::Miniscript::<bitcoin::PublicKey, Segwitv0>::from_str_insane(&format!(
            "or_d(pk({}),pk({}))",
            pks[0], pks[1]
        ))
        .unwrap();
        execution_check(
            ContextKind::SegwitV0,
            &ms.encode(),
            &HashPreimages::default(),
        )
        .expect("or_d baseline must be spendable via the malleable satisfier");
    }

    #[test]
    fn missing_preimage_fails_the_check() {
        let (pks, _) = keys(1);
        let h = sha256::Hash::hash(&[7u8; 32]);
        let ms = miniscript::Miniscript::<bitcoin::PublicKey, Segwitv0>::from_str_insane(&format!(
            "and_v(v:pk({}),sha256({}))",
            pks[0], h
        ))
        .unwrap();
        let err = execution_check(
            ContextKind::SegwitV0,
            &ms.encode(),
            &HashPreimages::default(),
        )
        .expect_err("hash with unknown preimage must fail");
        assert!(err.contains("no known preimage"), "{err}");
    }

    #[test]
    fn unsatisfiable_script_fails() {
        // sha256 without preimage AND nothing else: no assignment with a
        // known preimage exists.
        let h = sha256::Hash::hash(&[1u8; 32]);
        let ms = miniscript::Miniscript::<bitcoin::PublicKey, Segwitv0>::from_str_insane(&format!(
            "and_v(v:pk({}),sha256({}))",
            keys(1).0[0],
            h
        ))
        .unwrap();
        assert!(execution_check(
            ContextKind::SegwitV0,
            &ms.encode(),
            &HashPreimages::default()
        )
        .is_err());
    }

    #[test]
    fn garbage_script_fails() {
        let s = ScriptBuf::from_hex("51").expect("OP_1");
        assert!(execution_check(ContextKind::Tap, &s, &HashPreimages::default()).is_err());
    }

    #[test]
    fn legacy_or_i_selector_witness_executes() {
        // Regression: the or_i branch selector [1] must reach the
        // interpreter as OP_PUSHNUM_1, not a 1-byte data push that
        // minimal-if scriptSig parsing rejects.
        let (pks, _) = keys(2);
        let ms = miniscript::Miniscript::<bitcoin::PublicKey, Legacy>::from_str_insane(&format!(
            "and_v(v:pk({}),t:or_i(v:pk({}),v:after(700000)))",
            pks[0], pks[1]
        ))
        .unwrap();
        execution_check(ContextKind::Legacy, &ms.encode(), &HashPreimages::default())
            .expect("legacy or_i witness with a [1] selector must execute");
    }

    #[test]
    fn preimage_map_roundtrip() {
        let pre = [9u8; 32];
        let h = sha256::Hash::hash(&pre);
        let mut map = PreimageMap::new();
        map.insert(h.to_string(), "09".repeat(32));
        let typed = HashPreimages::from_hex_map(&map).unwrap();
        assert!(typed.sha256.contains_key(&h.to_byte_array()));
        assert!(HashPreimages::from_hex_map(&PreimageMap::default())
            .unwrap()
            .sha256
            .is_empty());
    }
}
