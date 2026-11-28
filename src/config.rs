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
use mdpeek_gui::{Effort, LlmBackendConfig, LlmProvider};
use mdpeek_gui::select::models::ModelEntry;
use mdpeek_gui::select::{ModelSpec, Models, SelectConfig};
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
    /// GenUI toggle defaults (`[genui]`; the old name `[aids]` is still read).
    #[serde(alias = "aids")]
    pub genui: GenuiSection,
    /// Which model chooses reading aids, and shared tuning (`[model]`).
    pub model: ModelSection,
    /// Models that can choose reading aids (`[models.<name>]`), in file order.
    pub models: toml::Table,
    /// Decision cache on disk (`[cache]`).
    pub cache: CacheSection,
    /// Why the file could not be read (shown in the UI), if it could not.
    #[serde(skip)]
    pub load_error: Option<String>,
    /// The file this configuration came from (or would be written to).
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

/// `[genui]` section: the GenUI toggle.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GenuiSection {
    /// Initial state of the toggle: `"on"` (default) or `"off"`.
    pub default: Option<OnOff>,
    /// Start choosing aids as soon as a document opens (default `true`); when
    /// `false`, the reader starts it.
    pub auto_generate: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnOff {
    On,
    Off,
}

/// `[model]` section. There is deliberately **no key field** anywhere: each
/// `[models.<name>]` entry names the environment variable that holds its key.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModelSection {
    /// The default model: a key of `[models]`.
    #[serde(rename = "use")]
    pub use_: Option<String>,
    pub timeout_ms: Option<u64>,
    pub excerpt_chars: Option<usize>,
    pub max_state_chars: Option<usize>,
    pub batch_sections: Option<usize>,
    pub confidence_threshold: Option<f32>,
    pub helps_threshold: Option<f32>,
    pub debounce_ms: Option<u64>,
}

/// `[cache]` section.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CacheSection {
    pub enabled: Option<bool>,
    /// Default: `$XDG_CACHE_HOME/mdpeek` (else `~/.cache/mdpeek`).
    pub dir: Option<PathBuf>,
    pub max_size_mb: Option<u64>,
    pub ttl_days: Option<u64>,
    pub memory_entries: Option<usize>,
}

/// Resolved `[cache]`.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheSettings {
    /// `None` = memory only (disabled, or no usable directory).
    pub dir: Option<PathBuf>,
    pub max_size_mb: u64,
    pub ttl_days: u64,
    pub memory_entries: usize,
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

/// `[llm]` section: the LLM behind Guided reading's commentary. (Reading aids
/// use `[model]` / `[models]`.)
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LlmConfig {
    /// Whether Guided reading may call the LLM. When `false` (default) its
    /// commentary comes from deterministic rules.
    pub enabled: bool,
    /// Removed with `mdpeek gen` (accepted only to say so).
    pub strategy: Option<toml::Value>,
    /// Removed with `mdpeek gen` (accepted only to say so).
    pub confidence_threshold: Option<toml::Value>,
    /// Which LLM backend to drive: `"anthropic_api"` (default),
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
            return Self {
                path: Some(path.to_path_buf()),
                ..Self::default()
            };
        }
        Self::load_from(path)
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

    /// Resolve `[model]` onto the built-in defaults (timeout clamped to
    /// 1 s..=5 min so a request always ends within a bounded time).
    pub fn select_config(&self) -> SelectConfig {
        let m = &self.model;
        let d = SelectConfig::default();
        SelectConfig {
            timeout_ms: m.timeout_ms.unwrap_or(d.timeout_ms).clamp(1_000, 300_000),
            confidence_threshold: unit_or(m.confidence_threshold, d.confidence_threshold),
            helps_threshold: unit_or(m.helps_threshold, d.helps_threshold),
            excerpt_chars: m.excerpt_chars.unwrap_or(d.excerpt_chars),
            max_state_chars: m.max_state_chars.unwrap_or(d.max_state_chars),
            batch_sections: m.batch_sections.unwrap_or(d.batch_sections).clamp(1, 64),
            debounce_ms: m.debounce_ms.unwrap_or(d.debounce_ms),
        }
    }

    /// The `[models]` entries (config order) and the default from `[model] use`.
    /// An entry that cannot be read is kept with its problem, so the UI can
    /// show it as invalid instead of it silently disappearing.
    pub fn models(&self) -> Models {
        let list = self
            .models
            .iter()
            .map(|(name, v)| match v.clone().try_into::<ModelEntry>() {
                Ok(e) => ModelSpec::from_entry(name, e),
                Err(e) => ModelSpec::broken(name, redact_quoted(e.message().trim_end())),
            })
            .collect();
        Models::new(list, self.model.use_.as_deref())
    }

    /// Problems with the configuration as a whole, for the UI.
    pub fn errors(&self, models: &Models) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(e) = &self.load_error {
            out.push(e.clone());
        }
        if self.llm.strategy.is_some() || self.llm.confidence_threshold.is_some() {
            out.push(
                "[llm] strategy / confidence_threshold were used by `mdpeek gen`, which was removed; delete them".into(),
            );
        }
        out.extend(models.errors.iter().cloned());
        out
    }

    /// Resolve `[cache]` (directory: `[cache] dir`, else `$XDG_CACHE_HOME/mdpeek`).
    pub fn cache_settings(&self) -> CacheSettings {
        let c = &self.cache;
        let dir = if c.enabled == Some(false) {
            None
        } else {
            c.dir
                .as_ref()
                .map(|d| expand_home(d))
                .or_else(mdpeek_gui::select::default_cache_dir)
        };
        CacheSettings {
            dir,
            max_size_mb: c.max_size_mb.unwrap_or(100).max(1),
            ttl_days: c.ttl_days.unwrap_or(30).max(1),
            memory_entries: c.memory_entries.unwrap_or(256).max(1),
        }
    }

    /// `[genui]`: (toggle on by default, start automatically).
    pub fn genui(&self) -> (bool, bool) {
        (
            self.genui.default != Some(OnOff::Off),
            self.genui.auto_generate.unwrap_or(true),
        )
    }

    /// Whether the `[llm]` section opted into LLM generation.
    pub fn llm_enabled(&self) -> bool {
        self.llm.enabled
    }

    fn load_from(path: &Path) -> Self {
        let at = |c: Config| Config {
            path: Some(path.to_path_buf()),
            ..c
        };
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            // A missing config file is the normal case, not an error.
            Err(_) => return at(Self::default()),
        };
        match toml::from_str::<Config>(&content) {
            Ok(config) => at(config),
            Err(e) => {
                // Print only the message (not the offending source line), so a
                // secret mistakenly pasted into config.toml is never echoed.
                let line = e
                    .span()
                    .map(|s| content[..s.start.min(content.len())].matches('\n').count() + 1);
                let msg = format!(
                    "{} could not be read{}: {}",
                    path.display(),
                    line.map(|l| format!(" (line {l})")).unwrap_or_default(),
                    redact_quoted(e.message().trim_end())
                );
                eprintln!("mdpeek: warning: {msg}");
                at(Config {
                    load_error: Some(msg),
                    ..Self::default()
                })
            }
        }
    }
}

/// A probability threshold from config: NaN / out-of-range values would make
/// every comparison pass or fail silently, so they fall back to the default
/// (NaN) or are clamped into `0.0..=1.0`.
fn unit_or(v: Option<f32>, default: f32) -> f32 {
    match v {
        Some(x) if x.is_nan() => default,
        Some(x) => x.clamp(0.0, 1.0),
        None => default,
    }
}

/// Replace quoted fragments in a TOML error message (`invalid type: string
/// "sk-…"`) so a secret pasted into the wrong key is never echoed to stderr.
fn redact_quoted(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len());
    let mut inside = false;
    for ch in msg.chars() {
        match (ch, inside) {
            ('"', false) => {
                inside = true;
                out.push_str("\"…");
            }
            ('"', true) => {
                inside = false;
                out.push('"');
            }
            (_, true) => {}
            (c, false) => out.push(c),
        }
    }
    out
}

/// `~/x` → `$HOME/x`.
fn expand_home(p: &Path) -> PathBuf {
    match p.strip_prefix("~") {
        Ok(rest) => std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(rest))
            .unwrap_or_else(|| p.to_path_buf()),
        Err(_) => p.to_path_buf(),
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
    fn model_defaults() {
        let config: Config = toml::from_str("").unwrap();
        let s = config.select_config();
        assert_eq!(s.timeout_ms, 30_000);
        assert_eq!(s.batch_sections, 8);
        assert_eq!(s.debounce_ms, 800);
        let m = config.models();
        assert_eq!(m.default, "jev");
        assert_eq!(m.list.len(), 1);
        assert_eq!(config.genui(), (true, true));
    }

    #[test]
    fn models_keep_file_order_and_report_problems() {
        let config: Config = toml::from_str(
            r#"
            [model]
            use = "sonnet"
            timeout_ms = 999999

            [models.sonnet]
            api = "anthropic"
            model = "claude-sonnet-5-5"

            [models.jev]
            api = "systemone"
            model = "jev-1.14"
            api_key = "sk-secret-value"

            [models.weird]
            api = "teleport"
            "#,
        )
        .unwrap();
        assert_eq!(config.select_config().timeout_ms, 300_000);
        let m = config.models();
        let names: Vec<_> = m.list.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["sonnet", "jev", "weird"]);
        assert_eq!(m.default, "sonnet");
        let jev = m.get("jev").unwrap().config_problem.clone().unwrap();
        assert!(jev.contains("api_key is not allowed"), "{jev}");
        assert!(!jev.contains("sk-secret-value"));
        let weird = m.get("weird").unwrap().config_problem.clone().unwrap();
        assert!(!weird.is_empty());
        assert!(config.errors(&m).is_empty());
    }

    #[test]
    fn enable_false_turns_an_entry_off() {
        let config: Config = toml::from_str(
            r#"
            [model]
            use = "jev"

            [models.jev]
            enable = false
            api = "systemone"
            model = "jev-1.14.0"

            [models.haiku]
            enable = true
            api = "anthropic"
            model = "claude-haiku-5-5"
            "#,
        )
        .unwrap();
        let m = config.models();
        let names: Vec<_> = m.list.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["haiku"]);
        assert_eq!(m.default, "haiku");
        let errs = config.errors(&m);
        assert!(errs[0].contains("use = \"jev\" is turned off"), "{errs:?}");
    }

    #[test]
    fn select_section_is_unknown() {
        assert!(toml::from_str::<Config>("[select]\nprovider = \"jev\"\n").is_err());
    }

    #[test]
    fn cache_dir_and_genui() {
        let config: Config = toml::from_str(
            "[cache]\ndir = \"/tmp/x\"\nmax_size_mb = 5\n[genui]\ndefault = \"off\"\nauto_generate = false\n",
        )
        .unwrap();
        // The old section name is still read.
        let old: Config = toml::from_str("[aids]\ndefault = \"off\"\n").unwrap();
        assert_eq!(old.genui(), (false, true));
        let c = config.cache_settings();
        assert_eq!(c.dir.as_deref(), Some(Path::new("/tmp/x")));
        assert_eq!(c.max_size_mb, 5);
        assert_eq!(config.genui(), (false, false));
        let off: Config = toml::from_str("[cache]\nenabled = false\n").unwrap();
        assert!(off.cache_settings().dir.is_none());
    }

    #[test]
    fn unreadable_config_keeps_the_reason() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.toml");
        std::fs::write(&p, "[model]\nuse = \"sk-very-secret\"\nbogus = 1\n").unwrap();
        let c = Config::load_explicit(&p);
        let e = c.load_error.unwrap();
        assert!(e.contains("line"), "{e}");
        assert!(!e.contains("sk-very-secret"));
        assert_eq!(c.path.as_deref(), Some(p.as_path()));
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
    fn removed_gen_settings_are_explained() {
        let config: Config =
            toml::from_str("[llm]\nenabled = true\nstrategy = \"llm_first\"\nconfidence_threshold = 0.8\n").unwrap();
        assert!(config.llm_enabled());
        let errs = config.errors(&config.models());
        assert!(errs[0].contains("`mdpeek gen`"), "{errs:?}");
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
