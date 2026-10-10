//! Human-style asm rendering: like Bitcoin Core asm, but minimal
//! number pushes are rendered as the decimal value a person would
//! write (`405 OP_CSV`), not raw push bytes (`OP_PUSHBYTES_2 9501`).
//! Pubkeys, hashes, and other data stay hex. The answer parser accepts
//! both dialects, so models may echo either.

use bitcoin::blockdata::opcodes::all;
use bitcoin::script::Instruction;

/// Render a script as human-style asm. Number pushes are decimal ONLY
/// when immediately followed by `OP_CSV` or `OP_CLTV` — the timelock
/// arguments a human writes as values (`144 OP_CSV`). Every other push
/// (pubkeys, hashes, protocol magic like the P2A blob or the ord
/// envelope) stays hex even when the bytes happen to parse as a
/// minimal CScriptNum, because in those positions the bytes are data,
/// not numbers.
pub fn to_human_asm(script: &bitcoin::Script) -> String {
    let ins: Vec<Option<Instruction<'_>>> = script.instructions().map(|r| r.ok()).collect();
    let mut out = String::new();
    let mut first = true;
    for (i, item) in ins.iter().enumerate() {
        let part = match item {
            Some(Instruction::Op(op)) => {
                if op.to_u8() == all::OP_PUSHBYTES_0.to_u8() {
                    "OP_0".to_string()
                } else {
                    format!("{op}")
                }
            }
            Some(Instruction::PushBytes(bytes)) => {
                // The empty push (opcode 0x00 / OP_0) is classified as
                // PushBytes(&[]) by the instruction iterator, not as an
                // Op. Render it as OP_0 explicitly.
                if bytes.is_empty() {
                    "OP_0".to_string()
                } else {
                    let numeric_context = matches!(
                        ins.get(i + 1),
                        Some(Some(Instruction::Op(next)))
                            if next.to_u8() == all::OP_CSV.to_u8()
                                || next.to_u8() == all::OP_CLTV.to_u8()
                    );
                    match if numeric_context {
                        decode_minimal_num(bytes.as_bytes())
                    } else {
                        None
                    } {
                        Some(v) => format!("{v}"),
                        None => bytes
                            .as_bytes()
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<String>(),
                    }
                }
            }
            None => continue,
        };
        if !first {
            out.push(' ');
        }
        first = false;
        out.push_str(&part);
    }
    out
}

/// Decode a byte string as CScriptNum if it is the MINIMAL encoding of
/// that value (so semantic data that happens to parse as a number is
/// not mangled) and it fits i64.
fn decode_minimal_num(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() || bytes.len() > 4 {
        return None;
    }
    // Sign-magnitude little-endian.
    let negative = bytes.last().expect("nonempty") & 0x80 != 0;
    let mut magnitude: u64 = 0;
    for (i, b) in bytes.iter().enumerate() {
        let b = if i + 1 == bytes.len() && negative {
            b & 0x7f
        } else {
            *b
        };
        magnitude |= (b as u64) << (8 * i);
    }
    let value = if negative {
        -(magnitude as i128)
    } else {
        magnitude as i128
    };
    if value > i64::MAX as i128 || value < i64::MIN as i128 {
        return None;
    }
    // Minimality: the canonical encoding of the decoded value must be
    // byte-identical.
    if encode_script_num(value as i64) != bytes {
        return None;
    }
    Some(value as i64)
}

/// Canonical minimal CScriptNum encoding.
fn encode_script_num(v: i64) -> Vec<u8> {
    if v == 0 {
        return Vec::new();
    }
    let neg = v < 0;
    let mut abs = v.unsigned_abs();
    let mut bytes = Vec::new();
    while abs > 0 {
        bytes.push((abs & 0xff) as u8);
        abs >>= 8;
    }
    if bytes.last().expect("nonzero") & 0x80 != 0 {
        bytes.push(if neg { 0x80 } else { 0x00 });
    } else if neg {
        *bytes.last_mut().expect("nonzero") |= 0x80;
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::script::PushBytesBuf;
    use bitcoin::ScriptBuf;

    fn script(pushes: &[&[u8]], ops: &[bitcoin::blockdata::opcodes::Opcode]) -> ScriptBuf {
        let mut b = bitcoin::script::Builder::new();
        for p in pushes {
            b = b.push_slice(PushBytesBuf::try_from(p.to_vec()).unwrap());
        }
        for op in ops {
            b = b.push_opcode(*op);
        }
        b.into_script()
    }

    #[test]
    fn numbers_render_decimal_before_timelocks() {
        // 405 = 0x0195 -> minimal bytes 95 01, followed by OP_CSV.
        let s = script(&[&[0x95, 0x01]], &[all::OP_CSV]);
        assert_eq!(to_human_asm(s.as_script()), "405 OP_CSV");
    }

    #[test]
    fn data_pushes_stay_hex_outside_timelock_context() {
        // The P2A blob 4e73 parses as minimal CScriptNum 29518 but is
        // data; it must never render as a decimal.
        let s = script(&[&[0x4e, 0x73]], &[]);
        assert_eq!(to_human_asm(s.as_script()), "4e73");
        // Same for the ordinals envelope magic "ord".
        let s = script(&[&[0x6f, 0x72, 0x64]], &[]);
        assert_eq!(to_human_asm(s.as_script()), "6f7264");
    }

    #[test]
    fn small_numbers_via_pushbytes() {
        // 144 = 90 00 minimal (high bit of 0x90 forces a sign byte).
        let s = script(&[&[0x90, 0x00]], &[all::OP_CSV]);
        assert_eq!(to_human_asm(s.as_script()), "144 OP_CSV");
        // Same push NOT in timelock context stays hex.
        let s = script(&[&[0x90, 0x00]], &[all::OP_DROP]);
        assert_eq!(to_human_asm(s.as_script()), "9000 OP_DROP");
    }

    #[test]
    fn pubkeys_and_hashes_stay_hex() {
        let key = vec![0x02u8; 33];
        let hash = vec![0xabu8; 20];
        let s = script(&[&key, &hash], &[all::OP_CHECKSIG]);
        let asm = to_human_asm(s.as_script());
        let key_hex = key.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert!(asm.contains(&key_hex));
        assert!(asm.contains(&hash.iter().map(|b| format!("{b:02x}")).collect::<String>()));
    }

    #[test]
    fn nonminimal_numbers_stay_hex() {
        // 7 encoded non-minimally as 07 00.
        let s = script(&[&[0x07, 0x00]], &[all::OP_DROP]);
        assert_eq!(to_human_asm(s.as_script()), "0700 OP_DROP");
    }

    #[test]
    fn round_trip_with_answer_parser() {
        use crate::answer::parse_script_answer;
        let s = script(&[&[0x95, 0x01]], &[all::OP_CSV]);
        let asm = to_human_asm(s.as_script());
        let parsed = parse_script_answer(&asm).expect("parses");
        assert_eq!(parsed, s);
    }
}

/// Render a script exactly as Bitcoin Core's asm (`ScriptToAsmStr`,
/// what `bitcoin-cli decodescript` prints): every push of up to four
/// bytes is its decimal value (as `CScriptNum`, sign-magnitude), longer
/// pushes are hex, `OP_0`/`OP_1NEGATE`/`OP_1`..`OP_16` are `0`/`-1`/
/// `1`..`16`, other opcodes carry Core's names. A truncated push ends
/// the output with `[error]`, as in Core.
pub fn to_core_asm(script: &bitcoin::Script) -> String {
    let mut parts: Vec<String> = Vec::new();
    for item in script.instructions() {
        match item {
            Ok(Instruction::PushBytes(bytes)) => {
                let b = bytes.as_bytes();
                parts.push(if b.len() <= 4 {
                    script_num_lossy(b).to_string()
                } else {
                    b.iter().map(|x| format!("{x:02x}")).collect()
                });
            }
            Ok(Instruction::Op(op)) => parts.push(core_op_name(op)),
            Err(_) => {
                parts.push("[error]".to_string());
                break;
            }
        }
    }
    parts.join(" ")
}

/// `CScriptNum(vch, false).getint()` for a push of at most four bytes.
fn script_num_lossy(bytes: &[u8]) -> i64 {
    let Some(&last) = bytes.last() else {
        return 0;
    };
    let mut magnitude: i64 = 0;
    for (i, b) in bytes.iter().enumerate() {
        let b = if i + 1 == bytes.len() { b & 0x7f } else { *b };
        magnitude |= i64::from(b) << (8 * i);
    }
    if last & 0x80 != 0 {
        -magnitude
    } else {
        magnitude
    }
}

/// Bitcoin Core's `GetOpName` for a non-push opcode.
fn core_op_name(op: bitcoin::opcodes::Opcode) -> String {
    let b = op.to_u8();
    if b == all::OP_PUSHNUM_NEG1.to_u8() {
        "-1".to_string()
    } else if (all::OP_PUSHNUM_1.to_u8()..=all::OP_PUSHNUM_16.to_u8()).contains(&b) {
        (b - all::OP_PUSHNUM_1.to_u8() + 1).to_string()
    } else if b == all::OP_CLTV.to_u8() {
        "OP_CHECKLOCKTIMEVERIFY".to_string()
    } else if b == all::OP_CSV.to_u8() {
        "OP_CHECKSEQUENCEVERIFY".to_string()
    } else if (0xbb..=0xfe).contains(&b) {
        // Unassigned in Core (OP_SUCCESS in tapscript): GetOpName's default.
        "OP_UNKNOWN".to_string()
    } else {
        format!("{op}")
    }
}

#[cfg(test)]
mod core_asm_tests {
    use super::*;
    use crate::answer::{parse_script_answer_in, AsmDialect};
    use bitcoin::ScriptBuf;

    /// The `asm` field `bitcoin-cli decodescript` prints for these
    /// scripts (checked against Bitcoin Core 31.1).
    #[test]
    fn matches_bitcoin_core() {
        let s = |hex: &str| ScriptBuf::from_hex(hex).unwrap();
        // OP_SIZE <0x20> OP_EQUALVERIFY OP_SHA256 <32 bytes> OP_EQUAL
        let h = "11".repeat(32);
        assert_eq!(
            to_core_asm(&s(&format!("82012088a820{h}87"))),
            format!("OP_SIZE 32 OP_EQUALVERIFY OP_SHA256 {h} OP_EQUAL")
        );
        // OP_2 <33> <33> OP_2 OP_CHECKMULTISIG
        let k = format!("02{}", "22".repeat(32));
        assert_eq!(
            to_core_asm(&s(&format!("5221{k}21{k}52ae"))),
            format!("2 {k} {k} 2 OP_CHECKMULTISIG")
        );
        // 870375 OP_CLTV OP_DROP, 144 OP_CSV, OP_0, OP_1NEGATE, -5
        assert_eq!(
            to_core_asm(&s("03e7470db175029000b2004f0185")),
            "870375 OP_CHECKLOCKTIMEVERIFY OP_DROP 144 OP_CHECKSEQUENCEVERIFY 0 -1 -5"
        );
        // A non-minimal push prints its value, as in Core.
        assert_eq!(to_core_asm(&s("020100")), "1");
        // Truncated push.
        assert_eq!(to_core_asm(&s("0302")), "[error]");
        // Opcode names, including Core's OP_UNKNOWN for unassigned bytes.
        assert_eq!(
            to_core_asm(&s("bab0bbfeff6150626566898a6a")),
            "OP_CHECKSIGADD OP_NOP1 OP_UNKNOWN OP_UNKNOWN OP_INVALIDOPCODE OP_NOP \
             OP_RESERVED OP_VER OP_VERIF OP_VERNOTIF OP_RESERVED1 OP_RESERVED2 OP_RETURN"
        );
    }

    /// Core asm of a minimally encoded script parses back to the same
    /// bytes in the Core dialect.
    #[test]
    fn round_trips_through_the_core_dialect() {
        let k = format!("03{}", "ab".repeat(32));
        let x = "cd".repeat(32);
        for hex in [
            format!("21{k}ac736421{k}ad03e7470db168"),
            format!("82012088a820{}87", "11".repeat(32)),
            format!("20{x}ac20{x}ba52a2"),
            format!("2102{}ac0300f0ffb2", "00".repeat(32)),
            "00".to_string(),
            "4f9c".to_string(),
        ] {
            let script = ScriptBuf::from_hex(&hex).unwrap();
            let asm = to_core_asm(&script);
            let back = parse_script_answer_in(&format!("{asm} OP_NOP"), AsmDialect::Core)
                .unwrap_or_else(|e| panic!("{asm}: {e:?}"));
            let mut want = script.to_bytes();
            want.push(0x61);
            assert_eq!(back.to_bytes(), want, "{asm}");
        }
    }
}
