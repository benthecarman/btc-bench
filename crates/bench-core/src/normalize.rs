//! Witness-preserving rewrites of common hand-written script idioms
//! into the encoding Miniscript decodes.
//!
//! The decode gate is the only way the oracle can read a script's
//! meaning, but correct scripts written the textbook way often fall
//! outside Miniscript's exact encoding (`<n> OP_CHECKLOCKTIMEVERIFY
//! OP_DROP` from BIP65 is the common case). Scoring those as wrong
//! punishes working scripts. Each rule below rewrites one idiom into the
//! Miniscript form, and the rewritten script then goes through the
//! unchanged oracle. The grader only uses the rewrite when the
//! submitted script itself does not decode.
//!
//! Soundness rests on each rule, not on an opcode model: every rule
//! either preserves execution for every witness, or provably preserves
//! the set of resource assignments (signers, revealed preimages, chain
//! state) that can spend. A rewrite never makes a script spendable by
//! someone the submitted script excludes. Rules:
//!
//! 1. `<n> OP_CLTV|OP_CSV OP_DROP` → `<n> OP_CLTV|OP_CSV OP_VERIFY`,
//!    for a literal, minimally encoded `n > 0`. The timelock opcodes
//!    leave `n` on the stack, and `OP_VERIFY` on a nonzero value is
//!    `OP_DROP`: identical execution for every witness.
//! 2. A `multi_a`-shaped chain `<k1> OP_CHECKSIG (<ki> OP_CHECKSIGADD)+`
//!    followed by `<k> OP_GREATERTHANOREQUAL` or `<k> OP_EQUAL`
//!    (optionally `OP_VERIFY`, or the `…VERIFY` opcode) becomes
//!    `<k> OP_NUMEQUAL[VERIFY]`; a chain ending the script, or followed
//!    by `OP_VERIFY`, becomes `OP_1 OP_NUMEQUAL[VERIFY]` (a nonzero
//!    count is truthy). Not an execution identity: it preserves the
//!    reachable outcome bit. Each signature slot holds either a valid
//!    signature from a key holder who chose to sign, the empty vector,
//!    or aborts the script (tapscript NULLFAIL), so the count ranges
//!    over every value from 0 to the number of available signers.
//!    "Count ≥ k" and "count = k" are therefore each reachable exactly
//!    when at least k signers are available, and false is always
//!    reachable; everything after the comparison sees only that bit.
//!    The contiguous range needs every slot to be freely choosable, so
//!    the rule does not fire when the script can copy a stack item into
//!    a slot (any duplicating opcode, or `OP_IFDUP` ahead of the
//!    chain). `OP_EQUAL` equals `OP_NUMEQUAL` here because
//!    `OP_CHECKSIGADD` pushes a minimally encoded count and `k` is
//!    required to be minimal.
//! 3. `OP_SHA256|OP_HASH256|OP_RIPEMD160|OP_HASH160 <digest>
//!    OP_EQUAL[VERIFY]` without a preceding `OP_SIZE <32>
//!    OP_EQUALVERIFY` gets that check inserted. The rewrite only rejects
//!    preimages that are not 32 bytes; the holder's preimage is 32 bytes
//!    (fixture preimages always are), and finding any other preimage of
//!    the digest breaks the same preimage resistance the oracle already
//!    assumes. The witness is unchanged.
//!
//! Rules 2 and 3 never shorten the script, and rule 1 keeps its length,
//! so a rewrite that clears Miniscript's resource checks implies the
//! submitted script clears consensus limits too. Unknown opcodes, bad
//! pushes and anything unmatched are copied byte for byte: the rewrite
//! touches only the opcodes named above, so non-minimal pushes and
//! other encoding problems still fail the decode gate exactly as
//! before.

use bitcoin::opcodes::all::*;
use bitcoin::opcodes::Opcode;
use bitcoin::script::{Instruction, Script, ScriptBuf};

use crate::task::ContextKind;

/// One parsed instruction with its byte range in the source script.
struct Ins<'a> {
    ins: Instruction<'a>,
    raw: &'a [u8],
}

fn op(i: &Ins<'_>) -> Option<Opcode> {
    match i.ins {
        Instruction::Op(o) => Some(o),
        Instruction::PushBytes(_) => None,
    }
}

fn is_op(i: Option<&Ins<'_>>, o: Opcode) -> bool {
    i.and_then(op) == Some(o)
}

/// A literal, minimally encoded script number (up to 5 bytes, the
/// timelock opcodes' operand size). None for anything else.
fn number(i: &Ins<'_>) -> Option<i64> {
    match i.ins {
        Instruction::Op(o) => {
            let b = o.to_u8();
            if b == OP_PUSHNUM_NEG1.to_u8() {
                Some(-1)
            } else if (OP_PUSHNUM_1.to_u8()..=OP_PUSHNUM_16.to_u8()).contains(&b) {
                Some(i64::from(b - OP_PUSHNUM_1.to_u8() + 1))
            } else {
                None
            }
        }
        Instruction::PushBytes(p) => {
            let b = p.as_bytes();
            if b.is_empty() || b.len() > 5 {
                return None;
            }
            // Minimal: no redundant sign byte, and values a single
            // opcode can push must not use a data push.
            let last = b[b.len() - 1];
            if last & 0x7f == 0 && (b.len() == 1 || b[b.len() - 2] & 0x80 == 0) {
                return None;
            }
            let mut v: i64 = 0;
            for (n, &byte) in b.iter().enumerate() {
                v |= i64::from(byte) << (8 * n);
            }
            if last & 0x80 != 0 {
                v &= !(0x80_i64 << (8 * (b.len() - 1)));
                v = -v;
            }
            if b.len() == 1 && (-1..=16).contains(&v) {
                return None;
            }
            // The raw bytes must be the push's own minimal opcode.
            (i.raw.len() == b.len() + 1).then_some(v)
        }
    }
}

fn is_xonly_push(i: Option<&Ins<'_>>) -> bool {
    matches!(i.map(|i| &i.ins), Some(Instruction::PushBytes(p)) if p.len() == 32)
}

/// Copies a stack item onto the top (or deeper) of the stack.
fn duplicates(o: Opcode) -> bool {
    [
        OP_DUP, OP_2DUP, OP_3DUP, OP_OVER, OP_2OVER, OP_PICK, OP_TUCK,
    ]
    .contains(&o)
}

fn digest_len(o: Opcode) -> Option<usize> {
    if o == OP_SHA256 || o == OP_HASH256 {
        Some(32)
    } else if o == OP_RIPEMD160 || o == OP_HASH160 {
        Some(20)
    } else {
        None
    }
}

/// Apply the rewrites. None when no rule fired or the script does not
/// parse into instructions (a truncated push, for example).
pub fn normalize(script: &Script) -> Option<ScriptBuf> {
    let bytes = script.as_bytes();
    let mut starts = Vec::new();
    let mut parsed = Vec::new();
    for item in script.instruction_indices() {
        let (start, ins) = item.ok()?;
        starts.push(start);
        parsed.push(ins);
    }
    let ins: Vec<Ins<'_>> = parsed
        .into_iter()
        .enumerate()
        .map(|(n, ins)| {
            let end = starts.get(n + 1).copied().unwrap_or(bytes.len());
            Ins {
                ins,
                raw: &bytes[starts[n]..end],
            }
        })
        .collect();
    let any_dup = ins.iter().filter_map(op).any(duplicates);

    let mut out: Vec<u8> = Vec::with_capacity(bytes.len() + 8);
    let mut fired = false;
    let mut n = 0;
    while n < ins.len() {
        let cur = &ins[n];
        let next = ins.get(n + 1);

        // Rule 1: <n> CLTV|CSV DROP → <n> CLTV|CSV VERIFY.
        if number(cur).is_some_and(|v| v > 0)
            && (is_op(next, OP_CLTV) || is_op(next, OP_CSV))
            && is_op(ins.get(n + 2), OP_DROP)
        {
            out.extend_from_slice(cur.raw);
            out.extend_from_slice(ins[n + 1].raw);
            out.push(OP_VERIFY.to_u8());
            fired = true;
            n += 3;
            continue;
        }

        // Rule 2: a CHECKSIG/CHECKSIGADD chain with a threshold
        // comparison that Miniscript spells differently.
        if is_xonly_push(Some(cur)) && is_op(next, OP_CHECKSIG) && !any_dup {
            let mut end = n + 2;
            while is_xonly_push(ins.get(end)) && is_op(ins.get(end + 1), OP_CHECKSIGADD) {
                end += 2;
            }
            let ifdup_before = ins[..n].iter().filter_map(op).any(|o| o == OP_IFDUP);
            if end > n + 2 && !ifdup_before {
                let chain = &ins[n..end];
                let tail = ins.get(end);
                let k = tail.and_then(number).filter(|&k| k >= 1);
                let cmp = ins.get(end + 1).and_then(op);
                let then_verify = is_op(ins.get(end + 2), OP_VERIFY);
                // (threshold push bytes, verify form, instructions consumed after the chain)
                let rewrite: Option<(&[u8], bool, usize)> = match (k, cmp) {
                    (Some(_), Some(c)) if c == OP_GREATERTHANOREQUAL || c == OP_EQUAL => Some((
                        tail.expect("k parsed").raw,
                        then_verify,
                        2 + usize::from(then_verify),
                    )),
                    (Some(_), Some(c)) if c == OP_EQUALVERIFY => {
                        Some((tail.expect("k parsed").raw, true, 2))
                    }
                    _ if tail.is_none() => Some((&[0x51], false, 0)),
                    _ if is_op(tail, OP_VERIFY) => Some((&[0x51], true, 1)),
                    _ => None,
                };
                if let Some((k_raw, verify, consumed)) = rewrite {
                    for c in chain {
                        out.extend_from_slice(c.raw);
                    }
                    out.extend_from_slice(k_raw);
                    out.push(
                        if verify {
                            OP_NUMEQUALVERIFY
                        } else {
                            OP_NUMEQUAL
                        }
                        .to_u8(),
                    );
                    fired = true;
                    n = end + consumed;
                    continue;
                }
            }
        }

        // Rule 3: hash equality without the 32-byte size check.
        if let Some(len) = op(cur).and_then(digest_len) {
            let digest =
                matches!(next.map(|i| &i.ins), Some(Instruction::PushBytes(p)) if p.len() == len);
            let equal = is_op(ins.get(n + 2), OP_EQUAL) || is_op(ins.get(n + 2), OP_EQUALVERIFY);
            let checked = n >= 3
                && is_op(ins.get(n - 3), OP_SIZE)
                && ins[n - 2].raw == [0x01, 0x20]
                && is_op(ins.get(n - 1), OP_EQUALVERIFY);
            if digest && equal && !checked {
                out.extend_from_slice(&[OP_SIZE.to_u8(), 0x01, 0x20, OP_EQUALVERIFY.to_u8()]);
                for c in &ins[n..n + 3] {
                    out.extend_from_slice(c.raw);
                }
                fired = true;
                n += 3;
                continue;
            }
        }

        out.extend_from_slice(cur.raw);
        n += 1;
    }
    fired.then(|| ScriptBuf::from_bytes(out))
}

/// The script the oracle should read for a candidate in one context:
/// the candidate itself when it decodes, else its rewrite when that
/// decodes, else the candidate (so decode errors always describe the
/// submitted bytes).
#[derive(Clone, Debug)]
pub struct Decodable {
    pub script: ScriptBuf,
    /// The rewrite was needed to decode.
    pub normalized: bool,
}

pub fn decodable(kind: ContextKind, candidate: &ScriptBuf) -> Decodable {
    if !crate::oracle::decodes_in_context(kind, candidate) {
        if let Some(rewritten) = normalize(candidate.as_script()) {
            if crate::oracle::decodes_in_context(kind, &rewritten) {
                return Decodable {
                    script: rewritten,
                    normalized: true,
                };
            }
        }
    }
    Decodable {
        script: candidate.clone(),
        normalized: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::answer::parse_script_answer;

    const A: &str = "8dbde3dfe6d2bd25bb577264fbe5dbf2ae62596f9bed580ca4f95a73f4ed0d55";
    const B: &str = "746a23c9785e0973d8c2bcf6c01203ca235a71bc65f3bc5f1e796c43be40dc80";
    const C: &str = "50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0";
    const CA: &str = "0305fb297329fc11a2a06c96dd9d816dbc4ae9f5a765a216be9ec44bf5c72c4aa3";
    const CB: &str = "025569df718d5230fe97d56eddec096139f96a6e6671258bb6f985a7734f98451c";
    const H20: &str = "376c98b536b360076ed3f0c0bbc7df58bf28ebb7";

    fn asm(s: &str) -> ScriptBuf {
        parse_script_answer(s).expect("test asm parses")
    }

    fn norm(s: &str) -> Option<ScriptBuf> {
        normalize(asm(s).as_script())
    }

    /// The rewrite of `input` is exactly `want`.
    fn rewrites_to(input: &str, want: &str) {
        assert_eq!(norm(input), Some(asm(want)), "{input}");
    }

    #[test]
    fn timelock_drop_becomes_verify() {
        rewrites_to(
            &format!("OP_IF 870375 OP_CHECKLOCKTIMEVERIFY OP_DROP {CB} OP_ELSE {CA} OP_ENDIF OP_CHECKSIG"),
            &format!("OP_IF 870375 OP_CHECKLOCKTIMEVERIFY OP_VERIFY {CB} OP_ELSE {CA} OP_ENDIF OP_CHECKSIG"),
        );
        rewrites_to(
            &format!("144 OP_CHECKSEQUENCEVERIFY OP_DROP {CA} OP_CHECKSIG"),
            &format!("144 OP_CHECKSEQUENCEVERIFY OP_VERIFY {CA} OP_CHECKSIG"),
        );
    }

    #[test]
    fn timelock_rule_needs_a_positive_literal() {
        // OP_VERIFY fails on 0 where OP_DROP would not: not an identity.
        assert!(norm(&format!(
            "0 OP_CHECKLOCKTIMEVERIFY OP_DROP {CA} OP_CHECKSIG"
        ))
        .is_none());
        // OP_1NEGATE OP_CSV OP_DROP.
        let s = ScriptBuf::from_hex("4fb275").unwrap();
        assert!(normalize(&s).is_none());
        // A witness-supplied locktime is not a literal.
        assert!(norm(&format!("OP_CHECKLOCKTIMEVERIFY OP_DROP {CA} OP_CHECKSIG")).is_none());
    }

    #[test]
    fn non_minimal_numbers_are_not_literals() {
        // 0x0100 is a non-minimal encoding of 1.
        let s = ScriptBuf::from_hex("020100b175").unwrap();
        assert!(normalize(&s).is_none());
        // A data push of a value OP_1..OP_16 can push.
        let s = ScriptBuf::from_hex("0105b175").unwrap();
        assert!(normalize(&s).is_none());
    }

    #[test]
    fn checksigadd_threshold_forms() {
        let base = format!("{A} OP_CHECKSIG {B} OP_CHECKSIGADD {C} OP_CHECKSIGADD");
        for (tail, want) in [
            ("OP_2 OP_GREATERTHANOREQUAL", "OP_2 OP_NUMEQUAL"),
            (
                "OP_2 OP_GREATERTHANOREQUAL OP_VERIFY",
                "OP_2 OP_NUMEQUALVERIFY",
            ),
            ("OP_2 OP_EQUAL", "OP_2 OP_NUMEQUAL"),
            ("OP_2 OP_EQUALVERIFY", "OP_2 OP_NUMEQUALVERIFY"),
            ("", "OP_1 OP_NUMEQUAL"),
            ("OP_VERIFY", "OP_1 OP_NUMEQUALVERIFY"),
        ] {
            rewrites_to(&format!("{base} {tail}"), &format!("{base} {want}"));
        }
        // Already Miniscript: nothing to do.
        assert!(norm(&format!("{base} OP_2 OP_NUMEQUAL")).is_none());
    }

    #[test]
    fn checksigadd_rule_refuses_copied_slots() {
        // A duplicated stack item can force a slot's value, so the
        // count range is no longer contiguous: no rewrite.
        let s = format!("OP_DUP {A} OP_CHECKSIG {B} OP_CHECKSIGADD OP_1 OP_GREATERTHANOREQUAL");
        assert!(norm(&s).is_none());
        let s = format!("OP_IFDUP OP_DROP {A} OP_CHECKSIG {B} OP_CHECKSIGADD");
        assert!(norm(&s).is_none());
        // k = 0 would make the threshold trivially satisfiable.
        let s = format!("{A} OP_CHECKSIG {B} OP_CHECKSIGADD OP_0 OP_GREATERTHANOREQUAL");
        assert!(norm(&s).is_none());
        // A 33-byte key in tapscript is an unknown key type that any
        // non-empty signature satisfies: not a chain slot.
        let s = format!("{A} OP_CHECKSIG {CB} OP_CHECKSIGADD OP_2 OP_EQUALVERIFY");
        assert!(norm(&s).is_none());
    }

    #[test]
    fn hash_gets_size_check_once() {
        // OP_SIZE <0x20> OP_EQUALVERIFY, Miniscript's encoding.
        let checked = ScriptBuf::from_hex(&format!("82012088a914{H20}87")).unwrap();
        assert_eq!(
            norm(&format!("OP_HASH160 {H20} OP_EQUAL")),
            Some(checked.clone())
        );
        assert!(normalize(&checked).is_none());
        // Wrong digest length for the hash: not this idiom.
        assert!(norm(&format!("OP_SHA256 {H20} OP_EQUAL")).is_none());
    }

    #[test]
    fn decodable_prefers_the_submitted_script() {
        // or_d(pk(A), and_v(v:pk(B), after(n))) decodes as written.
        let ms = asm(&format!(
            "{CA} OP_CHECKSIG OP_IFDUP OP_NOTIF {CB} OP_CHECKSIGVERIFY 870375 OP_CHECKLOCKTIMEVERIFY OP_ENDIF"
        ));
        let d = decodable(ContextKind::SegwitV0, &ms);
        assert!(!d.normalized);
        assert_eq!(d.script, ms);
        // The BIP65 idiom needs the rewrite.
        let hand = asm(&format!(
            "OP_IF 870375 OP_CHECKLOCKTIMEVERIFY OP_DROP {CB} OP_ELSE {CA} OP_ENDIF OP_CHECKSIG"
        ));
        let d = decodable(ContextKind::SegwitV0, &hand);
        assert!(d.normalized);
        // A script no rule rescues stays as submitted, so its decode
        // error describes the submitted bytes.
        let broken = asm(&format!("870375 OP_CHECKLOCKTIMEVERIFY {CA} OP_CHECKSIG"));
        let d = decodable(ContextKind::SegwitV0, &broken);
        assert!(!d.normalized);
        assert_eq!(d.script, broken);
    }
}
