//! Generated-UI cache (design doc §6).
//!
//! - [`key`]   — content hash = `hash(markdown) + generator + schema version`.
//! - [`store`] — read/write `GuiCacheEntry` under `.cache/mdpeek/`.

pub mod key;
pub mod store;

#[allow(unused_imports)]
pub use key::{SCHEMA_VERSION, content_hash};
pub use store::{CacheStore, GuiCacheEntry};
