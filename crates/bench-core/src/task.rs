//! Fixture and answer schemas, serialized as JSONL.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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

/// Task 1: write the inner script satisfying the English spec.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WriteFixture {
    pub id: String,
    pub tier: Tier,
    pub context: ContextKind,
    /// Deterministic English specification (from the verbalizer).
    pub spec_en: String,
    /// Keys available to the script, labels referenced by `spec_en`.
    pub keys: Vec<KeyVar>,
    /// Concrete policy string (diagnostic aid; answer keys are the bytes).
    pub reference_policy: String,
    /// Miniscript text of the compiled reference (diagnostic aid).
    pub reference_miniscript: String,
    /// Answer key: compiled reference script bytes as hex.
    pub reference_script_hex: String,
}

/// Task 2: optimize the baseline script.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptimizeFixture {
    pub id: String,
    pub tier: Tier,
    pub context: ContextKind,
    pub spec_en: String,
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
}

impl Fixture {
    pub fn id(&self) -> &str {
        match self {
            Fixture::Write(f) => &f.id,
            Fixture::Optimize(f) => &f.id,
            Fixture::Identify(f) => &f.id,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "task", rename_all = "lowercase")]
pub enum TaskAnswer {
    Script(ScriptAnswer),
    Identify(IdentifyAnswer),
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
