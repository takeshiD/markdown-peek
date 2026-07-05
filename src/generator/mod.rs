//! Generator layer (design doc §3.4 / §7): UI plan + document → UI IR.
//!
//! - [`rules`] — `RulesGenerator`, the deterministic offline default.
//! - [`llm`]   — `ClaudeGenerator` (`feature = "llm"`), fills only what rules
//!   can't and falls back to rules when `ANTHROPIC_API_KEY` is unset.
//!
//! The [`traits`] module defines the `Generator` contract and the lightweight
//! [`traits::GenInput`] stand-in for Layer 2's `DocumentModel`.

pub mod llm;
pub mod rules;
pub mod traits;

pub use rules::RulesGenerator;
#[allow(unused_imports)]
pub use traits::{DocType, GenInput, Generator};
