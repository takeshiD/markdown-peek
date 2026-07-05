//! Inspect the Layer 2 rules analysis of a markdown file.
//!
//! ```sh
//! cargo run -p mdpeek-analyzer --example analyze -- docs/sample-design-doc.md
//! ```
//!
//! Prints the rules-stage `DocumentModel` / `SemanticPanel` plus the code-block
//! and table analyses — i.e. exactly the deterministic material Layer 3's
//! `RulesGenerator` will turn into UI components.

use mdpeek_analyzer::analyzer::{code, table};
use std::env;
use std::fs;

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: analyze <file.md>");
        std::process::exit(2);
    });
    let src = fs::read_to_string(&path).expect("read markdown file");
    let a = mdpeek_analyzer::analyze(&src, Some(&path));

    println!("== document type ==");
    println!(
        "  {:?}  (confidence {:.2}, by {:?})",
        a.model.doc_type.value, a.model.doc_type.confidence, a.model.doc_type.by
    );

    println!("\n== outline ==");
    for row in &a.panel.outline {
        println!(
            "  {}{}  (L{})",
            "  ".repeat((row.level.saturating_sub(1)) as usize),
            row.title,
            row.link.range.start_line
        );
    }

    println!("\n== todos ==");
    for t in &a.panel.todos {
        let mark = if t.done { "[x]" } else { "[ ]" };
        println!("  {mark} {} ({}, L{})", t.text, t.marker, t.link.range.start_line);
    }

    println!("\n== risks ==");
    for e in &a.panel.risks {
        println!("  - {} (L{})", e.text, e.link.range.start_line);
    }

    println!("\n== open questions ==");
    for e in &a.panel.open_questions {
        println!("  - {} (L{})", e.text, e.link.range.start_line);
    }

    println!("\n== code blocks (intent) ==");
    for (id, intent) in code::classify(&a.tree) {
        println!("  {:?} -> {:?}", id, intent);
    }

    println!("\n== tables ==");
    for (_id, info) in table::analyze_all(&src, &a.tree) {
        println!(
            "  columns={:?} status_column={:?} rows={}",
            info.columns, info.status_column, info.row_count
        );
    }

    println!("\n== links ==");
    for l in &a.model.links {
        println!("  {} -> {} (L{})", l.text, l.url, l.range.start_line);
    }
}
