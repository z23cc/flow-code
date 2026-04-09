//! flowctl-core: Core types, ID parsing, and state machine for flowctl.
//!
//! This is a leaf crate with zero workspace dependencies. It defines the
//! fundamental data structures, enums, and validation logic used by all
//! other flowctl crates.

#![forbid(unsafe_code)]

pub mod approvals;
pub mod changes;
pub mod code_structure;
pub mod codex_sync;
pub mod events;
pub mod frecency;
pub mod fuzzy;
pub mod graph_store;
pub mod compress;
pub mod config;
pub mod dag;
pub mod error;
pub mod frontmatter;
pub mod id;
pub mod json_store;
pub mod lifecycle;
pub mod ngram_index;
pub mod outputs;
pub mod patch;
pub mod pipeline;
pub mod project_context;
pub mod repo_map;
pub mod review_protocol;
pub mod state_machine;
pub mod types;

// Re-export commonly used items at crate root.
pub use changes::{Changes, ChangesApplier, ApplyResult, Mutation};
pub use dag::TaskDag;
pub use error::{CoreError, ServiceError, ServiceResult};
pub use id::{parse_id, slugify, EpicId, ParsedId, TaskId};
pub use outputs::{OutputEntry, OutputsStore};
pub use pipeline::PipelinePhase;
pub use state_machine::{Status, Transition, TransitionError};
pub use types::{Epic, Evidence, Phase, Task, TaskSize};
pub use approvals::FileApprovalStore;
