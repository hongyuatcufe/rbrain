//! Validation primitives for research_run evidence checks.
//!
//! Validators inspect recorded rbrain state only. They do not execute analysis
//! code, call LLMs, or mutate pages directly.

pub mod actions;
pub mod result;
pub mod validators;

pub use actions::SuggestedAction;
pub use result::{ValidatorResult, ValidatorStatus};
pub use validators::{
    artifact_hash_present, finding_has_supporting_evidence, produced_artifact_exists,
    research_run_has_input,
};
