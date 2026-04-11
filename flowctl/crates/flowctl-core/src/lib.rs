//! flowctl-core: Goal-driven adaptive development engine.
//!
//! V4 architecture — engine-driven, 3-tool protocol.
//!
//! Modules:
//! - `domain` — Goal, Node, PlanVersion, ActionSpec, Escalation
//! - `storage` — Goal-scoped file stores
//! - `engine` — Orchestrator, GoalEngine, Planner, Scheduler, Escalation
//! - `context` — ContextAssembler (rich work packages)
//! - `knowledge` — Learner, Pattern, Methodology
//! - `provider` — ProviderRegistry, traits
//! - `quality` — PolicyEngine, GuardRunner
//! - `locks` — File lock primitives
//! - `graph_store` — Symbol-level code graph
//! - `ngram_index` — Trigram search index

#![forbid(unsafe_code)]

pub mod code_structure;
pub mod context;
pub mod domain;
pub mod engine;
mod fs_utils;
pub mod graph_store;
pub mod knowledge;
pub mod locks;
pub mod ngram_index;
pub mod provider;
pub mod quality;
pub mod storage;

// Re-export key types at crate root.
pub use domain::{ActionSpec, ActionType, SubmitInput, SubmitStatus};
pub use engine::Orchestrator;
pub use knowledge::LearningKind;
