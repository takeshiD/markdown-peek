//! LLM-backed generation (design doc §7).
//!
//! Three interchangeable backends, selected by `[llm] provider` in config:
//!
//! - [`claude_code`] — shells out to the `claude` CLI (Claude Code).
//! - [`codex`]       — shells out to the `codex` CLI (OpenAI Codex).
//! - [`anthropic`]   — calls the Anthropic API directly (`feature = "llm"`,
//!   needs `reqwest`/`tokio` + `ANTHROPIC_API_KEY`).
//!
//! The two CLI backends need no extra crates (just `std::process`), so they are
//! available in the default build; only the HTTP backend is feature-gated.
//! Every backend returns **UI IR only**, re-validated by [`crate::ir`] before
//! use — an LLM can never introduce a component outside the registry or a
//! fabricated range (§8).

pub mod claude_code;
pub mod codex;
pub mod prompt;

#[cfg(feature = "llm")]
pub mod anthropic;

use anyhow::{Context, Result};
use clap::ValueEnum;
use serde::Deserialize;

use crate::generator::traits::Generator;
use crate::ir::{LineIndex, UiNode, validate_json};

/// Build the single prompt string CLI backends receive (system + user prompt,
/// plus an optional trailing effort hint). `requested_kinds` empty = model's
/// discretion.
pub(crate) fn build_cli_prompt(markdown: &str, effort_hint: &str) -> String {
    let mut p = format!(
        "{}\n\n{}",
        prompt::system_prompt(),
        prompt::user_prompt(markdown, &[])
    );
    if !effort_hint.is_empty() {
        p.push_str(&format!("\n\n{effort_hint}."));
    }
    p
}

/// Parse CLI stdout into validated UI IR (the §8 security boundary).
pub(crate) fn parse_and_validate(stdout: &str, markdown: &str) -> Result<Vec<UiNode>> {
    let total_lines = LineIndex::new(markdown).line_count();
    let json = prompt::extract_json_array(stdout);
    validate_json(json, total_lines).context("LLM output failed IR validation")
}

/// Which LLM backend to drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
pub enum LlmProvider {
    /// Anthropic API over HTTP (default; requires `--features llm`).
    #[default]
    AnthropicApi,
    /// The `claude` CLI (Claude Code), run in headless print mode.
    ClaudeCode,
    /// The `codex` CLI (OpenAI Codex), run via `codex exec`.
    Codex,
}

/// Reasoning effort, mapped per-backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
#[clap(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
}

impl Effort {
    /// Canonical string (used for Codex `model_reasoning_effort`).
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
        }
    }

    /// Claude Code has no effort flag; steer its thinking budget with a prompt
    /// keyword instead (empty = no hint).
    pub fn claude_think_hint(self) -> &'static str {
        match self {
            Effort::Low => "",
            Effort::Medium => "think",
            Effort::High => "ultrathink",
        }
    }
}

/// Resolved LLM backend selection (provider + optional model + effort). Built
/// from `[llm]` config merged with any `mdpeek gen` CLI overrides.
#[derive(Debug, Clone)]
pub struct LlmBackendConfig {
    pub provider: LlmProvider,
    pub model: Option<String>,
    pub effort: Option<Effort>,
}

impl LlmBackendConfig {
    /// Instantiate the concrete [`Generator`] for this configuration.
    pub fn build(&self) -> Result<Box<dyn Generator>> {
        match self.provider {
            LlmProvider::ClaudeCode => Ok(Box::new(claude_code::ClaudeCodeGenerator::new(
                self.model.clone(),
                self.effort,
            ))),
            LlmProvider::Codex => Ok(Box::new(codex::CodexGenerator::new(
                self.model.clone(),
                self.effort,
            ))),
            LlmProvider::AnthropicApi => {
                #[cfg(feature = "llm")]
                {
                    Ok(Box::new(anthropic::AnthropicApiGenerator::new(
                        self.model.clone(),
                        self.effort,
                    )))
                }
                #[cfg(not(feature = "llm"))]
                {
                    anyhow::bail!(
                        "provider \"anthropic_api\" needs a build with `--features llm`; \
                         use provider \"claude_code\" or \"codex\" for a default build"
                    )
                }
            }
        }
    }
}
