//! flowctl-core: Core types, ID parsing, and state machine for flowctl.
//!
//! This is a leaf crate with zero workspace dependencies. It defines the
//! fundamental data structures, enums, and validation logic used by all
//! other flowctl crates.

pub mod approvals;
pub mod dag;
pub mod error;
pub mod frontmatter;
pub mod id;
pub mod outputs;
pub mod review_protocol;
pub mod state_machine;
pub mod task_profile;
pub mod types;

// Re-export commonly used items at crate root.
pub use dag::TaskDag;
pub use error::CoreError;
pub use id::{parse_id, slugify, EpicId, ParsedId, TaskId};
pub use outputs::OutputEntry;
pub use state_machine::{Status, Transition, TransitionError};
pub use types::{Epic, Evidence, Phase, Task};
