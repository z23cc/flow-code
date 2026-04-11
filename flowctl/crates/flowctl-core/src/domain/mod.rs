//! V3 domain types — Goal-driven adaptive engine.
//!
//! These types supersede the V1 Epic + Pipeline model (see ADR-011).

pub mod escalation;
pub mod goal;
pub mod node;
pub mod plan;

pub use escalation::*;
pub use goal::*;
pub use node::*;
pub use plan::*;
