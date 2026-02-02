#![forbid(unsafe_code)]

//! Code Explorer screen — SQLite C source with syntax highlighting and search.

use ftui_core::event::{Event, KeyCode, KeyEventKind, Modifiers};
use ftui_core::geometry::Rect;
use ftui_extras::filesize;
use ftui_extras::syntax::{GenericTokenizer, GenericTokenizerConfig, SyntaxHighlighter};
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
use ftui_widgets::json_view::JsonView;
use ftui_widgets::paragraph::Paragraph;
use ftui_widgets::scrollbar::{Scrollbar, ScrollbarOrientation, ScrollbarState};

use super::{HelpEntry, Screen};
use crate::theme;

/// Embedded SQLite amalgamation source.
const SQLITE_SOURCE: &str = include_str!("../../data/sqlite3.c");

/// C language tokenizer configuration.
fn c_tokenizer() -> GenericTokenizer {
    GenericTokenizer::new(GenericTokenizerConfig {
        name: "C",
        extensions: &["c", "h"],
        keywords: &[
            "auto",
            "break",
            "case",
            "const",
            "continue",
            "default",
            "do",
            "else",
            "enum",
            "extern",
            "for",
            "goto",
            "if",
            "inline",
            "register",
            "restrict",
            "return",
            "sizeof",
            "static",
            "struct",
            "switch",
            "typedef",
            "union",
            "volatile",
            "while",
            "_Alignas",
            "_Alignof",
            "_Atomic",
            "_Bool",
            "_Complex",
            "_Generic",
            "_Imaginary",
            "_Noreturn",
            "_Static_assert",
            "_Thread_local",
        ],
        control_keywords: &[
            "if", "else", "for", "while", "do", "switch", "case", "default", "break", "continue",
            "return", "goto",
        ],
        type_keywords: &[
            "void",
            "char",
            "short",
            "int",
            "long",
            "float",
            "double",
            "signed",
            "unsigned",
            "size_t",
            "ssize_t",
            "int8_t",
            "int16_t",
            "int32_t",
            "int64_t",
            "uint8_t",
            "uint16_t",
            "uint32_t",
            "uint64_t",
            "ptrdiff_t",
            "intptr_t",
            "uintptr_t",
            "FILE",
            "NULL",
        ],
        line_comment: "//",
        block_comment_start: "/*",
        block_comment_end: "*/",
    })
}

/// Code Explorer screen state.
pub struct CodeExplorer {
    /// All source lines.
    lines: Vec<&'static str>,
    /// Current scroll offset (top visible line).
    scroll_offset: usize,
    /// Viewport height in lines.
    viewport_height: u16,
    /// Syntax highlighter with C language support.
    highlighter: SyntaxHighlighter,
    /// Search input.
    search_input: TextInput,
    /// Whether search bar is active.
    search_active: bool,
    /// Goto-line input.
    goto_input: TextInput,
    /// Whether goto-line is active.
    goto_active: bool,
    /// Line indices matching search query.
    search_matches: Vec<usize>,
    /// Current match index.
    current_match: usize,
    /// File metadata as JSON string.
    metadata_json: String,
}

impl CodeExplorer {
    pub fn new() -> Self {
        let lines: Vec<&'static str> = SQLITE_SOURCE.lines().collect();
        let line_count = lines.len();
        let byte_size = SQLITE_SOURCE.len() as u64;

        let mut highlighter = SyntaxHighlighter::new();
        highlighter.register_tokenizer(Box::new(c_tokenizer()));

        let metadata_json = format!(
            "{{\n  \"filename\": \"sqlite3.c\",\n  \"lines\": {},\n  \"size\": \"{}\",\n  \"size_bytes\": {},\n  \"language\": \"C\",\n  \"description\": \"SQLite amalgamation\"\n}}",
            line_count,
            filesize::decimal(byte_size),
            byte_size,
        );

        Self {
            lines,
            scroll_offset: 0,
            viewport_height: 30,
            highlighter,
            search_input: TextInput::new()
                .with_placeholder("Search code... (/ to focus)")
                .with_style(Style::new().fg(theme::fg::PRIMARY).bg(theme::bg::SURFACE))
                .with_placeholder_style(Style::new().fg(theme::fg::MUTED)),
            search_active: false,
            goto_input: TextInput::new()
                .with_placeholder("Line number...")
                .with_style(Style::new().fg(theme::fg::PRIMARY).bg(theme::bg::SURFACE))
                .with_placeholder_style(Style::new().fg(theme::fg::MUTED)),
            goto_active: false,
            search_matches: Vec::new(),
            current_match: 0,
            metadata_json,
        }
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

    fn goto_line(&mut self) {
        if let Ok(line_num) = self.goto_input.value().trim().parse::<usize>()
            && line_num > 0
        {
            self.scroll_to(line_num.saturating_sub(1));
        }
    }

    /// Find the nearest function context from comments/declarations near current position.
    fn current_context(&self) -> &str {
        // Walk backwards from scroll position looking for a function-like declaration
        let start = self.scroll_offset;
        for i in (start.saturating_sub(200)..=start).rev() {
            if i >= self.lines.len() {
                continue;
            }
            let line = self.lines[i].trim();
            // C function definition: something like `int foo(` or `static void bar(`
            if line.contains('(')
                && !line.starts_with(' ')
                && !line.starts_with('\t')
                && !line.starts_with("/*")
                && !line.starts_with("*")
                && !line.starts_with("#")
                && line.len() > 5
            {
                return self.lines[i];
            }
        }
        "Top of file"
    }
}

impl Screen for CodeExplorer {
    type Message = Event;

    fn update(&mut self, event: &Event) -> Cmd<Self::Message> {
        if let Event::Key(key) = event {
            if key.kind != KeyEventKind::Press {
                return Cmd::None;
            }

            // Goto-line mode
            if self.goto_active {
                match (key.code, key.modifiers) {
                    (KeyCode::Escape, _) => {
                        self.goto_active = false;
                        self.goto_input.set_focused(false);
                        return Cmd::None;
                    }
                    (KeyCode::Enter, _) => {
                        self.goto_line();
                        self.goto_active = false;
                        self.goto_input.set_focused(false);
                        return Cmd::None;
                    }
                    _ => {
                        self.goto_input.handle_event(event);
                        return Cmd::None;
                    }
                }
            }

            // Search mode
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
                            self.perform_search();
                        }
                        return Cmd::None;
                    }
                }
            }

            // Normal mode
            match (key.code, key.modifiers) {
                (KeyCode::Char('/'), Modifiers::NONE) => {
                    self.search_active = true;
                    self.search_input.set_focused(true);
                }
                (KeyCode::Char('g'), Modifiers::CTRL) => {
                    self.goto_active = true;
                    self.goto_input.set_focused(true);
                    self.goto_input.set_value("");
                }
                (KeyCode::Char('n'), Modifiers::NONE) => self.next_match(),
                (KeyCode::Char('N'), Modifiers::NONE) | (KeyCode::Char('n'), Modifiers::SHIFT) => {
                    self.prev_match();
                }
                (KeyCode::Char('j'), Modifiers::NONE) | (KeyCode::Down, _) => self.scroll_by(1),
                (KeyCode::Char('k'), Modifiers::NONE) | (KeyCode::Up, _) => self.scroll_by(-1),
                (KeyCode::Char('d'), Modifiers::CTRL) | (KeyCode::PageDown, _) => {
                    self.scroll_by(self.viewport_height as i32 / 2);
                }
                (KeyCode::Char('u'), Modifiers::CTRL) | (KeyCode::PageUp, _) => {
                    self.scroll_by(-(self.viewport_height as i32 / 2));
                }
                (KeyCode::Home, _) => self.scroll_to(0),
                (KeyCode::End, _) => self.scroll_to(self.total_lines()),
                _ => {}
            }
        }
        Cmd::None
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        if area.height < 6 || area.width < 40 {
            Paragraph::new("Terminal too small")
                .style(theme::muted())
                .render(area, frame);
            return;
        }

        // Vertical: search/goto bar (optional) + body + status
        let v_constraints = if self.search_active || self.goto_active {
            vec![
                Constraint::Fixed(1),
                Constraint::Min(4),
                Constraint::Fixed(1),
            ]
        } else {
            vec![Constraint::Min(4), Constraint::Fixed(1)]
        };
        let v_chunks = Flex::vertical().constraints(v_constraints).split(area);

        let (body_area, status_area) = if self.search_active || self.goto_active {
            self.render_input_bar(frame, v_chunks[0]);
            (v_chunks[1], v_chunks[2])
        } else {
            (v_chunks[0], v_chunks[1])
        };

        // Body: code (75%) + sidebar (25%)
        let h_chunks = Flex::horizontal()
            .constraints([Constraint::Percentage(75.0), Constraint::Percentage(25.0)])
            .split(body_area);

        self.render_code_panel(frame, h_chunks[0]);
        self.render_sidebar(frame, h_chunks[1]);
        self.render_status_bar(frame, status_area);
    }

    fn keybindings(&self) -> Vec<HelpEntry> {
        vec![
            HelpEntry {
                key: "/",
                action: "Search",
            },
            HelpEntry {
                key: "Ctrl+G",
                action: "Goto line",
            },
            HelpEntry {
                key: "n/N",
                action: "Next/prev match",
            },
            HelpEntry {
                key: "j/k",
                action: "Scroll",
            },
            HelpEntry {
                key: "PgUp/PgDn",
                action: "Page scroll",
            },
        ]
    }

    fn title(&self) -> &'static str {
        "Code Explorer"
    }

    fn tab_label(&self) -> &'static str {
        "Code"
    }
}

impl CodeExplorer {
    fn render_input_bar(&self, frame: &mut Frame, area: Rect) {
        let h = Flex::horizontal()
            .constraints([
                Constraint::Fixed(10),
                Constraint::Min(10),
                Constraint::Fixed(20),
            ])
            .split(area);

        if self.goto_active {
            Paragraph::new(" Goto Ln:")
                .style(Style::new().fg(theme::accent::INFO).attrs(StyleFlags::BOLD))
                .render(h[0], frame);
            self.goto_input.render(h[1], frame);
        } else {
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
    }

    fn render_code_panel(&self, frame: &mut Frame, area: Rect) {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .title("sqlite3.c")
            .title_alignment(Alignment::Center)
            .style(theme::content_border());
        let inner = block.inner(area);
        block.render(area, frame);

        if inner.height == 0 || inner.width < 10 {
            return;
        }

        let text_width = inner.width.saturating_sub(1);
        let text_area = Rect::new(inner.x, inner.y, text_width, inner.height);
        let scrollbar_area = Rect::new(inner.x + text_width, inner.y, 1, inner.height);
        let vh = inner.height as usize;

        let query = self.search_input.value();
        let has_query = query.len() >= 2;

        // Render visible lines with syntax highlighting
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

            // Line number
            let num_width = 7u16.min(text_area.width);
            let content_width = text_area.width.saturating_sub(num_width);
            let num_area = Rect::new(line_area.x, line_area.y, num_width, 1);
            let content_area = Rect::new(
                line_area.x.saturating_add(num_width),
                line_area.y,
                content_width,
                1,
            );

            let line_num = format!("{:>6} ", line_idx + 1);
            Paragraph::new(line_num)
                .style(Style::new().fg(theme::fg::MUTED))
                .render(num_area, frame);

            // Determine if this is a search match
            let is_current_match = has_query
                && !self.search_matches.is_empty()
                && self.search_matches.get(self.current_match) == Some(&line_idx);
            let is_any_match = has_query && self.search_matches.contains(&line_idx);

            if is_current_match {
                Paragraph::new(line)
                    .style(
                        Style::new()
                            .fg(theme::bg::DEEP)
                            .bg(theme::accent::WARNING)
                            .attrs(StyleFlags::BOLD),
                    )
                    .render(content_area, frame);
            } else if is_any_match {
                Paragraph::new(line)
                    .style(Style::new().fg(theme::fg::PRIMARY).bg(theme::bg::HIGHLIGHT))
                    .render(content_area, frame);
            } else {
                // Syntax-highlighted line
                let highlighted = self.highlighter.highlight(line, "c");
                Paragraph::new(highlighted).render(content_area, frame);
            }
        }

        // Scrollbar
        let mut scrollbar_state = ScrollbarState::new(self.total_lines(), self.scroll_offset, vh);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_style(Style::new().fg(theme::accent::PRIMARY))
            .track_style(Style::new().fg(theme::bg::SURFACE));
        StatefulWidget::render(&scrollbar, scrollbar_area, frame, &mut scrollbar_state);
    }

    fn render_sidebar(&self, frame: &mut Frame, area: Rect) {
        let rows = Flex::vertical()
            .constraints([Constraint::Percentage(40.0), Constraint::Percentage(60.0)])
            .split(area);

        // Top: JSON metadata
        let json_block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("File Info")
            .title_alignment(Alignment::Center)
            .style(theme::content_border());
        let json_inner = json_block.inner(rows[0]);
        json_block.render(rows[0], frame);

        let json_view = JsonView::new(&self.metadata_json)
            .with_indent(2)
            .with_key_style(Style::new().fg(theme::accent::PRIMARY))
            .with_string_style(Style::new().fg(theme::accent::SUCCESS))
            .with_number_style(Style::new().fg(theme::accent::WARNING))
            .with_punct_style(Style::new().fg(theme::fg::MUTED));
        json_view.render(json_inner, frame);

        // Bottom: context
        let ctx_block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Context")
            .title_alignment(Alignment::Center)
            .style(theme::content_border());
        let ctx_inner = ctx_block.inner(rows[1]);
        ctx_block.render(rows[1], frame);

        let context = self.current_context();
        let ctx_text = format!(
            "Line {}\n\nNearest function:\n{}",
            self.scroll_offset + 1,
            context,
        );
        Paragraph::new(ctx_text)
            .style(Style::new().fg(theme::fg::SECONDARY))
            .render(ctx_inner, frame);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let total = self.total_lines();
        let pos = self.scroll_offset + 1;
        let pct = (self.scroll_offset * 100).checked_div(total).unwrap_or(0);
        let size = filesize::decimal(SQLITE_SOURCE.len() as u64);

        let status = format!(" Line {pos}/{total} ({pct}%) | {size} | C");
        Paragraph::new(status)
            .style(Style::new().fg(theme::fg::MUTED).bg(theme::bg::SURFACE))
            .render(area, frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_explorer_initial_render() {
        let ce = CodeExplorer::new();
        assert_eq!(ce.scroll_offset, 0);
        assert!(
            ce.total_lines() > 100_000,
            "sqlite3.c should have 100K+ lines"
        );
        // First line should contain a comment
        assert!(
            ce.lines[0].contains("/*") || ce.lines[0].contains("/***"),
            "First line: {}",
            ce.lines[0]
        );
    }

    #[test]
    fn code_explorer_goto_line() {
        let mut ce = CodeExplorer::new();
        ce.viewport_height = 40;
        ce.goto_input.set_value("1000");
        ce.goto_line();
        assert_eq!(ce.scroll_offset, 999);
    }

    #[test]
    fn code_explorer_search() {
        let mut ce = CodeExplorer::new();
        ce.search_input.set_value("sqlite3_open");
        ce.perform_search();
        assert!(
            !ce.search_matches.is_empty(),
            "Should find 'sqlite3_open' in sqlite3.c"
        );
    }

    #[test]
    fn code_explorer_json_metadata() {
        let ce = CodeExplorer::new();
        assert!(ce.metadata_json.contains("\"filename\": \"sqlite3.c\""));
        assert!(ce.metadata_json.contains("\"language\": \"C\""));
        assert!(ce.metadata_json.contains("\"lines\":"));
    }

    #[test]
    fn code_explorer_line_numbers() {
        let ce = CodeExplorer::new();
        // Verify line count matches actual file
        let actual_lines = SQLITE_SOURCE.lines().count();
        assert_eq!(ce.total_lines(), actual_lines);
    }
}
