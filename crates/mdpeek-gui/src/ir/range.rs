//! Source ranges and byte-offset → (line, column) conversion.
//!
//! Every generated UI node is anchored back to the original Markdown via a
//! [`SourceRange`] (design doc §1 "全 UI は sourceRange に紐づく"). `pulldown-cmark`
//! yields byte offsets, so [`LineIndex`] maps those offsets to 1-based
//! line / column positions in a single pass.
//!
//! NOTE: Layer 1 (`parser` module) will eventually own the canonical
//! `SourceRange`/`LineIndex`. This module keeps a self-contained copy so Layer 3
//! can be built and tested without waiting for the Layer 1 merge; the types are
//! deliberately kept minimal and compatible with the design doc §4.1.

use serde::{Deserialize, Serialize};

/// A 1-based, inclusive-start / exclusive-end span into the source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRange {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

impl SourceRange {
    /// True when both endpoints fall within a document of `total_lines` lines.
    /// Used by the validator to reject hallucinated ranges (design §3.5).
    pub fn within(&self, total_lines: u32) -> bool {
        self.start_line >= 1
            && self.end_line >= self.start_line
            && self.end_line <= total_lines
            && self.start_column >= 1
            && self.end_column >= 1
    }
}

/// Maps byte offsets to (line, column). Built once per document.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of the first character of each line (0-based line → offset).
    line_starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        LineIndex {
            line_starts,
            len: source.len(),
        }
    }

    /// Total number of lines in the document (>= 1).
    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    /// Convert a byte offset to a 1-based (line, column) pair. Offsets past the
    /// end clamp to the final position.
    fn line_col(&self, offset: usize) -> (u32, u32) {
        let offset = offset.min(self.len);
        // Largest line_start <= offset.
        let line = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        let col = offset - self.line_starts[line];
        ((line as u32) + 1, (col as u32) + 1)
    }

    /// Convert a byte range into a [`SourceRange`].
    pub fn range(&self, span: std::ops::Range<usize>) -> SourceRange {
        let (start_line, start_column) = self.line_col(span.start);
        let (end_line, end_column) = self.line_col(span.end);
        SourceRange {
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_offsets_to_line_col() {
        let src = "abc\ndef\n";
        let idx = LineIndex::new(src);
        assert_eq!(idx.line_count(), 3); // "abc", "def", ""
        assert_eq!(idx.range(0..1).start_line, 1);
        assert_eq!(idx.range(0..1).start_column, 1);
        // 'd' is at byte offset 4 → line 2, col 1.
        let r = idx.range(4..5);
        assert_eq!((r.start_line, r.start_column), (2, 1));
    }

    #[test]
    fn within_bounds_check() {
        let r = SourceRange {
            start_line: 1,
            start_column: 1,
            end_line: 3,
            end_column: 2,
        };
        assert!(r.within(3));
        assert!(!r.within(2));
    }
}
