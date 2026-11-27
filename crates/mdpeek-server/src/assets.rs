//! Local image serving for the browser preview (#55, #56).
//!
//! The HTML renderer rewrites document-relative image URLs to
//! `/asset?path=<relative path>` ([`ASSET_PREFIX`]). This module resolves such
//! a path against the directory of the open document and serves it only when
//! it canonicalizes (symlinks followed) to an existing image file under an
//! allowed root — the discovered repository/working tree, or, for a document
//! opened outside it, that document's own repository (or directory). Absolute
//! paths, `..` escapes and non-image files are rejected with 404.

use std::path::{Component, Path, PathBuf};

/// Prefix the HTML renderer puts in front of the percent-encoded path.
pub const ASSET_PREFIX: &str = "/asset?path=";

/// `Content-Type` for the image extensions we serve; `None` = not served.
pub fn image_content_type(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jpe" | "jfif" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "apng" => "image/apng",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        _ => return None,
    })
}

/// Roots an asset for `doc` may live under: `roots` when the document is
/// inside one of them, otherwise the document's own root (`doc_root`).
fn roots_for(
    roots: &[PathBuf],
    doc: &Path,
    doc_root: impl FnOnce(&Path) -> PathBuf,
) -> Vec<PathBuf> {
    if roots.iter().any(|r| doc.starts_with(r)) {
        return roots.to_vec();
    }
    let dir = doc.parent().unwrap_or(Path::new("/"));
    doc_root(dir).canonicalize().ok().into_iter().collect()
}

/// Resolve the client-supplied `rel` (relative to the directory of `doc`) to
/// an image file under the allowed roots. `doc_root` maps a directory outside
/// `roots` to the root that should confine it (its git toplevel, or itself).
pub fn resolve(
    roots: &[PathBuf],
    doc: &Path,
    rel: &str,
    doc_root: impl FnOnce(&Path) -> PathBuf,
) -> Option<(PathBuf, &'static str)> {
    let rel_path = Path::new(rel);
    // Only plain relative paths: no `/abs`, `C:\`, `\\server`, or NUL bytes.
    if rel.is_empty()
        || rel.contains('\0')
        || rel.starts_with(['/', '\\'])
        || rel_path
            .components()
            .any(|c| matches!(c, Component::RootDir | Component::Prefix(_)))
    {
        return None;
    }
    let doc = doc.canonicalize().ok()?;
    let canon = doc.parent()?.join(rel_path).canonicalize().ok()?;
    if !canon.is_file() {
        return None;
    }
    let content_type = image_content_type(&canon)?;
    roots_for(roots, &doc, doc_root)
        .iter()
        .any(|r| canon.starts_with(r))
        .then_some((canon, content_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        outside: PathBuf,
        doc: PathBuf,
    }

    /// `<tmp>/repo/{docs/doc.md, docs/img/a.png, b.png, secret.txt}` plus
    /// `<tmp>/outside.png`; the allowed root is `<tmp>/repo`.
    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let root = base.join("repo");
        std::fs::create_dir_all(root.join("docs/img")).unwrap();
        std::fs::write(root.join("docs/doc.md"), "# d").unwrap();
        std::fs::write(root.join("docs/img/a.png"), b"\x89PNG").unwrap();
        std::fs::write(root.join("b.png"), b"\x89PNG").unwrap();
        std::fs::write(root.join("secret.txt"), "s").unwrap();
        std::fs::write(base.join("outside.png"), b"\x89PNG").unwrap();
        Fixture {
            doc: root.join("docs/doc.md"),
            outside: base.join("outside.png"),
            root,
            _tmp: tmp,
        }
    }

    fn no_doc_root(_: &Path) -> PathBuf {
        panic!("document is inside the roots; doc_root must not be consulted")
    }

    #[test]
    fn serves_relative_images_inside_the_root() {
        let f = fixture();
        let roots = vec![f.root.clone()];
        let (p, ct) = resolve(&roots, &f.doc, "img/a.png", no_doc_root).unwrap();
        assert_eq!(p, f.root.join("docs/img/a.png"));
        assert_eq!(ct, "image/png");
        let (p, _) = resolve(&roots, &f.doc, "./img/../../b.png", no_doc_root).unwrap();
        assert_eq!(p, f.root.join("b.png"));
    }

    #[test]
    fn rejects_traversal_absolute_missing_and_non_images() {
        let f = fixture();
        let roots = vec![f.root.clone()];
        for rel in [
            "../../outside.png",
            "../../../../../../../../etc/passwd",
            f.outside.to_str().unwrap(),
            f.root.join("b.png").to_str().unwrap(), // absolute, even inside the root
            "../secret.txt",
            "../doc.md",
            "img/missing.png",
            "img",
            "",
        ] {
            assert!(resolve(&roots, &f.doc, rel, no_doc_root).is_none(), "{rel}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escaping_the_root() {
        let f = fixture();
        std::os::unix::fs::symlink(&f.outside, f.root.join("docs/link.png")).unwrap();
        let roots = vec![f.root.clone()];
        assert!(resolve(&roots, &f.doc, "link.png", no_doc_root).is_none());
    }

    #[test]
    fn document_outside_roots_is_confined_to_its_own_root() {
        let f = fixture();
        // The discovered roots do not contain the document (opened elsewhere).
        let roots = vec![f.root.join("docs/img")];
        let own_root = f.root.clone();
        let (p, _) = resolve(&roots, &f.doc, "../b.png", |_| own_root.clone()).unwrap();
        assert_eq!(p, f.root.join("b.png"));
        assert!(resolve(&roots, &f.doc, "../../outside.png", |_| f.root.clone()).is_none());
    }

    #[test]
    fn content_types_by_extension() {
        for (name, ct) in [
            ("a.PNG", "image/png"),
            ("a.jpg", "image/jpeg"),
            ("a.jpeg", "image/jpeg"),
            ("a.gif", "image/gif"),
            ("a.svg", "image/svg+xml"),
            ("a.webp", "image/webp"),
        ] {
            assert_eq!(image_content_type(Path::new(name)), Some(ct), "{name}");
        }
        for name in ["a.md", "a.txt", "a.html", "noext"] {
            assert_eq!(image_content_type(Path::new(name)), None, "{name}");
        }
    }
}
