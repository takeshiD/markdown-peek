//! Models that choose reading aids: the `[models.<name>]` entries of the
//! config, checked once when they are read.
//!
//! Two kinds:
//!
//! - **decision** models answer the System One request (`POST {base_url}/v1/systemone`):
//!   TypeSafe Jev, GPT-6 Luna Decision, Gemini Decision 1 (directly or through
//!   OpenCode), Microsoft Decision (Azure Foundry, `api-key` header).
//! - **llm** models are general chat models (Anthropic Messages, OpenAI
//!   Responses, OpenAI-compatible Chat Completions, or the `claude` / `codex`
//!   CLIs). They get the same questions as a JSON prompt and answer with the
//!   same JSON shape.
//!
//! Either way a model only picks option ids among candidates extracted from the
//! document; nothing it writes becomes content.
//!
//! Secrets: an entry names the *environment variable* that holds its key
//! (`api_key_env`). The key value is read from that variable when a request is
//! sent and never stored here, logged, or written anywhere.

use std::path::PathBuf;

use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};

/// Which wire protocol an entry speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Api {
    /// System One decision request (Jev, GPT-6 Luna Decision, Gemini Decision, Microsoft Decision).
    Systemone,
    /// Anthropic Messages API (`/v1/messages`).
    Anthropic,
    /// OpenAI Responses API (`/v1/responses`).
    Openai,
    /// OpenAI-compatible Chat Completions (`/v1/chat/completions`): GLM, DeepSeek, …
    OpenaiChat,
    /// The `claude` CLI (Claude Code), signed in on this machine.
    ClaudeCode,
    /// The `codex` CLI, signed in on this machine.
    Codex,
    /// Scripted answers for tests and E2E (not documented for users).
    Fake,
}

impl Api {
    pub fn as_str(self) -> &'static str {
        match self {
            Api::Systemone => "systemone",
            Api::Anthropic => "anthropic",
            Api::Openai => "openai",
            Api::OpenaiChat => "openai_chat",
            Api::ClaudeCode => "claude_code",
            Api::Codex => "codex",
            Api::Fake => "fake",
        }
    }

    pub fn kind(self) -> ModelKind {
        match self {
            Api::Systemone | Api::Fake => ModelKind::Decision,
            _ => ModelKind::Llm,
        }
    }

    fn default_base_url(self) -> Option<&'static str> {
        match self {
            Api::Systemone => Some("https://api.typesafe.ai"),
            Api::Anthropic => Some("https://api.anthropic.com"),
            Api::Openai => Some("https://api.openai.com"),
            _ => None,
        }
    }

    fn default_key_env(self) -> Option<&'static str> {
        match self {
            Api::Systemone => Some("TYPESAFE_API_KEY"),
            Api::Anthropic => Some("ANTHROPIC_API_KEY"),
            Api::Openai => Some("OPENAI_API_KEY"),
            _ => None,
        }
    }

    fn default_auth(self) -> Auth {
        match self {
            Api::Anthropic => Auth::XApiKey,
            _ => Auth::Bearer,
        }
    }

    /// Whether the protocol talks HTTP with a key.
    pub fn is_http(self) -> bool {
        matches!(
            self,
            Api::Systemone | Api::Anthropic | Api::Openai | Api::OpenaiChat
        )
    }

    /// Path appended to `base_url`.
    pub fn path(self) -> &'static str {
        match self {
            Api::Systemone => "/v1/systemone",
            Api::Anthropic => "/v1/messages",
            Api::Openai => "/v1/responses",
            Api::OpenaiChat => "/v1/chat/completions",
            _ => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Decision,
    Llm,
}

/// How the key travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Auth {
    /// `Authorization: Bearer <key>` (TypeSafe, OpenAI, OpenCode, …).
    Bearer,
    /// `x-api-key: <key>` (Anthropic).
    XApiKey,
    /// `api-key: <key>` (Azure / Microsoft Foundry).
    ApiKey,
}

impl Auth {
    pub fn header(self) -> &'static str {
        match self {
            Auth::Bearer => "authorization",
            Auth::XApiKey => "x-api-key",
            Auth::ApiKey => "api-key",
        }
    }
}

const EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// One `[models.<name>]` table as written in `config.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEntry {
    /// `false` turns the entry off: it is not offered or used, but stays in
    /// the file (default `true`).
    #[serde(alias = "enabled")]
    pub enable: Option<bool>,
    pub kind: Option<ModelKind>,
    pub api: Option<Api>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub auth: Option<Auth>,
    pub effort: Option<String>,
    /// `api = "fake"` only: scripted answers (JSON file).
    pub script: Option<PathBuf>,
    /// Never accepted: present only so the error can say what to do instead.
    /// The value is not kept.
    pub api_key: Option<IgnoredAny>,
}

/// A checked model entry. `problem` is set when it cannot be used as written.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelSpec {
    pub name: String,
    pub kind: ModelKind,
    pub api: Api,
    /// Model id sent to the API (empty for the CLIs' own default).
    pub model: String,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub auth: Auth,
    pub effort: Option<String>,
    #[serde(skip)]
    pub script: Option<PathBuf>,
    /// Problem in the entry itself (missing field, bad value…). Environment
    /// problems (an unset key variable) are checked by [`ModelSpec::problem`].
    #[serde(skip)]
    pub config_problem: Option<String>,
    /// `enable = false`: kept out of [`Models::list`].
    #[serde(skip)]
    pub disabled: bool,
}

impl ModelSpec {
    /// The built-in entry used when the config defines no models.
    pub fn builtin_jev() -> Self {
        ModelSpec::from_entry(
            "jev",
            ModelEntry {
                api: Some(Api::Systemone),
                model: Some("jev-1.14.0".into()),
                ..Default::default()
            },
        )
    }

    /// An entry that could not be read at all (wrong types, unknown keys).
    pub fn broken(name: &str, why: String) -> Self {
        ModelSpec {
            name: name.to_string(),
            kind: ModelKind::Decision,
            api: Api::Systemone,
            model: String::new(),
            base_url: None,
            api_key_env: None,
            auth: Auth::Bearer,
            effort: None,
            script: None,
            config_problem: Some(why),
            disabled: false,
        }
    }

    pub fn from_entry(name: &str, e: ModelEntry) -> Self {
        let mut problems: Vec<String> = Vec::new();
        if e.api_key.is_some() {
            problems.push(
                "api_key is not allowed in the config file; set api_key_env to the name of an environment variable that holds the key".into(),
            );
        }
        let api = match e.api {
            Some(a) => a,
            None => {
                problems.push(
                    "api is missing (systemone, anthropic, openai, openai_chat, claude_code or codex)"
                        .into(),
                );
                Api::Systemone
            }
        };
        if let Some(k) = e.kind
            && k != api.kind()
        {
            problems.push(format!(
                "kind = \"{}\" does not match api = \"{}\"",
                kind_str(k),
                api.as_str()
            ));
        }
        let model = e.model.clone().unwrap_or_default().trim().to_string();
        if model.is_empty() && !matches!(api, Api::ClaudeCode | Api::Codex | Api::Fake) {
            problems.push("model is missing".into());
        }
        let base_url = e
            .base_url
            .clone()
            .map(|u| u.trim().trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty())
            .or_else(|| api.default_base_url().map(str::to_string));
        if api.is_http() {
            match &base_url {
                None => problems.push("base_url is missing".into()),
                Some(u) if !super::base_url_is_safe(u) => {
                    problems.push("base_url must use https (plain http only for localhost)".into())
                }
                _ => {}
            }
        }
        let api_key_env = e
            .api_key_env
            .clone()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .or_else(|| api.default_key_env().map(str::to_string));
        if api.is_http() {
            match &api_key_env {
                None => problems.push("api_key_env is missing".into()),
                Some(v) if !is_env_name(v) => problems.push(
                    "api_key_env must be the name of an environment variable (letters, digits and _), not the key itself".into(),
                ),
                _ => {}
            }
        }
        if let Some(ef) = &e.effort
            && !EFFORTS.contains(&ef.as_str())
        {
            problems.push(format!("effort must be one of {}", EFFORTS.join(", ")));
        }
        ModelSpec {
            name: name.to_string(),
            kind: api.kind(),
            api,
            model,
            base_url,
            api_key_env,
            auth: e.auth.unwrap_or(api.default_auth()),
            effort: e.effort,
            script: e.script,
            config_problem: (!problems.is_empty()).then(|| problems.join("; ")),
            disabled: e.enable == Some(false),
        }
    }

    /// Why this entry cannot be used right now (config or environment), or
    /// `None` when it is ready. Never includes a key value.
    pub fn problem(&self) -> Option<String> {
        if let Some(p) = &self.config_problem {
            return Some(p.clone());
        }
        if self.api.is_http() {
            let var = self.api_key_env.as_deref().unwrap_or("");
            if read_key(var).is_none() {
                return Some(format!("environment variable {var} is not set"));
            }
            if !cfg!(feature = "llm") {
                return Some(
                    "this build has no network support (built without the `llm` feature)".into(),
                );
            }
        }
        None
    }

    /// Stable identity for cached decisions: what actually answers, not the
    /// entry's name (renaming an entry keeps its cache).
    pub fn cache_identity(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.api.as_str(),
            self.base_url.as_deref().unwrap_or(""),
            self.model,
            self.effort.as_deref().unwrap_or("")
        )
    }

    /// Full request URL for HTTP apis.
    pub fn url(&self) -> String {
        format!(
            "{}{}",
            self.base_url.as_deref().unwrap_or("").trim_end_matches('/'),
            self.api.path()
        )
    }

    /// What the UI shows next to the name.
    pub fn label(&self) -> String {
        if self.model.is_empty() {
            self.api.as_str().to_string()
        } else {
            self.model.clone()
        }
    }
}

fn kind_str(k: ModelKind) -> &'static str {
    match k {
        ModelKind::Decision => "decision",
        ModelKind::Llm => "llm",
    }
}

fn is_env_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !s.starts_with(|c: char| c.is_ascii_digit())
}

/// Read a key from the environment at call time (trimmed; empty = unset).
pub fn read_key(var: &str) -> Option<String> {
    if !is_env_name(var) {
        return None;
    }
    std::env::var(var)
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

/// Resolved model registry: the entries in config order and the default.
#[derive(Debug, Clone)]
pub struct Models {
    /// The enabled entries (never empty).
    pub list: Vec<ModelSpec>,
    /// Names of the entries turned off with `enable = false`.
    pub disabled: Vec<String>,
    pub default: String,
    /// Problems with the model settings as a whole (e.g. `[model] use` names
    /// no entry). Shown to the user.
    pub errors: Vec<String>,
}

impl Models {
    /// `entries` in config order; `use_name` from `[model] use`.
    pub fn new(entries: Vec<ModelSpec>, use_name: Option<&str>) -> Self {
        let mut errors = Vec::new();
        let configured = !entries.is_empty();
        let (off, on): (Vec<ModelSpec>, Vec<ModelSpec>) =
            entries.into_iter().partition(|m| m.disabled);
        let disabled: Vec<String> = off.into_iter().map(|m| m.name).collect();
        let list = if !configured {
            vec![ModelSpec::builtin_jev()]
        } else if on.is_empty() {
            // Every entry is off: nothing may be called. A placeholder that
            // cannot be used says why instead of falling back to a model the
            // reader did not ask for.
            let why = "every model in [models] is turned off (enable = false); set enable = true on one of them".to_string();
            errors.push(why.clone());
            vec![ModelSpec::broken(&disabled[0], why)]
        } else {
            on
        };
        let default = match use_name {
            Some(n) if list.iter().any(|m| m.name == n) => n.to_string(),
            Some(n) if disabled.iter().any(|d| d == n) => {
                errors.push(format!(
                    "[model] use = \"{n}\" is turned off (enable = false); using \"{}\"",
                    list[0].name
                ));
                list[0].name.clone()
            }
            Some(n) => {
                errors.push(format!(
                    "[model] use = \"{n}\" names no [models.{n}] entry; using \"{}\"",
                    list[0].name
                ));
                list[0].name.clone()
            }
            None => list[0].name.clone(),
        };
        Models {
            list,
            disabled,
            default,
            errors,
        }
    }

    pub fn get(&self, name: &str) -> Option<&ModelSpec> {
        self.list.iter().find(|m| m.name == name)
    }

    /// Why `name` cannot be picked: turned off, or no such entry.
    pub fn unavailable(&self, name: &str) -> String {
        if self.disabled.iter().any(|d| d == name) {
            format!("model \"{name}\" is turned off ([models.{name}] enable = false)")
        } else {
            format!("no model named \"{name}\" in [models]")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(api: Api, model: &str) -> ModelEntry {
        ModelEntry {
            api: Some(api),
            model: Some(model.into()),
            ..Default::default()
        }
    }

    #[test]
    fn defaults_per_api() {
        let j = ModelSpec::from_entry("jev", entry(Api::Systemone, "jev-1.14"));
        assert_eq!(j.kind, ModelKind::Decision);
        assert_eq!(j.url(), "https://api.typesafe.ai/v1/systemone");
        assert_eq!(j.api_key_env.as_deref(), Some("TYPESAFE_API_KEY"));
        assert_eq!(j.auth, Auth::Bearer);
        assert!(j.config_problem.is_none());

        let a = ModelSpec::from_entry("sonnet", entry(Api::Anthropic, "claude-sonnet-5-5"));
        assert_eq!(a.kind, ModelKind::Llm);
        assert_eq!(a.auth, Auth::XApiKey);
        assert_eq!(a.url(), "https://api.anthropic.com/v1/messages");
    }

    #[test]
    fn opencode_and_microsoft_urls() {
        let mut e = entry(Api::Systemone, "gemini-decision-1");
        e.base_url = Some("https://opencode.ai/inference/systemone/".into());
        e.api_key_env = Some("OPENCODE_API_KEY".into());
        let g = ModelSpec::from_entry("gemini", e);
        assert_eq!(
            g.url(),
            "https://opencode.ai/inference/systemone/v1/systemone"
        );

        let mut e = entry(Api::Systemone, "my-deployment");
        e.base_url = Some("https://res.services.ai.azure.com/providers/microsoft".into());
        e.api_key_env = Some("AZURE_API_KEY".into());
        e.auth = Some(Auth::ApiKey);
        let m = ModelSpec::from_entry("microsoft", e);
        assert_eq!(
            m.url(),
            "https://res.services.ai.azure.com/providers/microsoft/v1/systemone"
        );
        assert_eq!(m.auth.header(), "api-key");
    }

    #[test]
    fn problems_are_explained() {
        let e = ModelEntry {
            api: Some(Api::OpenaiChat),
            effort: Some("huge".into()),
            api_key: Some(IgnoredAny),
            ..Default::default()
        };
        let s = ModelSpec::from_entry("glm", e);
        let p = s.config_problem.unwrap();
        assert!(p.contains("api_key is not allowed"), "{p}");
        assert!(p.contains("model is missing"), "{p}");
        assert!(p.contains("base_url is missing"), "{p}");
        assert!(p.contains("api_key_env is missing"), "{p}");
        assert!(p.contains("effort must be one of"), "{p}");

        let mut e = entry(Api::Systemone, "jev");
        e.base_url = Some("http://example.com".into());
        e.api_key_env = Some("sk-ant-1234".into());
        let p = ModelSpec::from_entry("x", e).config_problem.unwrap();
        assert!(p.contains("https"), "{p}");
        assert!(p.contains("not the key itself"), "{p}");
    }

    #[test]
    fn unset_key_is_a_problem_without_the_value() {
        let mut e = entry(Api::Systemone, "jev");
        e.api_key_env = Some("MDPEEK_TEST_SURELY_UNSET_KEY".into());
        let s = ModelSpec::from_entry("jev", e);
        assert_eq!(
            s.problem().as_deref(),
            Some("environment variable MDPEEK_TEST_SURELY_UNSET_KEY is not set")
        );
    }

    #[test]
    fn registry_default_and_unknown_use() {
        let m = Models::new(vec![], None);
        assert_eq!(m.default, "jev");
        let list = vec![
            ModelSpec::from_entry("a", entry(Api::Fake, "")),
            ModelSpec::from_entry("b", entry(Api::Fake, "")),
        ];
        let m = Models::new(list.clone(), Some("b"));
        assert_eq!(m.default, "b");
        assert!(m.errors.is_empty());
        let m = Models::new(list, Some("zzz"));
        assert_eq!(m.default, "a");
        assert_eq!(m.errors.len(), 1);
    }

    #[test]
    fn disabled_entries_are_not_offered() {
        let off = |n: &str| {
            let mut e = entry(Api::Fake, "");
            e.enable = Some(false);
            ModelSpec::from_entry(n, e)
        };
        let on = ModelSpec::from_entry("b", entry(Api::Fake, ""));
        let m = Models::new(vec![off("a"), on.clone()], None);
        assert_eq!(m.list.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["b"]);
        assert_eq!(m.default, "b");
        assert_eq!(m.disabled, ["a"]);
        assert!(m.get("a").is_none());
        assert!(m.unavailable("a").contains("turned off"), "{}", m.unavailable("a"));
        assert!(m.unavailable("z").contains("no model named"));

        let m = Models::new(vec![off("a"), on], Some("a"));
        assert_eq!(m.default, "b");
        assert!(m.errors[0].contains("is turned off"), "{:?}", m.errors);

        // All off: no fallback to the built-in model; the placeholder says why.
        let m = Models::new(vec![off("a"), off("c")], Some("a"));
        assert_eq!(m.list.len(), 1);
        assert!(m.list[0].problem().unwrap().contains("every model"));
        assert_eq!(m.errors.len(), 1, "{:?}", m.errors);
    }

    #[test]
    fn cache_identity_ignores_the_entry_name() {
        let a = ModelSpec::from_entry("one", entry(Api::Systemone, "jev-1.14"));
        let b = ModelSpec::from_entry("two", entry(Api::Systemone, "jev-1.14"));
        assert_eq!(a.cache_identity(), b.cache_identity());
    }
}
