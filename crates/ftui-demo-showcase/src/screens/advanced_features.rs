#![forbid(unsafe_code)]

//! Advanced Features screen — diagnostics, timers, and error display.
//!
//! Demonstrates:
//! - `Traceback` with styled exception frames
//! - `Timer` with countdown display
//! - `Spinner` with multiple frame sets
//! - System information panel

use std::time::Duration;

use ftui_core::event::{Event, KeyCode, KeyEvent, KeyEventKind, Modifiers};
use ftui_core::geometry::Rect;
use ftui_extras::timer::{DisplayFormat, Timer};
use ftui_extras::traceback::{Traceback, TracebackFrame};
use ftui_layout::{Constraint, Flex};
use ftui_render::cell::PackedRgba;
use ftui_render::frame::Frame;
use ftui_runtime::Cmd;
use ftui_style::Style;
use ftui_widgets::block::{Alignment, Block};
use ftui_widgets::borders::{BorderType, Borders};
use ftui_widgets::paragraph::Paragraph;
use ftui_widgets::spinner::{self, Spinner, SpinnerState};
use ftui_widgets::{StatefulWidget, Widget};

use super::{HelpEntry, Screen};
use crate::theme;

/// Which panel has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Traceback,
    Timers,
    Info,
}

impl Panel {
    fn next(self) -> Self {
        match self {
            Self::Traceback => Self::Timers,
            Self::Timers => Self::Info,
            Self::Info => Self::Traceback,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Traceback => Self::Info,
            Self::Timers => Self::Traceback,
            Self::Info => Self::Timers,
        }
    }
}

pub struct AdvancedFeatures {
    focus: Panel,
    timer_compact: Timer,
    timer_clock: Timer,
    spinner_tick: usize,
    tick_count: u64,
}

impl AdvancedFeatures {
    pub fn new() -> Self {
        let mut timer_compact = Timer::new(Duration::from_secs(300)).format(DisplayFormat::Compact);
        timer_compact.start();

        let mut timer_clock =
            Timer::with_interval(Duration::from_secs(120), Duration::from_millis(100))
                .format(DisplayFormat::Clock);
        timer_clock.start();

        Self {
            focus: Panel::Traceback,
            timer_compact,
            timer_clock,
            spinner_tick: 0,
            tick_count: 0,
        }
    }

    fn render_traceback_panel(&self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Panel::Traceback;
        let border_style = if focused {
            Style::new().fg(theme::screen_accent::ADVANCED)
        } else {
            theme::content_border()
        };

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Error Traceback")
            .title_alignment(Alignment::Center)
            .style(border_style);

        let inner = block.inner(area);
        block.render(area, frame);

        if inner.is_empty() {
            return;
        }

        let traceback = build_sample_traceback();
        traceback.render(inner, frame);
    }

    fn render_timers_panel(&self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Panel::Timers;
        let border_style = if focused {
            Style::new().fg(theme::screen_accent::ADVANCED)
        } else {
            theme::content_border()
        };

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Timers & Spinners")
            .title_alignment(Alignment::Center)
            .style(border_style);

        let inner = block.inner(area);
        block.render(area, frame);

        if inner.is_empty() {
            return;
        }

        let rows = Flex::vertical()
            .constraints([
                Constraint::Fixed(1),
                Constraint::Fixed(1),
                Constraint::Fixed(1),
                Constraint::Fixed(1),
                Constraint::Fixed(1),
                Constraint::Fixed(1),
                Constraint::Fixed(1),
                Constraint::Fixed(1),
                Constraint::Min(1),
            ])
            .split(inner);

        // Timer compact
        let compact_text = format!(
            "Compact timer: {} ({}%)",
            self.timer_compact.view(),
            (self.timer_compact.progress() * 100.0) as u32
        );
        Paragraph::new(&*compact_text)
            .style(Style::new().fg(theme::fg::PRIMARY))
            .render(rows[0], frame);

        // Timer clock
        let clock_text = format!(
            "Clock timer:   {} ({}%)",
            self.timer_clock.view(),
            (self.timer_clock.progress() * 100.0) as u32
        );
        Paragraph::new(&*clock_text)
            .style(Style::new().fg(theme::fg::PRIMARY))
            .render(rows[1], frame);

        // Blank separator
        if !rows[3].is_empty() {
            Paragraph::new("Spinners:")
                .style(Style::new().fg(theme::fg::SECONDARY))
                .render(rows[3], frame);
        }

        // Spinner demos
        let spinner_sets = [
            ("dots", Spinner::new()),
            ("line", Spinner::new().frames(spinner::LINE)),
            ("braille", Spinner::new().frames(spinner::DOTS)),
        ];

        for (i, (name, spinner)) in spinner_sets.iter().enumerate() {
            let row_idx = 4 + i;
            if row_idx < rows.len() && !rows[row_idx].is_empty() {
                let cols = Flex::horizontal()
                    .constraints([
                        Constraint::Fixed(12),
                        Constraint::Fixed(3),
                        Constraint::Min(1),
                    ])
                    .split(rows[row_idx]);

                Paragraph::new(*name)
                    .style(Style::new().fg(theme::fg::MUTED))
                    .render(cols[0], frame);

                let styled_spinner = spinner
                    .clone()
                    .style(Style::new().fg(theme::screen_accent::ADVANCED));
                let mut state = SpinnerState {
                    current_frame: self.spinner_tick,
                };
                StatefulWidget::render(&styled_spinner, cols[1], frame, &mut state);
            }
        }

        // Progress indicator
        if !rows[7].is_empty() {
            let progress = self.timer_compact.progress();
            let bar_width = rows[7].width.saturating_sub(12) as usize;
            let filled = (progress * bar_width as f64) as usize;
            let bar: String = "\u{2588}"
                .repeat(filled)
                .chars()
                .chain("\u{2591}".repeat(bar_width.saturating_sub(filled)).chars())
                .collect();
            let label = format!("Progress: {bar}");
            Paragraph::new(&*label)
                .style(Style::new().fg(PackedRgba::rgb(100, 220, 140)))
                .render(rows[7], frame);
        }
    }

    fn render_info_panel(&self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Panel::Info;
        let border_style = if focused {
            Style::new().fg(theme::screen_accent::ADVANCED)
        } else {
            theme::content_border()
        };

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("System Info")
            .title_alignment(Alignment::Center)
            .style(border_style);

        let inner = block.inner(area);
        block.render(area, frame);

        if inner.is_empty() {
            return;
        }

        let info_lines = [
            "FrankenTUI Demo Showcase",
            "",
            "Framework: ftui (Rust)",
            "Rendering: 16-byte Cell model",
            "Color: 24-bit true color (RGB)",
            "Input: Kitty keyboard protocol",
            "",
            "Features demonstrated:",
            "  - Traceback error display",
            "  - Countdown timers (compact/clock)",
            "  - Terminal spinners",
            "  - Progress bars",
            "",
            "Controls:",
            "  r - Reset timers",
            "  Space - Pause/resume timers",
        ];

        for (i, &line) in info_lines.iter().enumerate() {
            if i >= inner.height as usize {
                break;
            }
            let style = if i == 0 {
                Style::new().fg(theme::screen_accent::ADVANCED)
            } else if line.starts_with("  -") || line.starts_with("  r") || line.starts_with("  S")
            {
                Style::new().fg(theme::fg::SECONDARY)
            } else {
                Style::new().fg(theme::fg::PRIMARY)
            };
            let row_area = Rect::new(inner.x, inner.y.saturating_add(i as u16), inner.width, 1);
            Paragraph::new(line).style(style).render(row_area, frame);
        }
    }
}

fn build_sample_traceback() -> Traceback {
    Traceback::new(
        vec![
            TracebackFrame::new("main", 42)
                .filename("src/main.rs")
                .source_context(
                    "fn main() -> Result<()> {\n    let config = Config::load()?;\n    app::run(config)?;\n    Ok(())\n}",
                    40,
                ),
            TracebackFrame::new("run", 128)
                .filename("src/app.rs")
                .source_context(
                    "pub fn run(config: Config) -> Result<()> {\n    let db = Database::connect(&config.db_url)?;\n    let server = Server::new(db);\n    server.listen(config.port)?;\n    Ok(())\n}",
                    125,
                ),
            TracebackFrame::new("connect", 56)
                .filename("src/database.rs")
                .source_context(
                    "pub fn connect(url: &str) -> Result<Self> {\n    let pool = Pool::builder()\n        .max_size(10)\n        .build(url)?;\n    Ok(Self { pool })\n}",
                    53,
                ),
        ],
        "ConnectionError",
        "failed to connect to database: connection refused (localhost:5432)",
    )
    .title("Application Error")
}

impl Screen for AdvancedFeatures {
    type Message = Event;

    fn update(&mut self, event: &Event) -> Cmd<Self::Message> {
        if let Event::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
            && modifiers.contains(Modifiers::CTRL)
        {
            self.focus = self.focus.next();
            return Cmd::None;
        }
        if let Event::Key(KeyEvent {
            code: KeyCode::Left,
            modifiers,
            kind: KeyEventKind::Press,
            ..
        }) = event
            && modifiers.contains(Modifiers::CTRL)
        {
            self.focus = self.focus.prev();
            return Cmd::None;
        }

        // Reset timers
        if let Event::Key(KeyEvent {
            code: KeyCode::Char('r'),
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            self.timer_compact.reset();
            self.timer_clock.reset();
        }

        // Pause/resume
        if let Event::Key(KeyEvent {
            code: KeyCode::Char(' '),
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            self.timer_compact.toggle();
            self.timer_clock.toggle();
        }

        Cmd::None
    }

    fn tick(&mut self, tick_count: u64) {
        self.tick_count = tick_count;
        self.spinner_tick = tick_count as usize;
        self.timer_compact.tick(Duration::from_millis(100));
        self.timer_clock.tick(Duration::from_millis(100));
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        if area.is_empty() {
            return;
        }

        let main = Flex::vertical()
            .constraints([Constraint::Min(1), Constraint::Fixed(1)])
            .split(area);

        let cols = Flex::horizontal()
            .constraints([Constraint::Percentage(50.0), Constraint::Percentage(50.0)])
            .split(main[0]);

        // Left: traceback
        self.render_traceback_panel(frame, cols[0]);

        // Right: split into timers and info
        let right_rows = Flex::vertical()
            .constraints([Constraint::Percentage(50.0), Constraint::Percentage(50.0)])
            .split(cols[1]);

        self.render_timers_panel(frame, right_rows[0]);
        self.render_info_panel(frame, right_rows[1]);

        // Status bar
        let status = format!(
            "Tick: {} | Ctrl+\u{2190}/\u{2192}: panels | r: reset | Space: pause/resume",
            self.tick_count
        );
        Paragraph::new(&*status)
            .style(Style::new().fg(theme::fg::MUTED).bg(theme::bg::SURFACE))
            .render(main[1], frame);
    }

    fn keybindings(&self) -> Vec<HelpEntry> {
        vec![
            HelpEntry {
                key: "Ctrl+\u{2190}/\u{2192}",
                action: "Switch panel",
            },
            HelpEntry {
                key: "r",
                action: "Reset timers",
            },
            HelpEntry {
                key: "Space",
                action: "Pause/resume",
            },
        ]
    }

    fn title(&self) -> &'static str {
        "Advanced Features"
    }

    fn tab_label(&self) -> &'static str {
        "Advanced"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        })
    }

    fn ctrl_press(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: Modifiers::CTRL,
            kind: KeyEventKind::Press,
        })
    }

    #[test]
    fn initial_state() {
        let screen = AdvancedFeatures::new();
        assert_eq!(screen.focus, Panel::Traceback);
        assert_eq!(screen.title(), "Advanced Features");
        assert!(screen.timer_compact.running());
    }

    #[test]
    fn panel_navigation() {
        let mut screen = AdvancedFeatures::new();
        screen.update(&ctrl_press(KeyCode::Right));
        assert_eq!(screen.focus, Panel::Timers);
        screen.update(&ctrl_press(KeyCode::Right));
        assert_eq!(screen.focus, Panel::Info);
        screen.update(&ctrl_press(KeyCode::Left));
        assert_eq!(screen.focus, Panel::Timers);
    }

    #[test]
    fn timer_toggle() {
        let mut screen = AdvancedFeatures::new();
        assert!(screen.timer_compact.running());
        screen.update(&press(KeyCode::Char(' ')));
        assert!(!screen.timer_compact.running());
        screen.update(&press(KeyCode::Char(' ')));
        assert!(screen.timer_compact.running());
    }

    #[test]
    fn timer_reset() {
        let mut screen = AdvancedFeatures::new();
        screen.tick(10);
        let initial = screen.timer_compact.initial();
        screen.update(&press(KeyCode::Char('r')));
        assert_eq!(screen.timer_compact.remaining(), initial);
    }

    #[test]
    fn traceback_has_frames() {
        let tb = build_sample_traceback();
        assert_eq!(tb.frames().len(), 3);
        assert_eq!(tb.exception_type(), "ConnectionError");
    }
}
