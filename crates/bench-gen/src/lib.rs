//! btc-bench generators: seeded key material, policy sampler, English
//! verbalizer, naive de-optimizer, identification corpus, fixture writer.
//!
//! Everything is a deterministic function of the seed: the same seed and
//! the same dependency pins produce byte-identical fixtures.

pub mod casual;
pub mod corpus;
pub mod fixtures;
pub mod keys;
pub mod naive;
pub mod policy;
pub mod prompt;
pub mod protocol;
pub mod rng;
pub mod verbal;
