//! Cache key derivation (design doc §6).
//!
//! Key = `hash(normalized markdown) + generator id + schema version`. Any change
//! to the document body, the generator, or the IR schema misses the cache and
//! forces regeneration.

use sha2::{Digest, Sha256};

/// Bump when `ir::node` types change shape — invalidates all cached entries.
pub const SCHEMA_VERSION: u32 = 1;

/// Normalize markdown before hashing so cosmetic churn (CRLF, trailing spaces)
/// doesn't needlessly bust the cache.
fn normalize(markdown: &str) -> String {
    markdown
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Content hash used as the cache filename stem and stored in the entry.
///
/// `filename` participates because document-type inference (and therefore the
/// generated UI) can depend on it — the same body served as `README.md` vs an
/// untitled buffer may plan differently. Only the basename is folded in.
pub fn content_hash(markdown: &str, model_id: &str, filename: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(normalize(markdown).as_bytes());
    hasher.update([0u8]); // domain separator
    hasher.update(model_id.as_bytes());
    hasher.update([0u8]);
    hasher.update(basename(filename).as_bytes());
    hasher.update([0u8]);
    hasher.update(SCHEMA_VERSION.to_le_bytes());
    let digest = hasher.finalize();
    // 32 hex chars is plenty to avoid collisions for a local cache.
    hex16(&digest)
}

/// Basename of a path-ish filename, or "" when absent.
fn basename(filename: Option<&str>) -> &str {
    match filename {
        Some(f) => f.rsplit(['/', '\\']).next().unwrap_or(f),
        None => "",
    }
}

fn hex16(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(32);
    for b in bytes.iter().take(16) {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_and_normalized() {
        let a = content_hash("# Hi\n\n- [ ] x\n", "rules", None);
        let b = content_hash("# Hi  \r\n\r\n- [ ] x  \r\n", "rules", None);
        assert_eq!(a, b, "CRLF/trailing space must normalize equal");
        assert_eq!(a.len(), 32);
    }

    #[test]
    fn model_and_schema_affect_key() {
        let a = content_hash("# Hi\n", "rules", None);
        let b = content_hash("# Hi\n", "claude-x", None);
        assert_ne!(a, b);
    }

    #[test]
    fn filename_affects_key_by_basename() {
        let plain = content_hash("# Hi\n", "rules", None);
        let readme = content_hash("# Hi\n", "rules", Some("README.md"));
        assert_ne!(plain, readme);
        // Only the basename matters, so directory differences don't.
        let a = content_hash("# Hi\n", "rules", Some("docs/README.md"));
        let b = content_hash("# Hi\n", "rules", Some("README.md"));
        assert_eq!(a, b);
    }
}
