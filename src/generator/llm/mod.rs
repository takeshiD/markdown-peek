//! LLM-backed generation (`feature = "llm"`), design doc §7.
//!
//! Only nodes that rules can't produce are delegated here, and every result is
//! re-validated by `ir::validate_json` before use. Falls back to rules when no
//! API key is configured.

pub mod claude;
pub mod prompt;

#[allow(unused_imports)]
pub use claude::ClaudeGenerator;
