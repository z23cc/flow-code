//! Task profiling: track estimated and actual durations for CPM weighting.
//!
//! `TaskProfile` stores per-task timing data that feeds into the DAG's
//! weighted critical path calculation. Profiles can be built from historical
//! `duration_seconds` evidence or from explicit estimates.

use std::collections::HashMap;

/// A task's execution profile used for CPM weighting.
#[derive(Debug, Clone)]
pub struct TaskProfile {
    /// Estimated duration in seconds (user-provided or from historical data).
    pub estimated_seconds: f64,
    /// Actual duration in seconds (filled after completion).
    pub actual_seconds: Option<f64>,
}

impl TaskProfile {
    pub fn new(estimated_seconds: f64) -> Self {
        Self {
            estimated_seconds,
            actual_seconds: None,
        }
    }

    /// Return the best available weight: actual if known, else estimated.
    pub fn weight(&self) -> f64 {
        self.actual_seconds.unwrap_or(self.estimated_seconds)
    }
}

/// Build a weight map suitable for `TaskDag::critical_path_weighted()` and
/// `TaskDag::cpm_priorities()` from a set of task profiles.
///
/// Tasks without a profile get weight 1.0 (the DAG methods' default).
pub fn weights_from_profiles(profiles: &HashMap<String, TaskProfile>) -> HashMap<String, f64> {
    profiles
        .iter()
        .map(|(id, p)| (id.clone(), p.weight()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_weight_uses_estimated() {
        let p = TaskProfile::new(5.0);
        assert!((p.weight() - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_profile_weight_prefers_actual() {
        let mut p = TaskProfile::new(5.0);
        p.actual_seconds = Some(3.0);
        assert!((p.weight() - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_weights_from_profiles() {
        let mut profiles = HashMap::new();
        profiles.insert("a".to_string(), TaskProfile::new(10.0));
        profiles.insert("b".to_string(), TaskProfile::new(2.0));
        let weights = weights_from_profiles(&profiles);
        assert!((weights["a"] - 10.0).abs() < f64::EPSILON);
        assert!((weights["b"] - 2.0).abs() < f64::EPSILON);
    }
}
