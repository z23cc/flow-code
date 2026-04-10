//! Pipeline phase state machine for epic-level workflow progression.
//!
//! Phases form a linear sequence:
//! Brainstorm → Plan → PlanReview → Work → ImplReview → Close → Completed.
//! No branching — each phase has exactly one successor.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Epic-level pipeline phases (linear sequence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelinePhase {
    Brainstorm,
    Plan,
    PlanReview,
    Work,
    ImplReview,
    Close,
    Completed,
}

static ALL_PHASES: &[PipelinePhase] = &[
    PipelinePhase::Brainstorm,
    PipelinePhase::Plan,
    PipelinePhase::PlanReview,
    PipelinePhase::Work,
    PipelinePhase::ImplReview,
    PipelinePhase::Close,
];

impl PipelinePhase {
    /// Returns the next phase in the pipeline, or `None` if this is the terminal phase.
    pub fn next(&self) -> Option<PipelinePhase> {
        match self {
            PipelinePhase::Brainstorm => Some(PipelinePhase::Plan),
            PipelinePhase::Plan => Some(PipelinePhase::PlanReview),
            PipelinePhase::PlanReview => Some(PipelinePhase::Work),
            PipelinePhase::Work => Some(PipelinePhase::ImplReview),
            PipelinePhase::ImplReview => Some(PipelinePhase::Close),
            PipelinePhase::Close => Some(PipelinePhase::Completed),
            PipelinePhase::Completed => None,
        }
    }

    /// Whether this is the terminal phase (no successor).
    pub fn is_terminal(&self) -> bool {
        matches!(self, PipelinePhase::Completed)
    }

    /// Ordered list of all pipeline phases.
    pub fn all() -> &'static [PipelinePhase] {
        ALL_PHASES
    }

    /// Short description of what this phase does.
    pub fn prompt_template(&self) -> &'static str {
        match self {
            PipelinePhase::Brainstorm => "Explore and pressure-test the idea before planning",
            PipelinePhase::Plan => "Draft a structured build plan from the request",
            PipelinePhase::PlanReview => "Review the plan for correctness and completeness",
            PipelinePhase::Work => "Execute tasks according to the plan",
            PipelinePhase::ImplReview => "Review the implementation for quality and correctness",
            PipelinePhase::Close => "Finalize and close the epic",
            PipelinePhase::Completed => "All pipeline phases complete",
        }
    }

    /// Parse a phase from its snake_case string representation.
    pub fn parse(s: &str) -> Option<PipelinePhase> {
        match s {
            "brainstorm" => Some(PipelinePhase::Brainstorm),
            "plan" => Some(PipelinePhase::Plan),
            "plan_review" => Some(PipelinePhase::PlanReview),
            "work" => Some(PipelinePhase::Work),
            "impl_review" => Some(PipelinePhase::ImplReview),
            "close" => Some(PipelinePhase::Close),
            "completed" => Some(PipelinePhase::Completed),
            _ => None,
        }
    }

    /// Return the snake_case name used for DB storage and JSON.
    pub fn as_str(&self) -> &'static str {
        match self {
            PipelinePhase::Brainstorm => "brainstorm",
            PipelinePhase::Plan => "plan",
            PipelinePhase::PlanReview => "plan_review",
            PipelinePhase::Work => "work",
            PipelinePhase::ImplReview => "impl_review",
            PipelinePhase::Close => "close",
            PipelinePhase::Completed => "completed",
        }
    }
}

impl fmt::Display for PipelinePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phase_sequence() {
        let mut phase = PipelinePhase::Brainstorm;
        let expected = [
            PipelinePhase::Plan,
            PipelinePhase::PlanReview,
            PipelinePhase::Work,
            PipelinePhase::ImplReview,
            PipelinePhase::Close,
            PipelinePhase::Completed,
        ];
        for exp in &expected {
            phase = phase.next().expect("expected next phase");
            assert_eq!(phase, *exp);
        }
        assert!(
            phase.next().is_none(),
            "Completed should have no next phase"
        );
    }

    #[test]
    fn test_is_terminal() {
        assert!(!PipelinePhase::Brainstorm.is_terminal());
        assert!(!PipelinePhase::Plan.is_terminal());
        assert!(!PipelinePhase::PlanReview.is_terminal());
        assert!(!PipelinePhase::Work.is_terminal());
        assert!(!PipelinePhase::ImplReview.is_terminal());
        assert!(!PipelinePhase::Close.is_terminal());
        assert!(PipelinePhase::Completed.is_terminal());
    }

    #[test]
    fn test_all_phases() {
        let all = PipelinePhase::all();
        assert_eq!(all.len(), 6);
        assert_eq!(all[0], PipelinePhase::Brainstorm);
        assert_eq!(all[5], PipelinePhase::Close);
    }

    #[test]
    fn test_prompt_template_not_empty() {
        for phase in PipelinePhase::all() {
            assert!(!phase.prompt_template().is_empty());
        }
    }

    #[test]
    fn test_parse_roundtrip() {
        for phase in PipelinePhase::all() {
            let s = phase.as_str();
            let parsed = PipelinePhase::parse(s).expect("should parse");
            assert_eq!(*phase, parsed);
        }
    }

    #[test]
    fn test_parse_invalid() {
        assert!(PipelinePhase::parse("invalid").is_none());
        assert!(PipelinePhase::parse("").is_none());
    }

    #[test]
    fn test_serde_roundtrip() {
        for phase in PipelinePhase::all() {
            let json = serde_json::to_string(phase).unwrap();
            let deserialized: PipelinePhase = serde_json::from_str(&json).unwrap();
            assert_eq!(*phase, deserialized);
        }
    }

    #[test]
    fn test_display() {
        assert_eq!(PipelinePhase::Brainstorm.to_string(), "brainstorm");
        assert_eq!(PipelinePhase::Plan.to_string(), "plan");
        assert_eq!(PipelinePhase::PlanReview.to_string(), "plan_review");
        assert_eq!(PipelinePhase::Close.to_string(), "close");
        assert_eq!(PipelinePhase::Completed.to_string(), "completed");
    }

    #[test]
    fn test_invalid_transition_rejection() {
        // Can't skip phases: brainstorm -> plan_review (must go through plan)
        assert_ne!(
            PipelinePhase::Brainstorm.next(),
            Some(PipelinePhase::PlanReview)
        );
        // Can't skip phases: plan -> work (must go through plan_review)
        assert_ne!(PipelinePhase::Plan.next(), Some(PipelinePhase::Work));
        // Can't go backwards: work -> plan_review
        assert_ne!(PipelinePhase::Work.next(), Some(PipelinePhase::PlanReview));
        // Close is actionable; completion is terminal
        assert_eq!(PipelinePhase::Close.next(), Some(PipelinePhase::Completed));
    }
}
