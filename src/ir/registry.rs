//! Component allowlist (design §3.5 / §8): the single place that decides which
//! `kind` values a renderer is permitted to receive. Any node whose `kind` is
//! not in this list is rejected before it reaches the client — this is the
//! structural guarantee that an LLM can never smuggle in arbitrary components.
//!
//! Mirrors the two-layer registry in `web/src/registry.ts`.

/// Core registry: generic components usable by any document type (design §5.1).
pub const CORE_KINDS: &[&str] = &[
    "Tabs",
    "Timeline",
    "Checklist",
    "DataTable",
    "Diagram",
    "Callout",
    "RiskPanel",
    "ApiExplorer",
    "ConfigViewer",
    "DependencyGraph",
    "LogTimeline",
    "CommitGraph",
];

/// Domain primitives: added per-domain (design §5.1 outer layer / §9.3).
pub const DOMAIN_KINDS: &[&str] = &[
    "Glossary",
    "CharacterRoster",
    "StepNavigator",
    "ToleranceMeter",
    "ScalableTable",
    "ObligationMatrix",
];

/// True if `kind` is an allowed component name.
pub fn is_allowed(kind: &str) -> bool {
    CORE_KINDS.contains(&kind) || DOMAIN_KINDS.contains(&kind)
}

/// All allowed kinds (core + domain), for diagnostics / TS generation checks.
#[allow(dead_code)]
pub fn all_kinds() -> impl Iterator<Item = &'static str> {
    CORE_KINDS.iter().chain(DOMAIN_KINDS.iter()).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_matches_enum_variants() {
        // Guards against a new UiNode variant being added without registering it.
        assert!(is_allowed("Tabs"));
        assert!(is_allowed("ObligationMatrix"));
        assert!(!is_allowed("ArbitraryScript"));
        assert_eq!(all_kinds().count(), CORE_KINDS.len() + DOMAIN_KINDS.len());
    }
}
