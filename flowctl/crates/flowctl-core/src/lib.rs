//! flowctl-core: Core types, ID parsing, and state machine for flowctl.
//!
//! This is a leaf crate with zero workspace dependencies. It defines the
//! fundamental data structures, enums, and validation logic used by all
//! other flowctl crates.

pub mod error;
pub mod id;
pub mod state_machine;
pub mod types;

// Re-export commonly used items at crate root.
pub use error::CoreError;
pub use id::{parse_id, slugify, EpicId, ParsedId, TaskId};
pub use state_machine::{Status, Transition, TransitionError};
pub use types::{Epic, Evidence, Phase, Task};
