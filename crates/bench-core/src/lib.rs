//! btc-bench core: task types, fixture schemas, the semantic equivalence
//! oracle, and the graders. No network I/O.
//!
//! The oracle is judge-free: candidate scripts must decode as Miniscript
//! in the task's script context, then prove semantic equivalence to the
//! reference by exhaustive evaluation over the task's closed atom set.
//! See DESIGN.md for the completeness argument.

pub mod answer;
pub mod exec;
pub mod grade;
pub mod human_asm;
pub mod oracle;
pub mod task;
pub mod toolbox;
pub mod truth;

pub use grade::{
    grade_identify, grade_optimize, grade_tree, grade_write, lint_report, parse_tr_answer,
    tree_agreement, weights_for, IdentifyResult, OptimizeResult, TreeResult, Weights, WriteResult,
};
pub use oracle::{
    agreement_semantic, check_equivalence, check_semantic, decodes_in_context, semantic_agreement,
    Verdict,
};
pub use task::{
    ContextKind, DescriptorAnswer, Fixture, IdentifyAnswer, IdentifyFixture, KeyVar,
    OptimizeFixture, ParamValue, ResponseRecord, TaskAnswer, Tier, TreeFixture, WriteFixture,
};

pub use exec::{execution_check, HashPreimages, PreimageMap};
