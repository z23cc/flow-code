//! V3 storage layer — goal-scoped file stores.
//!
//! Layout: .flow/goals/{id}/goal.json, plans/{rev}.json, attempts/{node}/{seq}.json
//! Knowledge: .flow/knowledge/learnings/, patterns/, rules/
//! Runtime: .flow/runtime/locks.json, sessions.json

pub mod attempt_store;
pub mod event_store;
pub mod goal_store;
pub mod knowledge_store;
pub mod plan_store;

pub use attempt_store::AttemptStore;
pub use event_store::EventStore;
pub use goal_store::GoalStore;
pub use knowledge_store::KnowledgeStore;
pub use plan_store::PlanStore;
