//! In-memory selection cache (§3.7). Never written to disk.
//!
//! Key = `(section hash, REGISTRY_VERSION, POLICY_VERSION, selector id/model,
//! candidate ids)`. Raw answers are cached (thresholds are applied later), so
//! threshold changes take effect without invalidation. Bounded LRU.

use std::collections::HashMap;
use std::sync::Mutex;

use super::sections::{DocPlan, REGISTRY_VERSION, Section};
use super::{Answer, POLICY_VERSION};

type DocEntry = (Option<Answer>, Option<(String, f32)>);

#[derive(Default)]
struct Inner {
    tick: u64,
    sections: HashMap<String, (Answer, u64)>,
    docs: HashMap<String, (DocEntry, u64)>,
}

pub struct SelectionCache {
    cap: usize,
    inner: Mutex<Inner>,
}

pub fn section_key(s: &Section, selector: &str) -> String {
    let cands: Vec<&str> = s.candidates.iter().map(|c| c.id).collect();
    format!(
        "s|{}|r{REGISTRY_VERSION}|p{POLICY_VERSION}|{selector}|{}",
        s.hash,
        cands.join(",")
    )
}

pub fn doc_key(p: &DocPlan, selector: &str) -> String {
    format!(
        "d|{}|r{REGISTRY_VERSION}|p{POLICY_VERSION}|{selector}",
        p.doc_signature
    )
}

impl SelectionCache {
    pub fn new(cap: usize) -> Self {
        SelectionCache {
            cap: cap.max(1),
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn get(&self, key: &str) -> Option<Answer> {
        let mut g = self.inner.lock().ok()?;
        g.tick += 1;
        let t = g.tick;
        let e = g.sections.get_mut(key)?;
        e.1 = t;
        Some(e.0.clone())
    }

    pub fn put(&self, key: String, a: Answer) {
        let Ok(mut g) = self.inner.lock() else { return };
        g.tick += 1;
        let t = g.tick;
        g.sections.insert(key, (a, t));
        let cap = self.cap;
        evict(&mut g.sections, cap);
    }

    pub fn get_doc(&self, key: &str) -> Option<DocEntry> {
        let mut g = self.inner.lock().ok()?;
        g.tick += 1;
        let t = g.tick;
        let e = g.docs.get_mut(key)?;
        e.1 = t;
        Some(e.0.clone())
    }

    pub fn put_doc(&self, key: String, a: Option<Answer>, doc_type: Option<(String, f32)>) {
        let Ok(mut g) = self.inner.lock() else { return };
        g.tick += 1;
        let t = g.tick;
        g.docs.insert(key, ((a, doc_type), t));
        let cap = self.cap;
        evict(&mut g.docs, cap);
    }

    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.sections.len() + g.docs.len()).unwrap_or(0)
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
