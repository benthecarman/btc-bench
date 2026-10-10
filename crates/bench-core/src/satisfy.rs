//! Task 6, satisfy: produce the witness that spends a script in a stated
//! situation. The answer lists the witness items in serialization order
//! (the order `bitcoin-cli` shows `txinwitness`; for P2SH, the scriptSig
//! pushes before the redeem script), with `<sig:LABEL>` where a
//! signature goes.
//!
//! Grading builds the real spend: the stated transaction (version 2,
//! nLockTime, nSequence), the output the script locks (P2SH, P2WSH, or a
//! single tapleaf under an unspendable internal key), signatures from
//! keys derived from the task id ([`secret_key`]) in place of the
//! placeholders (SIGHASH_ALL; SIGHASH_DEFAULT in tapscript), and runs it
//! through Bitcoin Core's consensus script verification
//! (libbitcoinconsensus, all consensus flags including taproot). Relay
//! policy (MINIMALIF and NULLFAIL in segwit v0, CLEANSTACK in P2SH) is
//! not checked; tapscript enforces MINIMALIF and NULLFAIL by consensus.
//!
//! Some situations do not allow the spend (one signer short, a secret
//! not known, nLockTime one block early): there the right answer is
//! that no witness spends. That ground truth is the oracle's policy
//! semantics, cross-checked at generation by rust-miniscript's
//! satisfier finding no witness. A witness Core accepts always scores,
//! so a wrong ground truth can only cost the model, never pay it.

use std::collections::BTreeMap;

use bitcoin::absolute::LockTime;
use bitcoin::consensus::encode::serialize;
use bitcoin::hashes::{sha256, Hash};
use bitcoin::script::{Builder, PushBytesBuf};
use bitcoin::secp256k1::{Keypair, Message, Secp256k1, SecretKey};
use bitcoin::sighash::{EcdsaSighashType, Prevouts, SighashCache, TapSighashType};
use bitcoin::taproot::{LeafVersion, TapLeafHash, TaprootBuilder};
use bitcoin::transaction::Version;
use bitcoin::{
    Amount, OutPoint, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Txid, Witness, XOnlyPublicKey,
};

use crate::task::{ContextKind, SatisfyFixture, WitnessAnswer};

const AMOUNT: u64 = 100_000;

/// The unspendable internal key of the tapscript outputs (BIP341's H).
pub const NUMS: &str = "50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0";

/// The private key behind `label` in task `id`:
/// SHA256("btc-bench satisfy key" || 0 || id || 0 || label || 0 || n)
/// for the first counter n that gives a valid secret.
pub fn secret_key(id: &str, label: &str) -> SecretKey {
    for n in 0u32.. {
        let mut data = b"btc-bench satisfy key\0".to_vec();
        data.extend_from_slice(id.as_bytes());
        data.push(0);
        data.extend_from_slice(label.as_bytes());
        data.push(0);
        data.extend_from_slice(&n.to_le_bytes());
        if let Ok(sk) = SecretKey::from_slice(sha256::Hash::hash(&data).as_byte_array()) {
            return sk;
        }
    }
    unreachable!("a valid secret key is found within a few counters")
}

/// The public key of `label` in task `id` as it appears in a script of
/// this context: 33-byte compressed, or 32-byte x-only in tapscript.
pub fn public_key(context: ContextKind, id: &str, label: &str) -> Vec<u8> {
    let secp = Secp256k1::new();
    let sk = secret_key(id, label);
    match context {
        ContextKind::Tap => sk.x_only_public_key(&secp).0.serialize().to_vec(),
        _ => bitcoin::PublicKey::new(sk.public_key(&secp)).to_bytes(),
    }
}

/// The output a satisfy task's script locks.
pub struct Locked {
    pub script: ScriptBuf,
    pub script_pubkey: ScriptBuf,
    /// Tapscript control block.
    control: Option<Vec<u8>>,
}

pub fn locked(f: &SatisfyFixture) -> Result<Locked, String> {
    let script = ScriptBuf::from_hex(&f.script_hex).map_err(|e| e.to_string())?;
    Ok(match f.context {
        ContextKind::Legacy => Locked {
            script_pubkey: ScriptBuf::new_p2sh(&script.script_hash()),
            script,
            control: None,
        },
        ContextKind::SegwitV0 => Locked {
            script_pubkey: ScriptBuf::new_p2wsh(&script.wscript_hash()),
            script,
            control: None,
        },
        ContextKind::Tap => {
            let secp = Secp256k1::new();
            let internal: XOnlyPublicKey = NUMS.parse().expect("NUMS is a valid x-only key");
            let info = TaprootBuilder::new()
                .add_leaf(0, script.clone())
                .map_err(|e| e.to_string())?
                .finalize(&secp, internal)
                .map_err(|_| "taproot tree does not finalize".to_string())?;
            let control = info
                .control_block(&(script.clone(), LeafVersion::TapScript))
                .ok_or("no control block for the leaf")?
                .serialize();
            Locked {
                script_pubkey: ScriptBuf::new_p2tr_tweaked(info.output_key()),
                script,
                control: Some(control),
            }
        }
    })
}

/// The spending transaction the situation states, without its witness.
pub fn spending_tx(f: &SatisfyFixture) -> Transaction {
    let txid = Txid::from_byte_array(sha256::Hash::hash(f.id.as_bytes()).to_byte_array());
    Transaction {
        version: Version::TWO,
        lock_time: LockTime::from_consensus(f.lock_time),
        input: vec![TxIn {
            previous_output: OutPoint { txid, vout: 0 },
            script_sig: ScriptBuf::new(),
            sequence: Sequence(f.sequence),
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(AMOUNT - 1_000),
            script_pubkey: ScriptBuf::new_op_return([]),
        }],
    }
}

/// Every key's signature for this spend, by label.
pub fn signatures(f: &SatisfyFixture, l: &Locked) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let secp = Secp256k1::new();
    let tx = spending_tx(f);
    let mut out = BTreeMap::new();
    for k in &f.keys {
        let sk = secret_key(&f.id, &k.label);
        let sig = match f.context {
            ContextKind::Legacy | ContextKind::SegwitV0 => {
                let cache = SighashCache::new(&tx);
                let digest = if f.context == ContextKind::Legacy {
                    cache
                        .legacy_signature_hash(0, &l.script, EcdsaSighashType::All.to_u32())
                        .map_err(|e| e.to_string())?
                        .to_byte_array()
                } else {
                    SighashCache::new(&tx)
                        .p2wsh_signature_hash(
                            0,
                            &l.script,
                            Amount::from_sat(AMOUNT),
                            EcdsaSighashType::All,
                        )
                        .map_err(|e| e.to_string())?
                        .to_byte_array()
                };
                bitcoin::ecdsa::Signature {
                    signature: secp.sign_ecdsa(&Message::from_digest(digest), &sk),
                    sighash_type: EcdsaSighashType::All,
                }
                .to_vec()
            }
            ContextKind::Tap => {
                let prevout = TxOut {
                    value: Amount::from_sat(AMOUNT),
                    script_pubkey: l.script_pubkey.clone(),
                };
                let leaf = TapLeafHash::from_script(&l.script, LeafVersion::TapScript);
                let digest = SighashCache::new(&tx)
                    .taproot_script_spend_signature_hash(
                        0,
                        &Prevouts::All(&[prevout]),
                        leaf,
                        TapSighashType::Default,
                    )
                    .map_err(|e| e.to_string())?
                    .to_byte_array();
                let kp = Keypair::from_secret_key(&secp, &sk);
                secp.sign_schnorr_no_aux_rand(&Message::from_digest(digest), &kp)
                    .serialize()
                    .to_vec()
            }
        };
        out.insert(k.label.clone(), sig);
    }
    Ok(out)
}

/// Why an answer's witness items cannot be assembled.
fn item_bytes(
    item: &str,
    f: &SatisfyFixture,
    sigs: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, String> {
    let t = item.trim();
    if let Some(label) = t.strip_prefix("<sig:").and_then(|r| r.strip_suffix('>')) {
        let label = label.trim();
        if !f.keys.iter().any(|k| k.label == label) {
            return Err(format!("no key named {label} in this task"));
        }
        if !f.signers.iter().any(|s| s == label) {
            return Err(format!("{label} is not signing this transaction"));
        }
        return Ok(sigs[label].clone());
    }
    let hex = t.strip_prefix("0x").unwrap_or(t);
    bitcoin::hex::FromHex::from_hex(hex).map_err(|e| format!("witness item {t:?} is not hex: {e}"))
}

/// The spend with the given witness items in place.
fn assemble(f: &SatisfyFixture, l: &Locked, items: &[Vec<u8>]) -> Result<Transaction, String> {
    let mut tx = spending_tx(f);
    match f.context {
        ContextKind::Legacy => {
            let mut b = Builder::new();
            for it in items.iter().chain(std::iter::once(&l.script.to_bytes())) {
                let push = PushBytesBuf::try_from(it.clone())
                    .map_err(|_| "a scriptSig push exceeds 4 GB".to_string())?;
                b = b.push_slice(push);
            }
            tx.input[0].script_sig = b.into_script();
        }
        ContextKind::SegwitV0 | ContextKind::Tap => {
            let mut w = Witness::new();
            for it in items {
                w.push(it);
            }
            w.push(l.script.as_bytes());
            if let Some(c) = &l.control {
                w.push(c);
            }
            tx.input[0].witness = w;
        }
    }
    Ok(tx)
}

/// Does Bitcoin Core's consensus verification accept this spend?
fn core_accepts(l: &Locked, tx: &Transaction) -> bool {
    let spk = l.script_pubkey.as_bytes();
    let utxo = bitcoinconsensus::Utxo {
        script_pubkey: spk.as_ptr(),
        script_pubkey_len: spk.len() as u32,
        value: AMOUNT as i64,
    };
    bitcoinconsensus::verify_with_flags(
        spk,
        AMOUNT,
        &serialize(tx),
        Some(&[utxo]),
        0,
        bitcoinconsensus::VERIFY_ALL_PRE_TAPROOT | bitcoinconsensus::VERIFY_TAPROOT,
    )
    .is_ok()
}

#[derive(Clone, Debug)]
pub struct SatisfyResult {
    pub score: f64,
    pub reason: Option<String>,
}

/// Task 6: assemble the spend and run it through Bitcoin Core, or
/// check the claim that nothing spends.
pub fn grade_satisfy(f: &SatisfyFixture, answer: &WitnessAnswer) -> SatisfyResult {
    let fail = |reason: String| SatisfyResult {
        score: 0.0,
        reason: Some(reason),
    };
    if answer.unspendable {
        return if f.spendable {
            fail("a witness spends this output in this situation".to_string())
        } else {
            SatisfyResult {
                score: 1.0,
                reason: None,
            }
        };
    }
    let witness = &answer.witness;
    let l = match locked(f) {
        Ok(l) => l,
        Err(e) => return fail(format!("fixture script is invalid: {e}")),
    };
    let sigs = match signatures(f, &l) {
        Ok(s) => s,
        Err(e) => return fail(format!("cannot sign: {e}")),
    };
    let mut items = Vec::with_capacity(witness.len());
    for it in witness {
        match item_bytes(it, f, &sigs) {
            Ok(b) => items.push(b),
            Err(e) => return fail(e),
        }
    }
    let tx = match assemble(f, &l, &items) {
        Ok(tx) => tx,
        Err(e) => return fail(e),
    };
    if core_accepts(&l, &tx) {
        SatisfyResult {
            score: 1.0,
            reason: None,
        }
    } else {
        // Bitcoin Core's reject reason for a failing input script; the
        // consensus library reports no finer script error.
        fail("mandatory-script-verify-flag-failed".to_string())
    }
}

/// Witness items as answer text: placeholders for these signatures,
/// hex otherwise.
pub fn to_template(items: &[Vec<u8>], sigs: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    items
        .iter()
        .map(|it| match sigs.iter().find(|(_, s)| *s == it) {
            Some((label, _)) if !it.is_empty() => format!("<sig:{label}>"),
            _ => it.iter().map(|b| format!("{b:02x}")).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{KeyVar, Tier};

    fn fixture(context: ContextKind, script: ScriptBuf, signers: &[&str]) -> SatisfyFixture {
        SatisfyFixture {
            prompt_version: 2,
            id: "t6-test".into(),
            tier: Tier::Easy,
            context,
            script_hex: script.to_hex_string(),
            keys: ["Alice", "Bob"]
                .iter()
                .map(|l| KeyVar {
                    label: l.to_string(),
                    pubkey: public_key(context, "t6-test", l)
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect(),
                })
                .collect(),
            signers: signers.iter().map(|s| s.to_string()).collect(),
            preimages: vec![],
            lock_time: 0,
            sequence: 0xffff_fffe,
            spendable: true,
            reference_witness: vec![],
            source: "test".into(),
        }
    }

    /// `IF <A> ELSE <B> ENDIF CHECKSIG`: the selector goes last (top of
    /// the stack), so Alice's spend is [sig, 01] and Bob's [sig, ""].
    #[test]
    fn branch_selector_order_matters_in_every_context() {
        for context in [ContextKind::Legacy, ContextKind::SegwitV0, ContextKind::Tap] {
            let a = public_key(context, "t6-test", "Alice");
            let b = public_key(context, "t6-test", "Bob");
            let script = Builder::new()
                .push_opcode(bitcoin::opcodes::all::OP_IF)
                .push_slice(PushBytesBuf::try_from(a).unwrap())
                .push_opcode(bitcoin::opcodes::all::OP_ELSE)
                .push_slice(PushBytesBuf::try_from(b).unwrap())
                .push_opcode(bitcoin::opcodes::all::OP_ENDIF)
                .push_opcode(bitcoin::opcodes::all::OP_CHECKSIG)
                .into_script();
            let f = fixture(context, script.clone(), &["Alice"]);
            let w = |v: &[&str]| WitnessAnswer {
                witness: v.iter().map(|s| s.to_string()).collect(),
                unspendable: false,
            };
            assert_eq!(
                grade_satisfy(&f, &w(&["<sig:Alice>", "01"])).score,
                1.0,
                "{context:?}"
            );
            // Reversed order puts the signature where the selector goes.
            assert_eq!(grade_satisfy(&f, &w(&["01", "<sig:Alice>"])).score, 0.0);
            // Bob is not signing.
            let r = grade_satisfy(&f, &w(&["<sig:Bob>", ""]));
            assert_eq!(
                r.reason.as_deref(),
                Some("Bob is not signing this transaction")
            );
            let f = fixture(context, script, &["Bob"]);
            assert_eq!(
                grade_satisfy(&f, &w(&["<sig:Bob>", ""])).score,
                1.0,
                "{context:?}"
            );
        }
    }

    #[test]
    fn templates_name_the_signatures() {
        let sigs: BTreeMap<String, Vec<u8>> = [("Alice".to_string(), vec![0x30, 0x01])].into();
        assert_eq!(
            to_template(&[vec![], vec![0x30, 0x01], vec![0x01]], &sigs),
            vec!["", "<sig:Alice>", "01"]
        );
    }
}
