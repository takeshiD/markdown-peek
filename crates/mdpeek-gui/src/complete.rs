//! Raw text completion across the same LLM backends used for UI-IR generation
//! (design §7), but returning the model's **plain text** with no IR validation.
//!
//! UI-IR generation ([`crate::generator::llm`]) constrains the model to a JSON
//! array of registry components. Some features instead need free-form or ad-hoc
//! JSON output — the server's Generative Scrollytelling commentary, for example.
//! This module reuses the exact backend selection (provider + model + effort) so
//! that both paths honour the same `[llm]` config, and stays offline-safe: the
//! caller decides what to do when no backend/key is available.

use anyhow::{Context, Result};

use crate::generator::llm::{Effort, LlmBackendConfig, LlmProvider};

/// Run a single completion against the configured backend and return the raw
/// response text (trimmed).
///
/// Blocking: the CLI backends shell out via `std::process`; the Anthropic
/// backend blocks on a scoped current-thread runtime (mirroring
/// `Generator for AnthropicApiGenerator`), so callers should invoke this off the
/// async executor (e.g. `tokio::task::spawn_blocking`).
pub fn complete_text_blocking(
    backend: &LlmBackendConfig,
    system: &str,
    user: &str,
) -> Result<String> {
    match backend.provider {
        LlmProvider::ClaudeCode => claude_code_complete(backend, system, user),
        LlmProvider::Codex => codex_complete(backend, system, user),
        LlmProvider::AnthropicApi => {
            #[cfg(feature = "llm")]
            {
                anthropic_complete(backend, system, user)
            }
            #[cfg(not(feature = "llm"))]
            {
                let _ = (system, user);
                anyhow::bail!(
                    "provider \"anthropic_api\" needs a build with `--features llm`; \
                     use provider \"claude_code\" or \"codex\" for a default build"
                )
            }
        }
    }
}

/// `claude -p "<system>\n\n<user>" --allowed-tools "" --max-turns 1
/// --output-format text [--model <m>]`. Effort maps to a thinking keyword.
fn claude_code_complete(backend: &LlmBackendConfig, system: &str, user: &str) -> Result<String> {
    use std::process::Command;

    let mut prompt = format!("{system}\n\n{user}");
    let hint = backend.effort.map(Effort::claude_think_hint).unwrap_or("");
    if !hint.is_empty() {
        prompt.push_str(&format!("\n\n{hint}."));
    }

    let mut cmd = Command::new("claude");
    cmd.arg("-p")
        .arg(&prompt)
        .arg("--allowed-tools")
        .arg("\"\"")
        .arg("--max-turns")
        .arg("1")
        .arg("--output-format")
        .arg("text");
    if let Some(model) = &backend.model {
        cmd.arg("--model").arg(model);
    }

    let output = cmd
        .output()
        .context("failed to run `claude` (Claude Code CLI); is it installed and on PATH?")?;
    if !output.status.success() {
        anyhow::bail!(
            "claude exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// `codex exec [--model <m>] [-c model_reasoning_effort="<e>"] "<prompt>"`.
fn codex_complete(backend: &LlmBackendConfig, system: &str, user: &str) -> Result<String> {
    use std::process::Command;

    let prompt = format!("{system}\n\n{user}");

    let mut cmd = Command::new("codex");
    cmd.arg("exec");
    if let Some(model) = &backend.model {
        cmd.arg("--model").arg(model);
    }
    if let Some(effort) = backend.effort {
        cmd.arg("-c")
            .arg(format!("model_reasoning_effort=\"{}\"", effort.as_str()));
    }
    cmd.arg(&prompt);

    let output = cmd
        .output()
        .context("failed to run `codex` (Codex CLI); is it installed and on PATH?")?;
    if !output.status.success() {
        anyhow::bail!(
            "codex exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Anthropic Messages API completion, feature-gated like the IR generator.
/// Blocks on a scoped runtime so the sync signature holds across backends.
#[cfg(feature = "llm")]
fn anthropic_complete(backend: &LlmBackendConfig, system: &str, user: &str) -> Result<String> {
    const API_URL: &str = "https://api.anthropic.com/v1/messages";
    const API_VERSION: &str = "2023-06-01";
    const DEFAULT_MODEL: &str = "claude-sonnet-5";

    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;
    let model = backend
        .model
        .clone()
        .or_else(|| std::env::var("MDPEEK_LLM_MODEL").ok())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    let mut body = serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "system": system,
        "messages": [{ "role": "user", "content": user }],
    });
    if let Some(effort) = backend.effort {
        body["output_config"] = serde_json::json!({ "effort": effort.as_str() });
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to build runtime for Anthropic completion")?;
    rt.block_on(async {
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
        Ok(text.trim().to_string())
    })
}
