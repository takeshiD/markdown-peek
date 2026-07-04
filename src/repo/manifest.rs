//! Project manifest awareness: `Cargo.toml` and `package.json`.
//!
//! Surfaces the package identity, declared binaries/scripts, and cross-checks
//! the manifest's `readme`/`repository` metadata against the working tree so
//! docs-code drift (e.g. a `readme = "README.md"` pointing at a missing file)
//! is visible.

use serde::Serialize;
use std::path::Path;

/// A parsed project manifest with the fields Layer 4 cares about.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Manifest {
    pub kind: ManifestKind,
    /// Manifest filename, e.g. `Cargo.toml`.
    pub file: String,
    pub name: Option<String>,
    pub version: Option<String>,
    /// Declared binaries (Cargo `[[bin]]` names) or npm `bin` keys.
    pub binaries: Vec<String>,
    /// npm `scripts` names (empty for Cargo).
    pub scripts: Vec<String>,
    /// The manifest's declared readme, if any, and whether it exists on disk.
    pub readme: Option<ReadmeRef>,
    pub repository: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ManifestKind {
    Cargo,
    Npm,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReadmeRef {
    pub path: String,
    pub exists: bool,
}

/// Discover and parse the manifests present at `root`.
pub fn discover(root: &Path) -> Vec<Manifest> {
    let mut out = Vec::new();
    let cargo = root.join("Cargo.toml");
    if cargo.is_file()
        && let Ok(text) = std::fs::read_to_string(&cargo)
        && let Some(m) = parse_cargo(&text, root)
    {
        out.push(m);
    }
    let pkg = root.join("package.json");
    if pkg.is_file()
        && let Ok(text) = std::fs::read_to_string(&pkg)
        && let Some(m) = parse_package_json(&text, root)
    {
        out.push(m);
    }
    out
}

/// Parse a `Cargo.toml`. `root` is used only to check the `readme` file.
pub fn parse_cargo(text: &str, root: &Path) -> Option<Manifest> {
    let value: toml::Value = toml::from_str(text).ok()?;
    let package = value.get("package");
    let name = package
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let version = package
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let repository = package
        .and_then(|p| p.get("repository"))
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // `readme` may be a string or `false`/absent. Cargo's default is README.md
    // when the field is omitted but a README exists; we only report an explicit
    // string here.
    let readme = package
        .and_then(|p| p.get("readme"))
        .and_then(|v| v.as_str())
        .map(|path| ReadmeRef {
            exists: root.join(path).is_file(),
            path: path.to_string(),
        });

    // Explicit `[[bin]]` names, falling back to the package name for the
    // implicit `src/main.rs` binary.
    let mut binaries: Vec<String> = value
        .get("bin")
        .and_then(|b| b.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|b| b.get("name").and_then(|v| v.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if binaries.is_empty()
        && root.join("src/main.rs").is_file()
        && let Some(n) = &name
    {
        binaries.push(n.clone());
    }

    Some(Manifest {
        kind: ManifestKind::Cargo,
        file: "Cargo.toml".to_string(),
        name,
        version,
        binaries,
        scripts: Vec::new(),
        readme,
        repository,
    })
}

/// Parse a `package.json`. `root` is used only to check the `readme` file.
pub fn parse_package_json(text: &str, root: &Path) -> Option<Manifest> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let version = value
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // `repository` is either a string or `{ "url": "..." }`.
    let repository = match value.get("repository") {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Object(o)) => {
            o.get("url").and_then(|v| v.as_str()).map(str::to_string)
        }
        _ => None,
    };

    // `bin` is either a string (single binary named after the package) or an
    // object mapping command → path.
    let binaries = match value.get("bin") {
        Some(serde_json::Value::String(_)) => name.iter().cloned().collect(),
        Some(serde_json::Value::Object(o)) => o.keys().cloned().collect(),
        _ => Vec::new(),
    };

    let scripts = value
        .get("scripts")
        .and_then(|s| s.as_object())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();

    // npm has no standard `readme` field; fall back to a README.md on disk.
    let readme = ["README.md", "readme.md"].iter().find_map(|f| {
        root.join(f).is_file().then(|| ReadmeRef {
            path: (*f).to_string(),
            exists: true,
        })
    });

    Some(Manifest {
        kind: ManifestKind::Npm,
        file: "package.json".to_string(),
        name,
        version,
        binaries,
        scripts,
        readme,
        repository,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cargo_bin_and_readme() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("README.md"), "x").unwrap();
        let text = r#"
[package]
name = "mdpeek"
version = "1.2.3"
readme = "README.md"
repository = "https://github.com/x/y"

[[bin]]
name = "mdpeek"
path = "src/main.rs"
"#;
        let m = parse_cargo(text, root).unwrap();
        assert_eq!(m.name.as_deref(), Some("mdpeek"));
        assert_eq!(m.version.as_deref(), Some("1.2.3"));
        assert_eq!(m.binaries, vec!["mdpeek".to_string()]);
        assert_eq!(m.readme.as_ref().unwrap().path, "README.md");
        assert!(m.readme.as_ref().unwrap().exists);
    }

    #[test]
    fn flags_missing_cargo_readme() {
        let dir = tempfile::tempdir().unwrap();
        let text = "[package]\nname = \"x\"\nreadme = \"MISSING.md\"\n";
        let m = parse_cargo(text, dir.path()).unwrap();
        assert!(!m.readme.unwrap().exists);
    }

    #[test]
    fn parses_package_json() {
        let dir = tempfile::tempdir().unwrap();
        let text = r#"{
            "name": "widget",
            "version": "0.1.0",
            "bin": { "widget": "cli.js", "widgetx": "x.js" },
            "scripts": { "build": "tsc", "test": "jest" },
            "repository": { "url": "git+https://github.com/a/b.git" }
        }"#;
        let m = parse_package_json(text, dir.path()).unwrap();
        assert_eq!(m.name.as_deref(), Some("widget"));
        assert_eq!(m.kind, ManifestKind::Npm);
        let mut bins = m.binaries.clone();
        bins.sort();
        assert_eq!(bins, vec!["widget".to_string(), "widgetx".to_string()]);
        let mut scripts = m.scripts.clone();
        scripts.sort();
        assert_eq!(scripts, vec!["build".to_string(), "test".to_string()]);
        assert_eq!(
            m.repository.as_deref(),
            Some("git+https://github.com/a/b.git")
        );
    }

    #[test]
    fn discover_finds_both() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"a\"\n").unwrap();
        std::fs::write(root.join("package.json"), "{\"name\":\"b\"}").unwrap();
        let ms = discover(root);
        assert_eq!(ms.len(), 2);
    }
}
