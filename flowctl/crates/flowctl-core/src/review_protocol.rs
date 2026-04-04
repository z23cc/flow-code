//! Cross-model review protocol types and consensus logic.
//!
//! Defines structured types for multi-model adversarial review:
//! - `ReviewFinding`: individual issue found during review
//! - `ReviewVerdict`: per-model verdict (Ship / NeedsWork / Abstain)
//! - `ModelReview`: a single model's complete review
//! - `ConsensusResult`: aggregated result from multiple model reviews
//! - `compute_consensus()`: conservative consensus algorithm

use serde::{Deserialize, Serialize};

// ── Finding severity ────────────────────────────────────────────────

/// Severity level for a review finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Critical => write!(f, "critical"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

// ── ReviewFinding ───────────────────────────────────────────────────

/// A single finding from a model review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewFinding {
    /// Severity of the finding.
    pub severity: Severity,
    /// Category (e.g., "security", "performance", "logic", "style").
    pub category: String,
    /// Human-readable description of the issue.
    pub description: String,
    /// File path where the issue was found (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Line number where the issue was found (if applicable).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

// ── ReviewVerdict ───────────────────────────────────────────────────

/// A model's verdict on the reviewed code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewVerdict {
    /// Code is ready to ship.
    Ship,
    /// Code needs additional work before shipping.
    NeedsWork,
    /// Model cannot make a determination.
    Abstain,
}

impl std::fmt::Display for ReviewVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviewVerdict::Ship => write!(f, "SHIP"),
            ReviewVerdict::NeedsWork => write!(f, "NEEDS_WORK"),
            ReviewVerdict::Abstain => write!(f, "ABSTAIN"),
        }
    }
}

// ── ModelReview ─────────────────────────────────────────────────────

/// A complete review from a single model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelReview {
    /// Model identifier (e.g., "codex/gpt-5.4", "claude/opus-4").
    pub model: String,
    /// The model's verdict.
    pub verdict: ReviewVerdict,
    /// Individual findings from the review.
    pub findings: Vec<ReviewFinding>,
    /// Confidence score in [0.0, 1.0].
    pub confidence: f64,
}

// ── ConsensusResult ─────────────────────────────────────────────────

/// Aggregated consensus from multiple model reviews.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConsensusResult {
    /// All models agree on the same verdict.
    Consensus {
        verdict: ReviewVerdict,
        /// Combined confidence (average of participating models).
        confidence: f64,
    },
    /// Models disagree on the verdict.
    Conflict {
        /// Individual model reviews for human inspection.
        reviews: Vec<ModelReview>,
    },
    /// Not enough reviews to determine consensus (need at least 2).
    InsufficientReviews,
}

impl std::fmt::Display for ConsensusResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsensusResult::Consensus { verdict, confidence } => {
                write!(f, "Consensus: {} (confidence: {:.0}%)", verdict, confidence * 100.0)
            }
            ConsensusResult::Conflict { reviews } => {
                write!(f, "Conflict: {} models disagree", reviews.len())
            }
            ConsensusResult::InsufficientReviews => {
                write!(f, "Insufficient reviews for consensus")
            }
        }
    }
}

// ── Consensus algorithm ─────────────────────────────────────────────

/// Compute consensus from multiple model reviews.
///
/// Algorithm (conservative):
/// - Fewer than 2 reviews → `InsufficientReviews`
/// - Filter out `Abstain` verdicts for consensus calculation
/// - If all non-abstain models agree → `Consensus` with that verdict
/// - If ANY model says `NeedsWork` → `Consensus(NeedsWork)` (conservative)
/// - Otherwise (mixed Ship/Abstain with disagreement) → `Conflict`
pub fn compute_consensus(reviews: &[ModelReview]) -> ConsensusResult {
    if reviews.len() < 2 {
        return ConsensusResult::InsufficientReviews;
    }

    // Filter out abstaining models for the actual vote
    let voting_reviews: Vec<&ModelReview> = reviews
        .iter()
        .filter(|r| r.verdict != ReviewVerdict::Abstain)
        .collect();

    // All abstained — insufficient signal
    if voting_reviews.is_empty() {
        return ConsensusResult::InsufficientReviews;
    }

    // Check if any model says NeedsWork (conservative: block on any objection)
    let has_needs_work = voting_reviews
        .iter()
        .any(|r| r.verdict == ReviewVerdict::NeedsWork);

    if has_needs_work {
        // Conservative: any NeedsWork → overall NeedsWork
        let avg_confidence = voting_reviews
            .iter()
            .map(|r| r.confidence)
            .sum::<f64>()
            / voting_reviews.len() as f64;
        return ConsensusResult::Consensus {
            verdict: ReviewVerdict::NeedsWork,
            confidence: avg_confidence,
        };
    }

    // Check unanimous agreement among voters
    let first_verdict = &voting_reviews[0].verdict;
    let all_agree = voting_reviews.iter().all(|r| &r.verdict == first_verdict);

    if all_agree {
        let avg_confidence = voting_reviews
            .iter()
            .map(|r| r.confidence)
            .sum::<f64>()
            / voting_reviews.len() as f64;
        ConsensusResult::Consensus {
            verdict: first_verdict.clone(),
            confidence: avg_confidence,
        }
    } else {
        ConsensusResult::Conflict {
            reviews: reviews.to_vec(),
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_review(model: &str, verdict: ReviewVerdict, confidence: f64) -> ModelReview {
        ModelReview {
            model: model.to_string(),
            verdict,
            findings: vec![],
            confidence,
        }
    }

    fn make_review_with_findings(
        model: &str,
        verdict: ReviewVerdict,
        confidence: f64,
        findings: Vec<ReviewFinding>,
    ) -> ModelReview {
        ModelReview {
            model: model.to_string(),
            verdict,
            findings,
            confidence,
        }
    }

    #[test]
    fn test_insufficient_reviews_empty() {
        let result = compute_consensus(&[]);
        assert!(matches!(result, ConsensusResult::InsufficientReviews));
    }

    #[test]
    fn test_insufficient_reviews_single() {
        let reviews = vec![make_review("codex", ReviewVerdict::Ship, 0.9)];
        let result = compute_consensus(&reviews);
        assert!(matches!(result, ConsensusResult::InsufficientReviews));
    }

    #[test]
    fn test_consensus_both_ship() {
        let reviews = vec![
            make_review("codex", ReviewVerdict::Ship, 0.9),
            make_review("claude", ReviewVerdict::Ship, 0.85),
        ];
        let result = compute_consensus(&reviews);
        match result {
            ConsensusResult::Consensus { verdict, confidence } => {
                assert_eq!(verdict, ReviewVerdict::Ship);
                assert!((confidence - 0.875).abs() < 0.001);
            }
            _ => panic!("expected Consensus, got {:?}", result),
        }
    }

    #[test]
    fn test_consensus_both_needs_work() {
        let reviews = vec![
            make_review("codex", ReviewVerdict::NeedsWork, 0.8),
            make_review("claude", ReviewVerdict::NeedsWork, 0.7),
        ];
        let result = compute_consensus(&reviews);
        match result {
            ConsensusResult::Consensus { verdict, confidence } => {
                assert_eq!(verdict, ReviewVerdict::NeedsWork);
                assert!((confidence - 0.75).abs() < 0.001);
            }
            _ => panic!("expected Consensus, got {:?}", result),
        }
    }

    #[test]
    fn test_conservative_any_needs_work() {
        // One says Ship, one says NeedsWork → conservative NeedsWork
        let reviews = vec![
            make_review("codex", ReviewVerdict::Ship, 0.9),
            make_review("claude", ReviewVerdict::NeedsWork, 0.85),
        ];
        let result = compute_consensus(&reviews);
        match result {
            ConsensusResult::Consensus { verdict, .. } => {
                assert_eq!(verdict, ReviewVerdict::NeedsWork);
            }
            _ => panic!("expected NeedsWork consensus, got {:?}", result),
        }
    }

    #[test]
    fn test_abstain_filtered_out() {
        // One abstains, one ships → Consensus(Ship)
        let reviews = vec![
            make_review("codex", ReviewVerdict::Ship, 0.9),
            make_review("claude", ReviewVerdict::Abstain, 0.3),
        ];
        let result = compute_consensus(&reviews);
        match result {
            ConsensusResult::Consensus { verdict, confidence } => {
                assert_eq!(verdict, ReviewVerdict::Ship);
                assert!((confidence - 0.9).abs() < 0.001);
            }
            _ => panic!("expected Consensus, got {:?}", result),
        }
    }

    #[test]
    fn test_all_abstain_insufficient() {
        let reviews = vec![
            make_review("codex", ReviewVerdict::Abstain, 0.3),
            make_review("claude", ReviewVerdict::Abstain, 0.2),
        ];
        let result = compute_consensus(&reviews);
        assert!(matches!(result, ConsensusResult::InsufficientReviews));
    }

    #[test]
    fn test_three_models_consensus() {
        let reviews = vec![
            make_review("codex", ReviewVerdict::Ship, 0.9),
            make_review("claude", ReviewVerdict::Ship, 0.85),
            make_review("gemini", ReviewVerdict::Ship, 0.8),
        ];
        let result = compute_consensus(&reviews);
        match result {
            ConsensusResult::Consensus { verdict, confidence } => {
                assert_eq!(verdict, ReviewVerdict::Ship);
                assert!((confidence - 0.85).abs() < 0.001);
            }
            _ => panic!("expected Consensus, got {:?}", result),
        }
    }

    #[test]
    fn test_severity_display() {
        assert_eq!(format!("{}", Severity::Critical), "critical");
        assert_eq!(format!("{}", Severity::Warning), "warning");
        assert_eq!(format!("{}", Severity::Info), "info");
    }

    #[test]
    fn test_verdict_display() {
        assert_eq!(format!("{}", ReviewVerdict::Ship), "SHIP");
        assert_eq!(format!("{}", ReviewVerdict::NeedsWork), "NEEDS_WORK");
        assert_eq!(format!("{}", ReviewVerdict::Abstain), "ABSTAIN");
    }

    #[test]
    fn test_consensus_result_display() {
        let c = ConsensusResult::Consensus {
            verdict: ReviewVerdict::Ship,
            confidence: 0.9,
        };
        assert_eq!(format!("{c}"), "Consensus: SHIP (confidence: 90%)");

        let c = ConsensusResult::InsufficientReviews;
        assert_eq!(format!("{c}"), "Insufficient reviews for consensus");
    }

    #[test]
    fn test_finding_serialization() {
        let finding = ReviewFinding {
            severity: Severity::Critical,
            category: "security".to_string(),
            description: "SQL injection vulnerability".to_string(),
            file: Some("src/db.rs".to_string()),
            line: Some(42),
        };
        let json = serde_json::to_value(&finding).unwrap();
        assert_eq!(json["severity"], "critical");
        assert_eq!(json["category"], "security");
        assert_eq!(json["file"], "src/db.rs");
        assert_eq!(json["line"], 42);
    }

    #[test]
    fn test_finding_without_location() {
        let finding = ReviewFinding {
            severity: Severity::Info,
            category: "style".to_string(),
            description: "Consider using const".to_string(),
            file: None,
            line: None,
        };
        let json = serde_json::to_value(&finding).unwrap();
        assert_eq!(json["severity"], "info");
        // Optional fields should be absent
        assert!(json.get("file").is_none());
        assert!(json.get("line").is_none());
    }

    #[test]
    fn test_model_review_with_findings() {
        let review = make_review_with_findings(
            "codex",
            ReviewVerdict::NeedsWork,
            0.85,
            vec![
                ReviewFinding {
                    severity: Severity::Critical,
                    category: "logic".to_string(),
                    description: "Off-by-one in loop".to_string(),
                    file: Some("src/main.rs".to_string()),
                    line: Some(10),
                },
                ReviewFinding {
                    severity: Severity::Warning,
                    category: "performance".to_string(),
                    description: "Unnecessary clone".to_string(),
                    file: None,
                    line: None,
                },
            ],
        );
        assert_eq!(review.findings.len(), 2);
        assert_eq!(review.findings[0].severity, Severity::Critical);
        assert_eq!(review.findings[1].severity, Severity::Warning);
    }

    #[test]
    fn test_consensus_result_serialization() {
        let result = ConsensusResult::Consensus {
            verdict: ReviewVerdict::Ship,
            confidence: 0.9,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["type"], "consensus");
        assert_eq!(json["verdict"], "SHIP");

        let result = ConsensusResult::Conflict {
            reviews: vec![
                make_review("a", ReviewVerdict::Ship, 0.9),
                make_review("b", ReviewVerdict::NeedsWork, 0.8),
            ],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["type"], "conflict");
        assert_eq!(json["reviews"].as_array().unwrap().len(), 2);
    }
}
