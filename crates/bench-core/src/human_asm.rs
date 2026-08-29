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
