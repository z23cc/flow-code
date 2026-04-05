//! Cross-model review protocol types and consensus logic.
//!
//! Defines structured types for multi-model adversarial review:
//! - `ReviewFinding`: individual issue found during review (CE findings-schema compatible)
//! - `ReviewVerdict`: per-model verdict (Ship / NeedsWork / Abstain)
//! - `ModelReview`: a single model's complete review
//! - `ConsensusResult`: aggregated result from multiple model reviews
//! - `compute_consensus()`: conservative consensus algorithm
//! - `filter_by_confidence()`: suppress low-confidence findings per CE thresholds

use serde::{Deserialize, Serialize};

// ── Finding severity ────────────────────────────────────────────────

/// Severity level for a review finding (CE findings-schema P0-P3 scale).
///
/// - P0: Critical breakage, exploitable vulnerability, data loss. Must fix before merge.
/// - P1: High-impact defect likely hit in normal usage. Should fix.
/// - P2: Moderate issue with meaningful downside. Fix if straightforward.
/// - P3: Low-impact, narrow scope, minor improvement. User's discretion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// Critical breakage, exploitable vulnerability, data loss/corruption.
    P0,
    /// High-impact defect likely hit in normal usage, breaking contract.
    P1,
    /// Moderate issue with meaningful downside (edge case, perf regression).
    P2,
    /// Low-impact, narrow scope, minor improvement.
    P3,
}

impl Severity {
    /// Minimum confidence threshold for this severity to pass filtering.
    /// P0 findings have a lower bar (0.5) to avoid suppressing critical issues.
    pub fn min_confidence(&self) -> f64 {
        match self {
            Severity::P0 => 0.5,
            _ => 0.6,
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::P0 => write!(f, "P0"),
            Severity::P1 => write!(f, "P1"),
            Severity::P2 => write!(f, "P2"),
            Severity::P3 => write!(f, "P3"),
        }
    }
}

// ── Autofix class ───────────────────────────────────────────────────

/// How a finding should be fixed (CE findings-schema autofix classification).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutofixClass {
    /// Local, deterministic fix suitable for automated application.
    SafeAuto,
    /// Concrete fix exists but changes behavior/contracts; needs approval.
    GatedAuto,
    /// Actionable but requires design decisions or cross-cutting changes.
    Manual,
    /// Informational only, surfaced in report but no code change expected.
    Advisory,
}

impl std::fmt::Display for AutofixClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutofixClass::SafeAuto => write!(f, "safe_auto"),
            AutofixClass::GatedAuto => write!(f, "gated_auto"),
            AutofixClass::Manual => write!(f, "manual"),
            AutofixClass::Advisory => write!(f, "advisory"),
        }
    }
}

// ── Finding owner ───────────────────────────────────────────────────

/// Who owns the next action for a finding (CE findings-schema owner).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FindingOwner {
    /// The in-skill fixer can own this when policy allows.
    ReviewFixer,
    /// Turn into residual work for later resolution.
    DownstreamResolver,
    /// A person must make a judgment call.
    Human,
    /// Operational or rollout follow-up; not a code-fix.
    Release,
}

impl std::fmt::Display for FindingOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FindingOwner::ReviewFixer => write!(f, "review-fixer"),
            FindingOwner::DownstreamResolver => write!(f, "downstream-resolver"),
            FindingOwner::Human => write!(f, "human"),
            FindingOwner::Release => write!(f, "release"),
        }
    }
}

// ── ReviewFinding ───────────────────────────────────────────────────

/// A single finding from a model review (CE findings-schema compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewFinding {
    /// Severity of the finding (P0-P3).
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
    /// Reviewer confidence in this finding (0.0-1.0).
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    /// How this finding should be fixed.
    #[serde(default = "default_autofix_class")]
    pub autofix_class: AutofixClass,
    /// Who owns the next action.
    #[serde(default = "default_owner")]
    pub owner: FindingOwner,
    /// Code-grounded evidence (at least 1 item).
    #[serde(default = "default_evidence")]
    pub evidence: Vec<String>,
    /// Whether this issue exists in unchanged code unrelated to the current diff.
    #[serde(default)]
    pub pre_existing: bool,
    /// Whether any fix must be re-verified with targeted tests or follow-up review.
    #[serde(default)]
    pub requires_verification: bool,
    /// Concrete minimal fix suggestion, if obvious.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_fix: Option<String>,
    /// Impact and failure mode — what breaks, not what is wrong.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why_it_matters: Option<String>,
}

fn default_confidence() -> f64 {
    0.8
}

fn default_autofix_class() -> AutofixClass {
    AutofixClass::Manual
}

fn default_owner() -> FindingOwner {
    FindingOwner::ReviewFixer
}

fn default_evidence() -> Vec<String> {
    vec![]
}

// ── Confidence filtering ────────────────────────────────────────────

/// Filter findings by confidence threshold per CE schema rules.
///
/// Suppresses findings with confidence < 0.6, except P0 findings which
/// are kept at confidence >= 0.5.
pub fn filter_by_confidence(findings: Vec<ReviewFinding>) -> Vec<ReviewFinding> {
    findings
        .into_iter()
        .filter(|f| f.confidence >= f.severity.min_confidence())
        .collect()
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

    fn make_finding(severity: Severity, confidence: f64) -> ReviewFinding {
        ReviewFinding {
            severity,
            category: "test".to_string(),
            description: "test finding".to_string(),
            file: None,
            line: None,
            confidence,
            autofix_class: AutofixClass::Manual,
            owner: FindingOwner::ReviewFixer,
            evidence: vec!["test evidence".to_string()],
            pre_existing: false,
            requires_verification: false,
            suggested_fix: None,
            why_it_matters: None,
        }
    }

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
        assert_eq!(format!("{}", Severity::P0), "P0");
        assert_eq!(format!("{}", Severity::P1), "P1");
        assert_eq!(format!("{}", Severity::P2), "P2");
        assert_eq!(format!("{}", Severity::P3), "P3");
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
    fn test_finding_serialization_roundtrip() {
        let finding = ReviewFinding {
            severity: Severity::P0,
            category: "security".to_string(),
            description: "SQL injection vulnerability".to_string(),
            file: Some("src/db.rs".to_string()),
            line: Some(42),
            confidence: 0.95,
            autofix_class: AutofixClass::SafeAuto,
            owner: FindingOwner::ReviewFixer,
            evidence: vec!["Unsanitized user input at line 42".to_string()],
            pre_existing: false,
            requires_verification: true,
            suggested_fix: Some("Use parameterized query".to_string()),
            why_it_matters: Some("Allows arbitrary SQL execution".to_string()),
        };
        let json_str = serde_json::to_string(&finding).unwrap();
        let roundtripped: ReviewFinding = serde_json::from_str(&json_str).unwrap();
        assert_eq!(roundtripped.severity, Severity::P0);
        assert_eq!(roundtripped.confidence, 0.95);
        assert_eq!(roundtripped.autofix_class, AutofixClass::SafeAuto);
        assert_eq!(roundtripped.owner, FindingOwner::ReviewFixer);
        assert_eq!(roundtripped.evidence.len(), 1);
        assert!(!roundtripped.pre_existing);
        assert!(roundtripped.requires_verification);
        assert_eq!(roundtripped.suggested_fix.as_deref(), Some("Use parameterized query"));
        assert_eq!(roundtripped.why_it_matters.as_deref(), Some("Allows arbitrary SQL execution"));
    }

    #[test]
    fn test_finding_serialization_json_shape() {
        let finding = ReviewFinding {
            severity: Severity::P1,
            category: "logic".to_string(),
            description: "Off-by-one".to_string(),
            file: Some("src/main.rs".to_string()),
            line: Some(10),
            confidence: 0.85,
            autofix_class: AutofixClass::GatedAuto,
            owner: FindingOwner::Human,
            evidence: vec!["Loop bound is exclusive".to_string()],
            pre_existing: true,
            requires_verification: false,
            suggested_fix: None,
            why_it_matters: None,
        };
        let json = serde_json::to_value(&finding).unwrap();
        assert_eq!(json["severity"], "P1");
        assert_eq!(json["category"], "logic");
        assert_eq!(json["file"], "src/main.rs");
        assert_eq!(json["line"], 10);
        assert_eq!(json["confidence"], 0.85);
        assert_eq!(json["autofix_class"], "gated_auto");
        assert_eq!(json["owner"], "human");
        assert!(json["pre_existing"].as_bool().unwrap());
        // Optional None fields should be absent
        assert!(json.get("suggested_fix").is_none());
        assert!(json.get("why_it_matters").is_none());
    }

    #[test]
    fn test_finding_without_location() {
        let finding = ReviewFinding {
            severity: Severity::P3,
            category: "style".to_string(),
            description: "Consider using const".to_string(),
            file: None,
            line: None,
            confidence: 0.7,
            autofix_class: AutofixClass::Advisory,
            owner: FindingOwner::DownstreamResolver,
            evidence: vec!["Variable never mutated".to_string()],
            pre_existing: false,
            requires_verification: false,
            suggested_fix: None,
            why_it_matters: None,
        };
        let json = serde_json::to_value(&finding).unwrap();
        assert_eq!(json["severity"], "P3");
        assert!(json.get("file").is_none());
        assert!(json.get("line").is_none());
    }

    #[test]
    fn test_finding_deserialization_with_defaults() {
        // Minimal JSON with only required old-style fields; new fields use defaults
        let json_str = r#"{
            "severity": "P2",
            "category": "perf",
            "description": "Unnecessary allocation"
        }"#;
        let finding: ReviewFinding = serde_json::from_str(json_str).unwrap();
        assert_eq!(finding.severity, Severity::P2);
        assert_eq!(finding.confidence, 0.8); // default
        assert_eq!(finding.autofix_class, AutofixClass::Manual); // default
        assert_eq!(finding.owner, FindingOwner::ReviewFixer); // default
        assert!(finding.evidence.is_empty()); // default
        assert!(!finding.pre_existing);
        assert!(!finding.requires_verification);
    }

    #[test]
    fn test_model_review_with_findings() {
        let review = make_review_with_findings(
            "codex",
            ReviewVerdict::NeedsWork,
            0.85,
            vec![
                ReviewFinding {
                    severity: Severity::P0,
                    category: "logic".to_string(),
                    description: "Off-by-one in loop".to_string(),
                    file: Some("src/main.rs".to_string()),
                    line: Some(10),
                    confidence: 0.9,
                    autofix_class: AutofixClass::SafeAuto,
                    owner: FindingOwner::ReviewFixer,
                    evidence: vec!["line 10: i <= len".to_string()],
                    pre_existing: false,
                    requires_verification: true,
                    suggested_fix: Some("Change <= to <".to_string()),
                    why_it_matters: Some("Buffer overread".to_string()),
                },
                ReviewFinding {
                    severity: Severity::P2,
                    category: "performance".to_string(),
                    description: "Unnecessary clone".to_string(),
                    file: None,
                    line: None,
                    confidence: 0.7,
                    autofix_class: AutofixClass::SafeAuto,
                    owner: FindingOwner::ReviewFixer,
                    evidence: vec!["clone() on borrowed ref".to_string()],
                    pre_existing: false,
                    requires_verification: false,
                    suggested_fix: None,
                    why_it_matters: None,
                },
            ],
        );
        assert_eq!(review.findings.len(), 2);
        assert_eq!(review.findings[0].severity, Severity::P0);
        assert_eq!(review.findings[1].severity, Severity::P2);
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

    // ── Confidence filtering tests ──────────────────────────────────

    #[test]
    fn test_filter_by_confidence_suppresses_low_confidence() {
        let findings = vec![
            make_finding(Severity::P1, 0.5),  // below 0.6 → suppressed
            make_finding(Severity::P2, 0.6),  // at threshold → kept
            make_finding(Severity::P3, 0.3),  // below 0.6 → suppressed
        ];
        let filtered = filter_by_confidence(findings);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].severity, Severity::P2);
    }

    #[test]
    fn test_filter_by_confidence_p0_lower_threshold() {
        let findings = vec![
            make_finding(Severity::P0, 0.5),  // at P0 threshold (0.5) → kept
            make_finding(Severity::P0, 0.49), // below P0 threshold → suppressed
            make_finding(Severity::P0, 0.9),  // well above → kept
        ];
        let filtered = filter_by_confidence(findings);
        assert_eq!(filtered.len(), 2);
        assert!((filtered[0].confidence - 0.5).abs() < 0.001);
        assert!((filtered[1].confidence - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_filter_by_confidence_keeps_high_confidence() {
        let findings = vec![
            make_finding(Severity::P1, 0.85),
            make_finding(Severity::P2, 0.7),
            make_finding(Severity::P3, 0.95),
        ];
        let filtered = filter_by_confidence(findings);
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn test_filter_by_confidence_empty_input() {
        let filtered = filter_by_confidence(vec![]);
        assert!(filtered.is_empty());
    }

    // ── Enum serialization tests ────────────────────────────────────

    #[test]
    fn test_autofix_class_serialization() {
        assert_eq!(serde_json::to_value(AutofixClass::SafeAuto).unwrap(), "safe_auto");
        assert_eq!(serde_json::to_value(AutofixClass::GatedAuto).unwrap(), "gated_auto");
        assert_eq!(serde_json::to_value(AutofixClass::Manual).unwrap(), "manual");
        assert_eq!(serde_json::to_value(AutofixClass::Advisory).unwrap(), "advisory");
    }

    #[test]
    fn test_autofix_class_deserialization() {
        let v: AutofixClass = serde_json::from_str("\"safe_auto\"").unwrap();
        assert_eq!(v, AutofixClass::SafeAuto);
        let v: AutofixClass = serde_json::from_str("\"gated_auto\"").unwrap();
        assert_eq!(v, AutofixClass::GatedAuto);
    }

    #[test]
    fn test_finding_owner_serialization() {
        assert_eq!(serde_json::to_value(FindingOwner::ReviewFixer).unwrap(), "review-fixer");
        assert_eq!(serde_json::to_value(FindingOwner::DownstreamResolver).unwrap(), "downstream-resolver");
        assert_eq!(serde_json::to_value(FindingOwner::Human).unwrap(), "human");
        assert_eq!(serde_json::to_value(FindingOwner::Release).unwrap(), "release");
    }

    #[test]
    fn test_finding_owner_deserialization() {
        let v: FindingOwner = serde_json::from_str("\"review-fixer\"").unwrap();
        assert_eq!(v, FindingOwner::ReviewFixer);
        let v: FindingOwner = serde_json::from_str("\"downstream-resolver\"").unwrap();
        assert_eq!(v, FindingOwner::DownstreamResolver);
    }

    #[test]
    fn test_severity_min_confidence() {
        assert!((Severity::P0.min_confidence() - 0.5).abs() < 0.001);
        assert!((Severity::P1.min_confidence() - 0.6).abs() < 0.001);
        assert!((Severity::P2.min_confidence() - 0.6).abs() < 0.001);
        assert!((Severity::P3.min_confidence() - 0.6).abs() < 0.001);
    }
}
