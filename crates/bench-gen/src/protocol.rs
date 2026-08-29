//! Protocol identification corpus: Lightning commitment scripts across
//! every era, and the Liquid federation peg.
//!
//! Provenance:
//! - P2WSH families (to_local, to_remote under anchors, keyed anchors,
//!   offered/received HTLC with and without the anchors CSV clause):
//!   transcribed from BOLT 3 at lightning/bolts master commit
//!   `152897261850` (03-transactions.md) and cross-checked against
//!   rust-lightning's `get_revokeable_redeemscript`,
//!   `get_htlc_redeemscript_with_explicit_keys`, and the keyed-anchor
//!   builders in `lightning/src/ln/chan_utils.rs`. Zero-fee commitments
//!   use the same HTLC scripts as the no-anchors era (verified in
//!   rust-lightning's `test_anchors`: identical hex for both feature
//!   sets), so no separate ZFC variants exist.
//! - Taproot families: transcribed from `bolt-simple-taproot.md` in
//!   lightning/bolts PR #1330 (t-bast:zero-fee-taproot-commitments).
//!   Items present the P2TR output plus one tapleaf script; the output
//!   key is a fresh x-only point.
//! - Liquid: constructed from the documented federation structure
//!   (N-of-M functionary multisig with a CSV-gated emergency 2-of-3
//!   backup; Blockstream Liquid multisig FAQ). NOT byte-pinned to the
//!   production fedpegscript — sourcing the live chainparams hex is a
//!   follow-up.
//!
//! Deliberately excluded as byte-indistinguishable from existing
//! families: the LN funding output (= bare_multisig 2-of-2), legacy
//! to_remote (= P2WPKH), HTLC-success/timeout second-stage outputs
//! (= ln_to_local), and the shared P2A anchor (= p2a).

use std::collections::BTreeMap;

use bench_core::task::{IdentifyFixture, ParamValue};
use bitcoin::blockdata::opcodes::all as a;
use bitcoin::hashes::{hash160, sha256, Hash};
use bitcoin::script::{Builder, PushBytesBuf};
use bitcoin::ScriptBuf;

use crate::rng::SeededRng;

fn push(bytes: &[u8]) -> PushBytesBuf {
    PushBytesBuf::try_from(bytes.to_vec()).expect("push data in range")
}

fn pk_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).expect("valid hex"))
        .collect()
}

fn item(
    id: &str,
    family: &str,
    params: Vec<(&str, ParamValue)>,
    witness_script: ScriptBuf,
) -> IdentifyFixture {
    let spk = Builder::new()
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_PUSHBYTES_0)
        .push_slice(push(
            &sha256::Hash::hash(witness_script.as_bytes()).to_byte_array(),
        ))
        .into_script();
    IdentifyFixture {
        id: id.to_string(),
        family: family.to_string(),
        params: params
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<BTreeMap<_, _>>(),
        spk_hex: spk.to_hex_string(),
        inner_script_hex: Some(witness_script.to_hex_string()),
    }
}

/// P2TR item: `OP_1 <x-only key>` spk plus one tapleaf script.
fn tr_item(
    id: &str,
    family: &str,
    params: Vec<(&str, ParamValue)>,
    output_key: &[u8; 32],
    tapleaf: ScriptBuf,
) -> IdentifyFixture {
    let spk = Builder::new()
        .push_opcode(bitcoin::blockdata::opcodes::all::OP_PUSHNUM_1)
        .push_slice(push(output_key))
        .into_script();
    IdentifyFixture {
        id: id.to_string(),
        family: family.to_string(),
        params: params
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<BTreeMap<_, _>>(),
        spk_hex: spk.to_hex_string(),
        inner_script_hex: Some(tapleaf.to_hex_string()),
    }
}

/// `to_local` witness script (identical across legacy, anchors, and
/// zero-fee commitment eras; also the HTLC-success/HTLC-timeout
/// second-stage output shape).
pub fn ln_to_local(revocation: &str, delayed: &str, to_self_delay: u16) -> ScriptBuf {
    Builder::new()
        .push_opcode(a::OP_IF)
        .push_slice(push(&pk_bytes(revocation)))
        .push_opcode(a::OP_ELSE)
        .push_int(to_self_delay as i64)
        .push_opcode(a::OP_CSV)
        .push_opcode(a::OP_DROP)
        .push_slice(push(&pk_bytes(delayed)))
        .push_opcode(a::OP_ENDIF)
        .push_opcode(a::OP_CHECKSIG)
        .into_script()
}

/// `to_remote` witness script under option_anchors.
pub fn ln_to_remote_anchors(remote: &str) -> ScriptBuf {
    Builder::new()
        .push_slice(push(&pk_bytes(remote)))
        .push_opcode(a::OP_CHECKSIGVERIFY)
        .push_int(1)
        .push_opcode(a::OP_CSV)
        .into_script()
}

/// Keyed anchor witness script (option_anchors).
pub fn ln_keyed_anchor(funding: &str) -> ScriptBuf {
    Builder::new()
        .push_slice(push(&pk_bytes(funding)))
        .push_opcode(a::OP_CHECKSIG)
        .push_opcode(a::OP_IFDUP)
        .push_opcode(a::OP_NOTIF)
        .push_int(16)
        .push_opcode(a::OP_CSV)
        .push_opcode(a::OP_ENDIF)
        .into_script()
}

/// The shared HTLC prefix: revocation-key check through the outer
/// IF/ELSE, then the remote HTLC key and the 32-byte-size probe.
fn htlc_common(revocation: &str, remote_htlc: &str) -> Builder {
    Builder::new()
        .push_opcode(a::OP_DUP)
        .push_opcode(a::OP_HASH160)
        .push_slice(push(
            &hash160::Hash::hash(&pk_bytes(revocation)).to_byte_array(),
        ))
        .push_opcode(a::OP_EQUAL)
        .push_opcode(a::OP_IF)
        .push_opcode(a::OP_CHECKSIG)
        .push_opcode(a::OP_ELSE)
        .push_slice(push(&pk_bytes(remote_htlc)))
        .push_opcode(a::OP_SWAP)
        .push_opcode(a::OP_SIZE)
        .push_int(32)
        .push_opcode(a::OP_EQUAL)
}

/// Offered HTLC witness script. The anchors variant appends
/// `1 OP_CHECKSEQUENCEVERIFY OP_DROP` before the final OP_ENDIF.
pub fn ln_offered_htlc(
    revocation: &str,
    remote_htlc: &str,
    local_htlc: &str,
    payment_hash: &[u8; 32],
    anchors: bool,
) -> ScriptBuf {
    let payment_hash160 = hash160::Hash::hash(payment_hash).to_byte_array();
    let mut b = htlc_common(revocation, remote_htlc)
        .push_opcode(a::OP_NOTIF)
        .push_opcode(a::OP_DROP)
        .push_int(2)
        .push_opcode(a::OP_SWAP)
        .push_slice(push(&pk_bytes(local_htlc)))
        .push_int(2)
        .push_opcode(a::OP_CHECKMULTISIG)
        .push_opcode(a::OP_ELSE)
        .push_opcode(a::OP_HASH160)
        .push_slice(push(&payment_hash160))
        .push_opcode(a::OP_EQUALVERIFY)
        .push_opcode(a::OP_CHECKSIG)
        .push_opcode(a::OP_ENDIF);
    if anchors {
        b = b.push_int(1).push_opcode(a::OP_CSV).push_opcode(a::OP_DROP);
    }
    b.push_opcode(a::OP_ENDIF).into_script()
}

/// Received HTLC witness script. The anchors variant appends
/// `1 OP_CHECKSEQUENCEVERIFY OP_DROP` before the final OP_ENDIF.
pub fn ln_received_htlc(
    revocation: &str,
    remote_htlc: &str,
    local_htlc: &str,
    payment_hash: &[u8; 32],
    cltv_expiry: u32,
    anchors: bool,
) -> ScriptBuf {
    let payment_hash160 = hash160::Hash::hash(payment_hash).to_byte_array();
    let mut b = htlc_common(revocation, remote_htlc)
        .push_opcode(a::OP_IF)
        .push_opcode(a::OP_HASH160)
        .push_slice(push(&payment_hash160))
        .push_opcode(a::OP_EQUALVERIFY)
        .push_int(2)
        .push_opcode(a::OP_SWAP)
        .push_slice(push(&pk_bytes(local_htlc)))
        .push_int(2)
        .push_opcode(a::OP_CHECKMULTISIG)
        .push_opcode(a::OP_ELSE)
        .push_opcode(a::OP_DROP)
        .push_int(cltv_expiry as i64)
        .push_opcode(a::OP_CLTV)
        .push_opcode(a::OP_DROP)
        .push_opcode(a::OP_CHECKSIG)
        .push_opcode(a::OP_ENDIF);
    if anchors {
        b = b.push_int(1).push_opcode(a::OP_CSV).push_opcode(a::OP_DROP);
    }
    b.push_opcode(a::OP_ENDIF).into_script()
}

// --- Taproot era (bolt-simple-taproot, PR #1330) ---

/// TR to_local delay tapscript.
pub fn ln_tr_delay_tapscript(delayed: &str, to_self_delay: u16) -> ScriptBuf {
    Builder::new()
        .push_slice(push(&pk_bytes(delayed)))
        .push_opcode(a::OP_CHECKSIGVERIFY)
        .push_int(to_self_delay as i64)
        .push_opcode(a::OP_CSV)
        .into_script()
}

/// TR to_remote tapscript (NUMS-internal-key variant).
pub fn ln_tr_to_remote_tapscript(remote: &str) -> ScriptBuf {
    Builder::new()
        .push_slice(push(&pk_bytes(remote)))
        .push_opcode(a::OP_CHECKSIGVERIFY)
        .push_int(1)
        .push_opcode(a::OP_CSV)
        .into_script()
}

/// TR anchor tapscript.
pub fn ln_tr_anchor_tapscript() -> ScriptBuf {
    Builder::new()
        .push_int(16)
        .push_opcode(a::OP_CSV)
        .into_script()
}

/// TR offered-HTLC timeout tapleaf.
pub fn ln_tr_offered_timeout_tapscript(local_htlc: &str, remote_htlc: &str) -> ScriptBuf {
    Builder::new()
        .push_slice(push(&pk_bytes(local_htlc)))
        .push_opcode(a::OP_CHECKSIGVERIFY)
        .push_slice(push(&pk_bytes(remote_htlc)))
        .push_opcode(a::OP_CHECKSIG)
        .into_script()
}

/// TR accepted-HTLC timeout tapleaf (v2 form with the CSV-1 clause).
pub fn ln_tr_accepted_timeout_tapscript(remote_htlc: &str, cltv_expiry: u32) -> ScriptBuf {
    Builder::new()
        .push_slice(push(&pk_bytes(remote_htlc)))
        .push_opcode(a::OP_CHECKSIGVERIFY)
        .push_int(1)
        .push_opcode(a::OP_CSV)
        .push_opcode(a::OP_VERIFY)
        .push_int(cltv_expiry as i64)
        .push_opcode(a::OP_CLTV)
        .into_script()
}

/// Liquid federation peg: N-of-M functionary multisig with a
/// CSV-gated emergency 2-of-3 backup (documented structure).
pub fn liquid_fedpeg(keys: &[String], csv: u32) -> ScriptBuf {
    let n_functionaries = keys.len() - 3;
    let k = n_functionaries * 2 / 3 + 1;
    let mut b = Builder::new().push_opcode(a::OP_IF).push_int(k as i64);
    for key in &keys[..n_functionaries] {
        b = b.push_slice(push(&pk_bytes(key)));
    }
    let mut b = b
        .push_int(n_functionaries as i64)
        .push_opcode(a::OP_CHECKMULTISIG)
        .push_opcode(a::OP_ELSE)
        .push_int(csv as i64)
        .push_opcode(a::OP_CSV)
        .push_opcode(a::OP_DROP)
        .push_int(2);
    for key in &keys[n_functionaries..] {
        b = b.push_slice(push(&pk_bytes(key)));
    }
    b.push_int(3)
        .push_opcode(a::OP_CHECKMULTISIG)
        .push_opcode(a::OP_ENDIF)
        .into_script()
}

/// Emit the full protocol corpus for one identify group `i`, with fresh
/// keys. Deterministic per rng stream.
pub fn protocol_items(rng: &mut SeededRng, i: usize) -> Vec<IdentifyFixture> {
    use bitcoin::key::XOnlyPublicKey;
    let keys = crate::keys::generate_raw(rng, 8);
    let mut payment_hash = [0u8; 32];
    rng.bytes(&mut payment_hash);
    let mut output_key = [0u8; 32];
    rng.bytes(&mut output_key);
    let tr_key: [u8; 32] = XOnlyPublicKey::from_slice(&output_key)
        .map(|k| k.serialize())
        .unwrap_or(output_key);
    let delay: u16 = (rng.range(48, 1024) as u16).max(1);
    let cltv: u32 = rng.range(400, 1000) as u32;
    let anchors = rng.bool();

    vec![
        item(
            &format!("t3-{i:04}-ln-to-local"),
            "ln_to_local",
            vec![("to_self_delay", ParamValue::Int(delay as u64))],
            ln_to_local(&keys[0], &keys[1], delay),
        ),
        item(
            &format!("t3-{i:04}-ln-to-remote-anchors"),
            "ln_to_remote_anchors",
            vec![],
            ln_to_remote_anchors(&keys[2]),
        ),
        item(
            &format!("t3-{i:04}-ln-keyed-anchor"),
            "ln_keyed_anchor",
            vec![],
            ln_keyed_anchor(&keys[3]),
        ),
        item(
            &format!("t3-{i:04}-ln-offered-htlc"),
            "ln_offered_htlc",
            vec![("anchors", ParamValue::Bool(anchors))],
            ln_offered_htlc(&keys[0], &keys[2], &keys[4], &payment_hash, anchors),
        ),
        item(
            &format!("t3-{i:04}-ln-received-htlc"),
            "ln_received_htlc",
            vec![
                ("cltv_expiry", ParamValue::Int(cltv as u64)),
                ("anchors", ParamValue::Bool(anchors)),
            ],
            ln_received_htlc(&keys[0], &keys[2], &keys[4], &payment_hash, cltv, anchors),
        ),
        tr_item(
            &format!("t3-{i:04}-ln-tr-to-local"),
            "ln_tr_to_local",
            vec![("to_self_delay", ParamValue::Int(delay as u64))],
            &tr_key,
            ln_tr_delay_tapscript(&keys[1], delay),
        ),
        tr_item(
            &format!("t3-{i:04}-ln-tr-to-remote"),
            "ln_tr_to_remote",
            vec![],
            &tr_key,
            ln_tr_to_remote_tapscript(&keys[2]),
        ),
        tr_item(
            &format!("t3-{i:04}-ln-tr-anchor"),
            "ln_tr_anchor",
            vec![],
            &tr_key,
            ln_tr_anchor_tapscript(),
        ),
        tr_item(
            &format!("t3-{i:04}-ln-tr-offered-htlc"),
            "ln_tr_offered_htlc",
            vec![],
            &tr_key,
            ln_tr_offered_timeout_tapscript(&keys[4], &keys[2]),
        ),
        tr_item(
            &format!("t3-{i:04}-ln-tr-accepted-htlc"),
            "ln_tr_accepted_htlc",
            vec![("cltv_expiry", ParamValue::Int(cltv as u64))],
            &tr_key,
            ln_tr_accepted_timeout_tapscript(&keys[2], cltv),
        ),
        item(
            &format!("t3-{i:04}-liquid-fedpeg"),
            "liquid_fedpeg",
            vec![
                ("k", ParamValue::Int(9)),
                ("n", ParamValue::Int(12)),
                ("csv", ParamValue::Int(4032)),
            ],
            {
                let fed_keys = crate::keys::generate_raw(rng, 15);
                liquid_fedpeg(&fed_keys, 4032)
            },
        ),
    ]
}

/// Protocol families (subset of FAMILIES used for rotation).
pub const PROTOCOL_FAMILIES: &[&str] = &[
    "ln_to_local",
    "ln_to_remote_anchors",
    "ln_keyed_anchor",
    "ln_offered_htlc",
    "ln_received_htlc",
    "ln_tr_to_local",
    "ln_tr_to_remote",
    "ln_tr_anchor",
    "ln_tr_offered_htlc",
    "ln_tr_accepted_htlc",
    "liquid_fedpeg",
];

#[cfg(test)]
mod tests {
    use super::*;

    // Fixed keys: compressed secp points from the generator with a fixed
    // seed, pinned by regenerating below and asserting the exact script
    // hex. These snapshots lock the transcription of every BOLT 3 /
    // bolt-simple-taproot template.
    const K0: &str = "0243242525d74152c3833a768fffd6596324cf756729b9f61fee0d1a9bd0088a13";

    fn k(i: u8) -> String {
        format!("02{i:02x}{}", "33".repeat(31))
    }

    #[test]
    fn ln_to_local_shape() {
        // OP_IF <revoc> OP_ELSE 144 CSV DROP <delayed> OP_ENDIF CHECKSIG
        let s = ln_to_local(K0, &k(2), 144);
        assert!(s.to_hex_string().starts_with(&format!("6321{}", K0)));
        assert_eq!(
            s.to_hex_string(),
            format!("6321{K0}67029000b27521{}68ac", k(2))
        );
    }

    #[test]
    fn ln_to_remote_anchors_shape() {
        let s = ln_to_remote_anchors(K0);
        // <key> OP_CHECKSIGVERIFY OP_1 OP_CSV
        assert_eq!(s.to_hex_string(), format!("21{K0}ad51b2"));
    }

    #[test]
    fn ln_keyed_anchor_shape() {
        let s = ln_keyed_anchor(K0);
        // <key> OP_CHECKSIG OP_IFDUP OP_NOTIF OP_16 CSV OP_ENDIF
        assert_eq!(s.to_hex_string(), format!("21{K0}ac736460b268"));
    }

    #[test]
    fn offered_htlc_anchors_clause() {
        let h = [7u8; 32];
        let plain = ln_offered_htlc(K0, &k(2), &k(4), &h, false);
        let anchored = ln_offered_htlc(K0, &k(2), &k(4), &h, true);
        // The anchors variant is the plain script with `01 b2 75`
        // inserted before the final OP_ENDIF byte.
        let p = plain.to_hex_string();
        let a = anchored.to_hex_string();
        assert_eq!(a.len(), p.len() + 6);
        assert!(a.starts_with(&p[..p.len() - 2]));
        assert!(a.ends_with("51b27568"));
        // Structure: DUP HASH160 <20> EQUAL IF CHECKSIG ELSE <remote>
        // SWAP SIZE 32 EQUAL NOTIF DROP 2 SWAP <local> 2 CHECKMULTISIG
        // ELSE HASH160 <20> EQUALVERIFY CHECKSIG ENDIF
        assert!(p.starts_with("76a914"));
        assert!(p.ends_with("68"));
    }

    #[test]
    fn received_htlc_anchors_clause() {
        let h = [7u8; 32];
        let plain = ln_received_htlc(K0, &k(2), &k(4), &h, 502, false);
        let anchored = ln_received_htlc(K0, &k(2), &k(4), &h, 502, true);
        let p = plain.to_hex_string();
        let a = anchored.to_hex_string();
        assert!(a.ends_with("51b27568"));
        assert!(p.ends_with("68"));
    }

    #[test]
    fn tr_tapleaves() {
        // to_local delay: <delayed> CSVVERIFY 144 CSV
        assert_eq!(
            ln_tr_delay_tapscript(K0, 144).to_hex_string(),
            format!("21{K0}ad029000b2")
        );
        // to_remote: <remote> CSVVERIFY 1 CSV
        assert_eq!(
            ln_tr_to_remote_tapscript(K0).to_hex_string(),
            format!("21{K0}ad51b2")
        );
        // anchor: 16 CSV
        assert_eq!(ln_tr_anchor_tapscript().to_hex_string(), "60b2");
        // offered timeout: <local> CSVVERIFY <remote> CHECKSIG
        assert_eq!(
            ln_tr_offered_timeout_tapscript(K0, &k(2)).to_hex_string(),
            format!("21{K0}ad21{}ac", k(2))
        );
        // accepted timeout: <remote> CSVVERIFY 1 CSV VERIFY <cltv> CLTV
        assert_eq!(
            ln_tr_accepted_timeout_tapscript(K0, 502).to_hex_string(),
            format!("21{K0}ad51b26902f601b1")
        );
    }

    #[test]
    fn liquid_structure() {
        let keys: Vec<String> = (0..15).map(|i| k(i as u8)).collect();
        let s = liquid_fedpeg(&keys, 4032).to_hex_string();
        // IF <k> <12 keys> <12> CHECKMULTISIG ELSE <csv push> CSV DROP
        // <3 keys> 3 CHECKMULTISIG ENDIF
        assert!(s.starts_with("63")); // OP_IF
        assert!(s.contains("5cae")); // <12> CHECKMULTISIG
        assert!(s.contains("67")); // OP_ELSE
        assert!(s.ends_with("53ae68")); // 3 CHECKMULTISIG ENDIF
        let k = 12 * 2 / 3 + 1;
        assert!(s.starts_with(&format!("63{:02x}", 0x50 + k)));
    }

    #[test]
    fn protocol_items_deterministic() {
        let mut a = SeededRng::new(4);
        let mut b = SeededRng::new(4);
        let ia = protocol_items(&mut a, 0);
        let ib = protocol_items(&mut b, 0);
        assert_eq!(
            serde_json::to_string(&ia).unwrap(),
            serde_json::to_string(&ib).unwrap()
        );
        assert_eq!(ia.len(), PROTOCOL_FAMILIES.len());
    }
}
