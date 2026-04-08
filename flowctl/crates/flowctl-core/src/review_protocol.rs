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

impl Severity {
    /// Numeric sort key for ordering (P0=0 is highest priority).
    pub fn sort_key(&self) -> u8 {
        match self {
            Severity::P0 => 0,
            Severity::P1 => 1,
            Severity::P2 => 2,
            Severity::P3 => 3,
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

impl AutofixClass {
    /// Restrictiveness order for conservative routing (higher = more restrictive).
    /// SafeAuto(0) < GatedAuto(1) < Manual(2) < Advisory(3).
    pub fn restrictiveness(&self) -> u8 {
        match self {
            AutofixClass::SafeAuto => 0,
            AutofixClass::GatedAuto => 1,
            AutofixClass::Manual => 2,
            AutofixClass::Advisory => 3,
        }
    }
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
    /// Name of the reviewer persona that produced this finding (e.g., "correctness-reviewer").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<String>,
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

// ── Fingerprint & dedup ─────────────────────────────────────────────

impl ReviewFinding {
    /// Generate a dedup fingerprint: normalized(file) + line_bucket(line, +/-3) + normalized(title).
    /// Two findings with the same fingerprint are considered duplicates.
    pub fn fingerprint(&self) -> String {
        let file_part = self.file.as_deref().unwrap_or("_unknown_");
        let line_bucket = self.line.map(|l| l / 6 * 6).unwrap_or(0); // ±3 bucket
        let title_normalized = self
            .description
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        format!("{}:{}:{}", file_part, line_bucket, title_normalized)
    }
}

// ── Merge pipeline ─────────────────────────────────────────────────

/// Result of the merge pipeline.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// Deduplicated, calibrated findings (sorted: P0 first, confidence desc).
    pub findings: Vec<ReviewFinding>,
    /// Findings flagged as pre-existing (separated from actionable findings).
    pub pre_existing: Vec<ReviewFinding>,
    /// Statistics about the merge process.
    pub stats: MergeStats,
}

/// Statistics from the merge pipeline.
#[derive(Debug, Clone)]
pub struct MergeStats {
    /// Total findings received as input.
    pub total_input: usize,
    /// Findings suppressed by confidence gate.
    pub suppressed: usize,
    /// Findings removed by deduplication.
    pub deduplicated: usize,
    /// Findings whose confidence was boosted by cross-reviewer agreement.
    pub boosted: usize,
    /// Findings separated as pre-existing.
    pub pre_existing_count: usize,
}

/// Merge findings from multiple reviewers into a deduplicated, calibrated list.
///
/// Pipeline steps:
/// 1. Confidence gate (filter_by_confidence)
/// 2. Deduplicate by fingerprint, keeping highest-confidence finding per fingerprint
/// 3. Conservative routing: on fingerprint collision, keep most restrictive autofix_class
/// 4. Cross-reviewer boost: if 2+ reviewers produced same fingerprint, boost confidence +0.10 (cap 1.0)
/// 5. Separate pre_existing findings
/// 6. Sort: P0 first, then confidence descending, then file, then line
pub fn merge_findings(all_findings: Vec<ReviewFinding>) -> MergeResult {
    let total_input = all_findings.len();

    // Step 1: Confidence gate
    let after_gate = filter_by_confidence(all_findings);
    let suppressed = total_input - after_gate.len();

    // Step 2-4: Group by fingerprint for dedup + boost + conservative routing
    let mut fingerprint_groups: std::collections::HashMap<String, Vec<ReviewFinding>> =
        std::collections::HashMap::new();
    for finding in after_gate {
        let fp = finding.fingerprint();
        fingerprint_groups.entry(fp).or_default().push(finding);
    }

    let mut deduplicated_count = 0;
    let mut boosted_count = 0;
    let mut merged: Vec<ReviewFinding> = Vec::new();

    for (_fp, mut group) in fingerprint_groups {
        // Count distinct reviewers for this fingerprint
        let distinct_reviewers: std::collections::HashSet<String> = group
            .iter()
            .filter_map(|f| f.reviewer.clone())
            .collect();
        let multi_reviewer = distinct_reviewers.len() >= 2;

        // Keep the finding with highest confidence
        group.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        // Track dedup count (all but the winner)
        deduplicated_count += group.len() - 1;

        let mut winner = group.remove(0);

        // Conservative routing: keep most restrictive autofix_class from the group
        for other in &group {
            if other.autofix_class.restrictiveness() > winner.autofix_class.restrictiveness() {
                winner.autofix_class = other.autofix_class.clone();
            }
        }

        // Cross-reviewer boost
        if multi_reviewer {
            winner.confidence = (winner.confidence + 0.10).min(1.0);
            boosted_count += 1;
        }

        merged.push(winner);
    }

    // Step 5: Separate pre_existing
    let (pre_existing, actionable): (Vec<_>, Vec<_>) =
        merged.into_iter().partition(|f| f.pre_existing);
    let pre_existing_count = pre_existing.len();

    // Step 6: Sort actionable findings
    let mut findings = actionable;
    findings.sort_by(|a, b| {
        a.severity
            .sort_key()
            .cmp(&b.severity.sort_key())
            .then(b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal))
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
    });

    MergeResult {
        findings,
        pre_existing,
        stats: MergeStats {
            total_input,
            suppressed,
            deduplicated: deduplicated_count,
            boosted: boosted_count,
            pre_existing_count,
        },
    }
}

// ── Partition findings ─────────────────────────────────────────────

/// Findings partitioned by autofix class for downstream routing.
#[derive(Debug, Clone)]
pub struct PartitionedFindings {
    /// SafeAuto findings routed to review-fixer for automatic application.
    pub fixer_queue: Vec<ReviewFinding>,
    /// GatedAuto + Manual findings routed to downstream-resolver.
    pub residual_queue: Vec<ReviewFinding>,
    /// Advisory findings included in report only; no code change expected.
    pub report_only: Vec<ReviewFinding>,
}

/// Partition merged findings by autofix class for downstream routing.
///
/// - `safe_auto` → fixer_queue (review-fixer)
/// - `gated_auto` | `manual` → residual_queue (downstream-resolver)
/// - `advisory` → report_only
pub fn partition_findings(findings: Vec<ReviewFinding>) -> PartitionedFindings {
    let mut fixer_queue = Vec::new();
    let mut residual_queue = Vec::new();
    let mut report_only = Vec::new();

    for f in findings {
        match f.autofix_class {
            AutofixClass::SafeAuto => fixer_queue.push(f),
            AutofixClass::GatedAuto | AutofixClass::Manual => residual_queue.push(f),
            AutofixClass::Advisory => report_only.push(f),
        }
    }

    PartitionedFindings {
        fixer_queue,
        residual_queue,
        report_only,
    }
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
            reviewer: None,
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
            reviewer: None,
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
            reviewer: None,
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
            reviewer: None,
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
                    reviewer: None,
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
                    reviewer: None,
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

    // ── Severity sort_key tests ────────────────────────────────────────

    #[test]
    fn test_severity_sort_key() {
        assert_eq!(Severity::P0.sort_key(), 0);
        assert_eq!(Severity::P1.sort_key(), 1);
        assert_eq!(Severity::P2.sort_key(), 2);
        assert_eq!(Severity::P3.sort_key(), 3);
    }

    // ── AutofixClass restrictiveness tests ─────────────────────────────

    #[test]
    fn test_autofix_class_restrictiveness() {
        assert_eq!(AutofixClass::SafeAuto.restrictiveness(), 0);
        assert_eq!(AutofixClass::GatedAuto.restrictiveness(), 1);
        assert_eq!(AutofixClass::Manual.restrictiveness(), 2);
        assert_eq!(AutofixClass::Advisory.restrictiveness(), 3);
        // Verify ordering
        assert!(AutofixClass::SafeAuto.restrictiveness() < AutofixClass::Manual.restrictiveness());
        assert!(AutofixClass::Manual.restrictiveness() < AutofixClass::Advisory.restrictiveness());
    }

    // ── Fingerprint tests ──────────────────────────────────────────────

    fn make_finding_full(
        severity: Severity,
        confidence: f64,
        file: Option<&str>,
        line: Option<u32>,
        description: &str,
        reviewer: Option<&str>,
        autofix_class: AutofixClass,
        pre_existing: bool,
    ) -> ReviewFinding {
        ReviewFinding {
            severity,
            category: "test".to_string(),
            description: description.to_string(),
            file: file.map(|s| s.to_string()),
            line,
            confidence,
            autofix_class,
            owner: FindingOwner::ReviewFixer,
            evidence: vec!["evidence".to_string()],
            pre_existing,
            requires_verification: false,
            suggested_fix: None,
            why_it_matters: None,
            reviewer: reviewer.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_fingerprint_same_file_line_title() {
        let a = make_finding_full(Severity::P1, 0.8, Some("src/main.rs"), Some(10), "Buffer overflow", None, AutofixClass::Manual, false);
        let b = make_finding_full(Severity::P2, 0.7, Some("src/main.rs"), Some(10), "Buffer overflow", None, AutofixClass::SafeAuto, false);
        assert_eq!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn test_fingerprint_line_bucket_within_3() {
        // Lines 10 and 11 are in the same bucket (10/6*6 = 6, 11/6*6 = 6)
        let a = make_finding_full(Severity::P1, 0.8, Some("src/main.rs"), Some(10), "issue", None, AutofixClass::Manual, false);
        let b = make_finding_full(Severity::P1, 0.8, Some("src/main.rs"), Some(11), "issue", None, AutofixClass::Manual, false);
        assert_eq!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn test_fingerprint_different_bucket() {
        // Lines 5 and 12 are in different buckets (5/6*6=0, 12/6*6=12)
        let a = make_finding_full(Severity::P1, 0.8, Some("src/main.rs"), Some(5), "issue", None, AutofixClass::Manual, false);
        let b = make_finding_full(Severity::P1, 0.8, Some("src/main.rs"), Some(12), "issue", None, AutofixClass::Manual, false);
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn test_fingerprint_normalizes_case_and_punctuation() {
        let a = make_finding_full(Severity::P1, 0.8, Some("f.rs"), Some(1), "Buffer Overflow!", None, AutofixClass::Manual, false);
        let b = make_finding_full(Severity::P1, 0.8, Some("f.rs"), Some(1), "buffer overflow", None, AutofixClass::Manual, false);
        assert_eq!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn test_fingerprint_no_file() {
        let a = make_finding_full(Severity::P1, 0.8, None, None, "issue", None, AutofixClass::Manual, false);
        let b = make_finding_full(Severity::P1, 0.8, None, None, "issue", None, AutofixClass::Manual, false);
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert!(a.fingerprint().starts_with("_unknown_:"));
    }

    // ── Dedup tests ────────────────────────────────────────────────────

    #[test]
    fn test_dedup_same_fingerprint_keeps_one() {
        let findings = vec![
            make_finding_full(Severity::P1, 0.8, Some("f.rs"), Some(10), "issue", Some("r1"), AutofixClass::Manual, false),
            make_finding_full(Severity::P1, 0.7, Some("f.rs"), Some(10), "issue", Some("r1"), AutofixClass::Manual, false),
        ];
        let result = merge_findings(findings);
        assert_eq!(result.findings.len(), 1);
        // Should keep the higher-confidence one
        assert!((result.findings[0].confidence - 0.8).abs() < 0.001 ||
                // If boosted (same reviewer won't boost, so 0.8)
                (result.findings[0].confidence - 0.8).abs() < 0.001);
        assert_eq!(result.stats.deduplicated, 1);
    }

    // ── Cross-reviewer boost tests ─────────────────────────────────────

    #[test]
    fn test_boost_two_reviewers_same_fingerprint() {
        let findings = vec![
            make_finding_full(Severity::P1, 0.8, Some("f.rs"), Some(10), "issue", Some("reviewer-a"), AutofixClass::Manual, false),
            make_finding_full(Severity::P1, 0.7, Some("f.rs"), Some(10), "issue", Some("reviewer-b"), AutofixClass::Manual, false),
        ];
        let result = merge_findings(findings);
        assert_eq!(result.findings.len(), 1);
        // Highest confidence was 0.8, boosted by +0.10 = 0.9
        assert!((result.findings[0].confidence - 0.9).abs() < 0.001);
        assert_eq!(result.stats.boosted, 1);
    }

    #[test]
    fn test_boost_caps_at_1_0() {
        let findings = vec![
            make_finding_full(Severity::P1, 0.95, Some("f.rs"), Some(10), "issue", Some("reviewer-a"), AutofixClass::Manual, false),
            make_finding_full(Severity::P1, 0.7, Some("f.rs"), Some(10), "issue", Some("reviewer-b"), AutofixClass::Manual, false),
        ];
        let result = merge_findings(findings);
        assert_eq!(result.findings.len(), 1);
        // 0.95 + 0.10 = 1.05 capped to 1.0
        assert!((result.findings[0].confidence - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_no_boost_single_reviewer() {
        let findings = vec![
            make_finding_full(Severity::P1, 0.8, Some("f.rs"), Some(10), "issue", Some("reviewer-a"), AutofixClass::Manual, false),
            make_finding_full(Severity::P1, 0.7, Some("f.rs"), Some(10), "issue", Some("reviewer-a"), AutofixClass::Manual, false),
        ];
        let result = merge_findings(findings);
        assert_eq!(result.findings.len(), 1);
        // Same reviewer, no boost: confidence stays 0.8
        assert!((result.findings[0].confidence - 0.8).abs() < 0.001);
        assert_eq!(result.stats.boosted, 0);
    }

    // ── Conservative routing tests ─────────────────────────────────────

    #[test]
    fn test_conservative_routing_keeps_most_restrictive() {
        let findings = vec![
            make_finding_full(Severity::P1, 0.8, Some("f.rs"), Some(10), "issue", Some("r1"), AutofixClass::SafeAuto, false),
            make_finding_full(Severity::P1, 0.7, Some("f.rs"), Some(10), "issue", Some("r2"), AutofixClass::Manual, false),
        ];
        let result = merge_findings(findings);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].autofix_class, AutofixClass::Manual);
    }

    // ── Merge pipeline end-to-end tests ────────────────────────────────

    #[test]
    fn test_merge_separates_pre_existing() {
        let findings = vec![
            make_finding_full(Severity::P1, 0.8, Some("f.rs"), Some(10), "new issue", None, AutofixClass::Manual, false),
            make_finding_full(Severity::P2, 0.8, Some("g.rs"), Some(20), "old issue", None, AutofixClass::Manual, true),
        ];
        let result = merge_findings(findings);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.pre_existing.len(), 1);
        assert_eq!(result.pre_existing[0].description, "old issue");
        assert_eq!(result.stats.pre_existing_count, 1);
    }

    #[test]
    fn test_merge_sort_order_p0_first_then_confidence_desc() {
        let findings = vec![
            make_finding_full(Severity::P2, 0.9, Some("a.rs"), Some(1), "low sev high conf", None, AutofixClass::Manual, false),
            make_finding_full(Severity::P0, 0.7, Some("b.rs"), Some(1), "high sev low conf", None, AutofixClass::Manual, false),
            make_finding_full(Severity::P1, 0.85, Some("c.rs"), Some(1), "mid sev mid conf", None, AutofixClass::Manual, false),
            make_finding_full(Severity::P1, 0.9, Some("d.rs"), Some(1), "mid sev high conf", None, AutofixClass::Manual, false),
        ];
        let result = merge_findings(findings);
        assert_eq!(result.findings.len(), 4);
        // P0 first
        assert_eq!(result.findings[0].severity, Severity::P0);
        // Then P1s sorted by confidence desc
        assert_eq!(result.findings[1].severity, Severity::P1);
        assert!((result.findings[1].confidence - 0.9).abs() < 0.001);
        assert_eq!(result.findings[2].severity, Severity::P1);
        assert!((result.findings[2].confidence - 0.85).abs() < 0.001);
        // Then P2
        assert_eq!(result.findings[3].severity, Severity::P2);
    }

    #[test]
    fn test_merge_suppresses_low_confidence() {
        let findings = vec![
            make_finding_full(Severity::P1, 0.3, Some("f.rs"), Some(10), "low conf", None, AutofixClass::Manual, false),
            make_finding_full(Severity::P1, 0.8, Some("g.rs"), Some(20), "high conf", None, AutofixClass::Manual, false),
        ];
        let result = merge_findings(findings);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].description, "high conf");
        assert_eq!(result.stats.suppressed, 1);
    }

    #[test]
    fn test_merge_empty_input() {
        let result = merge_findings(vec![]);
        assert!(result.findings.is_empty());
        assert!(result.pre_existing.is_empty());
        assert_eq!(result.stats.total_input, 0);
    }

    #[test]
    fn test_merge_full_pipeline() {
        // 5 findings: 1 low-confidence (suppressed), 2 duplicates (deduped+boosted), 1 pre-existing, 1 unique
        let findings = vec![
            // Suppressed by confidence gate
            make_finding_full(Severity::P1, 0.3, Some("a.rs"), Some(1), "too low", None, AutofixClass::Manual, false),
            // Two duplicates from different reviewers (will be deduped + boosted)
            make_finding_full(Severity::P0, 0.8, Some("b.rs"), Some(10), "critical bug", Some("r1"), AutofixClass::SafeAuto, false),
            make_finding_full(Severity::P0, 0.7, Some("b.rs"), Some(10), "critical bug", Some("r2"), AutofixClass::Manual, false),
            // Pre-existing
            make_finding_full(Severity::P2, 0.8, Some("c.rs"), Some(20), "old code", None, AutofixClass::Manual, true),
            // Unique finding
            make_finding_full(Severity::P1, 0.9, Some("d.rs"), Some(30), "unique issue", None, AutofixClass::GatedAuto, false),
        ];
        let result = merge_findings(findings);

        assert_eq!(result.stats.total_input, 5);
        assert_eq!(result.stats.suppressed, 1);
        assert_eq!(result.stats.deduplicated, 1);
        assert_eq!(result.stats.boosted, 1);
        assert_eq!(result.stats.pre_existing_count, 1);

        // Actionable: critical bug (boosted) + unique issue
        assert_eq!(result.findings.len(), 2);
        // P0 first
        assert_eq!(result.findings[0].severity, Severity::P0);
        // Boosted: 0.8 + 0.1 = 0.9
        assert!((result.findings[0].confidence - 0.9).abs() < 0.001);
        // Conservative routing: SafeAuto + Manual → Manual
        assert_eq!(result.findings[0].autofix_class, AutofixClass::Manual);

        assert_eq!(result.findings[1].severity, Severity::P1);
        assert_eq!(result.pre_existing.len(), 1);
    }

    // ── Partition tests ────────────────────────────────────────────────

    #[test]
    fn test_partition_routes_by_autofix_class() {
        let findings = vec![
            make_finding_full(Severity::P1, 0.8, Some("a.rs"), Some(1), "safe fix", None, AutofixClass::SafeAuto, false),
            make_finding_full(Severity::P1, 0.8, Some("b.rs"), Some(1), "gated fix", None, AutofixClass::GatedAuto, false),
            make_finding_full(Severity::P1, 0.8, Some("c.rs"), Some(1), "manual fix", None, AutofixClass::Manual, false),
            make_finding_full(Severity::P1, 0.8, Some("d.rs"), Some(1), "advisory", None, AutofixClass::Advisory, false),
        ];
        let partitioned = partition_findings(findings);
        assert_eq!(partitioned.fixer_queue.len(), 1);
        assert_eq!(partitioned.fixer_queue[0].description, "safe fix");
        assert_eq!(partitioned.residual_queue.len(), 2);
        assert_eq!(partitioned.report_only.len(), 1);
        assert_eq!(partitioned.report_only[0].description, "advisory");
    }

    #[test]
    fn test_partition_empty_input() {
        let partitioned = partition_findings(vec![]);
        assert!(partitioned.fixer_queue.is_empty());
        assert!(partitioned.residual_queue.is_empty());
        assert!(partitioned.report_only.is_empty());
    }

    // ── Reviewer field serialization test ──────────────────────────────

    #[test]
    fn test_reviewer_field_serialization() {
        let mut finding = make_finding(Severity::P1, 0.8);
        // None → field absent in JSON
        let json = serde_json::to_value(&finding).unwrap();
        assert!(json.get("reviewer").is_none());

        // Some → field present
        finding.reviewer = Some("correctness-reviewer".to_string());
        let json = serde_json::to_value(&finding).unwrap();
        assert_eq!(json["reviewer"], "correctness-reviewer");

        // Roundtrip
        let json_str = serde_json::to_string(&finding).unwrap();
        let rt: ReviewFinding = serde_json::from_str(&json_str).unwrap();
        assert_eq!(rt.reviewer.as_deref(), Some("correctness-reviewer"));
    }

    #[test]
    fn test_reviewer_field_deserialization_absent() {
        // JSON without reviewer field should deserialize fine (None)
        let json_str = r#"{
            "severity": "P1",
            "category": "test",
            "description": "test"
        }"#;
        let finding: ReviewFinding = serde_json::from_str(json_str).unwrap();
        assert!(finding.reviewer.is_none());
    }
}
