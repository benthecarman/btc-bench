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
fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}
fn is_zero_usize(v: &usize) -> bool {
    *v == 0
}

/// Task 1: write the inner script satisfying the English spec.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WriteFixture {
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "task", rename_all = "lowercase")]
pub enum Fixture {
    Write(WriteFixture),
    Optimize(OptimizeFixture),
    Identify(IdentifyFixture),
    Tree(TreeFixture),
}

impl Fixture {
    pub fn id(&self) -> &str {
        match self {
            Fixture::Write(f) => &f.id,
            Fixture::Optimize(f) => &f.id,
            Fixture::Identify(f) => &f.id,
            Fixture::Tree(f) => &f.id,
        }
    }
}

/// A model's answer to a write/optimize task: one script, hex or asm.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptAnswer {
    pub script: String,
}

/// A model's answer to an identify task.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IdentifyAnswer {
    pub label: String,
    #[serde(default)]
    pub params: BTreeMap<String, ParamValue>,
}

/// A model's answer to a tree task: one `tr(...)` descriptor.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DescriptorAnswer {
    pub descriptor: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "task", rename_all = "lowercase")]
pub enum TaskAnswer {
    Script(ScriptAnswer),
    Identify(IdentifyAnswer),
    Descriptor(DescriptorAnswer),
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
}
