//! Node graphs from diagrams drawn with box-drawing / ASCII characters in a
//! code block (issue #62), e.g.
//!
//! ```text
//! SourceRange parser ──┬─▶ Layer2 analyzer ──▶ Layer3 renderer
//!                      └─▶ #16 差分 (BlockId 安定化)
//! ```
//!
//! Deterministic and conservative. The block is laid on a grid of display
//! columns (CJK characters take two), then every arrow head (`▶ ▼ ◀ ▲ → > v <
//! ^`) is read as one edge:
//!
//! - its **target** is the text (or box) the head points at;
//! - its **sources** are found by walking back along the connector (`─ │ ┬ ┼
//!   └ …`, `- | +`) until text or a box is reached; a connector that leaves a
//!   box border means the box, and a box that holds a flow of its own means
//!   that flow's single end (or start, for an arrow into the box);
//! - text right beside a vertical connector (`│  UI IR (JSON)`) or between two
//!   shafts (`──yes──▶`) is the edge label.
//!
//! Rows of text directly under a row of nodes (aligned with them) are those
//! nodes' notes, not new nodes. A simple box (`┌──┐ │ X │ └──┘`) is a node.
//! When any arrow cannot be resolved, or the result is too small, nothing is
//! returned. Directory trees (`├── src/`) are left to their own aid.
//!
//! Every node and edge keeps the source line(s) it was read from.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::OnceLock;

use regex::Regex;
use unicode_width::UnicodeWidthChar;

use super::graph::{Builder, MAX_NAME_CHARS, MAX_NODES, MIN_EDGES, MIN_NODES, split_name};
use crate::ir::SourceRange;
use crate::ir::node::{NodeGraphNode, NodeMeta};

const N: u8 = 1;
const E: u8 = 2;
const S: u8 = 4;
const W: u8 = 8;
const ALL: u8 = N | E | S | W;
const HORIZ: u8 = E | W;
const VERT: u8 = N | S;

/// Diagrams larger than this are not parsed (cost bound).
const MAX_ROWS: usize = 200;
const MAX_COLS: usize = 400;

fn opposite(d: u8) -> u8 {
    match d {
        N => S,
        S => N,
        E => W,
        _ => E,
    }
}

fn delta(d: u8) -> (isize, isize) {
    match d {
        N => (-1, 0),
        S => (1, 0),
        E => (0, 1),
        _ => (0, -1),
    }
}

/// Directions a box-drawing character connects to.
fn box_dirs(ch: char) -> Option<u8> {
    let d = match ch {
        '─' | '━' | '═' | '┄' | '┅' | '┈' | '┉' | '╌' | '╍' => HORIZ,
        '│' | '┃' | '║' | '┆' | '┇' | '┊' | '┋' | '╎' | '╏' => VERT,
        '┌' | '┍' | '┎' | '┏' | '╒' | '╓' | '╔' | '╭' => E | S,
        '┐' | '┑' | '┒' | '┓' | '╕' | '╖' | '╗' | '╮' => W | S,
        '└' | '┕' | '┖' | '┗' | '╘' | '╙' | '╚' | '╰' => N | E,
        '┘' | '┙' | '┚' | '┛' | '╛' | '╜' | '╝' | '╯' => N | W,
        '├' | '┝' | '┞' | '┟' | '┠' | '┡' | '┢' | '┣' | '╞' | '╟' | '╠' => {
            VERT | E
        }
        '┤' | '┥' | '┦' | '┧' | '┨' | '┩' | '┪' | '┫' | '╡' | '╢' | '╣' => {
            VERT | W
        }
        '┬' | '┭' | '┮' | '┯' | '┰' | '┱' | '┲' | '┳' | '╤' | '╥' | '╦' => {
            HORIZ | S
        }
        '┴' | '┵' | '┶' | '┷' | '┸' | '┹' | '┺' | '┻' | '╧' | '╨' | '╩' => {
            HORIZ | N
        }
        '┼'..='╋' | '╪' | '╫' | '╬' => ALL,
        '\u{2500}'..='\u{257f}' => 0,
        _ => return None,
    };
    Some(d)
}

/// Direction an arrow-head character points to (always a head).
fn head_dir(ch: char) -> Option<u8> {
    Some(match ch {
        '▶' | '►' | '▸' | '▷' | '→' | '⇒' | '⟶' | '➡' | '➔' | '➜' => E,
        '◀' | '◄' | '◂' | '◁' | '←' | '⇐' | '⟵' => W,
        '▼' | '▾' | '▽' | '↓' | '⇓' => S,
        '▲' | '▴' | '△' | '↑' | '⇑' => N,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Space,
    Text,
    /// A connector, with the directions it joins.
    Line(u8),
    /// An arrow head pointing to a direction.
    Head(u8),
}

/// A run of text on one row (display columns `start..end`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Tok {
    row: usize,
    start: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum End {
    Tok(Tok),
    Box(usize),
}

#[derive(Debug, Clone)]
struct Rect {
    top: usize,
    bottom: usize,
    left: usize,
    right: usize,
    title: Option<String>,
}

enum Look {
    Line(usize, usize),
    End(End),
    Stop,
}

struct Grid {
    /// One char per display column; `'\0'` is the second half of a wide char.
    rows: Vec<Vec<char>>,
    kinds: Vec<Vec<Kind>>,
    border: HashMap<(usize, usize), usize>,
    boxes: Vec<Rect>,
}

fn connects(ch: char, d: u8) -> bool {
    match ch {
        '|' => d & VERT != 0,
        '+' => true,
        _ => box_dirs(ch).is_some_and(|x| x & d != 0),
    }
}

fn is_shaft(ch: char) -> bool {
    matches!(ch, '-' | '=') || box_dirs(ch).is_some_and(|d| d == HORIZ)
}

fn is_vbar(ch: char) -> bool {
    ch == '|' || box_dirs(ch).is_some_and(|d| d & VERT == VERT && d & HORIZ != HORIZ)
}

impl Grid {
    fn new(lines: &[&str]) -> Grid {
        let rows: Vec<Vec<char>> = lines
            .iter()
            .map(|l| {
                let mut row = Vec::new();
                for ch in l.chars() {
                    if ch == '\t' {
                        row.push(' ');
                        while row.len() % 4 != 0 {
                            row.push(' ');
                        }
                        continue;
                    }
                    match ch.width().unwrap_or(0) {
                        0 => {}
                        1 => row.push(ch),
                        _ => {
                            row.push(ch);
                            row.push('\0');
                        }
                    }
                }
                row
            })
            .collect();
        let mut g = Grid {
            kinds: Vec::new(),
            rows,
            border: HashMap::new(),
            boxes: Vec::new(),
        };
        g.kinds = (0..g.rows.len())
            .map(|r| (0..g.rows[r].len()).map(|c| g.classify(r, c)).collect())
            .collect();
        g.find_boxes();
        g
    }

    fn at(&self, r: isize, c: isize) -> char {
        if r < 0 || c < 0 {
            return ' ';
        }
        self.rows
            .get(r as usize)
            .and_then(|row| row.get(c as usize))
            .copied()
            .unwrap_or(' ')
    }

    fn kind(&self, r: isize, c: isize) -> Kind {
        if r < 0 || c < 0 {
            return Kind::Space;
        }
        self.kinds
            .get(r as usize)
            .and_then(|row| row.get(c as usize))
            .copied()
            .unwrap_or(Kind::Space)
    }

    fn border_at(&self, r: isize, c: isize) -> Option<usize> {
        if r < 0 || c < 0 {
            return None;
        }
        self.border.get(&(r as usize, c as usize)).copied()
    }

    fn is_text(&self, r: isize, c: isize) -> bool {
        self.kind(r, c) == Kind::Text && self.border_at(r, c).is_none()
    }

    /// ASCII characters are connectors only in context (`-->`, `+--+`, `|`).
    fn classify(&self, r: usize, c: usize) -> Kind {
        let (ri, ci) = (r as isize, c as isize);
        let ch = self.rows[r][c];
        let prev = self.at(ri, ci - 1);
        let next = self.at(ri, ci + 1);
        let up = self.at(ri - 1, ci);
        let down = self.at(ri + 1, ci);
        if ch == ' ' {
            return Kind::Space;
        }
        if ch == '\0' {
            return Kind::Text;
        }
        if let Some(d) = box_dirs(ch) {
            return Kind::Line(d);
        }
        if let Some(d) = head_dir(ch) {
            return Kind::Head(d);
        }
        let boxy = |x: char| box_dirs(x).is_some();
        match ch {
            '-' if matches!(prev, '-' | '<' | '+' | '|')
                || matches!(next, '-' | '>' | '+' | '|')
                || boxy(prev)
                || boxy(next) =>
            {
                Kind::Line(HORIZ)
            }
            '=' if prev == '=' || next == '=' || next == '>' || prev == '<' => Kind::Line(HORIZ),
            '|' => Kind::Line(VERT),
            '+' if matches!(prev, '-' | '=')
                || matches!(next, '-' | '=')
                || matches!(up, '|' | '+')
                || matches!(down, '|' | '+') =>
            {
                Kind::Line(ALL)
            }
            '>' if is_shaft(prev) => Kind::Head(E),
            '<' if is_shaft(next) => Kind::Head(W),
            'v' | 'V' if connects(up, S) && !prev.is_alphanumeric() && !next.is_alphanumeric() => {
                Kind::Head(S)
            }
            '^' if connects(down, N) && !prev.is_alphanumeric() && !next.is_alphanumeric() => {
                Kind::Head(N)
            }
            _ => Kind::Text,
        }
    }

    fn is_hline(&self, r: usize, c: usize) -> bool {
        matches!(self.kind(r as isize, c as isize), Kind::Line(d) if d & HORIZ == HORIZ)
    }

    /// Rectangles `┌─┐ │ │ └─┘` / `+-+ | | +-+`, tolerating a misaligned
    /// right border and a title in the top border.
    fn find_boxes(&mut self) {
        for r in 0..self.rows.len() {
            for c in 0..self.rows[r].len() {
                let ch = self.rows[r][c];
                let (ri, ci) = (r as isize, c as isize);
                let unicode = matches!(ch, '┌' | '┏' | '╔' | '╭');
                let ascii = ch == '+'
                    && matches!(self.at(ri, ci + 1), '-' | '=')
                    && matches!(self.at(ri + 1, ci), '|' | '+');
                if !(unicode || ascii) || !self.is_hline(r, c + 1) {
                    continue;
                }
                let is_tr = |g: &Grid, j: usize| {
                    let x = g.rows[r][j];
                    if unicode {
                        matches!(x, '┐' | '┓' | '╗' | '╮')
                    } else {
                        x == '+' && !matches!(g.at(ri, j as isize + 1), '-' | '=')
                    }
                };
                // Top edge (may carry a title).
                let mut tr = None;
                for j in c + 1..self.rows[r].len() {
                    if is_tr(self, j) {
                        tr = Some(j);
                        break;
                    }
                    match self.kinds[r][j] {
                        Kind::Line(d) if d & HORIZ == HORIZ => {}
                        // `┌──▼──┐`: an arrow into the box drawn on its border.
                        Kind::Text | Kind::Space | Kind::Head(N | S) => {}
                        _ => break,
                    }
                }
                let Some(tr) = tr else { continue };
                // Left edge down to the bottom-left corner.
                let is_bl = |x: char| {
                    if unicode {
                        matches!(x, '└' | '┗' | '╚' | '╰')
                    } else {
                        x == '+'
                    }
                };
                let (mut bottom, mut bars, mut others) = (None, 0usize, 0usize);
                for rr in r + 1..self.rows.len().min(r + 80) {
                    let rri = rr as isize;
                    let found = (c.saturating_sub(1)..=c + 1)
                        .find(|&cc| is_bl(self.at(rri, cc as isize)) && self.is_hline(rr, cc + 1));
                    if let Some(cc) = found {
                        bottom = Some((rr, cc));
                        break;
                    }
                    if (c.saturating_sub(1)..=c + 1).any(|cc| is_vbar(self.at(rri, cc as isize))) {
                        bars += 1;
                    } else {
                        others += 1;
                    }
                }
                let Some((rb, cb)) = bottom else { continue };
                if bars == 0 || others > bars {
                    continue;
                }
                let is_br = |g: &Grid, k: usize| {
                    let x = g.rows[rb][k];
                    if unicode {
                        matches!(x, '┘' | '┛' | '╝' | '╯')
                    } else {
                        x == '+' && !matches!(g.at(rb as isize, k as isize + 1), '-' | '=')
                    }
                };
                let mut br = None;
                for k in cb + 1..self.rows[rb].len() {
                    if is_br(self, k) {
                        br = Some(k);
                        break;
                    }
                    if !self.is_hline(rb, k) && !matches!(self.kinds[rb][k], Kind::Head(N | S)) {
                        break;
                    }
                }
                let Some(br) = br else { continue };
                let (left, right) = (c.min(cb), tr.max(br));
                let title: String = (c + 1..tr)
                    .map(|j| match self.kinds[r][j] {
                        Kind::Text => self.rows[r][j],
                        _ => ' ',
                    })
                    .filter(|x| *x != '\0')
                    .collect();
                let title = title.trim();
                let idx = self.boxes.len();
                self.boxes.push(Rect {
                    top: r,
                    bottom: rb,
                    left,
                    right,
                    title: (!title.is_empty()).then(|| title.to_string()),
                });
                let mut mark = |p: (usize, usize)| {
                    self.border.entry(p).or_insert(idx);
                };
                for j in c..=tr {
                    mark((r, j));
                }
                for k in cb..=br {
                    mark((rb, k));
                }
                for rr in r + 1..rb {
                    let row = &self.rows[rr];
                    for cc in left.saturating_sub(1)..=left + 1 {
                        if row.get(cc).is_some_and(|x| is_vbar(*x)) {
                            mark((rr, cc));
                        }
                    }
                    let near = (right.saturating_sub(2)..=right + 2)
                        .find(|&cc| row.get(cc).is_some_and(|x| is_vbar(*x)));
                    // A right border pushed out by wide text: the first
                    // connector after the box's right edge.
                    let far = || {
                        (right + 3..row.len())
                            .find(|&cc| !matches!(self.kinds[rr][cc], Kind::Text | Kind::Space))
                            .filter(|&cc| is_vbar(row[cc]))
                    };
                    if let Some(cc) = near.or_else(far) {
                        mark((rr, cc));
                    }
                }
            }
        }
    }

    /// The text run around `(r, c)`; single spaces join words, two split.
    fn token_at(&self, r: usize, c: usize) -> Tok {
        let ri = r as isize;
        let mut s = c as isize;
        loop {
            if self.is_text(ri, s - 1) {
                s -= 1;
            } else if self.kind(ri, s - 1) == Kind::Space && self.is_text(ri, s - 2) {
                s -= 2;
            } else {
                break;
            }
        }
        let mut e = c as isize + 1;
        loop {
            if self.is_text(ri, e) {
                e += 1;
            } else if self.kind(ri, e) == Kind::Space && self.is_text(ri, e + 1) {
                e += 2;
            } else {
                break;
            }
        }
        Tok {
            row: r,
            start: s as usize,
            end: e as usize,
        }
    }

    fn text(&self, t: Tok) -> String {
        self.rows[t.row][t.start..t.end]
            .iter()
            .filter(|c| **c != '\0')
            .collect::<String>()
            .trim()
            .to_string()
    }

    fn endpoint(&self, r: isize, c: isize) -> Option<End> {
        if let Some(b) = self.border_at(r, c) {
            return Some(End::Box(b));
        }
        self.is_text(r, c)
            .then(|| End::Tok(self.token_at(r as usize, c as usize)))
    }

    /// What lies next to `(r, c)` in direction `d`. Horizontally up to two
    /// spaces are skipped; vertically a misalignment of `tol` columns is
    /// tolerated.
    fn look(&self, r: usize, c: usize, d: u8, tol: usize) -> Look {
        let (dr, dc) = delta(d);
        let (ri, ci) = (r as isize, c as isize);
        let classify = |rr: isize, cc: isize| -> Look {
            if let Some(e) = self.endpoint(rr, cc) {
                return Look::End(e);
            }
            match self.kind(rr, cc) {
                Kind::Line(x) if rr >= 0 && cc >= 0 && x & opposite(d) != 0 => {
                    Look::Line(rr as usize, cc as usize)
                }
                _ => Look::Stop,
            }
        };
        if dr == 0 {
            let mut cc = ci + dc;
            let mut gap = 0;
            while gap < 2 && cc >= 0 && self.kind(ri, cc) == Kind::Space {
                cc += dc;
                gap += 1;
            }
            return classify(ri, cc);
        }
        let rr = ri + dr;
        let offsets: &[isize] = if tol >= 2 {
            &[0, -1, 1, -2, 2]
        } else {
            &[0, -1, 1]
        };
        for &o in offsets {
            match classify(rr, ci + o) {
                Look::Stop => continue,
                x => return x,
            }
        }
        Look::Stop
    }

    /// Where the head at `(r, c)` points to.
    fn target(&self, r: usize, c: usize, d: u8) -> Option<End> {
        let (dr, dc) = delta(d);
        let (ri, ci) = (r as isize, c as isize);
        if dr == 0 {
            let mut cc = ci + dc;
            let mut gap = 0;
            while gap < 2 && cc >= 0 && self.kind(ri, cc) == Kind::Space {
                cc += dc;
                gap += 1;
            }
            return self.endpoint(ri, cc);
        }
        [0, -1, 1, -2, 2]
            .iter()
            .find_map(|o| self.endpoint(ri + dr, ci + o))
    }

    /// Text right beside a vertical connector: the edge label.
    fn side_label(&self, r: usize, c: usize) -> Option<String> {
        let ri = r as isize;
        (c as isize + 1..=c as isize + 4)
            .find(|&cc| self.kind(ri, cc) != Kind::Space)
            .filter(|&cc| self.is_text(ri, cc))
            .map(|cc| self.text(self.token_at(r, cc as usize)))
    }

    /// Walk back from the head at `(r, c)` (pointing to `d`) along its
    /// connector; returns the sources found and an edge label.
    fn trace(&self, r: usize, c: usize, d: u8) -> (Vec<End>, Option<String>) {
        let mut ends = Vec::new();
        let mut label = None;
        let mut seen = HashSet::new();
        let mut queue = VecDeque::new();
        let tol = if d & VERT != 0 { 2 } else { 1 };
        self.follow(
            r,
            c,
            opposite(d),
            tol,
            &mut ends,
            &mut label,
            &mut seen,
            &mut queue,
        );
        while let Some((r, c)) = queue.pop_front() {
            let Kind::Line(dirs) = self.kinds[r][c] else {
                continue;
            };
            if dirs == VERT && label.is_none() {
                label = self.side_label(r, c);
            }
            for dd in [N, E, S, W] {
                if dirs & dd != 0 {
                    self.follow(r, c, dd, 1, &mut ends, &mut label, &mut seen, &mut queue);
                }
            }
        }
        (ends, label)
    }

    #[allow(clippy::too_many_arguments)]
    fn follow(
        &self,
        r: usize,
        c: usize,
        d: u8,
        tol: usize,
        ends: &mut Vec<End>,
        label: &mut Option<String>,
        seen: &mut HashSet<(usize, usize)>,
        queue: &mut VecDeque<(usize, usize)>,
    ) {
        match self.look(r, c, d, tol) {
            Look::Line(rr, cc) => {
                if seen.insert((rr, cc)) {
                    queue.push_back((rr, cc));
                }
            }
            Look::End(End::Tok(t)) if d & HORIZ != 0 => {
                // `──yes──▶`: text between two shafts labels the edge.
                let (_, dc) = delta(d);
                let ri = t.row as isize;
                let mut b = if dc < 0 {
                    t.start as isize - 1
                } else {
                    t.end as isize
                };
                if self.kind(ri, b) == Kind::Space {
                    b += dc;
                }
                let shaft = matches!(self.kind(ri, b), Kind::Line(x) if x == HORIZ)
                    && self.border_at(ri, b).is_none();
                if shaft {
                    if label.is_none() {
                        *label = Some(self.text(t));
                    }
                    if seen.insert((t.row, b as usize)) {
                        queue.push_back((t.row, b as usize));
                    }
                } else if !ends.contains(&End::Tok(t)) {
                    ends.push(End::Tok(t));
                }
            }
            Look::End(e) => {
                if !ends.contains(&e) {
                    ends.push(e);
                }
            }
            Look::Stop => {}
        }
    }

    /// Columns of the arrow heads on row `r`.
    fn heads(&self, r: usize) -> Vec<(usize, u8)> {
        (0..self.rows[r].len())
            .filter_map(|c| match self.kinds[r][c] {
                Kind::Head(d) => Some((c, d)),
                _ => None,
            })
            .collect()
    }

    /// Row `r` read as notes under the nodes `toks` of the row above: words
    /// aligned with those nodes and no connector (a bare `→` inside a note,
    /// as in `model → UI plan`, is text). Returns (node index, word).
    fn note_row(
        &self,
        r: usize,
        toks: &[Tok],
        parent_heads: &[usize],
    ) -> Option<Vec<(usize, String)>> {
        let ri = r as isize;
        let mut words: Vec<(usize, String)> = Vec::new();
        let mut cur: Option<(usize, String)> = None;
        for c in 0..self.rows[r].len() {
            let ci = c as isize;
            let k = if self.border_at(ri, ci).is_some() {
                Kind::Space
            } else {
                self.kinds[r][c]
            };
            match k {
                Kind::Space => {
                    if let Some(w) = cur.take() {
                        words.push(w);
                    }
                    continue;
                }
                Kind::Line(_) => return None,
                Kind::Head(_) => {
                    let bare = !matches!(self.kind(ri, ci - 1), Kind::Line(_))
                        && !matches!(self.kind(ri, ci + 1), Kind::Line(_));
                    let inside = toks.iter().any(|t| c + 1 >= t.start && c <= t.end + 1);
                    let aligned = parent_heads.iter().any(|h| h.abs_diff(c) <= 1);
                    if !bare || !inside || aligned {
                        return None;
                    }
                }
                Kind::Text => {}
            }
            let ch = self.rows[r][c];
            let w = cur.get_or_insert_with(|| (c, String::new()));
            if ch != '\0' {
                w.1.push(ch);
            }
        }
        if let Some(w) = cur.take() {
            words.push(w);
        }
        let first = toks.first()?;
        if words.is_empty() || words.iter().any(|(s, _)| s + 2 < first.start) {
            return None;
        }
        Some(
            words
                .into_iter()
                .map(|(s, w)| (toks.iter().rposition(|t| s + 1 >= t.start).unwrap_or(0), w))
                .collect(),
        )
    }

    /// Text inside a box without its borders, one entry per non-empty row.
    fn box_lines(&self, bi: usize) -> Vec<String> {
        let b = &self.boxes[bi];
        (b.top + 1..b.bottom)
            .filter_map(|r| {
                let len = self.rows[r].len();
                // Up to this box's right border on the row (it may be pushed out by wide text).
                let end = (b.left + 2..len)
                    .find(|&c| self.border.get(&(r, c)) == Some(&bi))
                    .unwrap_or((b.right).min(len));
                let s: String = (b.left + 1..end)
                    .filter(|&c| self.border_at(r as isize, c as isize).is_none())
                    .map(|c| match self.kinds[r][c] {
                        Kind::Text => self.rows[r][c],
                        _ => ' ',
                    })
                    .filter(|x| *x != '\0')
                    .collect();
                let s = s
                    .split("  ")
                    .map(str::trim)
                    .filter(|x| !x.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                (!s.is_empty()).then_some(s)
            })
            .collect()
    }
}

/// A directory tree listing (`├── src/`, `` `-- lib ``) is not a flow.
fn looks_like_tree(lines: &[&str]) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE
        .get_or_init(|| Regex::new(r"^[\s│|]*(?:[├└]─+|[|`]--+)\s+[^\s─\-▶►>→┬┴┼┐┘│|+]").unwrap());
    lines.iter().filter(|l| re.is_match(l)).count() >= 2
}

struct Raw {
    from: End,
    to: End,
    label: Option<String>,
    rows: (usize, usize),
}

/// Build a node graph from the lines of a code block drawn as a diagram.
/// `first_line` is the 1-based source line of `lines[0]`.
pub fn graph_from_diagram(
    lines: &[&str],
    first_line: u32,
    title: Option<String>,
    meta: NodeMeta,
) -> Option<NodeGraphNode> {
    if lines.len() > MAX_ROWS
        || lines.iter().any(|l| l.chars().count() > MAX_COLS)
        || looks_like_tree(lines)
    {
        return None;
    }
    let g = Grid::new(lines);

    // Read the arrows row by row; rows of notes under a row of nodes are
    // consumed as notes (their bare `→` are not arrows).
    let mut raws: Vec<Raw> = Vec::new();
    let mut row_toks: BTreeMap<usize, Vec<Tok>> = BTreeMap::new();
    let mut notes: HashMap<Tok, (Vec<String>, usize)> = HashMap::new();
    let mut parent: Option<usize> = None;
    let register = |row_toks: &mut BTreeMap<usize, Vec<Tok>>, e: End| {
        if let End::Tok(t) = e {
            let v = row_toks.entry(t.row).or_default();
            if !v.contains(&t) {
                v.push(t);
                v.sort();
            }
        }
    };
    for r in 0..g.rows.len() {
        if let Some(p) = parent {
            let toks = row_toks.get(&p).cloned().unwrap_or_default();
            let heads: Vec<usize> = g.heads(p).into_iter().map(|h| h.0).collect();
            if let Some(words) = g.note_row(r, &toks, &heads) {
                for (i, w) in words {
                    let n = notes.entry(toks[i]).or_insert_with(|| (Vec::new(), r));
                    n.0.push(w);
                    n.1 = r;
                }
                continue;
            }
        }
        parent = None;
        for (c, d) in g.heads(r) {
            let (to, (froms, label)) = match g.border_at(r as isize, c as isize) {
                // A head drawn on a box border: into the box, or out of it.
                Some(bi) => {
                    let bx = &g.boxes[bi];
                    match (r == bx.top, r == bx.bottom, d) {
                        (true, _, S) | (_, true, N) => (End::Box(bi), g.trace(r, c, d)),
                        (true, _, N) | (_, true, S) => {
                            (g.target(r, c, d)?, (vec![End::Box(bi)], None))
                        }
                        _ => return None,
                    }
                }
                None => (g.target(r, c, d)?, g.trace(r, c, d)),
            };
            if froms.is_empty() {
                return None;
            }
            register(&mut row_toks, to);
            for from in froms {
                register(&mut row_toks, from);
                raws.push(Raw {
                    from,
                    to,
                    label: label.clone(),
                    rows: (r, r),
                });
            }
        }
        if row_toks.get(&r).is_some_and(|v| !v.is_empty()) {
            parent = Some(r);
        }
    }

    // A box with a flow inside stands for that flow's single end (or start).
    let all_toks: Vec<Tok> = row_toks.values().flatten().copied().collect();
    let resolve = |e: End, as_source: bool| -> Option<End> {
        let End::Box(bi) = e else { return Some(e) };
        let b = &g.boxes[bi];
        let inner: Vec<Tok> = all_toks
            .iter()
            .filter(|t| t.row > b.top && t.row < b.bottom && t.end > b.left && t.start <= b.right)
            .copied()
            .collect();
        if inner.is_empty() {
            return Some(e);
        }
        let internal: Vec<(Tok, Tok)> = raws
            .iter()
            .filter_map(|x| match (x.from, x.to) {
                (End::Tok(a), End::Tok(c)) if inner.contains(&a) && inner.contains(&c) => {
                    Some((a, c))
                }
                _ => None,
            })
            .collect();
        let pick: Vec<Tok> = inner
            .iter()
            .filter(|t| {
                let outs = internal.iter().any(|x| x.0 == **t);
                let ins = internal.iter().any(|x| x.1 == **t);
                if as_source {
                    ins && !outs
                } else {
                    outs && !ins
                }
            })
            .copied()
            .collect();
        match (pick.as_slice(), &b.title) {
            ([one], _) => Some(End::Tok(*one)),
            (_, Some(_)) => Some(e),
            _ => None,
        }
    };
    let mut edges: Vec<Raw> = Vec::new();
    for x in &raws {
        let (from, to) = (resolve(x.from, true)?, resolve(x.to, false)?);
        let row_of = |e: End| match e {
            End::Tok(t) => t.row,
            End::Box(b) => x.rows.0.clamp(g.boxes[b].top, g.boxes[b].bottom),
        };
        let rows = [x.rows.0, row_of(from), row_of(to)];
        edges.push(Raw {
            from,
            to,
            label: x.label.clone(),
            rows: (*rows.iter().min().unwrap(), *rows.iter().max().unwrap()),
        });
    }

    // Nodes in reading order.
    let mut ends: Vec<End> = Vec::new();
    for x in &edges {
        for e in [x.from, x.to] {
            if !ends.contains(&e) {
                ends.push(e);
            }
        }
    }
    let pos = |e: &End| match e {
        End::Tok(t) => (t.row, t.start),
        End::Box(b) => (g.boxes[*b].top, g.boxes[*b].left),
    };
    ends.sort_by_key(pos);
    let range = |a: usize, b: usize| {
        let end = lines.get(b).map(|l| l.chars().count() as u32).unwrap_or(0);
        SourceRange {
            start_line: first_line + a as u32,
            start_column: 1,
            end_line: first_line + b as u32,
            end_column: end + 1,
        }
    };
    let mut b = Builder::default();
    let mut ids: HashMap<End, String> = HashMap::new();
    for e in &ends {
        let (label, note, r) = match e {
            End::Tok(t) => {
                let text = g.text(*t);
                match notes.get(t) {
                    Some((words, last)) => {
                        if text.chars().count() > MAX_NAME_CHARS {
                            return None;
                        }
                        (text, Some(words.join(" ")), range(t.row, *last))
                    }
                    None => {
                        let (l, n) = split_name(&text)?;
                        (l, n, range(t.row, t.row))
                    }
                }
            }
            End::Box(bi) => {
                let bx = &g.boxes[*bi];
                let mut body = g.box_lines(*bi);
                let label = match &bx.title {
                    Some(t) => t.clone(),
                    None if body.is_empty() => return None,
                    None => body.remove(0),
                };
                let note = (!body.is_empty()).then(|| body.join(" "));
                let (label, note) = match note {
                    Some(n) if label.chars().count() <= MAX_NAME_CHARS => (label, Some(n)),
                    Some(_) => return None,
                    None => split_name(&label)?,
                };
                (label, note, range(bx.top, bx.bottom))
            }
        };
        ids.insert(*e, b.node(&label, note, r));
    }
    for x in &edges {
        b.edge(
            ids[&x.from].clone(),
            ids[&x.to].clone(),
            x.label.clone(),
            range(x.rows.0, x.rows.1),
        );
    }
    if b.nodes.len() < MIN_NODES || b.edges.len() < MIN_EDGES || b.nodes.len() > MAX_NODES {
        return None;
    }
    Some(NodeGraphNode {
        meta,
        title,
        nodes: b.nodes,
        edges: b.edges,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Option<NodeGraphNode> {
        let lines: Vec<&str> = text.lines().collect();
        graph_from_diagram(&lines, 10, None, NodeMeta::default())
    }
    fn labels(g: &NodeGraphNode) -> Vec<&str> {
        g.nodes.iter().map(|n| n.label.as_str()).collect()
    }
    fn edges(g: &NodeGraphNode) -> Vec<(String, String, Option<String>)> {
        let name = |id: &str| g.nodes.iter().find(|n| n.id == id).unwrap().label.clone();
        g.edges
            .iter()
            .map(|e| (name(&e.from), name(&e.to), e.label.clone()))
            .collect()
    }
    fn e(a: &str, b: &str) -> (String, String, Option<String>) {
        (a.into(), b.into(), None)
    }

    /// AGENTS.md §10 "依存関係(クリティカルパス)", verbatim.
    const CRITICAL_PATH: &str = include_str!("testdata/agents-critical-path.txt");
    /// AGENTS.md §1 "全体アーキテクチャ", verbatim.
    const ARCHITECTURE: &str = include_str!("testdata/agents-architecture.txt");

    #[test]
    fn critical_path_branches_from_the_junction() {
        let g = parse(CRITICAL_PATH).unwrap();
        assert_eq!(
            labels(&g),
            [
                "SourceRange parser",
                "Layer2 analyzer",
                "Layer3 IR/generator",
                "Layer3 renderer",
                "#16 差分",
                "#14 worktree",
                "Layer4"
            ]
        );
        assert_eq!(g.nodes[4].note.as_deref(), Some("BlockId 安定化"));
        assert_eq!(
            edges(&g),
            [
                e("SourceRange parser", "Layer2 analyzer"),
                e("Layer2 analyzer", "Layer3 IR/generator"),
                e("Layer3 IR/generator", "Layer3 renderer"),
                e("SourceRange parser", "#16 差分"),
                e("#14 worktree", "Layer4"),
            ]
        );
        // Lines of the source: the branch spans the junction and its row.
        let branch = &g.edges[3];
        assert_eq!(branch.source_range.unwrap().start_line, 10);
        assert_eq!(branch.source_range.unwrap().end_line, 11);
        assert_eq!(g.nodes[6].source_range.unwrap().start_line, 12);
    }

    #[test]
    fn architecture_chain_box_and_fan_out() {
        let g = parse(ARCHITECTURE).unwrap();
        assert_eq!(
            labels(&g),
            [
                "document.md",
                "parser",
                "model",
                "analyzer",
                "planner",
                "generator",
                "ir(validate)",
                "web renderer (Preact)",
                "tui renderer (ratatui)"
            ]
        );
        let chain = [
            "document.md",
            "parser",
            "model",
            "analyzer",
            "planner",
            "generator",
            "ir(validate)",
        ];
        let mut want: Vec<_> = chain.windows(2).map(|w| e(w[0], w[1])).collect();
        let ir = Some("UI IR (JSON, serde)".to_string());
        want.push((
            "ir(validate)".into(),
            "web renderer (Preact)".into(),
            ir.clone(),
        ));
        want.push(("ir(validate)".into(), "tui renderer (ratatui)".into(), ir));
        assert_eq!(edges(&g), want);
        // Text under a node is its note, verbatim (a `→` in it is not an arrow).
        let note = |l: &str| g.nodes.iter().find(|n| n.label == l).unwrap().note.clone();
        assert_eq!(
            note("parser").as_deref(),
            Some("(pulldown -cmark OffsetIter")
        );
        assert_eq!(
            note("planner").as_deref(),
            Some("(semantic model → UI plan)")
        );
        assert_eq!(
            note("web renderer (Preact)").as_deref(),
            Some("registry[kind] → component (全 UiNode 対応)")
        );
        assert_eq!(note("document.md"), None);
        // The note rows belong to the node's range.
        let parser = g.nodes.iter().find(|n| n.label == "parser").unwrap();
        assert_eq!(parser.source_range.unwrap().start_line, 12);
        assert_eq!(parser.source_range.unwrap().end_line, 15);
    }

    #[test]
    fn ascii_boxes_and_arrows() {
        let g = parse(
            "+--------+      +---------+\n\
             | parse  | ---> | analyze |\n\
             +--------+      +---------+\n\
             \x20                    |\n\
             \x20                    v\n\
             \x20               +--------+\n\
             \x20               | render |\n\
             \x20               +--------+\n",
        )
        .unwrap();
        assert_eq!(labels(&g), ["parse", "analyze", "render"]);
        assert_eq!(edges(&g), [e("parse", "analyze"), e("analyze", "render")]);
        assert_eq!(g.nodes[2].source_range.unwrap().start_line, 15);
    }

    #[test]
    fn unicode_boxes_with_labels_on_arrows() {
        let g = parse(
            "┌───────┐          ┌───────┐\n\
             │ Draft │──review─▶│ Ready │\n\
             └───────┘          └───┬───┘\n\
             \x20                       │ merge\n\
             \x20                       ▼\n\
             \x20                    Released\n",
        )
        .unwrap();
        assert_eq!(labels(&g), ["Draft", "Ready", "Released"]);
        assert_eq!(
            edges(&g),
            [
                ("Draft".into(), "Ready".into(), Some("review".into())),
                ("Ready".into(), "Released".into(), Some("merge".into())),
            ]
        );
    }

    #[test]
    fn stacked_boxes_with_arrows_drawn_on_their_borders() {
        let g = parse(
            "┌──────────────┐\n\
             │ Control plane │\n\
             │  src/cli.ts   │\n\
             └──────┬───────┘\n\
             \x20      │ spawn + env\n\
             ┌──────▼───────┐\n\
             │ Runner       │\n\
             └──────┬───────┘\n\
             \x20      │\n\
             ┌──────▼───────┐\n\
             │ Report       │\n\
             └──────────────┘\n",
        )
        .unwrap();
        assert_eq!(labels(&g), ["Control plane", "Runner", "Report"]);
        assert_eq!(g.nodes[0].note.as_deref(), Some("src/cli.ts"));
        assert_eq!(
            edges(&g),
            [
                (
                    "Control plane".into(),
                    "Runner".into(),
                    Some("spawn + env".into())
                ),
                e("Runner", "Report"),
            ]
        );
        assert_eq!(g.nodes[1].source_range.unwrap().start_line, 15);
        assert_eq!(g.nodes[1].source_range.unwrap().end_line, 17);
    }

    #[test]
    fn plain_arrow_chains() {
        let g = parse("fetch --> parse --> render\nparse -> cache\n").unwrap();
        assert_eq!(labels(&g), ["fetch", "parse", "render", "cache"]);
        assert_eq!(g.edges.len(), 3);
    }

    #[test]
    fn directory_trees_are_not_graphs() {
        assert!(parse("src/\n├── main.rs   → entry point\n├── lib.rs    → library\n└── cli.rs    → arguments\n").is_none());
        assert!(parse("crates/\n├── mdpeek-core/\n│   └── src/\n└── mdpeek-cli/\n").is_none());
    }

    #[test]
    fn tables_prose_and_tiny_diagrams_are_not_graphs() {
        assert!(parse("+------+-------+\n| name | value |\n+------+-------+\n| a    | 1     |\n+------+-------+\n").is_none());
        assert!(
            parse("This block is plain prose.\nIt has no arrows at all, only words.\n").is_none()
        );
        assert!(parse("A ──▶ B\n").is_none());
        assert!(
            parse("A --> B --> \n").is_none(),
            "an arrow to nowhere is not read"
        );
        // One arrow that cannot be read makes the whole diagram unreliable.
        assert!(parse("A ──▶ B ──▶ C\n      ◀──▶\n").is_none());
    }
}
