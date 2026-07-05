//! Codex backend: drive the local `codex` CLI non-interactively (design §7).
//! No network crate needed — works in the default build when the `codex` CLI is
//! installed and authenticated.
//!
//! Invocation: `codex exec [--model <model>] [-c model_reasoning_effort="<e>"]
//! "<prompt>"`. Codex supports a reasoning-effort config key, so `effort` maps
//! to it directly.

use std::process::Command;

use anyhow::{Context, Result};

use super::{Effort, build_cli_prompt, parse_and_validate};
use crate::generator::traits::{GenInput, Generator};
use crate::ir::UiNode;

pub struct CodexGenerator {
    model: Option<String>,
    effort: Option<Effort>,
}

impl CodexGenerator {
    pub fn new(model: Option<String>, effort: Option<Effort>) -> Self {
        CodexGenerator { model, effort }
    }
}

impl Generator for CodexGenerator {
    fn generate(&self, input: &GenInput<'_>) -> Result<Vec<UiNode>> {
        // Codex takes reasoning effort as a config key, not a prompt hint.
        let prompt = build_cli_prompt(input.markdown, "");

        let mut cmd = Command::new("codex");
        cmd.arg("exec");
        if let Some(model) = &self.model {
            cmd.arg("--model").arg(model);
        }
        if let Some(effort) = self.effort {
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

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_and_validate(&stdout, input.markdown)
    }

    fn model_id(&self) -> String {
        format!("codex-{}", self.model.as_deref().unwrap_or("default"))
    }
}
