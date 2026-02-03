#![forbid(unsafe_code)]

//! Command Palette widget for instant action search.
//!
//! This module provides a fuzzy-search command palette with:
//! - Bayesian match scoring with evidence ledger
//! - Incremental scoring with query-prefix pruning
//! - Word-start, prefix, substring, and fuzzy matching
//! - Conformal rank confidence for tie-break stability
//! - Match position tracking for highlighting
//!
//! # Usage
//!
//! ```ignore
//! let mut palette = CommandPalette::new();
//! palette.register("Open File", Some("Open a file from disk"), &["file", "open"]);
//! palette.register("Save File", Some("Save current file"), &["file", "save"]);
//! palette.open(); // Show the palette
//!
//! // In your update loop, handle events:
//! if let Some(action) = palette.handle_event(event) {
//!     match action {
//!         PaletteAction::Execute(id) => { /* run the action */ }
//!         PaletteAction::Dismiss => { /* palette was closed */ }
//!     }
//! }
//!
//! // Render as a Widget
//! palette.render(area, &mut frame);
//! ```
//!
//! # Submodules
//!
//! - [`scorer`]: Bayesian fuzzy matcher with explainable scoring

pub mod scorer;

pub use scorer::{
    BayesianScorer, ConformalRanker, EvidenceKind, EvidenceLedger, IncrementalScorer,
    IncrementalStats, MatchResult, MatchType, RankConfidence, RankStability, RankedItem,
    RankedResults, RankingSummary,
};

use ftui_core::event::{Event, KeyCode, KeyEvent, KeyEventKind, Modifiers};
use ftui_core::geometry::Rect;
use ftui_render::cell::{Cell, CellAttrs, CellContent, PackedRgba, StyleFlags as CellStyleFlags};
use ftui_render::frame::Frame;
use ftui_style::Style;

use crate::Widget;

// ---------------------------------------------------------------------------
// Action Item
// ---------------------------------------------------------------------------

/// A single action that can be invoked from the command palette.
#[derive(Debug, Clone)]
pub struct ActionItem {
    /// Unique identifier for this action.
    pub id: String,
    /// Display title (searched by the scorer).
    pub title: String,
    /// Optional description shown below the title.
    pub description: Option<String>,
    /// Tags for boosting search relevance.
    pub tags: Vec<String>,
    /// Category for visual grouping (e.g., "Git", "File", "View").
    pub category: Option<String>,
}

impl ActionItem {
    /// Create a new action item.
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: None,
            tags: Vec::new(),
            category: None,
        }
    }

    /// Set description (builder).
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set tags (builder).
    pub fn with_tags(mut self, tags: &[&str]) -> Self {
        self.tags = tags.iter().map(|s| (*s).to_string()).collect();
        self
    }

    /// Set category (builder).
    pub fn with_category(mut self, cat: impl Into<String>) -> Self {
        self.category = Some(cat.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Palette Action
// ---------------------------------------------------------------------------

/// Action returned from event handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteAction {
    /// User selected an action to execute (contains the action ID).
    Execute(String),
    /// User dismissed the palette (Esc).
    Dismiss,
}

// ---------------------------------------------------------------------------
// Palette Style
// ---------------------------------------------------------------------------

/// Visual styling for the command palette.
#[derive(Debug, Clone)]
pub struct PaletteStyle {
    /// Border style.
    pub border: Style,
    /// Query input style.
    pub input: Style,
    /// Normal result item style.
    pub item: Style,
    /// Selected/highlighted result item style.
    pub item_selected: Style,
    /// Match highlight style (for matched characters).
    pub match_highlight: Style,
    /// Description text style.
    pub description: Style,
    /// Category badge style.
    pub category: Style,
    /// Empty state / hint text style.
    pub hint: Style,
}

impl Default for PaletteStyle {
    fn default() -> Self {
        // Colors chosen for WCAG AA contrast ratios against bg(30,30,40):
        // - item (190,190,200) on bg(30,30,40) ≈ 9.5:1 (AAA)
        // - selected (255,255,255) on bg(50,50,75) ≈ 8.8:1 (AAA)
        // - highlight (255,210,60) on bg(30,30,40) ≈ 11:1 (AAA)
        // - description (140,140,160) on bg(30,30,40) ≈ 5.2:1 (AA)
        Self {
            border: Style::new().fg(PackedRgba::rgb(100, 100, 120)),
            input: Style::new().fg(PackedRgba::rgb(220, 220, 230)),
            item: Style::new().fg(PackedRgba::rgb(190, 190, 200)),
            item_selected: Style::new()
                .fg(PackedRgba::rgb(255, 255, 255))
                .bg(PackedRgba::rgb(50, 50, 75)),
            match_highlight: Style::new().fg(PackedRgba::rgb(255, 210, 60)),
            description: Style::new().fg(PackedRgba::rgb(140, 140, 160)),
            category: Style::new().fg(PackedRgba::rgb(100, 180, 255)),
            hint: Style::new().fg(PackedRgba::rgb(100, 100, 120)),
        }
    }
}

// ---------------------------------------------------------------------------
// Scored Item (internal)
// ---------------------------------------------------------------------------

/// Internal: a scored result with corpus index.
#[derive(Debug)]
struct ScoredItem {
    /// Index into the actions vec.
    action_index: usize,
    /// Match result from scorer.
    result: MatchResult,
}

// ---------------------------------------------------------------------------
// Command Palette Widget
// ---------------------------------------------------------------------------

/// Command palette widget for instant action search.
///
/// Provides a fuzzy-search overlay with keyboard navigation, match highlighting,
/// and incremental scoring for responsive keystroke handling.
///
/// # Invariants
///
/// 1. `selected` is always < `filtered.len()` (or 0 when empty).
/// 2. Results are sorted by descending score with stable tie-breaking.
/// 3. Query changes trigger incremental re-scoring (not full rescan)
///    when the query extends the previous one.
#[derive(Debug)]
pub struct CommandPalette {
    /// Registered actions.
    actions: Vec<ActionItem>,
    /// Current query text.
    query: String,
    /// Cursor position in the query (byte offset for simplicity).
    cursor: usize,
    /// Currently selected index in filtered results.
    selected: usize,
    /// Scroll offset for visible window.
    scroll_offset: usize,
    /// Whether the palette is visible.
    visible: bool,
    /// Visual styling.
    style: PaletteStyle,
    /// Incremental scorer for fast keystroke handling.
    scorer: IncrementalScorer,
    /// Current filtered results.
    filtered: Vec<ScoredItem>,
    /// Generation counter for corpus invalidation.
    generation: u64,
    /// Maximum visible results.
    max_visible: usize,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPalette {
    /// Create a new empty command palette.
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
            query: String::new(),
            cursor: 0,
            selected: 0,
            scroll_offset: 0,
            visible: false,
            style: PaletteStyle::default(),
            scorer: IncrementalScorer::new(),
            filtered: Vec::new(),
            generation: 0,
            max_visible: 10,
        }
    }

    /// Set the visual style (builder).
    pub fn with_style(mut self, style: PaletteStyle) -> Self {
        self.style = style;
        self
    }

    /// Set max visible results (builder).
    pub fn with_max_visible(mut self, n: usize) -> Self {
        self.max_visible = n;
        self
    }

    // --- Action Registration ---

    /// Register a new action.
    pub fn register(
        &mut self,
        title: impl Into<String>,
        description: Option<&str>,
        tags: &[&str],
    ) -> &mut Self {
        let title = title.into();
        let id = title.to_lowercase().replace(' ', "_");
        let mut item = ActionItem::new(id, title);
        if let Some(desc) = description {
            item.description = Some(desc.to_string());
        }
        item.tags = tags.iter().map(|s| (*s).to_string()).collect();
        self.actions.push(item);
        self.generation = self.generation.wrapping_add(1);
        self
    }

    /// Register an action item directly.
    pub fn register_action(&mut self, action: ActionItem) -> &mut Self {
        self.actions.push(action);
        self.generation = self.generation.wrapping_add(1);
        self
    }

    /// Number of registered actions.
    pub fn action_count(&self) -> usize {
        self.actions.len()
    }

    // --- Visibility ---

    /// Open the palette (show it and focus the query input).
    pub fn open(&mut self) {
        self.visible = true;
        self.query.clear();
        self.cursor = 0;
        self.selected = 0;
        self.scroll_offset = 0;
        self.scorer.invalidate();
        self.update_filtered();
    }

    /// Close the palette.
    pub fn close(&mut self) {
        self.visible = false;
        self.query.clear();
        self.cursor = 0;
        self.filtered.clear();
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        if self.visible {
            self.close();
        } else {
            self.open();
        }
    }

    /// Whether the palette is currently visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    // --- Query Access ---

    /// Current query string.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Number of filtered results.
    pub fn result_count(&self) -> usize {
        self.filtered.len()
    }

    /// Currently selected index.
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Get the currently selected action, if any.
    pub fn selected_action(&self) -> Option<&ActionItem> {
        self.filtered
            .get(self.selected)
            .map(|si| &self.actions[si.action_index])
    }

    /// Get scorer statistics for diagnostics.
    pub fn scorer_stats(&self) -> &IncrementalStats {
        self.scorer.stats()
    }

    // --- Event Handling ---

    /// Handle an input event. Returns a `PaletteAction` if the user executed
    /// or dismissed the palette.
    ///
    /// Returns `None` if the event was consumed but no action was triggered,
    /// or if the palette is not visible.
    pub fn handle_event(&mut self, event: &Event) -> Option<PaletteAction> {
        if !self.visible {
            // Check for open shortcut (Ctrl+P)
            if let Event::Key(KeyEvent {
                code: KeyCode::Char('p'),
                modifiers,
                kind: KeyEventKind::Press,
            }) = event
                && modifiers.contains(Modifiers::CTRL)
            {
                self.open();
            }
            return None;
        }

        match event {
            Event::Key(KeyEvent {
                code,
                modifiers,
                kind: KeyEventKind::Press,
            }) => self.handle_key(*code, *modifiers),
            _ => None,
        }
    }

    /// Handle a key press while the palette is open.
    fn handle_key(&mut self, code: KeyCode, modifiers: Modifiers) -> Option<PaletteAction> {
        match code {
            KeyCode::Escape => {
                self.close();
                return Some(PaletteAction::Dismiss);
            }

            KeyCode::Enter => {
                if let Some(si) = self.filtered.get(self.selected) {
                    let id = self.actions[si.action_index].id.clone();
                    self.close();
                    return Some(PaletteAction::Execute(id));
                }
            }

            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.adjust_scroll();
                }
            }

            KeyCode::Down => {
                if !self.filtered.is_empty() && self.selected < self.filtered.len() - 1 {
                    self.selected += 1;
                    self.adjust_scroll();
                }
            }

            KeyCode::PageUp => {
                self.selected = self.selected.saturating_sub(self.max_visible);
                self.adjust_scroll();
            }

            KeyCode::PageDown => {
                if !self.filtered.is_empty() {
                    self.selected = (self.selected + self.max_visible).min(self.filtered.len() - 1);
                    self.adjust_scroll();
                }
            }

            KeyCode::Home => {
                self.selected = 0;
                self.scroll_offset = 0;
            }

            KeyCode::End => {
                if !self.filtered.is_empty() {
                    self.selected = self.filtered.len() - 1;
                    self.adjust_scroll();
                }
            }

            KeyCode::Backspace => {
                if !self.query.is_empty() {
                    // Remove last character
                    self.query.pop();
                    self.cursor = self.query.len();
                    self.selected = 0;
                    self.scroll_offset = 0;
                    self.update_filtered();
                }
            }

            KeyCode::Char(c) => {
                if modifiers.contains(Modifiers::CTRL) {
                    // Ctrl+A: select all (move cursor to start)
                    if c == 'a' {
                        self.cursor = 0;
                    }
                    // Ctrl+U: clear query
                    if c == 'u' {
                        self.query.clear();
                        self.cursor = 0;
                        self.selected = 0;
                        self.scroll_offset = 0;
                        self.update_filtered();
                    }
                } else {
                    self.query.push(c);
                    self.cursor = self.query.len();
                    self.selected = 0;
                    self.scroll_offset = 0;
                    self.update_filtered();
                }
            }

            _ => {}
        }

        None
    }

    /// Re-score the corpus against the current query.
    fn update_filtered(&mut self) {
        let titles: Vec<&str> = self.actions.iter().map(|a| a.title.as_str()).collect();

        let results = self
            .scorer
            .score_corpus(&self.query, &titles, Some(self.generation));

        self.filtered = results
            .into_iter()
            .map(|(idx, result)| ScoredItem {
                action_index: idx,
                result,
            })
            .collect();

        // Clamp selection.
        if !self.filtered.is_empty() {
            self.selected = self.selected.min(self.filtered.len() - 1);
        } else {
            self.selected = 0;
        }
    }

    /// Adjust scroll_offset to keep selected item visible.
    fn adjust_scroll(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + self.max_visible {
            self.scroll_offset = self.selected + 1 - self.max_visible;
        }
    }
}

// ---------------------------------------------------------------------------
// Widget Implementation
// ---------------------------------------------------------------------------

impl Widget for CommandPalette {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if !self.visible || area.width < 10 || area.height < 5 {
            return;
        }

        // Calculate palette dimensions: centered, ~60% width, height based on results.
        let palette_width = (area.width * 3 / 5).max(30).min(area.width - 2);
        let result_rows = self.filtered.len().min(self.max_visible);
        // +3 for: border top, query line, border bottom. +1 if empty hint.
        let palette_height = (result_rows as u16 + 3)
            .max(5)
            .min(area.height.saturating_sub(2));
        let palette_x = area.x + (area.width.saturating_sub(palette_width)) / 2;
        let palette_y = area.y + area.height / 6; // ~1/6 from top

        let palette_area = Rect::new(palette_x, palette_y, palette_width, palette_height);

        // Clear the palette area.
        self.clear_area(palette_area, frame);

        // Draw border.
        self.draw_border(palette_area, frame);

        // Draw query input line.
        let input_area = Rect::new(
            palette_area.x + 2,
            palette_area.y + 1,
            palette_area.width.saturating_sub(4),
            1,
        );
        self.draw_query_input(input_area, frame);

        // Draw results list.
        let results_y = palette_area.y + 2;
        let results_height = palette_area.height.saturating_sub(3);
        let results_area = Rect::new(
            palette_area.x + 1,
            results_y,
            palette_area.width.saturating_sub(2),
            results_height,
        );
        self.draw_results(results_area, frame);

        // Position cursor in query input.
        let cursor_x = input_area.x + self.cursor.min(input_area.width as usize) as u16;
        frame.cursor_position = Some((cursor_x, input_area.y));
        frame.cursor_visible = true;
    }

    fn is_essential(&self) -> bool {
        true
    }
}

impl CommandPalette {
    /// Clear the palette area with a background color.
    fn clear_area(&self, area: Rect, frame: &mut Frame) {
        let bg = PackedRgba::rgb(30, 30, 40);
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                if let Some(cell) = frame.buffer.get_mut(x, y) {
                    *cell = Cell::from_char(' ');
                    cell.bg = bg;
                }
            }
        }
    }

    /// Draw a simple border around the palette.
    fn draw_border(&self, area: Rect, frame: &mut Frame) {
        let border_fg = self
            .style
            .border
            .fg
            .unwrap_or(PackedRgba::rgb(100, 100, 120));
        let bg = PackedRgba::rgb(30, 30, 40);

        // Top border with title.
        if let Some(cell) = frame.buffer.get_mut(area.x, area.y) {
            cell.content = CellContent::from_char('┌');
            cell.fg = border_fg;
            cell.bg = bg;
        }
        for x in (area.x + 1)..area.right().saturating_sub(1) {
            if let Some(cell) = frame.buffer.get_mut(x, area.y) {
                cell.content = CellContent::from_char('─');
                cell.fg = border_fg;
                cell.bg = bg;
            }
        }
        if area.width > 1
            && let Some(cell) = frame.buffer.get_mut(area.right() - 1, area.y)
        {
            cell.content = CellContent::from_char('┐');
            cell.fg = border_fg;
            cell.bg = bg;
        }

        // Title "Command Palette" in top border.
        let title = " Command Palette ";
        let title_x = area.x + (area.width.saturating_sub(title.len() as u16)) / 2;
        for (i, ch) in title.chars().enumerate() {
            let x = title_x + i as u16;
            if x < area.right()
                && let Some(cell) = frame.buffer.get_mut(x, area.y)
            {
                cell.content = CellContent::from_char(ch);
                cell.fg = PackedRgba::rgb(200, 200, 220);
                cell.bg = bg;
            }
        }

        // Side borders.
        for y in (area.y + 1)..area.bottom().saturating_sub(1) {
            if let Some(cell) = frame.buffer.get_mut(area.x, y) {
                cell.content = CellContent::from_char('│');
                cell.fg = border_fg;
                cell.bg = bg;
            }
            if area.width > 1
                && let Some(cell) = frame.buffer.get_mut(area.right() - 1, y)
            {
                cell.content = CellContent::from_char('│');
                cell.fg = border_fg;
                cell.bg = bg;
            }
        }

        // Bottom border.
        if area.height > 1 {
            let by = area.bottom() - 1;
            if let Some(cell) = frame.buffer.get_mut(area.x, by) {
                cell.content = CellContent::from_char('└');
                cell.fg = border_fg;
                cell.bg = bg;
            }
            for x in (area.x + 1)..area.right().saturating_sub(1) {
                if let Some(cell) = frame.buffer.get_mut(x, by) {
                    cell.content = CellContent::from_char('─');
                    cell.fg = border_fg;
                    cell.bg = bg;
                }
            }
            if area.width > 1
                && let Some(cell) = frame.buffer.get_mut(area.right() - 1, by)
            {
                cell.content = CellContent::from_char('┘');
                cell.fg = border_fg;
                cell.bg = bg;
            }
        }
    }

    /// Draw the query input line with prompt.
    fn draw_query_input(&self, area: Rect, frame: &mut Frame) {
        let input_fg = self
            .style
            .input
            .fg
            .unwrap_or(PackedRgba::rgb(220, 220, 230));
        let bg = PackedRgba::rgb(30, 30, 40);
        let prompt_fg = PackedRgba::rgb(100, 180, 255);

        // Draw ">" prompt.
        if let Some(cell) = frame.buffer.get_mut(area.x.saturating_sub(1), area.y) {
            cell.content = CellContent::from_char('>');
            cell.fg = prompt_fg;
            cell.bg = bg;
        }

        // Draw query text.
        if self.query.is_empty() {
            // Placeholder.
            let hint = "Type to search...";
            let hint_fg = self.style.hint.fg.unwrap_or(PackedRgba::rgb(100, 100, 120));
            for (i, ch) in hint.chars().enumerate() {
                let x = area.x + i as u16;
                if x >= area.right() {
                    break;
                }
                if let Some(cell) = frame.buffer.get_mut(x, area.y) {
                    cell.content = CellContent::from_char(ch);
                    cell.fg = hint_fg;
                    cell.bg = bg;
                }
            }
        } else {
            for (i, ch) in self.query.chars().enumerate() {
                let x = area.x + i as u16;
                if x >= area.right() {
                    break;
                }
                if ch.is_ascii()
                    && let Some(cell) = frame.buffer.get_mut(x, area.y)
                {
                    cell.content = CellContent::from_char(ch);
                    cell.fg = input_fg;
                    cell.bg = bg;
                }
            }
        }
    }

    /// Draw the filtered results list.
    fn draw_results(&self, area: Rect, frame: &mut Frame) {
        if self.filtered.is_empty() {
            // Empty state.
            let msg = if self.query.is_empty() {
                "No actions registered"
            } else {
                "No results"
            };
            let hint_fg = self.style.hint.fg.unwrap_or(PackedRgba::rgb(100, 100, 120));
            let bg = PackedRgba::rgb(30, 30, 40);
            for (i, ch) in msg.chars().enumerate() {
                let x = area.x + 1 + i as u16;
                if x >= area.right() {
                    break;
                }
                if let Some(cell) = frame.buffer.get_mut(x, area.y) {
                    cell.content = CellContent::from_char(ch);
                    cell.fg = hint_fg;
                    cell.bg = bg;
                }
            }
            return;
        }

        let item_fg = self.style.item.fg.unwrap_or(PackedRgba::rgb(180, 180, 190));
        let selected_fg = self
            .style
            .item_selected
            .fg
            .unwrap_or(PackedRgba::rgb(255, 255, 255));
        let selected_bg = self
            .style
            .item_selected
            .bg
            .unwrap_or(PackedRgba::rgb(60, 60, 80));
        let highlight_fg = self
            .style
            .match_highlight
            .fg
            .unwrap_or(PackedRgba::rgb(255, 200, 50));
        let desc_fg = self
            .style
            .description
            .fg
            .unwrap_or(PackedRgba::rgb(120, 120, 140));
        let cat_fg = self
            .style
            .category
            .fg
            .unwrap_or(PackedRgba::rgb(100, 180, 255));
        let bg = PackedRgba::rgb(30, 30, 40);

        let visible_end = (self.scroll_offset + area.height as usize).min(self.filtered.len());

        for (row_idx, si) in self.filtered[self.scroll_offset..visible_end]
            .iter()
            .enumerate()
        {
            let y = area.y + row_idx as u16;
            if y >= area.bottom() {
                break;
            }

            let action = &self.actions[si.action_index];
            let is_selected = (self.scroll_offset + row_idx) == self.selected;

            let row_fg = if is_selected { selected_fg } else { item_fg };
            let row_bg = if is_selected { selected_bg } else { bg };

            // Bold attribute for selected row (accessible without color).
            let row_attrs = if is_selected {
                CellAttrs::new(CellStyleFlags::BOLD, 0)
            } else {
                CellAttrs::default()
            };

            // Clear row.
            for x in area.x..area.right() {
                if let Some(cell) = frame.buffer.get_mut(x, y) {
                    cell.content = CellContent::from_char(' ');
                    cell.fg = row_fg;
                    cell.bg = row_bg;
                    cell.attrs = row_attrs;
                }
            }

            // Selection marker (visible without color — structural indicator).
            let mut col = area.x;
            if is_selected && let Some(cell) = frame.buffer.get_mut(col, y) {
                cell.content = CellContent::from_char('>');
                cell.fg = highlight_fg;
                cell.bg = row_bg;
                cell.attrs = CellAttrs::new(CellStyleFlags::BOLD, 0);
            }
            col += 2;

            // Category badge (if present).
            if let Some(ref cat) = action.category {
                let badge = format!("[{}] ", cat);
                for ch in badge.chars() {
                    if col >= area.right() {
                        break;
                    }
                    if let Some(cell) = frame.buffer.get_mut(col, y) {
                        cell.content = CellContent::from_char(ch);
                        cell.fg = cat_fg;
                        cell.bg = row_bg;
                        cell.attrs = row_attrs;
                    }
                    col += 1;
                }
            }

            // Title with match highlighting and ellipsis truncation.
            let title_max_width = area.right().saturating_sub(col) as usize;
            let title_len = action.title.chars().count();
            let needs_ellipsis = title_len > title_max_width && title_max_width > 3;
            let title_display_len = if needs_ellipsis {
                title_max_width.saturating_sub(1) // leave room for '…'
            } else {
                title_max_width
            };

            for (char_idx, ch) in action.title.chars().enumerate() {
                if char_idx >= title_display_len || col >= area.right() {
                    break;
                }
                let is_match = si.result.match_positions.contains(&char_idx);
                if let Some(cell) = frame.buffer.get_mut(col, y) {
                    cell.content = CellContent::from_char(ch);
                    cell.fg = if is_match { highlight_fg } else { row_fg };
                    cell.bg = row_bg;
                    cell.attrs = row_attrs;
                }
                col += 1;
            }

            // Ellipsis for truncated titles.
            if needs_ellipsis && col < area.right() {
                if let Some(cell) = frame.buffer.get_mut(col, y) {
                    cell.content = CellContent::from_char('\u{2026}'); // …
                    cell.fg = row_fg;
                    cell.bg = row_bg;
                    cell.attrs = row_attrs;
                }
                col += 1;
            }

            // Description (if space allows, with ellipsis truncation).
            if let Some(ref desc) = action.description {
                col += 2; // gap
                let max_desc_len = area.right().saturating_sub(col) as usize;
                if max_desc_len > 5 {
                    let desc_len = desc.chars().count();
                    let desc_needs_ellipsis = desc_len > max_desc_len && max_desc_len > 3;
                    let desc_display_len = if desc_needs_ellipsis {
                        max_desc_len.saturating_sub(1)
                    } else {
                        max_desc_len
                    };

                    for (i, ch) in desc.chars().enumerate() {
                        if i >= desc_display_len || col >= area.right() {
                            break;
                        }
                        if let Some(cell) = frame.buffer.get_mut(col, y) {
                            cell.content = CellContent::from_char(ch);
                            cell.fg = desc_fg;
                            cell.bg = row_bg;
                        }
                        col += 1;
                    }

                    if desc_needs_ellipsis
                        && col < area.right()
                        && let Some(cell) = frame.buffer.get_mut(col, y)
                    {
                        cell.content = CellContent::from_char('\u{2026}');
                        cell.fg = desc_fg;
                        cell.bg = row_bg;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod widget_tests {
    use super::*;

    #[test]
    fn new_palette_is_hidden() {
        let palette = CommandPalette::new();
        assert!(!palette.is_visible());
        assert_eq!(palette.action_count(), 0);
    }

    #[test]
    fn register_actions() {
        let mut palette = CommandPalette::new();
        palette.register("Open File", Some("Open a file"), &["file"]);
        palette.register("Save File", None, &[]);
        assert_eq!(palette.action_count(), 2);
    }

    #[test]
    fn open_shows_all_actions() {
        let mut palette = CommandPalette::new();
        palette.register("Open File", None, &[]);
        palette.register("Save File", None, &[]);
        palette.register("Close Tab", None, &[]);
        palette.open();
        assert!(palette.is_visible());
        assert_eq!(palette.result_count(), 3);
    }

    #[test]
    fn close_hides_palette() {
        let mut palette = CommandPalette::new();
        palette.open();
        assert!(palette.is_visible());
        palette.close();
        assert!(!palette.is_visible());
    }

    #[test]
    fn toggle_visibility() {
        let mut palette = CommandPalette::new();
        palette.toggle();
        assert!(palette.is_visible());
        palette.toggle();
        assert!(!palette.is_visible());
    }

    #[test]
    fn typing_filters_results() {
        let mut palette = CommandPalette::new();
        palette.register("Open File", None, &[]);
        palette.register("Save File", None, &[]);
        palette.register("Git: Commit", None, &[]);
        palette.open();
        assert_eq!(palette.result_count(), 3);

        // Type "git"
        let g = Event::Key(KeyEvent {
            code: KeyCode::Char('g'),
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });
        let i = Event::Key(KeyEvent {
            code: KeyCode::Char('i'),
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });
        let t = Event::Key(KeyEvent {
            code: KeyCode::Char('t'),
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });

        palette.handle_event(&g);
        palette.handle_event(&i);
        palette.handle_event(&t);

        assert_eq!(palette.query(), "git");
        // Only "Git: Commit" should match well
        assert!(palette.result_count() >= 1);
    }

    #[test]
    fn backspace_removes_character() {
        let mut palette = CommandPalette::new();
        palette.register("Open File", None, &[]);
        palette.open();

        let o = Event::Key(KeyEvent {
            code: KeyCode::Char('o'),
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });
        let bs = Event::Key(KeyEvent {
            code: KeyCode::Backspace,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });

        palette.handle_event(&o);
        assert_eq!(palette.query(), "o");
        palette.handle_event(&bs);
        assert_eq!(palette.query(), "");
    }

    #[test]
    fn esc_dismisses_palette() {
        let mut palette = CommandPalette::new();
        palette.open();

        let esc = Event::Key(KeyEvent {
            code: KeyCode::Escape,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });

        let result = palette.handle_event(&esc);
        assert_eq!(result, Some(PaletteAction::Dismiss));
        assert!(!palette.is_visible());
    }

    #[test]
    fn enter_executes_selected() {
        let mut palette = CommandPalette::new();
        palette.register("Open File", None, &[]);
        palette.open();

        let enter = Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });

        let result = palette.handle_event(&enter);
        assert_eq!(result, Some(PaletteAction::Execute("open_file".into())));
    }

    #[test]
    fn arrow_keys_navigate() {
        let mut palette = CommandPalette::new();
        palette.register("A", None, &[]);
        palette.register("B", None, &[]);
        palette.register("C", None, &[]);
        palette.open();

        assert_eq!(palette.selected_index(), 0);

        let down = Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });
        let up = Event::Key(KeyEvent {
            code: KeyCode::Up,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });

        palette.handle_event(&down);
        assert_eq!(palette.selected_index(), 1);
        palette.handle_event(&down);
        assert_eq!(palette.selected_index(), 2);
        // Can't go past end
        palette.handle_event(&down);
        assert_eq!(palette.selected_index(), 2);

        palette.handle_event(&up);
        assert_eq!(palette.selected_index(), 1);
        palette.handle_event(&up);
        assert_eq!(palette.selected_index(), 0);
        // Can't go below 0
        palette.handle_event(&up);
        assert_eq!(palette.selected_index(), 0);
    }

    #[test]
    fn home_end_navigation() {
        let mut palette = CommandPalette::new();
        for i in 0..20 {
            palette.register(format!("Action {}", i), None, &[]);
        }
        palette.open();

        let end = Event::Key(KeyEvent {
            code: KeyCode::End,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });
        let home = Event::Key(KeyEvent {
            code: KeyCode::Home,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });

        palette.handle_event(&end);
        assert_eq!(palette.selected_index(), 19);

        palette.handle_event(&home);
        assert_eq!(palette.selected_index(), 0);
    }

    #[test]
    fn ctrl_u_clears_query() {
        let mut palette = CommandPalette::new();
        palette.register("Open File", None, &[]);
        palette.open();

        let o = Event::Key(KeyEvent {
            code: KeyCode::Char('o'),
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });
        palette.handle_event(&o);
        assert_eq!(palette.query(), "o");

        let ctrl_u = Event::Key(KeyEvent {
            code: KeyCode::Char('u'),
            modifiers: Modifiers::CTRL,
            kind: KeyEventKind::Press,
        });
        palette.handle_event(&ctrl_u);
        assert_eq!(palette.query(), "");
    }

    #[test]
    fn ctrl_p_opens_palette() {
        let mut palette = CommandPalette::new();
        assert!(!palette.is_visible());

        let ctrl_p = Event::Key(KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: Modifiers::CTRL,
            kind: KeyEventKind::Press,
        });
        palette.handle_event(&ctrl_p);
        assert!(palette.is_visible());
    }

    #[test]
    fn selected_action_returns_correct_item() {
        let mut palette = CommandPalette::new();
        palette.register("Alpha", None, &[]);
        palette.register("Beta", None, &[]);
        palette.open();

        let action = palette.selected_action().unwrap();
        // With empty query, all items shown — first by score (neutral, so first registered)
        assert!(!action.title.is_empty());
    }

    #[test]
    fn register_action_item_directly() {
        let mut palette = CommandPalette::new();
        let item = ActionItem::new("custom_id", "Custom Action")
            .with_description("A custom action")
            .with_tags(&["custom", "test"])
            .with_category("Testing");

        palette.register_action(item);
        assert_eq!(palette.action_count(), 1);
    }

    #[test]
    fn events_ignored_when_hidden() {
        let mut palette = CommandPalette::new();
        // Not Ctrl+P, so should be ignored
        let a = Event::Key(KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });
        assert!(palette.handle_event(&a).is_none());
        assert!(!palette.is_visible());
    }

    // -----------------------------------------------------------------------
    // Accessibility / UX tests (bd-39y4.10)
    // -----------------------------------------------------------------------

    #[test]
    fn selected_row_has_bold_attribute() {
        use ftui_render::grapheme_pool::GraphemePool;

        let mut palette = CommandPalette::new();
        palette.register("Alpha", None, &[]);
        palette.register("Beta", None, &[]);
        palette.open();

        let area = Rect::from_size(60, 10);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(60, 10, &mut pool);
        palette.render(area, &mut frame);

        // The selected row (first result) should have bold cells.
        // Results start at y=2 inside the palette (after border + query).
        let palette_y = area.y + area.height / 6;
        let result_y = palette_y + 2;

        // Check that at least one cell in the first result row is bold
        let mut found_bold = false;
        for x in 0..60u16 {
            if let Some(cell) = frame.buffer.get(x, result_y) {
                if cell.attrs.flags().contains(CellStyleFlags::BOLD) {
                    found_bold = true;
                    break;
                }
            }
        }
        assert!(
            found_bold,
            "Selected row should have bold attribute for accessibility"
        );
    }

    #[test]
    fn selection_marker_visible() {
        use ftui_render::grapheme_pool::GraphemePool;

        let mut palette = CommandPalette::new();
        palette.register("Alpha", None, &[]);
        palette.open();

        let area = Rect::from_size(60, 10);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(60, 10, &mut pool);
        palette.render(area, &mut frame);

        // Find the '>' selection marker in the results area
        let palette_y = area.y + area.height / 6;
        let result_y = palette_y + 2;
        let mut found_marker = false;
        for x in 0..60u16 {
            if let Some(cell) = frame.buffer.get(x, result_y) {
                if cell.content.as_char() == Some('>') {
                    found_marker = true;
                    break;
                }
            }
        }
        assert!(
            found_marker,
            "Selection marker '>' should be visible (color-independent indicator)"
        );
    }

    #[test]
    fn long_title_truncated_with_ellipsis() {
        use ftui_render::grapheme_pool::GraphemePool;

        let mut palette = CommandPalette::new().with_max_visible(5);
        palette.register(
            "This Is A Very Long Action Title That Should Be Truncated With Ellipsis",
            None,
            &[],
        );
        palette.open();

        // Render in a narrow area
        let area = Rect::from_size(40, 10);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(40, 10, &mut pool);
        palette.render(area, &mut frame);

        // Find the ellipsis character '…' in the results area
        let palette_y = area.y + area.height / 6;
        let result_y = palette_y + 2;
        let mut found_ellipsis = false;
        for x in 0..40u16 {
            if let Some(cell) = frame.buffer.get(x, result_y) {
                if cell.content.as_char() == Some('\u{2026}') {
                    found_ellipsis = true;
                    break;
                }
            }
        }
        assert!(
            found_ellipsis,
            "Long titles should be truncated with '…' ellipsis"
        );
    }

    #[test]
    fn keyboard_only_flow_end_to_end() {
        let mut palette = CommandPalette::new();
        palette.register("Open File", Some("Open a file from disk"), &["file"]);
        palette.register("Save File", Some("Save current file"), &["file"]);
        palette.register("Git: Commit", Some("Commit changes"), &["git"]);

        // Step 1: Open with Ctrl+P
        let ctrl_p = Event::Key(KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: Modifiers::CTRL,
            kind: KeyEventKind::Press,
        });
        palette.handle_event(&ctrl_p);
        assert!(palette.is_visible());
        assert_eq!(palette.result_count(), 3);

        // Step 2: Type query to filter
        for ch in "git".chars() {
            let event = Event::Key(KeyEvent {
                code: KeyCode::Char(ch),
                modifiers: Modifiers::empty(),
                kind: KeyEventKind::Press,
            });
            palette.handle_event(&event);
        }
        assert!(palette.result_count() >= 1);

        // Step 3: Navigate down (in case selected isn't the right one)
        let down = Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });
        palette.handle_event(&down);

        // Step 4: Navigate back up
        let up = Event::Key(KeyEvent {
            code: KeyCode::Up,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });
        palette.handle_event(&up);
        assert_eq!(palette.selected_index(), 0);

        // Step 5: Execute with Enter
        let enter = Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });
        let result = palette.handle_event(&enter);
        assert!(matches!(result, Some(PaletteAction::Execute(_))));
        assert!(!palette.is_visible());
    }

    #[test]
    fn no_focus_trap_esc_always_dismisses() {
        let mut palette = CommandPalette::new();
        palette.register("Alpha", None, &[]);
        palette.open();

        // Type some query
        for ch in "xyz".chars() {
            let event = Event::Key(KeyEvent {
                code: KeyCode::Char(ch),
                modifiers: Modifiers::empty(),
                kind: KeyEventKind::Press,
            });
            palette.handle_event(&event);
        }
        assert_eq!(palette.result_count(), 0); // no matches

        // Esc should still dismiss even with no results
        let esc = Event::Key(KeyEvent {
            code: KeyCode::Escape,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        });
        let result = palette.handle_event(&esc);
        assert_eq!(result, Some(PaletteAction::Dismiss));
        assert!(!palette.is_visible());
    }

    #[test]
    fn wcag_aa_contrast_ratios() {
        // Verify the default style colors meet WCAG AA contrast requirements.
        // WCAG AA requires >= 4.5:1 for normal text.
        let style = PaletteStyle::default();
        let bg = PackedRgba::rgb(30, 30, 40);

        // Helper: relative luminance per WCAG 2.0
        fn relative_luminance(color: PackedRgba) -> f64 {
            fn linearize(c: u8) -> f64 {
                let v = c as f64 / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            }
            let r = linearize(color.r());
            let g = linearize(color.g());
            let b = linearize(color.b());
            0.2126 * r + 0.7152 * g + 0.0722 * b
        }

        fn contrast_ratio(fg: PackedRgba, bg: PackedRgba) -> f64 {
            let l1 = relative_luminance(fg);
            let l2 = relative_luminance(bg);
            let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
            (lighter + 0.05) / (darker + 0.05)
        }

        // Item text on palette background
        let item_fg = style.item.fg.unwrap();
        let item_ratio = contrast_ratio(item_fg, bg);
        assert!(
            item_ratio >= 4.5,
            "Item text contrast {:.1}:1 < 4.5:1 (WCAG AA)",
            item_ratio
        );

        // Selected text on selected background
        let sel_fg = style.item_selected.fg.unwrap();
        let sel_bg = style.item_selected.bg.unwrap();
        let sel_ratio = contrast_ratio(sel_fg, sel_bg);
        assert!(
            sel_ratio >= 4.5,
            "Selected text contrast {:.1}:1 < 4.5:1 (WCAG AA)",
            sel_ratio
        );

        // Match highlight on palette background
        let hl_fg = style.match_highlight.fg.unwrap();
        let hl_ratio = contrast_ratio(hl_fg, bg);
        assert!(
            hl_ratio >= 4.5,
            "Highlight text contrast {:.1}:1 < 4.5:1 (WCAG AA)",
            hl_ratio
        );

        // Description text on palette background
        let desc_fg = style.description.fg.unwrap();
        let desc_ratio = contrast_ratio(desc_fg, bg);
        assert!(
            desc_ratio >= 4.5,
            "Description text contrast {:.1}:1 < 4.5:1 (WCAG AA)",
            desc_ratio
        );
    }
}
