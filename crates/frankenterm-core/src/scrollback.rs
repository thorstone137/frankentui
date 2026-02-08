//! Scrollback buffer: lines that have scrolled off the visible viewport.
//!
//! Stores rows as `Vec<Cell>` so that SGR attributes, hyperlinks, and wide-char
//! flags are preserved through scrollback. Uses a `VecDeque` ring for O(1)
//! push/pop at both ends.

use std::collections::VecDeque;

use crate::cell::Cell;

/// A single line in the scrollback buffer.
///
/// Stores the cells that made up the row when it was evicted from the viewport.
/// The `wrapped` flag records whether the line was a soft-wrap continuation of
/// the previous line (used by reflow on resize).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrollbackLine {
    /// The cells of this line (may be shorter than the viewport width if
    /// trailing blanks were trimmed).
    pub cells: Vec<Cell>,
    /// Whether this line was a soft-wrap continuation (as opposed to a hard
    /// newline / CR+LF). Used by reflow policies.
    pub wrapped: bool,
}

impl ScrollbackLine {
    /// Create a new scrollback line from a cell slice.
    pub fn new(cells: &[Cell], wrapped: bool) -> Self {
        Self {
            cells: cells.to_vec(),
            wrapped,
        }
    }

    /// Number of cells in this line.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Whether this line has zero cells.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

/// Scrollback buffer with configurable line capacity.
///
/// Uses a `VecDeque` for O(1) push/pop. When over capacity, the oldest line
/// (front of the deque) is evicted.
#[derive(Debug, Clone)]
pub struct Scrollback {
    lines: VecDeque<ScrollbackLine>,
    capacity: usize,
}

impl Scrollback {
    /// Create a new scrollback with the given line capacity.
    ///
    /// A capacity of `0` means scrollback is disabled (all pushes are dropped).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(capacity.min(4096)),
            capacity,
        }
    }

    /// Maximum number of lines this scrollback can hold.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Change the scrollback capacity.
    ///
    /// If the new capacity is smaller than the current line count, the oldest
    /// lines are evicted.
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity;
        while self.lines.len() > capacity {
            self.lines.pop_front();
        }
    }

    /// Current number of stored lines.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether the scrollback is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Push a row (as a cell slice) into scrollback.
    ///
    /// `wrapped` indicates whether the row was a soft-wrap continuation.
    /// If over capacity, the oldest line is evicted.
    pub fn push_row(&mut self, cells: &[Cell], wrapped: bool) -> Option<ScrollbackLine> {
        if self.capacity == 0 {
            return None;
        }
        let evicted = if self.lines.len() == self.capacity {
            self.lines.pop_front()
        } else {
            None
        };
        self.lines.push_back(ScrollbackLine::new(cells, wrapped));
        evicted
    }

    /// Pop the most recent (newest) line from scrollback.
    ///
    /// Used when scrolling down to pull lines back into the viewport, or
    /// when the viewport grows taller and lines are reclaimed.
    pub fn pop_newest(&mut self) -> Option<ScrollbackLine> {
        self.lines.pop_back()
    }

    /// Peek at the most recent (newest) line without removing it.
    #[must_use]
    pub fn peek_newest(&self) -> Option<&ScrollbackLine> {
        self.lines.back()
    }

    /// Get a line by index (0 = oldest).
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&ScrollbackLine> {
        self.lines.get(index)
    }

    /// Iterate over stored lines from oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = &ScrollbackLine> {
        self.lines.iter()
    }

    /// Iterate over stored lines from newest to oldest.
    pub fn iter_rev(&self) -> impl Iterator<Item = &ScrollbackLine> {
        self.lines.iter().rev()
    }

    /// Clear all stored lines.
    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

impl Default for Scrollback {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Color, SgrAttrs, SgrFlags};

    fn make_row(text: &str) -> Vec<Cell> {
        text.chars().map(Cell::new).collect()
    }

    fn row_text(cells: &[Cell]) -> String {
        cells.iter().map(|c| c.content()).collect()
    }

    #[test]
    fn capacity_zero_drops_lines() {
        let mut sb = Scrollback::new(0);
        let _ = sb.push_row(&make_row("hello"), false);
        assert!(sb.is_empty());
    }

    #[test]
    fn push_and_retrieve() {
        let mut sb = Scrollback::new(10);
        let _ = sb.push_row(&make_row("first"), false);
        let _ = sb.push_row(&make_row("second"), true);
        assert_eq!(sb.len(), 2);

        let line0 = sb.get(0).unwrap();
        assert_eq!(row_text(&line0.cells), "first");
        assert!(!line0.wrapped);

        let line1 = sb.get(1).unwrap();
        assert_eq!(row_text(&line1.cells), "second");
        assert!(line1.wrapped);
    }

    #[test]
    fn bounded_capacity_evicts_oldest() {
        let mut sb = Scrollback::new(2);
        let _ = sb.push_row(&make_row("a"), false);
        let _ = sb.push_row(&make_row("b"), false);
        let _ = sb.push_row(&make_row("c"), false);
        assert_eq!(sb.len(), 2);
        assert_eq!(row_text(&sb.get(0).unwrap().cells), "b");
        assert_eq!(row_text(&sb.get(1).unwrap().cells), "c");
    }

    #[test]
    fn pop_newest_returns_most_recent() {
        let mut sb = Scrollback::new(10);
        let _ = sb.push_row(&make_row("old"), false);
        let _ = sb.push_row(&make_row("new"), false);
        let popped = sb.pop_newest().unwrap();
        assert_eq!(row_text(&popped.cells), "new");
        assert_eq!(sb.len(), 1);
    }

    #[test]
    fn pop_newest_empty_returns_none() {
        let mut sb = Scrollback::new(10);
        assert!(sb.pop_newest().is_none());
    }

    #[test]
    fn peek_newest() {
        let mut sb = Scrollback::new(10);
        let _ = sb.push_row(&make_row("line"), false);
        assert_eq!(row_text(&sb.peek_newest().unwrap().cells), "line");
        assert_eq!(sb.len(), 1); // not consumed
    }

    #[test]
    fn set_capacity_evicts_excess() {
        let mut sb = Scrollback::new(10);
        for i in 0..5 {
            let _ = sb.push_row(&make_row(&format!("line{i}")), false);
        }
        sb.set_capacity(2);
        assert_eq!(sb.len(), 2);
        assert_eq!(row_text(&sb.get(0).unwrap().cells), "line3");
        assert_eq!(row_text(&sb.get(1).unwrap().cells), "line4");
    }

    #[test]
    fn iter_oldest_to_newest() {
        let mut sb = Scrollback::new(10);
        let _ = sb.push_row(&make_row("a"), false);
        let _ = sb.push_row(&make_row("b"), false);
        let _ = sb.push_row(&make_row("c"), false);
        let texts: Vec<String> = sb.iter().map(|l| row_text(&l.cells)).collect();
        assert_eq!(texts, vec!["a", "b", "c"]);
    }

    #[test]
    fn iter_rev_newest_to_oldest() {
        let mut sb = Scrollback::new(10);
        let _ = sb.push_row(&make_row("a"), false);
        let _ = sb.push_row(&make_row("b"), false);
        let texts: Vec<String> = sb.iter_rev().map(|l| row_text(&l.cells)).collect();
        assert_eq!(texts, vec!["b", "a"]);
    }

    #[test]
    fn clear_empties_buffer() {
        let mut sb = Scrollback::new(10);
        let _ = sb.push_row(&make_row("x"), false);
        sb.clear();
        assert!(sb.is_empty());
    }

    #[test]
    fn preserves_cell_attributes() {
        let mut sb = Scrollback::new(10);
        let mut cells = make_row("AB");
        cells[0].attrs = SgrAttrs {
            flags: SgrFlags::BOLD,
            fg: Color::Rgb(255, 0, 0),
            bg: Color::Default,
            underline_color: None,
        };
        cells[1].hyperlink = 42;
        let _ = sb.push_row(&cells, false);

        let stored = sb.get(0).unwrap();
        assert!(stored.cells[0].attrs.flags.contains(SgrFlags::BOLD));
        assert_eq!(stored.cells[0].attrs.fg, Color::Rgb(255, 0, 0));
        assert_eq!(stored.cells[1].hyperlink, 42);
    }

    #[test]
    fn scrollback_line_len_and_empty() {
        let line = ScrollbackLine::new(&make_row("abc"), false);
        assert_eq!(line.len(), 3);
        assert!(!line.is_empty());

        let empty = ScrollbackLine::new(&[], false);
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }
}
