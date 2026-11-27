//! Layer 3 core (design doc §2 `mdpeek-core`): the UI IR and the adaptive
//! reading-aid pipeline.
//!
//! - [`ir`]       — `UiNode` wire format + validation + registry allowlist.
//! - [`planner`]  — document-level reading lenses (candidates for [`select`]).
//! - [`select`]   — candidates per section, chosen by a model (`[models]`).
//! - [`complete`] — plain-text completion for Guided reading (`[llm]`).
//!
//! UI-IR generation by an LLM (`mdpeek gen`, `/api/gui`) was removed; it is to
//! be reconsidered in issue #58.

pub mod complete;
pub mod ir;
pub mod planner;
pub mod select;

pub use complete::{Effort, LlmBackendConfig, LlmProvider, complete_text_blocking};
