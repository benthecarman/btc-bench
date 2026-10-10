//! Fixture and answer schemas, serialized as JSONL.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::exec::PreimageMap;

/// Script context of a write/optimize task. Determines which inner script
/// the model writes and which miniscript context decodes it.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextKind {
    /// P2SH redeemScript.
    Legacy,
    /// P2WSH witnessScript.
    SegwitV0,
    /// Taproot script-path leaf (tapscript).
    Tap,
}

impl ContextKind {
    /// Human phrase used in prompts.
    pub fn script_noun(self) -> &'static str {
        match self {
            ContextKind::Legacy => "P2SH redeem script",
            ContextKind::SegwitV0 => "P2WSH witness script",
            ContextKind::Tap => "taproot script-path leaf script (tapscript)",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Easy,
    Medium,
    Hard,
}

/// A labeled public key presented in the prompt. Hex is context-correct:
/// 33-byte compressed for legacy/segwit, 32-byte x-only for taproot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyVar {
    pub label: String,
    pub pubkey: String,
}

/// Serde helper: omit zero-valued additive fields so fixtures written
/// before the field existed stay byte-identical on regeneration.
fn is_false(v: &bool) -> bool {
    !*v
}
fn yes() -> bool {
    true
}
fn is_true(v: &bool) -> bool {
    *v
}
fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}
fn is_zero_usize(v: &usize) -> bool {
    *v == 0
}

/// Task 1: write the inner script satisfying the English spec.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WriteFixture {
    /// Prompt surface the fixture is posed with (0 = the original v1
    /// prompts and asm dialect; 2 = scaffolding removed, real Bitcoin
    /// Core asm). Recorded with the data so old runs re-grade exactly
    /// as they were posed.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub prompt_version: u32,
    /// The request leaves the script context to the model. Reference keys
    /// are compressed; tapscript answers use their corresponding x-only keys.
    #[serde(default, skip_serializing_if = "is_false")]
    pub choose_context: bool,
    /// Complete authored request, used verbatim instead of a generated wrapper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    pub id: String,
    pub tier: Tier,
    pub context: ContextKind,
    /// Deterministic English specification (from the verbalizer).
    pub spec_en: String,
    /// Verbalizer template family that produced `spec_en` (0 = the
    /// canonical benchmark phrasing).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub spec_family: u32,
    /// Boolean atom count of the policy (keys + hash preimages) — the
    /// continuous difficulty axis under the tier. 0 = unrecorded
    /// (fixture predates the field).
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub atoms: usize,
    /// Keys available to the script, labels referenced by `spec_en`.
    pub keys: Vec<KeyVar>,
    /// Concrete policy string (diagnostic aid; answer keys are the bytes).
    pub reference_policy: String,
    /// Miniscript text of the compiled reference (diagnostic aid).
    pub reference_miniscript: String,
    /// Answer key: compiled reference script bytes as hex.
    pub reference_script_hex: String,
    /// Known preimages for the policy's hash atoms (hex hash -> hex
    /// preimage). Leaks nothing: the reference script is already the
    /// answer key; lets the audit re-run the execution oracle.
    #[serde(default)]
    pub hash_preimages: PreimageMap,
}

/// Task 2: optimize the baseline script.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptimizeFixture {
    /// Prompt surface the fixture is posed with (0 = the original v1
    /// prompts and asm dialect; 2 = scaffolding removed, real Bitcoin
    /// Core asm). Recorded with the data so old runs re-grade exactly
    /// as they were posed.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub prompt_version: u32,
    pub id: String,
    pub tier: Tier,
    pub context: ContextKind,
    pub spec_en: String,
    /// Verbalizer template family for `spec_en` (0 = canonical).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub spec_family: u32,
    /// Boolean atom count of the policy; 0 = unrecorded.
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub atoms: usize,
    pub keys: Vec<KeyVar>,
    /// Deliberately naive but correct script handed to the model.
    pub baseline_script_hex: String,
    pub baseline_size: usize,
    pub baseline_weight: usize,
    /// Answer key: compiler-optimal script, weight, and size.
    pub optimal_script_hex: String,
    pub optimal_size: usize,
    pub optimal_weight: usize,
    pub reference_policy: String,
    pub reference_miniscript: String,
    /// Known preimages for the policy's hash atoms (hex hash -> hex
    /// preimage); see [`WriteFixture::hash_preimages`].
    #[serde(default)]
    pub hash_preimages: PreimageMap,
}

/// Task 4: design a full Taproot output (internal key + script tree)
/// for the English spec. The answer is a `tr(...)` descriptor, so the
/// model chooses the key path and the leaf split — the parts of
/// taproot design a single-leaf task cannot measure.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TreeFixture {
    /// Prompt surface the fixture is posed with (0 = the original v1
    /// prompts and asm dialect; 2 = scaffolding removed, real Bitcoin
    /// Core asm). Recorded with the data so old runs re-grade exactly
    /// as they were posed.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub prompt_version: u32,
    /// Complete authored request, used verbatim instead of a generated wrapper.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    pub id: String,
    pub tier: Tier,
    /// Deterministic English specification (from the verbalizer).
    pub spec_en: String,
    /// Verbalizer template family for `spec_en` (0 = canonical).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub spec_family: u32,
    /// Boolean atom count of the policy (excluding the unspendable
    /// key).
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub atoms: usize,
    /// Keys available to the design (x-only hex), labels referenced by
    /// `spec_en`.
    pub keys: Vec<KeyVar>,
    /// Provably unspendable internal key (x-only hex) offered in the
    /// prompt for policies with no key-path-worthy branch. The oracle
    /// pins this atom false on both sides before comparing.
    pub unspendable_key: String,
    /// Concrete policy string (diagnostic aid).
    pub reference_policy: String,
    /// Answer key: the compiler's tr() descriptor (`compile_tr`).
    pub reference_descriptor: String,
    /// Max satisfaction weight of the reference descriptor.
    pub reference_weight: usize,
    /// Naive single-leaf tr() over the whole policy — the weight-curve
    /// baseline a designed tree must beat.
    pub baseline_descriptor: String,
    pub baseline_weight: usize,
    /// Known preimages for the policy's hash atoms (see
    /// [`WriteFixture::hash_preimages`]).
    #[serde(default)]
    pub hash_preimages: PreimageMap,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Int(u64),
    Bool(bool),
    Str(String),
}

/// Task 3: identify what the script does.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentifyFixture {
    /// Prompt surface the fixture is posed with (0 = the original v1
    /// prompts and asm dialect; 2 = scaffolding removed, real Bitcoin
    /// Core asm). Recorded with the data so old runs re-grade exactly
    /// as they were posed.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub prompt_version: u32,
    pub id: String,
    /// Flat family label, e.g. "offered_htlc".
    pub family: String,
    /// Mechanically extracted parameters, e.g. k, n, timeout, delay.
    pub params: BTreeMap<String, ParamValue>,
    /// Raw output script (scriptPubKey) hex.
    pub spk_hex: String,
    /// RedeemScript / witnessScript hex when the family has one.
    pub inner_script_hex: Option<String>,
}

/// One diagnostic example of the complete spending contract.
/// These points do not define the safety boundary by themselves.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Requirement {
    /// Keys whose signatures are available at this point.
    pub keys: Vec<String>,
    /// Algorithm-qualified digests whose preimages are known at this point.
    #[serde(default)]
    pub hashes: Vec<String>,
    /// Chain height (for absolute timelocks).
    pub height: u32,
    /// Confirmations elapsed (for relative timelocks).
    pub age: u32,
    /// True: the output must be spendable here. False: it must not be.
    pub spendable: bool,
    /// Prose used to state the requirement in the prompt.
    pub description: String,
}

/// A judgment task: a complete spending contract with diagnostic examples.
/// Version 1 permits any equivalent encoding; spending behavior is explicit.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JudgmentFixture {
    /// Prompt surface the fixture is posed with (0 = the original v1
    /// prompts and asm dialect; 2 = scaffolding removed, real Bitcoin
    /// Core asm). Recorded with the data so old runs re-grade exactly
    /// as they were posed.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub prompt_version: u32,
    /// Version 1 states the complete spending policy in the request.
    /// Version 0 is the retired sparse-row format and must be regenerated.
    #[serde(default)]
    pub contract_version: u32,
    pub id: String,
    pub tier: Tier,
    pub context: ContextKind,
    /// The request as a person would put it.
    pub spec_en: String,
    pub keys: Vec<KeyVar>,
    /// What any acceptable design must do, and must never do.
    pub requirements: Vec<Requirement>,
    /// Private preimages for execution audits; never included in prompts.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hash_preimages: BTreeMap<String, String>,
    /// Machine-readable spending contract, fully stated by spec_en.
    /// Compared by behavior, never by compiled bytes.
    pub reference_policy: String,
}

/// Task 6: produce the witness that spends a given script in a given
/// situation (who signs, which secrets are known, the transaction's
/// nLockTime and nSequence). Graded by running the spend through
/// Bitcoin Core's consensus script verification.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SatisfyFixture {
    /// See [`WriteFixture::prompt_version`].
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub prompt_version: u32,
    pub id: String,
    pub tier: Tier,
    pub context: ContextKind,
    /// The script being spent: P2SH redeem script, P2WSH witness
    /// script, or tapleaf script (under an unspendable internal key).
    pub script_hex: String,
    /// Keys in the script. Their private keys are derived from the task
    /// id and label ([`crate::satisfy::secret_key`]), so the grader can
    /// sign for the placeholders.
    pub keys: Vec<KeyVar>,
    /// Labels of the keys that will sign.
    pub signers: Vec<String>,
    /// Secrets (hash preimages, hex) the spender knows.
    #[serde(default)]
    pub preimages: Vec<String>,
    /// The spending transaction's nLockTime and this input's nSequence.
    pub lock_time: u32,
    pub sequence: u32,
    /// Whether the policy allows the spend in this situation. When it
    /// does not, the right answer is that no witness spends.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub spendable: bool,
    /// A witness that spends, in serialization order, with `<sig:LABEL>`
    /// placeholders: the answer key, checked at generation and audit.
    /// Empty when the situation is unspendable.
    pub reference_witness: Vec<String>,
    /// The fixture the script came from.
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "task", rename_all = "lowercase")]
pub enum Fixture {
    Write(WriteFixture),
    Optimize(OptimizeFixture),
    Identify(IdentifyFixture),
    Tree(TreeFixture),
    Judgment(JudgmentFixture),
    Satisfy(SatisfyFixture),
}

/// How asm in answers is read (and embedded scripts are shown).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AsmDialect {
    /// The v1 house dialect: all-digit tokens are decimal only directly
    /// before OP_CHECKLOCKTIMEVERIFY/OP_CHECKSEQUENCEVERIFY, raw hex
    /// elsewhere; displayed opcodes use rust-bitcoin names.
    Legacy,
    /// Bitcoin Core's asm (`bitcoin-cli decodescript`): pushes of up to
    /// four bytes are decimal numbers, longer pushes hex, OP_1..OP_16 as
    /// `1`..`16`.
    Core,
}

/// First prompt version with scaffolding removed and Core asm.
pub const PROMPT_V2: u32 = 2;

pub fn dialect_for(prompt_version: u32) -> AsmDialect {
    if prompt_version >= PROMPT_V2 {
        AsmDialect::Core
    } else {
        AsmDialect::Legacy
    }
}

impl Fixture {
    pub fn prompt_version(&self) -> u32 {
        match self {
            Fixture::Write(f) => f.prompt_version,
            Fixture::Optimize(f) => f.prompt_version,
            Fixture::Identify(f) => f.prompt_version,
            Fixture::Tree(f) => f.prompt_version,
            Fixture::Judgment(f) => f.prompt_version,
            Fixture::Satisfy(f) => f.prompt_version,
        }
    }

    pub fn asm_dialect(&self) -> AsmDialect {
        dialect_for(self.prompt_version())
    }

    pub fn id(&self) -> &str {
        match self {
            Fixture::Write(f) => &f.id,
            Fixture::Optimize(f) => &f.id,
            Fixture::Identify(f) => &f.id,
            Fixture::Tree(f) => &f.id,
            Fixture::Judgment(f) => &f.id,
            Fixture::Satisfy(f) => &f.id,
        }
    }
}

/// A model's answer to a write/optimize task: one script, hex or asm.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptAnswer {
    pub script: String,
}

/// A model's answer to an identify task: the family label, nothing
/// else (identify is label-only by design; fixture params are
/// metadata). Extra fields in stored answers are ignored on parse.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentifyAnswer {
    pub label: String,
}

/// A model's answer to a tree task: one `tr(...)` descriptor.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DescriptorAnswer {
    pub descriptor: String,
}

/// A model's answer to a satisfy task: witness items in serialization
/// order (hex, `""` for empty, `<sig:LABEL>` for a signature).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WitnessAnswer {
    #[serde(default)]
    pub witness: Vec<String>,
    /// The answer that no witness can spend in this situation.
    #[serde(default, skip_serializing_if = "is_false")]
    pub unspendable: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "task", rename_all = "lowercase")]
pub enum TaskAnswer {
    Script(ScriptAnswer),
    Identify(IdentifyAnswer),
    Descriptor(DescriptorAnswer),
    Witness(WitnessAnswer),
}

/// One line of a responses JSONL file consumed by the grader.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResponseRecord {
    pub task_id: String,
    pub answer: TaskAnswer,
    /// Raw model output, kept for auditing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    /// Provider-reported finish reason (stop, length, tool_calls, ...),
    /// when the transport surfaced one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// Provider-reported completion tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<i64>,
    /// Diagnostic tool calls used (tool-assisted runs only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<u32>,
}
