//! Bitcoin Core check of the idiom rewrites in `normalize.rs`.
//!
//! The grader reads a non-Miniscript script through its rewrite, so the
//! claim to test is: the submitted script is spendable in exactly the
//! situations where the oracle says the rewritten one is. For each
//! template (one hand-written idiom per rule, in each context it occurs
//! in) and every resource set — which keys sign, whether the hash
//! preimage is known, whether the timelocks have passed — this searches
//! every witness over an adversarial alphabet (the empty vector,
//! non-minimal truthy values, each available signature in any slot, the
//! preimage if known) and runs it through Bitcoin Core's interpreter
//! (libbitcoinkernel, consensus flags). Core's spendability of both the
//! submitted and the rewritten script must equal the oracle's verdict.
//!
//! The witness search is bounded, so this is a check on the
//! implementation, not the proof (the proofs are in normalize.rs). A
//! negative control confirms the harness catches a rewrite that
//! changes meaning.
//!
//! Needs CMake and Boost headers to build Core:
//!   BOOST=$(nix build --no-link --print-out-paths nixpkgs#boost.dev)
//!   CMAKE_PREFIX_PATH=$BOOST cargo test -p bench-core --features kernel-check --release

#![cfg(feature = "kernel-check")]

use bench_core::normalize::normalize;
use bench_core::task::ContextKind;
use bench_core::truth::{eval, Atoms, TruthContext};
use bitcoin::absolute::LockTime;
use bitcoin::consensus::encode::serialize;
use bitcoin::hashes::{hash160, sha256, sha256d, Hash};
use bitcoin::opcodes::all::*;
use bitcoin::script::{Builder, PushBytesBuf};
use bitcoin::secp256k1::{Keypair, Message, Secp256k1, SecretKey};
use bitcoin::sighash::{EcdsaSighashType, Prevouts, SighashCache, TapSighashType};
use bitcoin::taproot::{LeafVersion, TapLeafHash, TaprootBuilder};
use bitcoin::transaction::Version;
use bitcoin::{
    Amount, OutPoint, PublicKey, ScriptBuf, Sequence, Transaction, TxIn, TxOut, Witness,
    XOnlyPublicKey,
};
use bitcoinkernel as krn;
use miniscript::policy::Liftable;
use miniscript::{Legacy, Miniscript, Segwitv0, Tap};
use std::cell::Cell;
use std::collections::BTreeMap;

thread_local! {
    /// Core script executions, for the per-template summary.
    static EXECUTIONS: Cell<u64> = const { Cell::new(0) };
}

const AMOUNT: u64 = 100_000;
const PREIMAGE: [u8; 32] = [0x42; 32];

/// What a template's script commits to: key pushes per context, and
/// the digests of the one preimage.
struct Material {
    keys: Vec<Vec<u8>>,
    sha256: Vec<u8>,
    hash256: Vec<u8>,
    hash160: Vec<u8>,
}

struct Template {
    name: &'static str,
    ctx: ContextKind,
    keys: usize,
    hash: bool,
    cltv: Option<u32>,
    csv: Option<u32>,
    /// Witness items an honest spend needs; the search goes one past.
    items: usize,
    build: fn(&Material) -> ScriptBuf,
}

fn push(b: Builder, bytes: &[u8]) -> Builder {
    b.push_slice(PushBytesBuf::try_from(bytes.to_vec()).unwrap())
}

fn templates() -> Vec<Template> {
    vec![
        Template {
            name: "BIP65 drop inside a key-select branch",
            ctx: ContextKind::Legacy,
            keys: 2,
            hash: false,
            cltv: Some(870_375),
            csv: None,
            items: 2,
            build: |m| {
                let b = Builder::new().push_opcode(OP_IF).push_int(870_375);
                let b = b.push_opcode(OP_CLTV).push_opcode(OP_DROP);
                let b = push(b, &m.keys[0]).push_opcode(OP_ELSE);
                push(b, &m.keys[1])
                    .push_opcode(OP_ENDIF)
                    .push_opcode(OP_CHECKSIG)
                    .into_script()
            },
        },
        Template {
            name: "BIP112 drop then signature",
            ctx: ContextKind::SegwitV0,
            keys: 1,
            hash: false,
            cltv: None,
            csv: Some(144),
            items: 1,
            build: |m| {
                let b = Builder::new()
                    .push_int(144)
                    .push_opcode(OP_CSV)
                    .push_opcode(OP_DROP);
                push(b, &m.keys[0]).push_opcode(OP_CHECKSIG).into_script()
            },
        },
        Template {
            name: "HASH160 without a size check",
            ctx: ContextKind::Legacy,
            keys: 2,
            hash: true,
            cltv: None,
            csv: None,
            items: 3,
            build: |m| {
                let b = push(Builder::new(), &m.keys[0]).push_opcode(OP_CHECKSIGVERIFY);
                let b = b.push_opcode(OP_IF).push_opcode(OP_HASH160);
                let b = push(b, &m.hash160)
                    .push_opcode(OP_EQUAL)
                    .push_opcode(OP_ELSE);
                push(b, &m.keys[1])
                    .push_opcode(OP_CHECKSIG)
                    .push_opcode(OP_ENDIF)
                    .into_script()
            },
        },
        Template {
            name: "SHA256 without a size check, verify branches",
            ctx: ContextKind::SegwitV0,
            keys: 2,
            hash: true,
            cltv: None,
            csv: None,
            items: 3,
            build: |m| {
                let b = Builder::new().push_opcode(OP_IF).push_opcode(OP_SHA256);
                let b = push(b, &m.sha256)
                    .push_opcode(OP_EQUALVERIFY)
                    .push_opcode(OP_ELSE);
                let b = push(b, &m.keys[0])
                    .push_opcode(OP_CHECKSIGVERIFY)
                    .push_opcode(OP_ENDIF);
                push(b, &m.keys[1]).push_opcode(OP_CHECKSIG).into_script()
            },
        },
        Template {
            name: "CHECKSIGADD with GREATERTHANOREQUAL",
            ctx: ContextKind::Tap,
            keys: 3,
            hash: false,
            cltv: None,
            csv: None,
            items: 3,
            build: |m| {
                let b = push(Builder::new(), &m.keys[0]).push_opcode(OP_CHECKSIG);
                let b = push(b, &m.keys[1]).push_opcode(OP_CHECKSIGADD);
                let b = push(b, &m.keys[2]).push_opcode(OP_CHECKSIGADD);
                b.push_int(2)
                    .push_opcode(OP_GREATERTHANOREQUAL)
                    .into_script()
            },
        },
        Template {
            name: "CHECKSIGADD count ending the script",
            ctx: ContextKind::Tap,
            keys: 2,
            hash: false,
            cltv: None,
            csv: None,
            items: 2,
            build: |m| {
                let b = push(Builder::new(), &m.keys[0]).push_opcode(OP_CHECKSIG);
                push(b, &m.keys[1])
                    .push_opcode(OP_CHECKSIGADD)
                    .into_script()
            },
        },
        Template {
            name: "GREATERTHANOREQUAL threshold feeding IFDUP NOTIF",
            ctx: ContextKind::Tap,
            keys: 3,
            hash: false,
            cltv: Some(876_512),
            csv: None,
            items: 3,
            build: |m| {
                let b = push(Builder::new(), &m.keys[0]).push_opcode(OP_CHECKSIG);
                let b = push(b, &m.keys[1]).push_opcode(OP_CHECKSIGADD);
                let b = b.push_int(2).push_opcode(OP_GREATERTHANOREQUAL);
                let b = b.push_opcode(OP_IFDUP).push_opcode(OP_NOTIF);
                let b = push(b, &m.keys[2]).push_opcode(OP_CHECKSIGVERIFY);
                b.push_int(876_512)
                    .push_opcode(OP_CLTV)
                    .push_opcode(OP_ENDIF)
                    .into_script()
            },
        },
        Template {
            name: "EQUALVERIFY threshold then HASH256 without a size check",
            ctx: ContextKind::Tap,
            keys: 2,
            hash: true,
            cltv: None,
            csv: None,
            items: 3,
            build: |m| {
                let b = push(Builder::new(), &m.keys[0]).push_opcode(OP_CHECKSIG);
                let b = push(b, &m.keys[1]).push_opcode(OP_CHECKSIGADD);
                let b = b
                    .push_int(1)
                    .push_opcode(OP_EQUALVERIFY)
                    .push_opcode(OP_HASH256);
                push(b, &m.hash256).push_opcode(OP_EQUAL).into_script()
            },
        },
        Template {
            name: "BIP112 drop, VERIFY-terminated count, signature",
            ctx: ContextKind::Tap,
            keys: 3,
            hash: false,
            cltv: None,
            csv: Some(52),
            items: 3,
            build: |m| {
                let b = Builder::new()
                    .push_int(52)
                    .push_opcode(OP_CSV)
                    .push_opcode(OP_DROP);
                let b = push(b, &m.keys[0]).push_opcode(OP_CHECKSIG);
                let b = push(b, &m.keys[1])
                    .push_opcode(OP_CHECKSIGADD)
                    .push_opcode(OP_VERIFY);
                push(b, &m.keys[2]).push_opcode(OP_CHECKSIG).into_script()
            },
        },
    ]
}

fn secret(i: usize) -> SecretKey {
    SecretKey::from_slice(&[i as u8 + 1; 32]).unwrap()
}

fn material(ctx: ContextKind, n: usize) -> Material {
    let secp = Secp256k1::new();
    let keys = (0..n)
        .map(|i| {
            let sk = secret(i);
            match ctx {
                ContextKind::Tap => sk.x_only_public_key(&secp).0.serialize().to_vec(),
                _ => PublicKey::new(sk.public_key(&secp)).to_bytes(),
            }
        })
        .collect();
    Material {
        keys,
        sha256: sha256::Hash::hash(&PREIMAGE).to_byte_array().to_vec(),
        hash256: sha256d::Hash::hash(&PREIMAGE).to_byte_array().to_vec(),
        hash160: hash160::Hash::hash(&PREIMAGE).to_byte_array().to_vec(),
    }
}

/// One situation: who signs, whether the preimage is known, whether
/// the timelocks have passed.
#[derive(Clone, Copy, Debug)]
struct Resources {
    signers: u32,
    preimage: bool,
    late: bool,
}

fn tx_fields(t: &Template, late: bool) -> (u32, u32) {
    let lock = t.cltv.map_or(0, |n| if late { n } else { n - 1 });
    let seq = t.csv.map_or(0xffff_fffe, |n| if late { n } else { n - 1 });
    (lock, seq)
}

fn spending_tx(t: &Template, late: bool) -> Transaction {
    let (lock, seq) = tx_fields(t, late);
    Transaction {
        version: Version::TWO,
        lock_time: LockTime::from_consensus(lock),
        input: vec![TxIn {
            previous_output: OutPoint::null(),
            script_sig: ScriptBuf::new(),
            sequence: Sequence(seq),
            witness: Witness::new(),
        }],
        output: vec![TxOut {
            value: Amount::from_sat(AMOUNT - 1_000),
            script_pubkey: ScriptBuf::new_op_return([]),
        }],
    }
}

/// The output a script locks, and how to finish a witness for it.
struct Lock {
    spk: ScriptBuf,
    control: Option<Vec<u8>>,
}

fn lock(ctx: ContextKind, script: &ScriptBuf) -> Lock {
    let secp = Secp256k1::new();
    match ctx {
        ContextKind::Legacy => Lock {
            spk: ScriptBuf::new_p2sh(&script.script_hash()),
            control: None,
        },
        ContextKind::SegwitV0 => Lock {
            spk: ScriptBuf::new_p2wsh(&script.wscript_hash()),
            control: None,
        },
        ContextKind::Tap => {
            let internal = XOnlyPublicKey::from_slice(&[
                0x50, 0x92, 0x9b, 0x74, 0xc1, 0xa0, 0x49, 0x54, 0xb7, 0x8b, 0x4b, 0x60, 0x35, 0xe9,
                0x7a, 0x5e, 0x07, 0x8a, 0x5a, 0x0f, 0x28, 0xec, 0x96, 0xd5, 0x47, 0xbf, 0xee, 0x9a,
                0xce, 0x80, 0x3a, 0xc0,
            ])
            .unwrap();
            let info = TaprootBuilder::new()
                .add_leaf(0, script.clone())
                .unwrap()
                .finalize(&secp, internal)
                .unwrap();
            let control = info
                .control_block(&(script.clone(), LeafVersion::TapScript))
                .unwrap()
                .serialize();
            Lock {
                spk: ScriptBuf::new_p2tr_tweaked(info.output_key()),
                control: Some(control),
            }
        }
    }
}

/// A valid signature by key `i` for spending `script` with `tx`.
fn signature(
    ctx: ContextKind,
    script: &ScriptBuf,
    l: &Lock,
    tx: &Transaction,
    i: usize,
) -> Vec<u8> {
    let secp = Secp256k1::new();
    let cache = SighashCache::new(tx);
    match ctx {
        ContextKind::Legacy | ContextKind::SegwitV0 => {
            let digest = if ctx == ContextKind::Legacy {
                cache
                    .legacy_signature_hash(0, script, EcdsaSighashType::All.to_u32())
                    .unwrap()
                    .to_byte_array()
            } else {
                SighashCache::new(tx)
                    .p2wsh_signature_hash(
                        0,
                        script,
                        Amount::from_sat(AMOUNT),
                        EcdsaSighashType::All,
                    )
                    .unwrap()
                    .to_byte_array()
            };
            let sig = secp.sign_ecdsa(&Message::from_digest(digest), &secret(i));
            bitcoin::ecdsa::Signature {
                signature: sig,
                sighash_type: EcdsaSighashType::All,
            }
            .to_vec()
        }
        ContextKind::Tap => {
            let prevout = TxOut {
                value: Amount::from_sat(AMOUNT),
                script_pubkey: l.spk.clone(),
            };
            let leaf = TapLeafHash::from_script(script, LeafVersion::TapScript);
            let digest = SighashCache::new(tx)
                .taproot_script_spend_signature_hash(
                    0,
                    &Prevouts::All(&[prevout]),
                    leaf,
                    TapSighashType::Default,
                )
                .unwrap()
                .to_byte_array();
            let kp = Keypair::from_secret_key(&secp, &secret(i));
            let sig = secp.sign_schnorr_no_aux_rand(&Message::from_digest(digest), &kp);
            sig.serialize().to_vec()
        }
    }
}

fn core_spends(
    ctx: ContextKind,
    script: &ScriptBuf,
    l: &Lock,
    tx: &Transaction,
    items: &[Vec<u8>],
) -> bool {
    let mut tx = tx.clone();
    match ctx {
        ContextKind::Legacy => {
            let mut b = Builder::new();
            for it in items {
                b = push(b, it);
            }
            tx.input[0].script_sig = push(b, script.as_bytes()).into_script();
        }
        _ => {
            let mut w = Witness::new();
            for it in items {
                w.push(it);
            }
            w.push(script.as_bytes());
            if let Some(c) = &l.control {
                w.push(c);
            }
            tx.input[0].witness = w;
        }
    }
    EXECUTIONS.with(|c| c.set(c.get() + 1));
    let ktx = krn::Transaction::new(&serialize(&tx)).unwrap();
    let spk = krn::ScriptPubkey::new(l.spk.as_bytes()).unwrap();
    let spent = vec![krn::TxOut::new(&spk, AMOUNT as i64)];
    let data = krn::PrecomputedTransactionData::new(&ktx, &spent).unwrap();
    krn::verify(
        &spk,
        Some(AMOUNT as i64),
        &ktx,
        0,
        Some(krn::VERIFY_ALL),
        &data,
    )
    .is_ok()
}

/// Search witnesses up to `max_len` items over the alphabet the
/// resources allow; the first spending witness, if any.
fn find_spend(
    t: &Template,
    script: &ScriptBuf,
    r: Resources,
    max_len: usize,
) -> Option<Vec<Vec<u8>>> {
    let tx = spending_tx(t, r.late);
    let l = lock(t.ctx, script);
    let mut alphabet: Vec<Vec<u8>> = vec![vec![], vec![0x01], vec![0x02]];
    for i in 0..t.keys {
        if r.signers & (1 << i) != 0 {
            alphabet.push(signature(t.ctx, script, &l, &tx, i));
        }
    }
    if t.hash && r.preimage {
        alphabet.push(PREIMAGE.to_vec());
    }
    for len in 0..=max_len {
        let mut idx = vec![0usize; len];
        loop {
            let items: Vec<Vec<u8>> = idx.iter().map(|&i| alphabet[i].clone()).collect();
            if core_spends(t.ctx, script, &l, &tx, &items) {
                return Some(items);
            }
            // Next index tuple, odometer style.
            let mut pos = 0;
            loop {
                if pos == len {
                    break;
                }
                idx[pos] += 1;
                if idx[pos] < alphabet.len() {
                    break;
                }
                idx[pos] = 0;
                pos += 1;
            }
            if pos == len {
                break;
            }
        }
    }
    None
}

/// The oracle's verdict on the rewritten script, per resource set.
fn oracle(t: &Template, rewritten: &ScriptBuf) -> Box<dyn Fn(Resources) -> bool> {
    let secp = Secp256k1::new();
    let key_names: Vec<String> = (0..t.keys)
        .map(|i| match t.ctx {
            ContextKind::Tap => secret(i).x_only_public_key(&secp).0.to_string(),
            _ => PublicKey::new(secret(i).public_key(&secp)).to_string(),
        })
        .collect();
    let s = rewritten.as_script();
    macro_rules! verdict {
        ($pk:ty, $ctx:ty) => {{
            let policy = Miniscript::<$pk, $ctx>::decode_consensus(s)
                .expect("rewrite decodes")
                .lift()
                .expect("rewrite lifts");
            let mut atoms = Atoms::default();
            Atoms::collect(&policy, &mut atoms);
            let (cltv, csv) = (t.cltv, t.csv);
            Box::new(move |r: Resources| {
                let template = Template {
                    name: "",
                    ctx: ContextKind::Legacy,
                    keys: 0,
                    hash: false,
                    cltv,
                    csv,
                    items: 0,
                    build: |_| ScriptBuf::new(),
                };
                let (height, age) = tx_fields(&template, r.late);
                let keys: BTreeMap<String, bool> = key_names
                    .iter()
                    .enumerate()
                    .map(|(i, k)| (k.clone(), r.signers & (1 << i) != 0))
                    .collect();
                let hashes = atoms
                    .hashes
                    .iter()
                    .map(|h| (h.clone(), r.preimage))
                    .collect();
                eval(
                    &policy,
                    &TruthContext {
                        keys,
                        hashes,
                        height,
                        age,
                    },
                )
            }) as Box<dyn Fn(Resources) -> bool>
        }};
    }
    match t.ctx {
        ContextKind::Legacy => verdict!(PublicKey, Legacy),
        ContextKind::SegwitV0 => verdict!(PublicKey, Segwitv0),
        ContextKind::Tap => verdict!(XOnlyPublicKey, Tap),
    }
}

fn all_resources(t: &Template) -> Vec<Resources> {
    let mut out = Vec::new();
    for signers in 0..(1u32 << t.keys) {
        for preimage in [false, true] {
            for late in [false, true] {
                if (!t.hash && preimage) || (t.cltv.is_none() && t.csv.is_none() && late) {
                    continue;
                }
                out.push(Resources {
                    signers,
                    preimage,
                    late,
                });
            }
        }
    }
    out
}

/// Core spendability of the submitted and the rewritten script against
/// the oracle's verdict on the rewrite, for every resource set.
fn check(t: &Template, submitted: &ScriptBuf, rewritten: &ScriptBuf) -> Result<(), String> {
    let verdict = oracle(t, rewritten);
    let before = EXECUTIONS.with(Cell::get);
    let (mut some_true, mut some_false) = (false, false);
    let mut situations = 0;
    for r in all_resources(t) {
        let want = verdict(r);
        situations += 1;
        some_true |= want;
        some_false |= !want;
        for (label, script) in [("submitted", submitted), ("rewritten", rewritten)] {
            let found = find_spend(t, script, r, t.items + 1);
            if found.is_some() != want {
                return Err(format!(
                    "{}: {label} script under {r:?}: oracle says {want}, Core {} ({found:?})",
                    t.name,
                    if found.is_some() {
                        "spends"
                    } else {
                        "cannot spend"
                    }
                ));
            }
        }
    }
    if !(some_true && some_false) {
        return Err(format!("{}: trivial policy, check is vacuous", t.name));
    }
    eprintln!(
        "{}: {situations} situations agree, {} Core executions",
        t.name,
        EXECUTIONS.with(Cell::get) - before
    );
    Ok(())
}

#[test]
fn rewrites_match_core_execution() {
    for t in templates() {
        let m = material(t.ctx, t.keys);
        let submitted = (t.build)(&m);
        assert!(
            !bench_core::decodes_in_context(t.ctx, &submitted),
            "{}: the template should need the rewrite",
            t.name
        );
        let rewritten =
            normalize(&submitted).unwrap_or_else(|| panic!("{}: no rule fired", t.name));
        assert!(
            bench_core::decodes_in_context(t.ctx, &rewritten),
            "{}: the rewrite does not decode",
            t.name
        );
        check(&t, &submitted, &rewritten).unwrap();
    }
}

/// The harness is not vacuous: a "rewrite" that drops the timelock
/// changes who can spend, and Core execution exposes it.
#[test]
fn negative_control_catches_a_meaning_change() {
    let t = &templates()[0];
    let m = material(t.ctx, t.keys);
    let submitted = (t.build)(&m);
    let b = push(Builder::new().push_opcode(OP_IF), &m.keys[0]).push_opcode(OP_ELSE);
    let wrong = push(b, &m.keys[1])
        .push_opcode(OP_ENDIF)
        .push_opcode(OP_CHECKSIG)
        .into_script();
    assert!(bench_core::decodes_in_context(t.ctx, &wrong));
    let err = check(t, &submitted, &wrong).unwrap_err();
    assert!(err.contains("submitted script"), "{err}");
}
