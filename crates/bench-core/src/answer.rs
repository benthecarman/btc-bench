//! Parse a model's script answer: hex or Bitcoin Core dialect asm.
//!
//! Disambiguation rule: a string containing an `OP_` token parses as asm;
//! everything else parses as hex. A push-only asm string with no `OP_`
//! token is byte-identical to its hex form, so treating it as hex is
//! lossless. Asm grammar (strict; anything else is malformed):
//! - `OP_*` opcode names
//! - bare even-length hex chunks are data pushes, minimally encoded
//! - all-decimal tokens are integer pushes, minimally encoded
//! - `OP_PUSHDATA1/2/4` followed by one hex chunk is an explicit push
//! - `[hex]` bracket form is accepted for data pushes

use bitcoin::blockdata::opcodes::Opcode;
use bitcoin::ScriptBuf;

/// Parse failures with the detail a model (or human) needs to repair
/// the answer: token positions, offending characters, and a did-you-mean
/// suggestion for near-miss opcode names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnswerError {
    Empty,
    /// A non-hex character in an otherwise hex answer.
    InvalidHexChar {
        context: String,
        pos: usize,
        ch: char,
    },
    /// Whole-answer hex with an odd number of characters.
    OddLengthHex {
        len: usize,
    },
    /// An asm hex chunk with an odd number of characters.
    OddLengthChunk {
        chunk: String,
        index: usize,
    },
    UnknownToken {
        token: String,
        index: usize,
    },
    BadInteger {
        token: String,
    },
    /// OP_PUSHDATA{1,2,4} not followed by a data chunk.
    MissingPushData {
        op: &'static str,
        index: usize,
    },
    MalformedPush {
        detail: String,
    },
}

// Shim: bitcoin 0.32 names the small-int pushes OP_PUSHNUM_n, the empty
// push OP_PUSHBYTES_0, and the timelock NOPs OP_CLTV/OP_CSV.
mod all {
    pub use bitcoin::blockdata::opcodes::all::*;
}

impl core::fmt::Display for AnswerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AnswerError::Empty => write!(
                f,
                "the answer is empty; submit the script as hex or Bitcoin Core asm"
            ),
            AnswerError::InvalidHexChar { context, pos, ch } => {
                let start = pos.saturating_sub(8);
                let end = (pos + 8).min(context.len());
                write!(
                    f,
                    "invalid hex character '{ch}' at position {pos} (context: \"...{}{}{}...\")",
                    &context[start..*pos],
                    ch,
                    &context[pos + ch.len_utf8()..end],
                )
            }
            AnswerError::OddLengthHex { len } => write!(
                f,
                "hex has an odd number of characters ({len}); every byte is exactly two hex characters"
            ),
            AnswerError::OddLengthChunk { chunk, index } => write!(
                f,
                "asm token #{index} is a hex chunk of odd length {}: \"{}\"",
                chunk.len(),
                chunk
            ),
            AnswerError::UnknownToken { token, index } => match suggest_opcode(token) {
                Some(best) => write!(
                    f,
                    "unknown asm token #{index}: \"{token}\" (did you mean {best}?)"
                ),
                None => write!(f, "unknown asm token #{index}: \"{token}\""),
            },
            AnswerError::BadInteger { token } => write!(
                f,
                "\"{token}\" is not a parseable integer (must fit in 64 signed bits)"
            ),
            AnswerError::MissingPushData { op, index } => write!(
                f,
                "asm token #{index} ({op}) must be followed by exactly one hex data chunk"
            ),
            AnswerError::MalformedPush { detail } => write!(f, "malformed push: {detail}"),
        }
    }
}

/// Edit distance over byte strings; inputs are short opcode names.
fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        core::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

/// Suggest the closest opcode name when the token looks like a near
/// miss of a real one (typo, hallucinated suffix, wrong case).
fn suggest_opcode(token: &str) -> Option<&'static str> {
    if !token.starts_with("OP_") {
        return None;
    }
    let mut best: Option<(usize, &'static str)> = None;
    for (name, _) in OPCODE_NAMES {
        let d = levenshtein(token, name);
        let budget = (name.len() / 4).max(1);
        if d <= budget && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, name));
        }
    }
    best.map(|(_, name)| name)
}

impl std::error::Error for AnswerError {}

/// Every opcode miniscript can emit plus the common pre-miniscript forms;
/// an asm token outside this table could not decode as miniscript anyway.
const OPCODE_NAMES: &[(&str, Opcode)] = &[
    ("OP_0", all::OP_PUSHBYTES_0),
    ("OP_PUSHDATA1", all::OP_PUSHDATA1),
    ("OP_PUSHDATA2", all::OP_PUSHDATA2),
    ("OP_PUSHDATA4", all::OP_PUSHDATA4),
    ("OP_PUSHNUM_NEG1", all::OP_PUSHNUM_NEG1),
    ("OP_PUSHNUM_1", all::OP_PUSHNUM_1),
    ("OP_PUSHNUM_2", all::OP_PUSHNUM_2),
    ("OP_PUSHNUM_3", all::OP_PUSHNUM_3),
    ("OP_PUSHNUM_4", all::OP_PUSHNUM_4),
    ("OP_PUSHNUM_5", all::OP_PUSHNUM_5),
    ("OP_PUSHNUM_6", all::OP_PUSHNUM_6),
    ("OP_PUSHNUM_7", all::OP_PUSHNUM_7),
    ("OP_PUSHNUM_8", all::OP_PUSHNUM_8),
    ("OP_PUSHNUM_9", all::OP_PUSHNUM_9),
    ("OP_PUSHNUM_10", all::OP_PUSHNUM_10),
    ("OP_PUSHNUM_11", all::OP_PUSHNUM_11),
    ("OP_PUSHNUM_12", all::OP_PUSHNUM_12),
    ("OP_PUSHNUM_13", all::OP_PUSHNUM_13),
    ("OP_PUSHNUM_14", all::OP_PUSHNUM_14),
    ("OP_PUSHNUM_15", all::OP_PUSHNUM_15),
    ("OP_PUSHNUM_16", all::OP_PUSHNUM_16),
    ("OP_1NEGATE", all::OP_PUSHNUM_NEG1),
    ("OP_1", all::OP_PUSHNUM_1),
    ("OP_2", all::OP_PUSHNUM_2),
    ("OP_3", all::OP_PUSHNUM_3),
    ("OP_4", all::OP_PUSHNUM_4),
    ("OP_5", all::OP_PUSHNUM_5),
    ("OP_6", all::OP_PUSHNUM_6),
    ("OP_7", all::OP_PUSHNUM_7),
    ("OP_8", all::OP_PUSHNUM_8),
    ("OP_9", all::OP_PUSHNUM_9),
    ("OP_10", all::OP_PUSHNUM_10),
    ("OP_11", all::OP_PUSHNUM_11),
    ("OP_12", all::OP_PUSHNUM_12),
    ("OP_13", all::OP_PUSHNUM_13),
    ("OP_14", all::OP_PUSHNUM_14),
    ("OP_15", all::OP_PUSHNUM_15),
    ("OP_16", all::OP_PUSHNUM_16),
    ("OP_NOP", all::OP_NOP),
    ("OP_IF", all::OP_IF),
    ("OP_NOTIF", all::OP_NOTIF),
    ("OP_ELSE", all::OP_ELSE),
    ("OP_ENDIF", all::OP_ENDIF),
    ("OP_VERIFY", all::OP_VERIFY),
    ("OP_RETURN", all::OP_RETURN),
    ("OP_TOALTSTACK", all::OP_TOALTSTACK),
    ("OP_FROMALTSTACK", all::OP_FROMALTSTACK),
    ("OP_2DROP", all::OP_2DROP),
    ("OP_2DUP", all::OP_2DUP),
    ("OP_3DUP", all::OP_3DUP),
    ("OP_2OVER", all::OP_2OVER),
    ("OP_2ROT", all::OP_2ROT),
    ("OP_2SWAP", all::OP_2SWAP),
    ("OP_IFDUP", all::OP_IFDUP),
    ("OP_DEPTH", all::OP_DEPTH),
    ("OP_DROP", all::OP_DROP),
    ("OP_DUP", all::OP_DUP),
    ("OP_NIP", all::OP_NIP),
    ("OP_OVER", all::OP_OVER),
    ("OP_PICK", all::OP_PICK),
    ("OP_ROLL", all::OP_ROLL),
    ("OP_ROT", all::OP_ROT),
    ("OP_SWAP", all::OP_SWAP),
    ("OP_TUCK", all::OP_TUCK),
    ("OP_SIZE", all::OP_SIZE),
    ("OP_EQUAL", all::OP_EQUAL),
    ("OP_EQUALVERIFY", all::OP_EQUALVERIFY),
    ("OP_1ADD", all::OP_1ADD),
    ("OP_1SUB", all::OP_1SUB),
    ("OP_NEGATE", all::OP_NEGATE),
    ("OP_ABS", all::OP_ABS),
    ("OP_NOT", all::OP_NOT),
    ("OP_0NOTEQUAL", all::OP_0NOTEQUAL),
    ("OP_ADD", all::OP_ADD),
    ("OP_SUB", all::OP_SUB),
    ("OP_BOOLAND", all::OP_BOOLAND),
    ("OP_BOOLOR", all::OP_BOOLOR),
    ("OP_NUMEQUAL", all::OP_NUMEQUAL),
    ("OP_NUMEQUALVERIFY", all::OP_NUMEQUALVERIFY),
    ("OP_NUMNOTEQUAL", all::OP_NUMNOTEQUAL),
    ("OP_LESSTHAN", all::OP_LESSTHAN),
    ("OP_GREATERTHAN", all::OP_GREATERTHAN),
    ("OP_LESSTHANOREQUAL", all::OP_LESSTHANOREQUAL),
    ("OP_GREATERTHANOREQUAL", all::OP_GREATERTHANOREQUAL),
    ("OP_MIN", all::OP_MIN),
    ("OP_MAX", all::OP_MAX),
    ("OP_WITHIN", all::OP_WITHIN),
    ("OP_RIPEMD160", all::OP_RIPEMD160),
    ("OP_SHA1", all::OP_SHA1),
    ("OP_SHA256", all::OP_SHA256),
    ("OP_HASH160", all::OP_HASH160),
    ("OP_HASH256", all::OP_HASH256),
    ("OP_CODESEPARATOR", all::OP_CODESEPARATOR),
    ("OP_CHECKSIG", all::OP_CHECKSIG),
    ("OP_CHECKSIGVERIFY", all::OP_CHECKSIGVERIFY),
    ("OP_CHECKMULTISIG", all::OP_CHECKMULTISIG),
    ("OP_CHECKMULTISIGVERIFY", all::OP_CHECKMULTISIGVERIFY),
    ("OP_NOP1", all::OP_NOP1),
    ("OP_CHECKLOCKTIMEVERIFY", all::OP_CLTV),
    ("OP_CLTV", all::OP_CLTV),
    ("OP_CSV", all::OP_CSV),
    ("OP_CHECKSEQUENCEVERIFY", all::OP_CSV),
    ("OP_NOP4", all::OP_NOP4),
    ("OP_NOP5", all::OP_NOP5),
    ("OP_NOP6", all::OP_NOP6),
    ("OP_NOP7", all::OP_NOP7),
    ("OP_NOP8", all::OP_NOP8),
    ("OP_NOP9", all::OP_NOP9),
    ("OP_NOP10", all::OP_NOP10),
    ("OP_CHECKSIGADD", all::OP_CHECKSIGADD),
];

fn opcode_by_name(name: &str) -> Option<Opcode> {
    let table = OPCODE_NAMES;
    table.iter().find(|(n, _)| *n == name).map(|(_, op)| *op)
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn hex_decode(s: &str) -> Result<Vec<u8>, AnswerError> {
    let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return Ok(Vec::new());
    }
    // First non-hex character, with its position for the message.
    if let Some((pos, ch)) = compact
        .char_indices()
        .find(|(_, c)| !(c.is_ascii() && hex_val(*c as u8).is_some()))
    {
        return Err(AnswerError::InvalidHexChar {
            context: compact,
            pos,
            ch,
        });
    }
    if compact.len() % 2 != 0 {
        return Err(AnswerError::OddLengthHex { len: compact.len() });
    }
    let b = compact.as_bytes();
    let mut out = Vec::with_capacity(b.len() / 2);
    for pair in b.chunks(2) {
        // Non-ascii already rejected above, so these cannot fail.
        let hi = hex_val(pair[0]).expect("checked");
        let lo = hex_val(pair[1]).expect("checked");
        out.push(hi << 4 | lo);
    }
    Ok(out)
}

fn is_hex_chunk(s: &str) -> bool {
    !s.is_empty() && s.len() % 2 == 0 && s.bytes().all(|c| hex_val(c).is_some())
}

fn is_decimal(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit())
}

/// Minimal data-push encoding, matching Bitcoin Core's `CScript::operator<<`.
fn push_data(out: &mut Vec<u8>, data: &[u8]) {
    let n = data.len();
    if n == 0 {
        out.push(all::OP_PUSHBYTES_0.to_u8());
    } else if n == 1 && (1..=16).contains(&data[0]) {
        out.push(all::OP_PUSHNUM_1.to_u8() + data[0] - 1);
    } else if n == 1 && data[0] == 0x81 {
        out.push(all::OP_PUSHNUM_NEG1.to_u8());
    } else if n <= 75 {
        out.push(n as u8);
        out.extend_from_slice(data);
    } else if n <= 255 {
        out.push(all::OP_PUSHDATA1.to_u8());
        out.push(n as u8);
        out.extend_from_slice(data);
    } else if n <= 65535 {
        out.push(all::OP_PUSHDATA2.to_u8());
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(data);
    } else {
        out.push(all::OP_PUSHDATA4.to_u8());
        out.extend_from_slice(&(n as u32).to_le_bytes());
        out.extend_from_slice(data);
    }
}

/// Minimal integer push (sign-magnitude little-endian), Core's `CScriptNum`
/// serialization, matching `Builder::push_int`.
fn push_int(out: &mut Vec<u8>, v: i64) {
    if v > 0 && v <= 16 {
        out.push(all::OP_PUSHNUM_1.to_u8() + v as u8 - 1);
    } else if v == 0 {
        out.push(all::OP_PUSHBYTES_0.to_u8());
    } else if v == -1 {
        out.push(all::OP_PUSHNUM_NEG1.to_u8());
    } else {
        let neg = v < 0;
        let mut abs = (v as i128).unsigned_abs() as u128;
        let mut bytes = Vec::new();
        while abs > 0 {
            bytes.push((abs & 0xff) as u8);
            abs >>= 8;
        }
        if bytes.last().map_or(false, |b| b & 0x80 != 0) {
            bytes.push(if neg { 0x80 } else { 0x00 });
        } else if neg {
            *bytes.last_mut().expect("nonzero value has bytes") |= 0x80;
        }
        push_data_raw(out, &bytes);
    }
}

/// Data push without the OP_0/OP_N/OP_1NEGATE shortcuts; for integer bytes.
fn push_data_raw(out: &mut Vec<u8>, data: &[u8]) {
    let n = data.len();
    if n <= 75 {
        out.push(n as u8);
        out.extend_from_slice(data);
    } else if n <= 255 {
        out.push(all::OP_PUSHDATA1.to_u8());
        out.push(n as u8);
        out.extend_from_slice(data);
    } else if n <= 65535 {
        out.push(all::OP_PUSHDATA2.to_u8());
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(data);
    } else {
        out.push(all::OP_PUSHDATA4.to_u8());
        out.extend_from_slice(&(n as u32).to_le_bytes());
        out.extend_from_slice(data);
    }
}

/// Explicit `OP_PUSHDATA{1,2,4} <hex>`: emit the opcode and a matching
/// length prefix, then the raw data.
fn push_explicit(out: &mut Vec<u8>, op: Opcode, data: &[u8]) -> Result<(), AnswerError> {
    let n = data.len();
    out.push(op.to_u8());
    match op.to_u8() {
        x if x == all::OP_PUSHDATA1.to_u8() => {
            if n > 255 {
                return Err(AnswerError::MalformedPush {
                    detail: "OP_PUSHDATA1 can carry at most 255 bytes".into(),
                });
            }
            out.push(n as u8);
        }
        x if x == all::OP_PUSHDATA2.to_u8() => {
            if n > 65535 {
                return Err(AnswerError::MalformedPush {
                    detail: "OP_PUSHDATA2 can carry at most 65535 bytes".into(),
                });
            }
            out.extend_from_slice(&(n as u16).to_le_bytes());
        }
        x if x == all::OP_PUSHDATA4.to_u8() => {
            out.extend_from_slice(&(n as u32).to_le_bytes());
        }
        _ => unreachable!("caller passes a pushdata opcode"),
    }
    out.extend_from_slice(data);
    Ok(())
}

/// Parse a script answer (hex or asm) into script bytes.
pub fn parse_script_answer(input: &str) -> Result<ScriptBuf, AnswerError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(AnswerError::Empty);
    }
    if !s.contains("OP_") {
        let bytes = hex_decode(s)?;
        return Ok(ScriptBuf::from_bytes(bytes));
    }

    let mut out: Vec<u8> = Vec::new();
    let tokens: Vec<&str> = s.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        let index = i + 1; // 1-based token number for diagnostics
        let tok = tokens[i];
        i += 1;
        let tok = tok
            .strip_prefix('[')
            .and_then(|t| t.strip_suffix(']'))
            .unwrap_or(tok);
        if let Some(rest) = tok.strip_prefix("OP_PUSHBYTES_") {
            // rust-bitcoin asm dialect: literal length-prefixed push.
            let n: usize = rest.parse().map_err(|_| AnswerError::UnknownToken {
                token: tok.to_string(),
                index,
            })?;
            if !(1..=75).contains(&n) {
                return Err(AnswerError::UnknownToken {
                    token: tok.to_string(),
                    index,
                });
            }
            let chunk = tokens.get(i).ok_or(AnswerError::MissingPushData {
                op: "OP_PUSHBYTES_n",
                index,
            })?;
            i += 1;
            let chunk = chunk
                .strip_prefix('[')
                .and_then(|t| t.strip_suffix(']'))
                .unwrap_or(chunk);
            if !is_hex_chunk(chunk) {
                return Err(AnswerError::OddLengthChunk {
                    chunk: chunk.to_string(),
                    index,
                });
            }
            let data = hex_decode(chunk)?;
            if data.len() != n {
                return Err(AnswerError::MalformedPush {
                    detail: format!(
                        "OP_PUSHBYTES_{n} declares {n} bytes but the chunk has {}",
                        data.len()
                    ),
                });
            }
            out.push(n as u8);
            out.extend_from_slice(&data);
        } else if let Some(op) = opcode_by_name(tok) {
            let is_pd = matches!(tok, "OP_PUSHDATA1" | "OP_PUSHDATA2" | "OP_PUSHDATA4");
            if is_pd {
                let chunk = tokens.get(i).ok_or(AnswerError::MissingPushData {
                    op: "OP_PUSHDATA",
                    index,
                })?;
                i += 1;
                let chunk = chunk
                    .strip_prefix('[')
                    .and_then(|t| t.strip_suffix(']'))
                    .unwrap_or(chunk);
                if !is_hex_chunk(chunk) {
                    return Err(AnswerError::OddLengthChunk {
                        chunk: chunk.to_string(),
                        index,
                    });
                }
                push_explicit(&mut out, op, &hex_decode(chunk)?)?;
            } else {
                out.push(op.to_u8());
            }
        } else if is_hex_chunk(tok) {
            push_data(&mut out, &hex_decode(tok)?);
        } else if is_decimal(tok) {
            let v: i64 = tok.parse().map_err(|_| AnswerError::BadInteger {
                token: tok.to_string(),
            })?;
            push_int(&mut out, v);
        } else {
            return Err(AnswerError::UnknownToken {
                token: tok.to_string(),
                index,
            });
        }
    }
    Ok(ScriptBuf::from_bytes(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::script::PushBytesBuf;

    fn script_ops(ops: &[bitcoin::blockdata::opcodes::Opcode], pushes: &[&[u8]]) -> ScriptBuf {
        let mut b = bitcoin::blockdata::script::Builder::new();
        for p in pushes {
            let pb = PushBytesBuf::try_from(p.to_vec()).unwrap();
            b = b.push_slice(pb);
        }
        for op in ops {
            b = b.push_opcode(*op);
        }
        b.into_script()
    }

    #[test]
    fn hex_roundtrip() {
        let script = script_ops(
            &[
                all::OP_DUP,
                all::OP_HASH160,
                all::OP_EQUALVERIFY,
                all::OP_CHECKSIG,
            ],
            &[&[0u8; 20]],
        );
        let hex = script.to_hex_string();
        let parsed = parse_script_answer(&hex).unwrap();
        assert_eq!(parsed, script);
    }

    #[test]
    fn asm_roundtrip() {
        let script = script_ops(
            &[
                all::OP_DUP,
                all::OP_HASH160,
                all::OP_EQUALVERIFY,
                all::OP_CHECKSIG,
            ],
            &[&[0u8; 20]],
        );
        let asm = script.as_script().to_asm_string();
        let parsed = parse_script_answer(&asm).unwrap();
        assert_eq!(parsed, script);
    }

    #[test]
    fn asm_with_pushdata() {
        let data = vec![0xabu8; 80]; // forces OP_PUSHDATA1 under minimal encoding
        let script = script_ops(&[all::OP_DROP], &[&data]);
        let asm = script.as_script().to_asm_string();
        let parsed = parse_script_answer(&asm).unwrap();
        assert_eq!(parsed, script);
        // Explicit PUSHDATA form parses too.
        let explicit = format!("OP_PUSHDATA1 {} OP_DROP", "ab".repeat(80));
        let parsed = parse_script_answer(&explicit).unwrap();
        assert_eq!(parsed, script);
    }

    #[test]
    fn integer_tokens() {
        let parsed = parse_script_answer("OP_1 OP_16 OP_0").unwrap();
        assert_eq!(
            parsed.as_bytes(),
            &[
                all::OP_PUSHNUM_1.to_u8(),
                all::OP_PUSHNUM_16.to_u8(),
                all::OP_PUSHBYTES_0.to_u8()
            ]
        );
        // 144 as a decimal push: sign-magnitude LE with a sign byte,
        // matching CScriptNum (0x90 0x00, not 0x90 0x01).
        let parsed = parse_script_answer("144 OP_DROP").unwrap();
        assert_eq!(parsed.as_bytes(), &[0x02, 0x90, 0x00, all::OP_DROP.to_u8()]);
    }

    #[test]
    fn error_messages_are_actionable() {
        // Unknown opcode with a near-miss name suggests the right one.
        let e = parse_script_answer("OP_DUP OP_HASH160 OP_CHECKMULTISIGSIG").unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("token #3"), "position: {msg}");
        assert!(
            msg.contains("did you mean OP_CHECKMULTISIG?"),
            "suggest: {msg}"
        );

        // Bad hex points at the offending character.
        let e = parse_script_answer("a3z9").unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("position 2"), "position: {msg}");
        assert!(msg.contains("'z'"), "char: {msg}");

        // Odd-length hex says what a byte needs.
        let e = parse_script_answer("abc").unwrap_err();
        assert!(e.to_string().contains("odd number of characters (3)"));

        // Missing push data names the opcode and token.
        let e = parse_script_answer("OP_DUP OP_PUSHDATA1").unwrap_err();
        let msg = e.to_string();
        assert!(msg.contains("token #2"), "position: {msg}");
        assert!(
            msg.contains("followed by exactly one hex data chunk"),
            "fix: {msg}"
        );
    }

    #[test]
    fn malformed() {
        assert!(parse_script_answer("").is_err());
        assert!(parse_script_answer("zz").is_err());
        assert!(parse_script_answer("OP_NOT_A_REAL_OPCODE").is_err());
        assert!(parse_script_answer("OP_DUP OP_HASH160 abc").is_err());
        assert!(parse_script_answer("OP_PUSHDATA1").is_err());
    }
}
