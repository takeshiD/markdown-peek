//! Claude Code backend: drive the local `claude` CLI in headless print mode
//! (design §7). No network crate needed — this works in the default build as
//! long as the `claude` CLI is installed and authenticated.
//!
//! Invocation: `claude -p "<prompt>" --output-format text [--model <model>]`.
//! Effort maps to a thinking-budget keyword appended to the prompt
//! (`think` / `ultrathink`), since Claude Code has no reasoning-effort flag.

use std::process::Command;

use anyhow::{Context, Result};

use super::{Effort, build_cli_prompt, parse_and_validate};
use crate::generator::traits::{GenInput, Generator};
use crate::ir::UiNode;

pub struct ClaudeCodeGenerator {
    model: Option<String>,
    effort: Option<Effort>,
}

impl ClaudeCodeGenerator {
    pub fn new(model: Option<String>, effort: Option<Effort>) -> Self {
        ClaudeCodeGenerator { model, effort }
    }
}

impl Generator for ClaudeCodeGenerator {
    fn generate(&self, input: &GenInput<'_>) -> Result<Vec<UiNode>> {
        let hint = self.effort.map(Effort::claude_think_hint).unwrap_or("");
        let prompt = build_cli_prompt(input.markdown, hint);

        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg(&prompt)
            .arg("--allowed-tools")
            .arg("\"\"")
            .arg("--max-turns")
            .arg("1")
            .arg("--output-format")
            .arg("text");
        if let Some(model) = &self.model {
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

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_and_validate(&stdout, input.markdown)
    }

    fn model_id(&self) -> String {
        format!("claude-code-{}", self.model.as_deref().unwrap_or("default"))
    }
}
