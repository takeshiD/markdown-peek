//! Layer 3.5 rules generator のデモ / 手動検証用。
//!
//! 使い方:
//!   cargo run --manifest-path crates/mdpeek-domain/Cargo.toml \
//!             --example generate -- production examples/production_order.md
//!   cargo run --manifest-path crates/mdpeek-domain/Cargo.toml \
//!             --example generate -- procedure examples/recipe.md
//!
//! 引数なしなら埋め込みサンプルで両ジェネレータを実行し、UI IR (JSON) を出す。

use mdpeek_domain::generators::{generate_procedure, generate_production_order};
use mdpeek_domain::{DomainNode, ParsedDoc, validate};

const SAMPLE_PRODUCTION: &str = include_str!("production_order.md");
const SAMPLE_RECIPE: &str = include_str!("recipe.md");

fn run(kind: &str, source: &str) {
    let doc = ParsedDoc::parse(source);
    let nodes: Vec<DomainNode> = match kind {
        "production" => generate_production_order(&doc),
        "procedure" => generate_procedure(&doc),
        other => {
            eprintln!("unknown doctype: {other} (use 'production' or 'procedure')");
            std::process::exit(2);
        }
    };
    for n in &nodes {
        if let Err(e) = validate(n) {
            eprintln!("validation failed: {e}");
            std::process::exit(1);
        }
    }
    println!("// doctype={kind}  nodes={}", nodes.len());
    println!("{}", serde_json::to_string_pretty(&nodes).unwrap());
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [kind, path] => {
            let src = std::fs::read_to_string(path).expect("read markdown file");
            run(kind, &src);
        }
        [] => {
            eprintln!("== production_order (embedded) ==");
            run("production", SAMPLE_PRODUCTION);
            eprintln!("\n== recipe/procedure (embedded) ==");
            run("procedure", SAMPLE_RECIPE);
        }
        _ => {
            eprintln!("usage: generate [production|procedure] <file.md>");
            std::process::exit(2);
        }
    }
}
