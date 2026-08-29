//! btc-bench core: task types, fixture schemas, the semantic equivalence
//! oracle, and the graders. No network I/O.
//!
//! The oracle is judge-free: candidate scripts must decode as Miniscript
//! in the task's script context, then prove semantic equivalence to the
//! reference by exhaustive evaluation over the task's closed atom set.
//! See DESIGN.md for the completeness argument.

pub mod answer;
pub mod grade;
pub mod oracle;
pub mod task;
pub mod truth;

pub use answer::{parse_script_answer, AnswerError};
pub use grade::{
    grade_identify, grade_optimize, grade_write, weights_for, IdentifyResult, OptimizeResult,
    Weights, WriteResult,
};
pub use oracle::{check_equivalence, Verdict};
pub use task::{
    ContextKind, Fixture, IdentifyAnswer, IdentifyFixture, KeyVar, OptimizeFixture, ParamValue,
    ResponseRecord, TaskAnswer, Tier, WriteFixture,
};
