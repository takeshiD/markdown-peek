//! Generation policy — how the pipeline chooses between deterministic **rules**
//! and the **LLM** (AGENTS.md §0.1 論点 F/K).
//!
//! Layer 2 is rules-only, but the policy type lives here as the single source of
//! truth: the `mdpeek` binary loads it from `config.toml` at startup and Layer 3's
//! generator (`RulesGenerator` / `ClaudeGenerator`) consults [`GenerationConfig`]
//! per node via [`GenerationConfig::should_use_llm`].

use serde::{Deserialize, Serialize};

/// Which source wins when both rules and the LLM could produce a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GenerationStrategy {
    /// Deterministic rules first; only consult the LLM for low-confidence or
    /// unfilled nodes. Reproducible and offline-friendly — the default.
    #[default]
    RulesFirst,
    /// Prefer the LLM; fall back to rules when the LLM is unavailable or fails.
    LlmFirst,
}

/// Default confidence below which a rules result is escalated to the LLM
/// (論点 K).
pub const DEFAULT_CONFIDENCE_THRESHOLD: f32 = 0.6;

/// The effective generation policy, resolved from configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GenerationConfig {
    /// Whether the LLM may be used at all. When `false` (e.g. no API key or the
    /// user disabled it) the pipeline is strictly rules-only, regardless of
    /// `strategy`.
    pub llm_enabled: bool,
    /// Rules-first vs LLM-first (see [`GenerationStrategy`]).
    pub strategy: GenerationStrategy,
    /// Confidence threshold for escalating a rules result to the LLM under
    /// [`GenerationStrategy::RulesFirst`].
    pub confidence_threshold: f32,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        GenerationConfig {
            llm_enabled: false,
            strategy: GenerationStrategy::RulesFirst,
            confidence_threshold: DEFAULT_CONFIDENCE_THRESHOLD,
        }
    }
}

impl GenerationConfig {
    /// Whether the generator should consult the LLM for a node whose rules-stage
    /// confidence is `confidence`.
    ///
    /// - LLM disabled → never (strictly rules).
    /// - `LlmFirst` → always (when enabled): the LLM is the preferred source.
    /// - `RulesFirst` → only when the rules result is below the confidence
    ///   threshold (escalate the uncertain cases).
    pub fn should_use_llm(&self, confidence: f32) -> bool {
        if !self.llm_enabled {
            return false;
        }
        match self.strategy {
            GenerationStrategy::LlmFirst => true,
            GenerationStrategy::RulesFirst => confidence < self.confidence_threshold,
        }
    }

    /// `true` when generation runs without ever calling the LLM.
    pub fn is_rules_only(&self) -> bool {
        !self.llm_enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_rules_only() {
        let cfg = GenerationConfig::default();
        assert!(cfg.is_rules_only());
        assert_eq!(cfg.strategy, GenerationStrategy::RulesFirst);
        assert!(!cfg.should_use_llm(0.1)); // disabled → never, even at low confidence
    }

    #[test]
    fn rules_first_escalates_only_low_confidence() {
        let cfg = GenerationConfig {
            llm_enabled: true,
            strategy: GenerationStrategy::RulesFirst,
            confidence_threshold: 0.6,
        };
        assert!(cfg.should_use_llm(0.3)); // uncertain → escalate to LLM
        assert!(!cfg.should_use_llm(0.9)); // confident → keep rules
        assert!(!cfg.should_use_llm(0.6)); // at threshold → keep rules
    }

    #[test]
    fn llm_first_always_uses_llm_when_enabled() {
        let cfg = GenerationConfig {
            llm_enabled: true,
            strategy: GenerationStrategy::LlmFirst,
            confidence_threshold: 0.6,
        };
        assert!(cfg.should_use_llm(0.99));
        assert!(cfg.should_use_llm(0.0));
        assert!(!cfg.is_rules_only());
    }

    #[test]
    fn llm_first_but_disabled_falls_back_to_rules() {
        let cfg = GenerationConfig {
            llm_enabled: false,
            strategy: GenerationStrategy::LlmFirst,
            confidence_threshold: 0.6,
        };
        // No API key / disabled: strictly rules even though strategy is llm_first.
        assert!(cfg.is_rules_only());
        assert!(!cfg.should_use_llm(0.0));
    }

    #[test]
    fn strategy_parses_from_snake_case() {
        let s: GenerationStrategy = serde_json::from_str("\"llm_first\"").unwrap();
        assert_eq!(s, GenerationStrategy::LlmFirst);
        let s: GenerationStrategy = serde_json::from_str("\"rules_first\"").unwrap();
        assert_eq!(s, GenerationStrategy::RulesFirst);
    }
}
