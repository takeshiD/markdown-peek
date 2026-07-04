//! `mdpeek-core` — the reusable core of markdown-peek.
//!
//! This crate holds the layer-independent Markdown machinery so the `mdpeek`
//! binary (and future `-cli` / `-server` / `-tui` crates) can share it. Today
//! it exposes the GFM stream adapters and the SourceRange-aware
//! [`parser::BlockTree`]; later layers (analyzer / model / IR / generator) land
//! here too. See `AGENTS.md` §2 / §10 Layer 1.

pub mod gfm;
pub mod parser;
