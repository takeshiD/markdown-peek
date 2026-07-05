//! Anthropic API adapter (design doc §7), behind `feature = "llm"`.
//!
//! Contract: send the document + schema constraints, receive **UI IR JSON
//! only**, then run it through [`crate::ir::validate_json`] (schema + registry
//! allowlist + sourceRange bounds). Anything that fails validation is dropped —
//! an LLM can never introduce a component outside the registry or a fabricated
//! range.
//!
//! Offline-safe: when `ANTHROPIC_API_KEY` is unset, generation falls back to
//! `RulesGenerator` so the experience never hard-depends on network or
//! credentials (design §7 "未設定なら自動で rules-only にフォールバック").

use anyhow::{Context, Result};

use super::prompt;
use crate::generator::traits::{GenInput, Generator};
use crate::ir::{LineIndex, UiNode, validate_json};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
/// Used when no model is configured; a current Claude model.
const DEFAULT_MODEL: &str = "claude-sonnet-5";

pub struct AnthropicApiGenerator {
    model: String,
}

impl AnthropicApiGenerator {
    /// Create with an explicit model, or `None` to use `MDPEEK_LLM_MODEL` /
    /// the built-in default.
    pub fn new(model: Option<String>) -> Self {
        let model = model
            .or_else(|| std::env::var("MDPEEK_LLM_MODEL").ok())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        AnthropicApiGenerator { model }
    }

    /// Generate UI IR via the Anthropic API, validating the result. Falls back
    /// to rules when no API key is set. Async because the server drives it
    /// inside tokio (design §7).
    pub async fn generate_async(&self, input: &GenInput<'_>) -> Result<Vec<UiNode>> {
        // No key → error out so the pipeline falls back to the rules planner
        // (which produces reading lenses, not body reprints).
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

        let total_lines = LineIndex::new(input.markdown).line_count();

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": prompt::system_prompt(),
            "messages": [{
                "role": "user",
                "content": prompt::user_prompt(input.markdown, &[]),
            }],
        });

        let client = reqwest::Client::new();
        let resp = client
            .post(API_URL)
            .header("x-api-key", api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Anthropic request failed")?
            .error_for_status()
            .context("Anthropic returned an error status")?;

        let json: serde_json::Value = resp.json().await.context("invalid Anthropic response")?;
        let text = json["content"][0]["text"]
            .as_str()
            .context("Anthropic response missing content text")?;

        let cleaned = prompt::strip_code_fence(text);
        // The security boundary: schema + allowlist + range verification.
        validate_json(cleaned, total_lines).context("LLM output failed validation")
    }
}

/// Blocking `Generator` impl so the API backend can be used from sync call sites
/// (fronts [`AnthropicApiGenerator::generate_async`] on a scoped runtime).
/// Server code should prefer `generate_async` directly.
impl Generator for AnthropicApiGenerator {
    fn generate(&self, input: &GenInput<'_>) -> Result<Vec<UiNode>> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to build runtime for AnthropicApiGenerator")?;
        rt.block_on(self.generate_async(input))
    }

    fn model_id(&self) -> String {
        format!("anthropic-{}", self.model)
    }
}
