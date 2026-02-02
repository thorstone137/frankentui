#![forbid(unsafe_code)]

//! Shakespeare Library screen — complete works with search and virtualized scroll.

use ftui_core::event::{Event, KeyCode, KeyEventKind, Modifiers};
use ftui_core::geometry::Rect;
use ftui_layout::{Constraint, Flex};
use ftui_render::frame::Frame;
use ftui_runtime::Cmd;
use ftui_style::{Style, StyleFlags};
use ftui_text::search::search_ascii_case_insensitive;
use ftui_widgets::StatefulWidget;
use ftui_widgets::Widget;
use ftui_widgets::block::{Alignment, Block};
use ftui_widgets::borders::{BorderType, Borders};
use ftui_widgets::input::TextInput;
use ftui_widgets::paragraph::Paragraph;
use ftui_widgets::scrollbar::{Scrollbar, ScrollbarOrientation, ScrollbarState};
use ftui_widgets::tree::{Tree, TreeGuides, TreeNode};

use super::{HelpEntry, Screen};
use crate::theme;

/// Embedded complete works of Shakespeare.
const SHAKESPEARE_TEXT: &str = include_str!("../../data/shakespeare.txt");

/// A table-of-contents entry (play/section title and its line number).
struct TocEntry {
    title: String,
    line: usize,
}

/// Shakespeare Library screen state.
pub struct Shakespeare {
    /// All lines of the text, as static string slices.
    lines: Vec<&'static str>,
    /// Current scroll offset (top visible line).
    scroll_offset: usize,
    /// Table of contents entries.
    toc_entries: Vec<TocEntry>,
    /// Tree widget for TOC display.
    toc_tree: Tree,
    /// Search input widget.
    search_input: TextInput,
    /// Whether search bar is focused/visible.
    search_active: bool,
    /// Line indices matching the current search query.
    search_matches: Vec<usize>,
    /// Index into search_matches for current highlighted match.
    current_match: usize,
    /// Viewport height (lines visible), updated each render.
    viewport_height: u16,
}

impl Shakespeare {
    pub fn new() -> Self {
        let lines: Vec<&'static str> = SHAKESPEARE_TEXT.lines().collect();
        let toc_entries = Self::build_toc(&lines);
        let toc_tree = Self::build_tree(&toc_entries);

        Self {
            lines,
            scroll_offset: 0,
            toc_entries,
            toc_tree,
            search_input: TextInput::new()
                .with_placeholder("Search... (/ to focus, Esc to close)")
                .with_style(Style::new().fg(theme::fg::PRIMARY).bg(theme::bg::SURFACE))
                .with_placeholder_style(Style::new().fg(theme::fg::MUTED)),
            search_active: false,
            search_matches: Vec::new(),
            current_match: 0,
            viewport_height: 20,
        }
    }

    /// Build table of contents from the text by detecting play titles.
    /// Titles are lines that appear in ALL CAPS and match known patterns.
    fn build_toc(lines: &[&str]) -> Vec<TocEntry> {
        let mut entries = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            // Skip empty lines and very short lines
            if trimmed.len() < 5 {
                continue;
            }
            // Detect section headers: lines that are all uppercase letters/spaces/punctuation
            // and appear after a blank line, typical of Gutenberg Shakespeare formatting.
            // Also check for "THE SONNETS", "ACT" headers, play titles, etc.
            if i > 0
                && trimmed.len() > 8
                && trimmed.len() < 80
                && is_title_line(trimmed)
                && (i == 0
                    || lines
                        .get(i.wrapping_sub(1))
                        .is_some_and(|l| l.trim().is_empty()))
            {
                entries.push(TocEntry {
                    title: trimmed.to_owned(),
                    line: i,
                });
            }
        }
        entries
    }

    /// Build a Tree widget from TOC entries.
    fn build_tree(entries: &[TocEntry]) -> Tree {
        let mut root = TreeNode::new("Complete Works").with_expanded(true);
        for entry in entries.iter().take(50) {
            // Truncate long titles for the tree
            let label = if entry.title.len() > 35 {
                format!("{}...", &entry.title[..32])
            } else {
                entry.title.clone()
            };
            root = root.child(TreeNode::new(label));
        }
        Tree::new(root)
            .with_show_root(true)
            .with_guides(TreeGuides::Unicode)
            .with_guide_style(Style::new().fg(theme::fg::MUTED))
            .with_label_style(Style::new().fg(theme::fg::SECONDARY))
            .with_root_style(
                Style::new()
                    .fg(theme::accent::PRIMARY)
                    .attrs(StyleFlags::BOLD),
            )
    }

    fn total_lines(&self) -> usize {
        self.lines.len()
    }

    fn scroll_by(&mut self, delta: i32) {
        let max_offset = self
            .total_lines()
            .saturating_sub(self.viewport_height as usize);
        if delta < 0 {
            self.scroll_offset = self
                .scroll_offset
                .saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.scroll_offset = (self.scroll_offset + delta as usize).min(max_offset);
        }
    }

    fn scroll_to(&mut self, line: usize) {
        let max_offset = self
            .total_lines()
            .saturating_sub(self.viewport_height as usize);
        self.scroll_offset = line.min(max_offset);
    }

    fn perform_search(&mut self) {
        let query = self.search_input.value().to_owned();
        self.search_matches.clear();
        self.current_match = 0;
        if query.len() < 2 {
            return;
        }
        for (i, line) in self.lines.iter().enumerate() {
            let results = search_ascii_case_insensitive(line, &query);
            if !results.is_empty() {
                self.search_matches.push(i);
            }
        }
        // Jump to first match
        if let Some(&first) = self.search_matches.first() {
            self.scroll_to(first.saturating_sub(3));
        }
    }

    fn next_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.current_match = (self.current_match + 1) % self.search_matches.len();
        let line = self.search_matches[self.current_match];
        self.scroll_to(line.saturating_sub(3));
    }

    fn prev_match(&mut self) {
        if self.search_matches.is_empty() {
            return;
        }
        self.current_match =
            (self.current_match + self.search_matches.len() - 1) % self.search_matches.len();
        let line = self.search_matches[self.current_match];
        self.scroll_to(line.saturating_sub(3));
    }

    /// Determine the current play/section name based on scroll position.
    fn current_section(&self) -> &str {
        let mut section = "Preamble";
        for entry in &self.toc_entries {
            if entry.line <= self.scroll_offset {
                section = &entry.title;
            } else {
                break;
            }
        }
        section
    }
}

/// Check if a line looks like a title (mostly uppercase with allowed punctuation).
fn is_title_line(s: &str) -> bool {
    let alpha_count = s.chars().filter(|c| c.is_alphabetic()).count();
    if alpha_count < 4 {
        return false;
    }
    let upper_count = s.chars().filter(|c| c.is_uppercase()).count();
    // At least 80% uppercase letters
    upper_count * 100 / alpha_count.max(1) >= 80
}

impl Screen for Shakespeare {
    type Message = Event;

    fn update(&mut self, event: &Event) -> Cmd<Self::Message> {
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Press {
                return Cmd::None;
            }

            // Search mode input handling
            if self.search_active {
                match (key.code, key.modifiers) {
                    (KeyCode::Escape, _) => {
                        self.search_active = false;
                        self.search_input.set_focused(false);
                        return Cmd::None;
                    }
                    (KeyCode::Enter, _) => {
                        self.perform_search();
                        return Cmd::None;
                    }
                    _ => {
                        let handled = self.search_input.handle_event(event);
                        if handled {
                            // Live search on each keystroke
                            self.perform_search();
                        }
                        return Cmd::None;
                    }
                }
            }

            // Normal mode keybindings
            match (key.code, key.modifiers) {
                (KeyCode::Char('/'), Modifiers::NONE) => {
                    self.search_active = true;
                    self.search_input.set_focused(true);
                }
                (KeyCode::Char('n'), Modifiers::NONE) => self.next_match(),
                (KeyCode::Char('N'), Modifiers::NONE) | (KeyCode::Char('n'), Modifiers::SHIFT) => {
                    self.prev_match();
                }
                (KeyCode::Char('j'), Modifiers::NONE) | (KeyCode::Down, Modifiers::NONE) => {
                    self.scroll_by(1);
                }
                (KeyCode::Char('k'), Modifiers::NONE) | (KeyCode::Up, Modifiers::NONE) => {
                    self.scroll_by(-1);
                }
                (KeyCode::Char('d'), Modifiers::CTRL) | (KeyCode::PageDown, _) => {
                    self.scroll_by(self.viewport_height as i32 / 2);
                }
                (KeyCode::Char('u'), Modifiers::CTRL) | (KeyCode::PageUp, _) => {
                    self.scroll_by(-(self.viewport_height as i32 / 2));
                }
                (KeyCode::Home, _) | (KeyCode::Char('g'), Modifiers::NONE) => {
                    self.scroll_to(0);
                }
                (KeyCode::End, _) | (KeyCode::Char('G'), Modifiers::NONE) => {
                    self.scroll_to(self.total_lines());
                }
                _ => {}
            }
        }
        Cmd::None
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        if area.height < 5 || area.width < 30 {
            Paragraph::new("Terminal too small")
                .style(theme::muted())
                .render(area, frame);
            return;
        }

        // Vertical layout: search bar (if active) + body + status
        let v_chunks = Flex::vertical()
            .constraints(if self.search_active {
                vec![
                    Constraint::Fixed(1),
                    Constraint::Min(4),
                    Constraint::Fixed(1),
                ]
            } else {
                vec![Constraint::Min(4), Constraint::Fixed(1)]
            })
            .split(area);

        let (body_area, status_area) = if self.search_active {
            self.render_search_bar(frame, v_chunks[0]);
            (v_chunks[1], v_chunks[2])
        } else {
            (v_chunks[0], v_chunks[1])
        };

        // Body: left (text) + right (TOC)
        let h_chunks = Flex::horizontal()
            .constraints([Constraint::Percentage(72.0), Constraint::Percentage(28.0)])
            .split(body_area);

        self.render_text_panel(frame, h_chunks[0]);
        self.render_toc_panel(frame, h_chunks[1]);
        self.render_status_bar(frame, status_area);
    }

    fn keybindings(&self) -> Vec<HelpEntry> {
        vec![
            HelpEntry {
                key: "/",
                action: "Search",
            },
            HelpEntry {
                key: "n/N",
                action: "Next/prev match",
            },
            HelpEntry {
                key: "j/k",
                action: "Scroll up/down",
            },
            HelpEntry {
                key: "g/G",
                action: "Top/bottom",
            },
            HelpEntry {
                key: "PgUp/PgDn",
                action: "Page scroll",
            },
        ]
    }

    fn title(&self) -> &'static str {
        "Shakespeare Library"
    }

    fn tab_label(&self) -> &'static str {
        "Shakespeare"
    }
}

impl Shakespeare {
    fn render_search_bar(&self, frame: &mut Frame, area: Rect) {
        let h = Flex::horizontal()
            .constraints([
                Constraint::Fixed(8),
                Constraint::Min(10),
                Constraint::Fixed(20),
            ])
            .split(area);

        Paragraph::new(" Search:")
            .style(Style::new().fg(theme::accent::INFO).attrs(StyleFlags::BOLD))
            .render(h[0], frame);
        self.search_input.render(h[1], frame);

        let match_info = if self.search_matches.is_empty() {
            if self.search_input.value().len() >= 2 {
                " No matches".to_owned()
            } else {
                String::new()
            }
        } else {
            format!(
                " {}/{} matches",
                self.current_match + 1,
                self.search_matches.len()
            )
        };
        Paragraph::new(match_info)
            .style(Style::new().fg(theme::fg::MUTED))
            .render(h[2], frame);
    }

    fn render_text_panel(&self, frame: &mut Frame, area: Rect) {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Heavy)
            .title("Complete Works of William Shakespeare")
            .title_alignment(Alignment::Center)
            .style(theme::content_border());
        let inner = block.inner(area);
        block.render(area, frame);

        if inner.height == 0 || inner.width < 5 {
            return;
        }

        // Reserve 1 col for scrollbar on the right
        let text_width = inner.width.saturating_sub(1);
        let text_area = Rect::new(inner.x, inner.y, text_width, inner.height);
        let scrollbar_area = Rect::new(inner.x + text_width, inner.y, 1, inner.height);

        // Update viewport height (conceptual - we'd need &mut self, but we can use the value
        // from the area directly)
        let vh = inner.height as usize;

        // Render visible lines
        let query = self.search_input.value();
        let has_query = query.len() >= 2;

        for row in 0..vh {
            let line_idx = self.scroll_offset + row;
            if line_idx >= self.lines.len() {
                break;
            }

            let line = self.lines[line_idx];
            let line_area = Rect::new(
                text_area.x,
                text_area.y.saturating_add(row as u16),
                text_area.width,
                1,
            );

            // Determine style: highlight if it's a search match
            let is_current_match = has_query
                && !self.search_matches.is_empty()
                && self.search_matches.get(self.current_match) == Some(&line_idx);
            let is_any_match = has_query && self.search_matches.contains(&line_idx);

            let style = if is_current_match {
                Style::new()
                    .fg(theme::bg::DEEP)
                    .bg(theme::accent::WARNING)
                    .attrs(StyleFlags::BOLD)
            } else if is_any_match {
                Style::new().fg(theme::fg::PRIMARY).bg(theme::bg::HIGHLIGHT)
            } else {
                Style::new().fg(theme::fg::SECONDARY)
            };

            // Line number prefix
            let line_num = format!("{:>6} ", line_idx + 1);
            let num_width = 7u16.min(text_area.width);
            let content_width = text_area.width.saturating_sub(num_width);

            let num_area = Rect::new(line_area.x, line_area.y, num_width, 1);
            let content_area = Rect::new(line_area.x + num_width, line_area.y, content_width, 1);

            Paragraph::new(line_num)
                .style(Style::new().fg(theme::fg::MUTED))
                .render(num_area, frame);
            Paragraph::new(line)
                .style(style)
                .render(content_area, frame);
        }

        // Scrollbar
        let mut scrollbar_state = ScrollbarState::new(self.total_lines(), self.scroll_offset, vh);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(theme::accent::PRIMARY))
            .track_style(Style::new().fg(theme::bg::SURFACE));
        StatefulWidget::render(&scrollbar, scrollbar_area, frame, &mut scrollbar_state);
    }

    fn render_toc_panel(&self, frame: &mut Frame, area: Rect) {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Contents")
            .title_alignment(Alignment::Center)
            .style(theme::content_border());
        let inner = block.inner(area);
        block.render(area, frame);

        if inner.height == 0 || inner.width < 5 {
            return;
        }

        self.toc_tree.render(inner, frame);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let section = self.current_section();
        let total = self.total_lines();
        let pos = self.scroll_offset + 1;
        let pct = (self.scroll_offset * 100).checked_div(total).unwrap_or(0);

        let status = format!(
            " Line {pos}/{total} ({pct}%) | {}",
            if section.len() > 40 {
                &section[..40]
            } else {
                section
            }
        );

        Paragraph::new(status)
            .style(Style::new().fg(theme::fg::MUTED).bg(theme::bg::SURFACE))
            .render(area, frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shakespeare_initial_state() {
        let s = Shakespeare::new();
        assert_eq!(s.scroll_offset, 0);
        assert!(s.total_lines() > 100_000, "Should have 100K+ lines");
        assert!(!s.toc_entries.is_empty(), "Should have TOC entries");
    }

    #[test]
    fn shakespeare_search_to_be() {
        let mut s = Shakespeare::new();
        s.search_input.set_value("To be, or not to be");
        s.perform_search();
        assert!(
            !s.search_matches.is_empty(),
            "Should find 'To be or not to be' in Shakespeare"
        );
    }

    #[test]
    fn shakespeare_scroll_to_end() {
        let mut s = Shakespeare::new();
        s.viewport_height = 40;
        s.scroll_to(s.total_lines());
        let max_offset = s.total_lines().saturating_sub(s.viewport_height as usize);
        assert_eq!(s.scroll_offset, max_offset);
    }

    #[test]
    fn shakespeare_scroll_navigation() {
        let mut s = Shakespeare::new();
        s.viewport_height = 40;
        s.scroll_by(10);
        assert_eq!(s.scroll_offset, 10);
        s.scroll_by(-5);
        assert_eq!(s.scroll_offset, 5);
        // Can't scroll below 0
        s.scroll_by(-100);
        assert_eq!(s.scroll_offset, 0);
    }

    #[test]
    fn shakespeare_toc_has_plays() {
        let s = Shakespeare::new();
        let titles: Vec<&str> = s.toc_entries.iter().map(|e| e.title.as_str()).collect();
        // Should find major plays
        assert!(
            titles.iter().any(|t| t.contains("HAMLET")),
            "TOC should contain Hamlet, found: {:?}",
            &titles[..titles.len().min(20)]
        );
    }
}
