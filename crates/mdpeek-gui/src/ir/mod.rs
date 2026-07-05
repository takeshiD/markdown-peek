//! UI IR — the canonical wire format for Generative UI (design doc §4.1).
//!
//! Layout mirrors the design's `mdpeek-core::ir`:
//! - [`node`]     — `UiNode` enum + all node payload types (source of truth).
//! - [`range`]    — `SourceRange` + `LineIndex` (byte offset → line/col).
//! - [`registry`] — component allowlist (security boundary).
//! - [`validate`] — schema + allowlist + sourceRange verification.

pub mod node;
pub mod range;
pub mod registry;
pub mod validate;

#[allow(unused_imports)]
pub use node::{Origin, Quantity, Severity, UiNode, Visibility};
pub use range::{LineIndex, SourceRange};
#[allow(unused_imports)]
pub use validate::{ValidateError, validate_json, validate_nodes};
