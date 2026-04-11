//! Provider registry — trait abstractions for review/planning backends.
//!
//! Providers are registered by name. Goals bind to specific providers via ProviderSet.
//! Core engine never hardcodes RP/Codex — uses these traits instead.

use std::collections::HashMap;

/// Result type for provider operations.
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Error from a provider.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider not found: {0}")]
    NotFound(String),
    #[error("provider error: {0}")]
    Internal(String),
}

/// Review provider trait — abstracts RP, Codex, or any review backend.
pub trait ReviewProvider: Send + Sync {
    fn review(&self, diff: &str, spec: &str) -> ProviderResult<ReviewResult>;
    fn name(&self) -> &str;
}

/// Planning provider trait — abstracts codebase assessment.
pub trait PlanningProvider: Send + Sync {
    fn assess(&self, request: &str, context: &str) -> ProviderResult<Assessment>;
    fn name(&self) -> &str;
}

/// Ask provider trait — abstracts Q&A capabilities.
pub trait AskProvider: Send + Sync {
    fn ask(&self, question: &str, context: &str) -> ProviderResult<String>;
    fn name(&self) -> &str;
}

/// Result from a review provider.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewResult {
    pub verdict: ReviewVerdict,
    pub score: u32,
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Ship,
    NeedsWork,
}

/// Result from a planning provider.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Assessment {
    pub affected_files: Vec<String>,
    pub complexity: String,
    pub risk_areas: Vec<String>,
}

/// Registry managing all providers.
pub struct ProviderRegistry {
    review: HashMap<String, Box<dyn ReviewProvider>>,
    planning: HashMap<String, Box<dyn PlanningProvider>>,
    ask: HashMap<String, Box<dyn AskProvider>>,
    pub default_review: Option<String>,
    pub default_planning: Option<String>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            review: HashMap::new(),
            planning: HashMap::new(),
            ask: HashMap::new(),
            default_review: None,
            default_planning: None,
        };
        // Register the built-in NoneProvider
        registry.register_review(Box::new(NoneReviewProvider));
        registry
    }

    pub fn register_review(&mut self, provider: Box<dyn ReviewProvider>) {
        let name = provider.name().to_string();
        self.review.insert(name, provider);
    }

    pub fn register_planning(&mut self, provider: Box<dyn PlanningProvider>) {
        let name = provider.name().to_string();
        self.planning.insert(name, provider);
    }

    pub fn register_ask(&mut self, provider: Box<dyn AskProvider>) {
        let name = provider.name().to_string();
        self.ask.insert(name, provider);
    }

    pub fn get_review(&self, name: &str) -> ProviderResult<&dyn ReviewProvider> {
        self.review
            .get(name)
            .map(|p| p.as_ref())
            .ok_or_else(|| ProviderError::NotFound(name.to_string()))
    }

    pub fn get_planning(&self, name: &str) -> ProviderResult<&dyn PlanningProvider> {
        self.planning
            .get(name)
            .map(|p| p.as_ref())
            .ok_or_else(|| ProviderError::NotFound(name.to_string()))
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// NoneProvider — does nothing, always returns Ship.
pub struct NoneReviewProvider;

impl ReviewProvider for NoneReviewProvider {
    fn review(&self, _diff: &str, _spec: &str) -> ProviderResult<ReviewResult> {
        Ok(ReviewResult {
            verdict: ReviewVerdict::Ship,
            score: 30,
            findings: Vec::new(),
        })
    }

    fn name(&self) -> &str {
        "none"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_none_provider() {
        let registry = ProviderRegistry::new();
        let provider = registry.get_review("none").unwrap();
        let result = provider.review("diff", "spec").unwrap();
        assert_eq!(result.verdict, ReviewVerdict::Ship);
        assert_eq!(result.score, 30);
    }

    #[test]
    fn test_registry_not_found() {
        let registry = ProviderRegistry::new();
        assert!(registry.get_review("nonexistent").is_err());
    }
}
