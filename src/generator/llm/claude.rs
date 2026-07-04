//! Anthropic Claude adapter (design doc §7), behind `feature = "llm"`.
//!
//! Contract: send the document + schema constraints, receive **UI IR JSON
//! only**, then run it through [`crate::ir::validate_json`] (schema + registry
//! allowlist + sourceRange bounds). Anything that fails validation is dropped —
//! an LLM can never introduce a component outside the registry or a fabricated
//! range.
//!
//! Offline-safe: when `ANTHROPIC_API_KEY` is unset, [`ClaudeGenerator`] falls
//! back to `RulesGenerator` so the default experience never depends on network
//! or credentials (design §7 "未設定なら自動で rules-only にフォールバック").

use anyhow::{Context, Result};

use crate::generator::rules::RulesGenerator;
use crate::generator::traits::{GenInput, Generator};
use crate::ir::{LineIndex, UiNode, validate_json};

use super::prompt;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
/// Overridable via `MDPEEK_LLM_MODEL`; defaults to a current Claude model.
const DEFAULT_MODEL: &str = "claude-sonnet-5";

pub struct ClaudeGenerator {
    model: String,
    /// Node kinds the planner wants the LLM to fill. Empty = model's discretion.
    requested_kinds: Vec<String>,
}

impl Default for ClaudeGenerator {
    fn default() -> Self {
        ClaudeGenerator {
            model: std::env::var("MDPEEK_LLM_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
            requested_kinds: Vec::new(),
        }
    }
}

impl ClaudeGenerator {
    pub fn with_requested_kinds(mut self, kinds: Vec<String>) -> Self {
        self.requested_kinds = kinds;
        self
    }

    /// Generate UI IR via Claude, validating the result. Falls back to rules on
    /// missing key. Async because the server drives it inside tokio (design §7).
    pub async fn generate_async(&self, input: &GenInput<'_>) -> Result<Vec<UiNode>> {
        let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") else {
            // Offline fallback: deterministic rules output.
            return RulesGenerator.generate(input);
        };

        let total_lines = LineIndex::new(input.markdown).line_count();
        let asks: Vec<&str> = self.requested_kinds.iter().map(String::as_str).collect();

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": prompt::system_prompt(),
            "messages": [{
                "role": "user",
                "content": prompt::user_prompt(input.markdown, &asks),
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
            .context("Claude request failed")?
            .error_for_status()
            .context("Claude returned an error status")?;

        let json: serde_json::Value = resp.json().await.context("invalid Claude response")?;
        let text = json["content"][0]["text"]
            .as_str()
            .context("Claude response missing content text")?;

        let cleaned = prompt::strip_code_fence(text);
        // The security boundary: schema + allowlist + range verification.
        let nodes = validate_json(cleaned, total_lines).context("LLM output failed validation")?;
        Ok(nodes)
    }
}

/// Blocking `Generator` impl so `ClaudeGenerator` can be used from sync call
/// sites; it just fronts [`ClaudeGenerator::generate_async`] on a scoped
/// runtime. Server code should prefer `generate_async` directly.
impl Generator for ClaudeGenerator {
    fn generate(&self, input: &GenInput<'_>) -> Result<Vec<UiNode>> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to build runtime for ClaudeGenerator")?;
        rt.block_on(self.generate_async(input))
    }

    fn model_id(&self) -> String {
        format!("claude-{}", self.model)
    }
}
