//! flowctl-core: Core types, ID parsing, and state machine for flowctl.
//!
//! This is a leaf crate with zero workspace dependencies. It defines the
//! fundamental data structures, enums, and validation logic used by all
//! other flowctl crates.
//!
//! V3 modules (Goal-driven engine):
//! - `domain` — Goal, Node, PlanVersion, Attempt, Escalation types
//! - `storage` — Goal-scoped file stores (TODO: Step 3)
//! - `engine` — GoalEngine, Planner, Scheduler (TODO: Step 4)
//! - `knowledge` — Learner, Pattern, Methodology (TODO: Step 4b)
//! - `provider` — ProviderRegistry, traits (TODO: Step 2c)
//! - `quality` — PolicyEngine, Guard depth (TODO: Step 4c)

#![forbid(unsafe_code)]

pub mod approvals;
pub mod changes;
pub mod code_structure;
pub mod codex_sync;
pub mod compress;
pub mod config;
pub mod dag;
pub mod domain;
pub mod engine;
pub mod error;
pub mod events;
pub mod frecency;
pub mod frontmatter;
pub mod fuzzy;
pub mod graph_store;
pub mod id;
pub mod json_store;
pub mod knowledge;
pub mod lifecycle;
pub mod ngram_index;
pub mod outputs;
pub mod patch;
pub mod paths;
pub mod pipeline;
pub mod project_context;
pub mod provider;
pub mod quality;
pub mod repo_map;
pub mod review_protocol;
pub mod state_machine;
pub mod storage;
pub mod types;

// Re-export commonly used items at crate root.
pub use approvals::FileApprovalStore;
pub use changes::{ApplyResult, Changes, ChangesApplier, Mutation};
pub use dag::TaskDag;
pub use error::{CoreError, ServiceError, ServiceResult};
pub use id::{EpicId, ParsedId, TaskId, parse_id, slugify};
pub use outputs::{OutputEntry, OutputsStore};
pub use paths::FlowPaths;
pub use pipeline::PipelinePhase;
pub use state_machine::{Status, Transition, TransitionError};
pub use types::{Epic, Evidence, Phase, Task, TaskSize};
