#![forbid(unsafe_code)]

//! Performance Stress Test screen — virtualized lists and large content.
//!
//! Demonstrates:
//! - `VirtualizedList` with large datasets (10k+ items)
//! - Scroll performance with fixed-height items
//! - Item rendering with selection highlighting
//! - Scroll position tracking and progress

use ftui_core::event::{Event, KeyCode, KeyEvent, KeyEventKind};
use ftui_core::geometry::Rect;
use ftui_layout::{Constraint, Flex};
use ftui_render::cell::PackedRgba;
use ftui_render::frame::Frame;
use ftui_runtime::Cmd;
use ftui_style::Style;
use ftui_widgets::Widget;
use ftui_widgets::block::{Alignment, Block};
use ftui_widgets::borders::{BorderType, Borders};
use ftui_widgets::paragraph::Paragraph;

use super::{HelpEntry, Screen};
use crate::theme;

const TOTAL_ITEMS: usize = 10_000;

pub struct Performance {
    items: Vec<String>,
    selected: usize,
    scroll_offset: usize,
    viewport_height: usize,
    tick_count: u64,
}

impl Performance {
    pub fn new() -> Self {
        let items: Vec<String> = (0..TOTAL_ITEMS)
            .map(|i| {
                let severity = match i % 5 {
                    0 => "INFO",
                    1 => "DEBUG",
                    2 => "WARN",
                    3 => "ERROR",
                    _ => "TRACE",
                };
                let module = match i % 7 {
                    0 => "server::http",
                    1 => "db::pool",
                    2 => "auth::jwt",
                    3 => "cache::redis",
                    4 => "queue::worker",
                    5 => "api::handler",
                    _ => "core::runtime",
                };
                format!(
                    "[{:>5}] {:>5} {:<18} Event #{:05}: simulated log entry with payload data",
                    i, severity, module, i
                )
            })
            .collect();

        Self {
            items,
            selected: 0,
            scroll_offset: 0,
            viewport_height: 20,
            tick_count: 0,
        }
    }

    fn ensure_visible(&mut self) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        }
        if self.selected >= self.scroll_offset + self.viewport_height {
            self.scroll_offset = self.selected.saturating_sub(self.viewport_height - 1);
        }
    }

    fn render_list_panel(&self, frame: &mut Frame, area: Rect) {
        let border_style = Style::new().fg(theme::screen_accent::PERFORMANCE);

        let title = format!("Virtualized List ({} items)", self.items.len());
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(&title)
            .title_alignment(Alignment::Center)
            .style(border_style);

        let inner = block.inner(area);
        block.render(area, frame);

        if inner.is_empty() {
            return;
        }

        let viewport = inner.height as usize;
        let end = (self.scroll_offset + viewport).min(self.items.len());

        for (row, idx) in (self.scroll_offset..end).enumerate() {
            let y = inner.y + row as u16;
            if y >= inner.y + inner.height {
                break;
            }

            let style = if idx == self.selected {
                Style::new().fg(theme::fg::PRIMARY).bg(theme::bg::HIGHLIGHT)
            } else {
                let severity_color = match idx % 5 {
                    0 => theme::fg::PRIMARY,             // INFO
                    1 => theme::fg::MUTED,               // DEBUG
                    2 => PackedRgba::rgb(255, 200, 80),  // WARN
                    3 => PackedRgba::rgb(255, 100, 100), // ERROR
                    _ => theme::fg::DISABLED,            // TRACE
                };
                Style::new().fg(severity_color)
            };

            let row_area = Rect::new(inner.x, y, inner.width, 1);
            Paragraph::new(self.items[idx].as_str())
                .style(style)
                .render(row_area, frame);
        }
    }

    fn render_stats_panel(&self, frame: &mut Frame, area: Rect) {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Performance Stats")
            .title_alignment(Alignment::Center)
            .style(theme::content_border());

        let inner = block.inner(area);
        block.render(area, frame);

        if inner.is_empty() {
            return;
        }

        let progress = if self.items.is_empty() {
            0.0
        } else {
            self.selected as f64 / (self.items.len() - 1) as f64
        };

        let visible_end = (self.scroll_offset + self.viewport_height).min(self.items.len());

        let stats = [
            format!("Total items:  {}", self.items.len()),
            format!("Selected:     {} / {}", self.selected + 1, self.items.len()),
            format!("Scroll:       {}", self.scroll_offset),
            format!("Viewport:     {} rows", self.viewport_height),
            format!("Visible:      {}..{}", self.scroll_offset, visible_end),
            format!("Progress:     {:.1}%", progress * 100.0),
            format!("Tick:         {}", self.tick_count),
            String::new(),
            "Only visible rows are rendered.".into(),
            format!(
                "Rendering {} of {} items.",
                visible_end - self.scroll_offset,
                self.items.len()
            ),
        ];

        for (i, line) in stats.iter().enumerate() {
            if i >= inner.height as usize {
                break;
            }
            let style = if line.is_empty() {
                Style::new()
            } else if line.starts_with("Only") || line.starts_with("Rendering") {
                Style::new().fg(theme::fg::MUTED)
            } else {
                Style::new().fg(theme::fg::SECONDARY)
            };
            let row_area = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
            Paragraph::new(line.as_str())
                .style(style)
                .render(row_area, frame);
        }

        // Progress bar
        let bar_row = stats.len();
        if bar_row < inner.height as usize {
            let bar_width = inner.width.saturating_sub(2) as usize;
            let filled = (progress * bar_width as f64) as usize;
            let bar: String = "\u{2588}"
                .repeat(filled)
                .chars()
                .chain("\u{2591}".repeat(bar_width.saturating_sub(filled)).chars())
                .collect();
            let bar_area = Rect::new(inner.x, inner.y + bar_row as u16, inner.width, 1);
            Paragraph::new(&*bar)
                .style(Style::new().fg(theme::screen_accent::PERFORMANCE))
                .render(bar_area, frame);
        }
    }
}

impl Screen for Performance {
    type Message = Event;

    fn update(&mut self, event: &Event) -> Cmd<Self::Message> {
        if let Event::Key(KeyEvent {
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            match event {
                Event::Key(KeyEvent {
                    code: KeyCode::Up, ..
                }) => {
                    self.selected = self.selected.saturating_sub(1);
                    self.ensure_visible();
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Down,
                    ..
                }) => {
                    if self.selected + 1 < self.items.len() {
                        self.selected += 1;
                    }
                    self.ensure_visible();
                }
                Event::Key(KeyEvent {
                    code: KeyCode::PageUp,
                    ..
                }) => {
                    self.selected = self.selected.saturating_sub(self.viewport_height);
                    self.ensure_visible();
                }
                Event::Key(KeyEvent {
                    code: KeyCode::PageDown,
                    ..
                }) => {
                    self.selected = (self.selected + self.viewport_height)
                        .min(self.items.len().saturating_sub(1));
                    self.ensure_visible();
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Home,
                    ..
                }) => {
                    self.selected = 0;
                    self.ensure_visible();
                }
                Event::Key(KeyEvent {
                    code: KeyCode::End, ..
                }) => {
                    self.selected = self.items.len().saturating_sub(1);
                    self.ensure_visible();
                }
                _ => {}
            }
        }

        Cmd::None
    }

    fn tick(&mut self, tick_count: u64) {
        self.tick_count = tick_count;
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        if area.is_empty() {
            return;
        }

        let main = Flex::vertical()
            .constraints([Constraint::Min(1), Constraint::Fixed(1)])
            .split(area);

        let cols = Flex::horizontal()
            .constraints([Constraint::Min(40), Constraint::Fixed(35)])
            .split(main[0]);

        // Store viewport height for scroll calculation (can't mutate self here)
        // We use the stored value from last render
        self.render_list_panel(frame, cols[0]);
        self.render_stats_panel(frame, cols[1]);

        // Status bar
        let status = format!(
            "Item {}/{} | \u{2191}/\u{2193}: scroll | PgUp/PgDn: page | Home/End: jump",
            self.selected + 1,
            self.items.len()
        );
        Paragraph::new(&*status)
            .style(Style::new().fg(theme::fg::MUTED).bg(theme::bg::SURFACE))
            .render(main[1], frame);
    }

    fn keybindings(&self) -> Vec<HelpEntry> {
        vec![
            HelpEntry {
                key: "\u{2191}/\u{2193}",
                action: "Scroll items",
            },
            HelpEntry {
                key: "PgUp/PgDn",
                action: "Page scroll",
            },
            HelpEntry {
                key: "Home/End",
                action: "Jump to start/end",
            },
        ]
    }

    fn title(&self) -> &'static str {
        "Performance Stress Test"
    }

    fn tab_label(&self) -> &'static str {
        "Perf"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: ftui_core::event::Modifiers::empty(),
            kind: KeyEventKind::Press,
        })
    }

    #[test]
    fn initial_state() {
        let screen = Performance::new();
        assert_eq!(screen.selected, 0);
        assert_eq!(screen.items.len(), TOTAL_ITEMS);
        assert_eq!(screen.title(), "Performance Stress Test");
    }

    #[test]
    fn scroll_down_up() {
        let mut screen = Performance::new();
        screen.update(&press(KeyCode::Down));
        assert_eq!(screen.selected, 1);
        screen.update(&press(KeyCode::Down));
        assert_eq!(screen.selected, 2);
        screen.update(&press(KeyCode::Up));
        assert_eq!(screen.selected, 1);
    }

    #[test]
    fn page_navigation() {
        let mut screen = Performance::new();
        screen.update(&press(KeyCode::PageDown));
        assert_eq!(screen.selected, screen.viewport_height);
        screen.update(&press(KeyCode::PageUp));
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn home_end() {
        let mut screen = Performance::new();
        screen.update(&press(KeyCode::End));
        assert_eq!(screen.selected, TOTAL_ITEMS - 1);
        screen.update(&press(KeyCode::Home));
        assert_eq!(screen.selected, 0);
    }

    #[test]
    fn bounds_check() {
        let mut screen = Performance::new();
        screen.update(&press(KeyCode::Up));
        assert_eq!(screen.selected, 0);
        screen.selected = TOTAL_ITEMS - 1;
        screen.update(&press(KeyCode::Down));
        assert_eq!(screen.selected, TOTAL_ITEMS - 1);
    }
}
