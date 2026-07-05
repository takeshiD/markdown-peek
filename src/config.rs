//! Configuration loaded from `config.toml`.
//!
//! The file lives at `$XDG_CONFIG_HOME/mdpeek/config.toml`, falling back to
//! `~/.config/mdpeek/config.toml` when `XDG_CONFIG_HOME` is unset (XDG Base
//! Directory specification). Every value is optional; missing keys fall back to
//! the built-in defaults. The effective precedence is:
//!
//! ```text
//! CLI arguments  >  config.toml  >  built-in defaults
//! ```

use crate::cli::ThemeChoice;
use crate::generator::llm::{Effort, LlmBackendConfig, LlmProvider};
use mdpeek_analyzer::generation::DEFAULT_CONFIDENCE_THRESHOLD;
use mdpeek_analyzer::{GenerationConfig, GenerationStrategy};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Top-level configuration mirroring the structure of `config.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Mode used when `mdpeek` is invoked without a subcommand.
    pub default_mode: Option<DefaultMode>,
    /// Browser previewer (`serve`) settings.
    pub server: ServerConfig,
    /// Terminal previewer (`term`) settings.
    pub term: TermConfig,
    /// Generative-UI / LLM settings (rules-first vs LLM-first). Consumed by the
    /// generator (Layer 3); read here at startup.
    pub llm: LlmConfig,
}

/// `[server]` section: browser previewer defaults.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    /// IP address to bind, e.g. `"127.0.0.1"`.
    pub host: Option<String>,
    /// Port to bind, e.g. `"3030"`.
    pub port: Option<String>,
    /// Default browser preview theme.
    pub theme: Option<BrowserTheme>,
}

/// `[term]` section: terminal previewer defaults.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TermConfig {
    /// Default colour theme for terminal rendering.
    pub theme: Option<ThemeChoice>,
    /// Pager command used for long output. An empty string disables paging and
    /// prints directly to stdout. When unset, `$PAGER` (or `less -R`) is used.
    pub pager: Option<String>,
}

/// `[llm]` section: how generated UI chooses between deterministic rules and
/// the LLM. This is read from `config.toml` at application startup; the actual
/// generator (Layer 3) consults the resolved [`GenerationConfig`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LlmConfig {
    /// Whether the LLM may be used at all. When `false` the pipeline is strictly
    /// rules-only regardless of `strategy`. (Layer 3 also degrades to rules-only
    /// automatically when no API key is present.)
    pub enabled: bool,
    /// `"rules_first"` (default) or `"llm_first"` — which source wins when both
    /// rules and the LLM could produce a result.
    pub strategy: GenerationStrategy,
    /// Confidence below which a rules result is escalated to the LLM under
    /// `rules_first`. Defaults to 0.6 when unset.
    pub confidence_threshold: Option<f32>,
    /// Which LLM backend to drive (Layer 3): `"anthropic_api"` (default),
    /// `"claude_code"`, or `"codex"`. See [`LlmProvider`].
    pub provider: LlmProvider,
    /// Model id passed to the backend (backend-specific). Omit for the
    /// backend's default.
    pub model: Option<String>,
    /// Reasoning effort: `"low"` | `"medium"` | `"high"`. Mapped per backend
    /// (Codex → `model_reasoning_effort`, Claude Code → thinking keyword).
    pub effort: Option<Effort>,
}

/// Mode selected when no subcommand is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DefaultMode {
    /// Launch the browser previewer.
    Serve,
    /// Render to the terminal.
    Term,
}

/// Browser preview theme (`light` / `dark`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BrowserTheme {
    Light,
    Dark,
}

impl Config {
    /// Load the configuration from the default location, falling back to
    /// built-in defaults when the file is absent or unreadable. A parse error
    /// is reported on stderr but is non-fatal so the tool stays usable.
    pub fn load() -> Self {
        match config_path() {
            Some(path) => Self::load_from(&path),
            None => Self::default(),
        }
    }

    /// Load from an explicit path given on the command line (`--config`).
    /// Unlike [`Config::load`], a missing file is reported as a warning because
    /// the user asked for this specific path.
    pub fn load_explicit(path: &Path) -> Self {
        if !path.exists() {
            eprintln!("mdpeek: warning: config file not found: {}", path.display());
            return Self::default();
        }
        Self::load_from(path)
    }

    /// Resolve the effective generation policy (rules-first vs LLM-first) from
    /// the `[llm]` config, applying built-in defaults for unset fields.
    pub fn generation_config(&self) -> GenerationConfig {
        GenerationConfig {
            llm_enabled: self.llm.enabled,
            strategy: self.llm.strategy,
            confidence_threshold: self
                .llm
                .confidence_threshold
                .unwrap_or(DEFAULT_CONFIDENCE_THRESHOLD),
        }
    }

    /// Resolve the LLM backend selection (provider + model + effort) from the
    /// `[llm]` config. Independent of `enabled`; the caller decides whether to
    /// use it (see `mdpeek gen --llm`).
    pub fn llm_backend_config(&self) -> LlmBackendConfig {
        LlmBackendConfig {
            provider: self.llm.provider,
            model: self.llm.model.clone(),
            effort: self.llm.effort,
        }
    }

    /// Whether the `[llm]` section opted into LLM generation.
    pub fn llm_enabled(&self) -> bool {
        self.llm.enabled
    }

    fn load_from(path: &Path) -> Self {
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            // A missing config file is the normal case, not an error.
            Err(_) => return Self::default(),
        };
        match toml::from_str(&content) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("mdpeek: warning: failed to parse {}: {e}", path.display());
                Self::default()
            }
        }
    }
}

/// Resolve the path to `config.toml` following the XDG Base Directory spec:
/// `$XDG_CONFIG_HOME/mdpeek/config.toml`, or `~/.config/mdpeek/config.toml`
/// when `XDG_CONFIG_HOME` is unset or empty.
pub fn config_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(base.join("mdpeek").join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_is_all_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.default_mode.is_none());
        assert!(config.server.host.is_none());
        assert!(config.term.pager.is_none());
    }

    #[test]
    fn full_config_parses() {
        let toml = r#"
            default_mode = "term"

            [server]
            host = "0.0.0.0"
            port = "8080"
            theme = "dark"

            [term]
            theme = "nord"
            pager = "less -R"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.default_mode, Some(DefaultMode::Term));
        assert_eq!(config.server.host.as_deref(), Some("0.0.0.0"));
        assert_eq!(config.server.port.as_deref(), Some("8080"));
        assert_eq!(config.server.theme, Some(BrowserTheme::Dark));
        assert_eq!(config.term.theme, Some(ThemeChoice::Nord));
        assert_eq!(config.term.pager.as_deref(), Some("less -R"));
    }

    #[test]
    fn partial_config_keeps_other_defaults() {
        let config: Config = toml::from_str("[server]\nport = \"9999\"").unwrap();
        assert_eq!(config.server.port.as_deref(), Some("9999"));
        assert!(config.server.host.is_none());
        assert!(config.default_mode.is_none());
    }

    #[test]
    fn unknown_key_is_rejected() {
        assert!(toml::from_str::<Config>("bogus = true").is_err());
    }

    #[test]
    fn empty_pager_disables_paging() {
        let config: Config = toml::from_str("[term]\npager = \"\"").unwrap();
        assert_eq!(config.term.pager.as_deref(), Some(""));
    }

    #[test]
    fn llm_defaults_to_rules_first_disabled() {
        let config: Config = toml::from_str("").unwrap();
        let policy = config.generation_config();
        assert!(policy.is_rules_only());
        assert_eq!(policy.strategy, GenerationStrategy::RulesFirst);
        assert!((policy.confidence_threshold - DEFAULT_CONFIDENCE_THRESHOLD).abs() < f32::EPSILON);
    }

    #[test]
    fn llm_first_strategy_parses() {
        let toml = r#"
            [llm]
            enabled = true
            strategy = "llm_first"
            confidence_threshold = 0.8
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        let policy = config.generation_config();
        assert!(!policy.is_rules_only());
        assert_eq!(policy.strategy, GenerationStrategy::LlmFirst);
        assert!((policy.confidence_threshold - 0.8).abs() < f32::EPSILON);
        // llm_first + enabled → always consult the LLM.
        assert!(policy.should_use_llm(0.99));
    }

    #[test]
    fn rules_first_is_the_explicit_alternative() {
        let toml = r#"
            [llm]
            enabled = true
            strategy = "rules_first"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        let policy = config.generation_config();
        assert_eq!(policy.strategy, GenerationStrategy::RulesFirst);
        // rules_first → only escalate low-confidence nodes.
        assert!(policy.should_use_llm(0.2));
        assert!(!policy.should_use_llm(0.95));
    }

    #[test]
    fn llm_unknown_key_is_rejected() {
        assert!(toml::from_str::<Config>("[llm]\nbogus = 1").is_err());
    }

    #[test]
    fn llm_backend_defaults_to_anthropic_api() {
        let config: Config = toml::from_str("").unwrap();
        let backend = config.llm_backend_config();
        assert_eq!(backend.provider, LlmProvider::AnthropicApi);
        assert!(backend.model.is_none());
        assert!(backend.effort.is_none());
    }

    #[test]
    fn llm_backend_provider_model_effort_parse() {
        let toml = r#"
            [llm]
            enabled = true
            provider = "codex"
            model = "gpt-5-codex"
            effort = "high"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.llm_enabled());
        let backend = config.llm_backend_config();
        assert_eq!(backend.provider, LlmProvider::Codex);
        assert_eq!(backend.model.as_deref(), Some("gpt-5-codex"));
        assert_eq!(backend.effort, Some(Effort::High));
    }

    #[test]
    fn llm_claude_code_provider_parses() {
        let config: Config =
            toml::from_str("[llm]\nprovider = \"claude_code\"\neffort = \"medium\"").unwrap();
        let backend = config.llm_backend_config();
        assert_eq!(backend.provider, LlmProvider::ClaudeCode);
        assert_eq!(backend.effort, Some(Effort::Medium));
    }

    #[test]
    fn explicit_missing_path_falls_back_to_defaults() {
        let config = Config::load_explicit(Path::new("/no/such/mdpeek-config.toml"));
        assert!(config.default_mode.is_none());
        assert!(config.server.port.is_none());
    }
}
