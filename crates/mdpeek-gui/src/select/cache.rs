//! Selection cache (§3.7): an in-memory LRU in front of an optional disk
//! cache (`$XDG_CACHE_HOME/mdpeek/select/…`, or `[cache] dir`).
//!
//! Key = `(section hash, REGISTRY_VERSION, POLICY_VERSION, model identity,
//! candidate ids)`. Raw answers are cached (thresholds are applied later), so
//! threshold changes take effect without invalidation.
//!
//! What reaches the disk: option ids and numbers only (`candidate`,
//! `confidence`, `helps`, alternatives, document type). File names are SHA-256
//! digests of the key. No document text, excerpt, heading or key is written.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::sections::{DocPlan, REGISTRY_VERSION, Section};
use super::{Answer, POLICY_VERSION};

pub type DocEntry = (Option<Answer>, Option<(String, f32)>);

/// Bumped when the on-disk entry format changes (older files are ignored).
const DISK_FORMAT: u32 = 1;

#[derive(Default)]
struct Inner {
    tick: u64,
    sections: HashMap<String, (Answer, u64)>,
    docs: HashMap<String, (DocEntry, u64)>,
}

pub struct SelectionCache {
    cap: usize,
    inner: Mutex<Inner>,
    disk: Option<DiskCache>,
}

pub fn section_key(s: &Section, model: &str) -> String {
    let cands: Vec<&str> = s.candidates.iter().map(|c| c.id).collect();
    format!(
        "s|{}|r{REGISTRY_VERSION}|p{POLICY_VERSION}|{model}|{}",
        s.hash,
        cands.join(",")
    )
}

pub fn doc_key(p: &DocPlan, model: &str) -> String {
    format!(
        "d|{}|r{REGISTRY_VERSION}|p{POLICY_VERSION}|{model}",
        p.doc_signature
    )
}

impl SelectionCache {
    /// Memory only.
    pub fn new(cap: usize) -> Self {
        SelectionCache {
            cap: cap.max(1),
            inner: Mutex::new(Inner::default()),
            disk: None,
        }
    }

    /// Memory in front of `disk`.
    pub fn with_disk(cap: usize, disk: Option<DiskCache>) -> Self {
        SelectionCache {
            disk,
            ..SelectionCache::new(cap)
        }
    }

    pub fn disk_dir(&self) -> Option<&Path> {
        self.disk.as_ref().map(|d| d.dir.as_path())
    }

    pub fn get(&self, key: &str) -> Option<Answer> {
        {
            let mut g = self.inner.lock().ok()?;
            g.tick += 1;
            let t = g.tick;
            if let Some(e) = g.sections.get_mut(key) {
                e.1 = t;
                return Some(e.0.clone());
            }
        }
        let a = self.disk.as_ref()?.get_section(key)?;
        self.put_mem(key.to_string(), a.clone());
        Some(a)
    }

    fn put_mem(&self, key: String, a: Answer) {
        let Ok(mut g) = self.inner.lock() else { return };
        g.tick += 1;
        let t = g.tick;
        g.sections.insert(key, (a, t));
        let cap = self.cap;
        evict(&mut g.sections, cap);
    }

    pub fn put(&self, key: String, a: Answer) {
        if let Some(d) = &self.disk {
            d.put_section(&key, &a);
        }
        self.put_mem(key, a);
    }

    pub fn get_doc(&self, key: &str) -> Option<DocEntry> {
        {
            let mut g = self.inner.lock().ok()?;
            g.tick += 1;
            let t = g.tick;
            if let Some(e) = g.docs.get_mut(key) {
                e.1 = t;
                return Some(e.0.clone());
            }
        }
        let e = self.disk.as_ref()?.get_doc(key)?;
        self.put_doc_mem(key.to_string(), e.clone());
        Some(e)
    }

    fn put_doc_mem(&self, key: String, e: DocEntry) {
        let Ok(mut g) = self.inner.lock() else { return };
        g.tick += 1;
        let t = g.tick;
        g.docs.insert(key, (e, t));
        let cap = self.cap;
        evict(&mut g.docs, cap);
    }

    pub fn put_doc(&self, key: String, a: Option<Answer>, doc_type: Option<(String, f32)>) {
        let e = (a, doc_type);
        if let Some(d) = &self.disk {
            d.put_doc(&key, &e);
        }
        self.put_doc_mem(key, e);
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .map(|g| g.sections.len() + g.docs.len())
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn evict<V>(map: &mut HashMap<String, (V, u64)>, cap: usize) {
    while map.len() > cap {
        let Some(oldest) = map.iter().min_by_key(|(_, v)| v.1).map(|(k, _)| k.clone()) else {
            break;
        };
        map.remove(&oldest);
    }
}

// ---------------------------------------------------------------------------
// Disk
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct RawAnswer {
    candidate: String,
    confidence: f32,
    helps: f32,
    #[serde(default)]
    alternatives: Vec<(String, f32)>,
}

impl RawAnswer {
    fn from(a: &Answer) -> Option<Self> {
        match a {
            Answer::Raw {
                candidate,
                confidence,
                helps,
                alternatives,
            } => Some(RawAnswer {
                candidate: candidate.clone(),
                confidence: *confidence,
                helps: *helps,
                alternatives: alternatives.clone(),
            }),
            Answer::Invalid(_) => None,
        }
    }
    fn into_answer(self) -> Option<Answer> {
        let ok = |x: f32| x.is_finite() && (0.0..=1.0).contains(&x);
        (ok(self.confidence) && ok(self.helps)).then_some(Answer::Raw {
            candidate: self.candidate,
            confidence: self.confidence,
            helps: self.helps,
            alternatives: self
                .alternatives
                .into_iter()
                .filter(|(_, p)| ok(*p))
                .collect(),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct SectionFile {
    v: u32,
    answer: RawAnswer,
}

#[derive(Serialize, Deserialize)]
struct DocFile {
    v: u32,
    document: Option<RawAnswer>,
    doc_type: Option<(String, f32)>,
}

/// Decisions on disk, bounded by age and total size.
pub struct DiskCache {
    dir: PathBuf,
    ttl: Duration,
    max_bytes: u64,
}

impl DiskCache {
    /// Create `dir` (mode 0700 on Unix) and prune it once.
    pub fn open(dir: PathBuf, ttl_days: u64, max_size_mb: u64) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
        let c = DiskCache {
            dir,
            ttl: Duration::from_secs(ttl_days.max(1) * 86_400),
            max_bytes: max_size_mb.max(1) * 1024 * 1024,
        };
        c.prune();
        Ok(c)
    }

    fn path(&self, key: &str) -> PathBuf {
        let h: String = Sha256::digest(key.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        self.dir
            .join("select")
            .join(&h[..2])
            .join(format!("{h}.json"))
    }

    fn read(&self, key: &str) -> Option<Vec<u8>> {
        let p = self.path(key);
        let age = std::fs::metadata(&p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())?;
        if age > self.ttl {
            let _ = std::fs::remove_file(&p);
            return None;
        }
        std::fs::read(&p).ok()
    }

    fn write(&self, key: &str, bytes: &[u8]) {
        let p = self.path(key);
        let Some(parent) = p.parent() else { return };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        let tmp = p.with_extension(format!("tmp{}", std::process::id()));
        if std::fs::write(&tmp, bytes).is_ok() && std::fs::rename(&tmp, &p).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }

    fn get_section(&self, key: &str) -> Option<Answer> {
        let f: SectionFile = serde_json::from_slice(&self.read(key)?).ok()?;
        (f.v == DISK_FORMAT)
            .then(|| f.answer.into_answer())
            .flatten()
    }

    fn put_section(&self, key: &str, a: &Answer) {
        let Some(answer) = RawAnswer::from(a) else {
            return;
        };
        if let Ok(b) = serde_json::to_vec(&SectionFile {
            v: DISK_FORMAT,
            answer,
        }) {
            self.write(key, &b);
        }
    }

    fn get_doc(&self, key: &str) -> Option<DocEntry> {
        let f: DocFile = serde_json::from_slice(&self.read(key)?).ok()?;
        if f.v != DISK_FORMAT {
            return None;
        }
        Some((f.document.and_then(RawAnswer::into_answer), f.doc_type))
    }

    fn put_doc(&self, key: &str, e: &DocEntry) {
        let f = DocFile {
            v: DISK_FORMAT,
            document: e.0.as_ref().and_then(RawAnswer::from),
            doc_type: e.1.clone(),
        };
        if let Ok(b) = serde_json::to_vec(&f) {
            self.write(key, &b);
        }
    }

    /// Drop expired entries, then the oldest ones while over the size limit.
    pub fn prune(&self) {
        let root = self.dir.join("select");
        let mut files: Vec<(PathBuf, SystemTime, u64)> = Vec::new();
        let Ok(dirs) = std::fs::read_dir(&root) else {
            return;
        };
        for d in dirs.flatten() {
            let Ok(entries) = std::fs::read_dir(d.path()) else {
                continue;
            };
            for e in entries.flatten() {
                let Ok(m) = e.metadata() else { continue };
                if !m.is_file() {
                    continue;
                }
                let t = m.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                files.push((e.path(), t, m.len()));
            }
        }
        let now = SystemTime::now();
        files.retain(|(p, t, _)| {
            let old = now
                .duration_since(*t)
                .map(|a| a > self.ttl)
                .unwrap_or(false);
            if old {
                let _ = std::fs::remove_file(p);
            }
            !old
        });
        let mut total: u64 = files.iter().map(|f| f.2).sum();
        if total <= self.max_bytes {
            return;
        }
        files.sort_by_key(|f| f.1);
        for (p, _, n) in files {
            if total <= self.max_bytes {
                break;
            }
            if std::fs::remove_file(&p).is_ok() {
                total = total.saturating_sub(n);
            }
        }
    }
}

/// `$XDG_CACHE_HOME/mdpeek`, else `~/.cache/mdpeek`.
pub fn default_cache_dir() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_CACHE_HOME") {
        Some(v) if !v.is_empty() && Path::new(&v).is_absolute() => PathBuf::from(v),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".cache"),
    };
    Some(base.join("mdpeek"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(c: &str) -> Answer {
        Answer::Raw {
            candidate: c.into(),
            confidence: 0.9,
            helps: 0.8,
            alternatives: vec![("Other".into(), 0.1)],
        }
    }

    #[test]
    fn lru_evicts_oldest() {
        let c = SelectionCache::new(2);
        c.put("a".into(), Answer::Invalid("x".into()));
        c.put("b".into(), Answer::Invalid("x".into()));
        let _ = c.get("a");
        c.put("c".into(), Answer::Invalid("x".into()));
        assert!(c.get("a").is_some());
        assert!(c.get("b").is_none());
        assert!(c.get("c").is_some());
    }

    #[test]
    fn disk_survives_a_new_process_and_holds_ids_only() {
        let dir = tempfile::tempdir().unwrap();
        {
            let c = SelectionCache::with_disk(
                8,
                Some(DiskCache::open(dir.path().into(), 30, 100).unwrap()),
            );
            c.put("s|abc|secret heading?".into(), raw("StepNavigator"));
            c.put_doc(
                "d|x".into(),
                Some(raw("SemanticOutline")),
                Some(("readme".into(), 0.9)),
            );
            c.put("s|bad".into(), Answer::Invalid("x".into()));
        }
        let c = SelectionCache::with_disk(
            8,
            Some(DiskCache::open(dir.path().into(), 30, 100).unwrap()),
        );
        assert_eq!(c.get("s|abc|secret heading?"), Some(raw("StepNavigator")));
        let (d, t) = c.get_doc("d|x").unwrap();
        assert_eq!(d, Some(raw("SemanticOutline")));
        assert_eq!(t.unwrap().0, "readme");
        assert!(c.get("s|bad").is_none(), "invalid answers are not cached");
        // File names are digests; contents are ids and numbers.
        let mut all = String::new();
        for d in std::fs::read_dir(dir.path().join("select"))
            .unwrap()
            .flatten()
        {
            for f in std::fs::read_dir(d.path()).unwrap().flatten() {
                assert!(!f.file_name().to_string_lossy().contains("secret"));
                all.push_str(&std::fs::read_to_string(f.path()).unwrap());
            }
        }
        assert!(!all.contains("secret"));
        assert!(all.contains("StepNavigator"));
    }

    #[test]
    fn prune_enforces_the_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let d = DiskCache {
            dir: dir.path().into(),
            ttl: Duration::from_secs(86_400),
            max_bytes: 300,
        };
        for i in 0..20 {
            d.put_section(&format!("k{i}"), &raw("StepNavigator"));
        }
        d.prune();
        let n: usize = std::fs::read_dir(dir.path().join("select"))
            .unwrap()
            .flatten()
            .map(|d| std::fs::read_dir(d.path()).unwrap().count())
            .sum();
        assert!(n > 0 && n < 20, "{n}");
    }
}
