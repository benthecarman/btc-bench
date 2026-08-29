//! Task 3 corpus: standard script families, parameterized with fresh
//! keys. Protocol families (BOLT 3, Liquid, coinswap, Revault) are a
//! later dataset revision pinned to exact spec commits; see DESIGN.md.

use std::collections::BTreeMap;

use crate::keys::KeySet;
use crate::rng::SeededRng;
use bench_core::task::{IdentifyFixture, ParamValue};
use bitcoin::hashes::Hash;
use bitcoin::hashes::{hash160, sha256};
use bitcoin::script::{Builder, PushBytesBuf};
use bitcoin::ScriptBuf;

fn push(bytes: &[u8]) -> PushBytesBuf {
    PushBytesBuf::try_from(bytes.to_vec()).expect("push data in range")
}

fn ms_script(k: usize, pks: &[String]) -> ScriptBuf {
    let mut b = Builder::new().push_int(k as i64);
    for pk in pks {
        b = b.push_slice(push(&hex_decode(pk)));
    }
    b.push_int(pks.len() as i64)
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_CHECKMULTISIG)
        .into_script()
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("valid hex"))
        .collect()
}

fn hash160_of(script: &ScriptBuf) -> Vec<u8> {
    hash160::Hash::hash(script.as_bytes())
        .to_byte_array()
        .to_vec()
}

fn sha256_of(script: &ScriptBuf) -> Vec<u8> {
    sha256::Hash::hash(script.as_bytes())
        .to_byte_array()
        .to_vec()
}

fn fixture(
    id: &str,
    family: &str,
    params: Vec<(&str, ParamValue)>,
    spk: ScriptBuf,
    inner: Option<ScriptBuf>,
) -> IdentifyFixture {
    IdentifyFixture {
        id: id.to_string(),
        family: family.to_string(),
        params: params
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<BTreeMap<_, _>>(),
        spk_hex: spk.to_hex_string(),
        inner_script_hex: inner.map(|s| s.to_hex_string()),
    }
}

/// Build one item per standard family with the given key material.
/// `i` is the item index used for stable IDs.
pub fn standards(rng: &mut SeededRng, keys: &KeySet, i: usize) -> Vec<IdentifyFixture> {
    let mut out = Vec::new();
    let pk0 = &keys.compressed[0];
    let pk1 = &keys.compressed[1];
    let pk2 = &keys.compressed[2];

    // P2PK
    let spk = Builder::new()
        .push_slice(push(&hex_decode(pk0)))
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_CHECKSIG)
        .into_script();
    out.push(fixture(
        &format!("t3-{i:04}-p2pk"),
        "p2pk",
        vec![],
        spk,
        None,
    ));

    // P2PKH
    let h = hash160::Hash::hash(&hex_decode(pk0)).to_byte_array();
    let spk = Builder::new()
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_DUP)
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_HASH160)
        .push_slice(push(&h))
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_EQUALVERIFY)
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_CHECKSIG)
        .into_script();
    out.push(fixture(
        &format!("t3-{i:04}-p2pkh"),
        "p2pkh",
        vec![],
        spk,
        None,
    ));

    // P2WPKH
    let spk = Builder::new()
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_PUSHBYTES_0)
        .push_slice(push(&h))
        .into_script();
    out.push(fixture(
        &format!("t3-{i:04}-p2wpkh"),
        "p2wpkh",
        vec![],
        spk,
        None,
    ));

    // Bare 2-of-3 multisig
    let inner = ms_script(2, &[pk0.clone(), pk1.clone(), pk2.clone()]);
    out.push(fixture(
        &format!("t3-{i:04}-bare-ms"),
        "bare_multisig",
        vec![("k", ParamValue::Int(2)), ("n", ParamValue::Int(3))],
        inner.clone(),
        None,
    ));

    // P2SH-wrapped 2-of-2
    let inner = ms_script(2, &[pk0.clone(), pk1.clone()]);
    let spk = Builder::new()
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_HASH160)
        .push_slice(push(&hash160_of(&inner)))
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_EQUAL)
        .into_script();
    out.push(fixture(
        &format!("t3-{i:04}-p2sh-ms"),
        "p2sh_multisig",
        vec![("k", ParamValue::Int(2)), ("n", ParamValue::Int(2))],
        spk,
        Some(inner),
    ));

    // P2WSH-wrapped 2-of-3
    let inner = ms_script(2, &[pk0.clone(), pk1.clone(), pk2.clone()]);
    let spk = Builder::new()
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_PUSHBYTES_0)
        .push_slice(push(&sha256_of(&inner)))
        .into_script();
    out.push(fixture(
        &format!("t3-{i:04}-p2wsh-ms"),
        "p2wsh_multisig",
        vec![("k", ParamValue::Int(2)), ("n", ParamValue::Int(3))],
        spk,
        Some(inner),
    ));

    // P2TR (key-path from the model's viewpoint: opaque OP_1 <32>)
    let mut buf = [0u8; 32];
    rng.bytes(&mut buf);
    let spk = Builder::new()
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_PUSHNUM_1)
        .push_slice(push(&buf))
        .into_script();
    out.push(fixture(
        &format!("t3-{i:04}-p2tr"),
        "p2tr",
        vec![],
        spk,
        None,
    ));

    // OP_RETURN data
    let mut data = vec![0u8; 20];
    rng.bytes(&mut data);
    let spk = Builder::new()
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_RETURN)
        .push_slice(push(&data))
        .into_script();
    out.push(fixture(
        &format!("t3-{i:04}-opreturn"),
        "op_return",
        vec![],
        spk,
        None,
    ));

    // P2A anchor (BIP-331 shared anchor, OP_1 <0x4e73>)
    let spk = Builder::new()
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_PUSHNUM_1)
        .push_slice(push(&[0x4e, 0x73]))
        .into_script();
    out.push(fixture(&format!("t3-{i:04}-p2a"), "p2a", vec![], spk, None));

    // Ordinals inscription reveal (tapleaf envelope)
    let mut content = [0u8; 8];
    rng.bytes(&mut content);
    let inner = Builder::new()
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_PUSHBYTES_0)
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_IF)
        .push_slice(push(b"ord"))
        .push_slice(push(&content))
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_ENDIF)
        .into_script();
    let mut tweak = [0u8; 32];
    rng.bytes(&mut tweak);
    let spk = Builder::new()
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_PUSHNUM_1)
        .push_slice(push(&tweak))
        .into_script();
    out.push(fixture(
        &format!("t3-{i:04}-ordinals"),
        "ordinals_inscription",
        vec![],
        spk,
        Some(inner),
    ));

    out
}

/// Families emitted by [`standards`]; used for prompt instructions.
pub const FAMILIES: &[&str] = &[
    "p2pk",
    "p2pkh",
    "p2wpkh",
    "bare_multisig",
    "p2sh_multisig",
    "p2wsh_multisig",
    "p2tr",
    "op_return",
    "p2a",
    "ordinals_inscription",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_builds_and_is_identifiable() {
        let mut rng = SeededRng::new(3);
        let keys = crate::keys::generate(&mut rng, 3);
        let items = standards(&mut rng, &keys, 0);
        assert_eq!(items.len(), FAMILIES.len());
        for item in &items {
            assert!(!item.spk_hex.is_empty());
            assert!(FAMILIES.contains(&item.family.as_str()));
            // multisig params extracted
            if item.family.contains("multisig") {
                assert!(item.params.contains_key("k"));
                assert!(item.params.contains_key("n"));
            }
        }
        // determinism
        let mut r2 = SeededRng::new(3);
        let k2 = crate::keys::generate(&mut r2, 3);
        let again = standards(&mut r2, &k2, 0);
        assert_eq!(
            serde_json::to_string(&items).unwrap(),
            serde_json::to_string(&again).unwrap()
        );
    }
}
