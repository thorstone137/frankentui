#![forbid(unsafe_code)]

//! Data Visualization screen — charts, sparklines, and canvas drawing.
//!
//! Demonstrates:
//! - `Sparkline` with live animated data
//! - `BarChart` with grouped/stacked modes
//! - `LineChart` with multi-series and Braille rendering
//! - `Canvas` with programmatic drawing (Lissajous curve)

use ftui_core::event::{Event, KeyCode, KeyEvent, KeyEventKind, Modifiers};
use ftui_core::geometry::Rect;
use ftui_extras::canvas::{Canvas, Mode, Painter};
use ftui_extras::charts::{BarChart, BarDirection, BarGroup, LineChart, Series, Sparkline};
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
use crate::data::ChartData;
use crate::theme;

/// Which chart panel is highlighted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChartPanel {
    Sparkline,
    BarChart,
    LineChart,
    Canvas,
}

impl ChartPanel {
    fn next(self) -> Self {
        match self {
            Self::Sparkline => Self::BarChart,
            Self::BarChart => Self::LineChart,
            Self::LineChart => Self::Canvas,
            Self::Canvas => Self::Sparkline,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Sparkline => Self::Canvas,
            Self::BarChart => Self::Sparkline,
            Self::LineChart => Self::BarChart,
            Self::Canvas => Self::LineChart,
        }
    }
}

pub struct DataViz {
    focus: ChartPanel,
    chart_data: ChartData,
    tick_count: u64,
    bar_horizontal: bool,
}

impl DataViz {
    pub fn new() -> Self {
        let mut chart_data = ChartData::default();
        for t in 0..30 {
            chart_data.tick(t);
        }
        Self {
            focus: ChartPanel::Sparkline,
            chart_data,
            tick_count: 30,
            bar_horizontal: false,
        }
    }

    fn render_sparkline_panel(&self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == ChartPanel::Sparkline;
        let border_style = if focused {
            Style::new().fg(theme::screen_accent::DATA_VIZ)
        } else {
            theme::content_border()
        };

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Sparklines")
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
            ])
            .split(inner);

        let sine_data: Vec<f64> = self.chart_data.sine_series.iter().copied().collect();
        let cos_data: Vec<f64> = self.chart_data.cosine_series.iter().copied().collect();
        let rand_data: Vec<f64> = self.chart_data.random_series.iter().copied().collect();

        // Labels and sparklines in alternating rows
        if !rows[0].is_empty() {
            Paragraph::new("Sine wave:")
                .style(Style::new().fg(theme::fg::SECONDARY))
                .render(rows[0], frame);
        }
        if !rows[1].is_empty() {
            Sparkline::new(&sine_data)
                .style(Style::new().fg(PackedRgba::rgb(100, 180, 255)))
                .gradient(
                    PackedRgba::rgb(50, 100, 180),
                    PackedRgba::rgb(130, 220, 255),
                )
                .render(rows[1], frame);
        }
        if !rows[2].is_empty() {
            Paragraph::new("Cosine wave:")
                .style(Style::new().fg(theme::fg::SECONDARY))
                .render(rows[2], frame);
        }
        if !rows[3].is_empty() {
            Sparkline::new(&cos_data)
                .style(Style::new().fg(PackedRgba::rgb(130, 220, 130)))
                .gradient(PackedRgba::rgb(50, 150, 50), PackedRgba::rgb(130, 255, 130))
                .render(rows[3], frame);
        }
        if !rows[4].is_empty() {
            Paragraph::new("Random noise:")
                .style(Style::new().fg(theme::fg::SECONDARY))
                .render(rows[4], frame);
        }
        if !rows[5].is_empty() {
            Sparkline::new(&rand_data)
                .style(Style::new().fg(PackedRgba::rgb(255, 180, 100)))
                .gradient(
                    PackedRgba::rgb(200, 100, 50),
                    PackedRgba::rgb(255, 220, 130),
                )
                .render(rows[5], frame);
        }
    }

    fn render_barchart_panel(&self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == ChartPanel::BarChart;
        let border_style = if focused {
            Style::new().fg(theme::screen_accent::DATA_VIZ)
        } else {
            theme::content_border()
        };

        let dir_label = if self.bar_horizontal {
            "Horizontal"
        } else {
            "Vertical"
        };
        let title = format!("Bar Chart ({dir_label})");
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

        let groups = vec![
            BarGroup::new("Q1", vec![42.0, 38.0, 55.0]),
            BarGroup::new("Q2", vec![58.0, 45.0, 62.0]),
            BarGroup::new("Q3", vec![35.0, 52.0, 48.0]),
            BarGroup::new("Q4", vec![70.0, 60.0, 75.0]),
        ];

        let direction = if self.bar_horizontal {
            BarDirection::Horizontal
        } else {
            BarDirection::Vertical
        };

        let chart = BarChart::new(groups)
            .direction(direction)
            .bar_width(2)
            .bar_gap(1)
            .group_gap(2)
            .colors(vec![
                PackedRgba::rgb(100, 180, 255),
                PackedRgba::rgb(255, 130, 100),
                PackedRgba::rgb(100, 220, 140),
            ])
            .style(Style::new().fg(theme::fg::PRIMARY));

        chart.render(inner, frame);
    }

    fn render_linechart_panel(&self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == ChartPanel::LineChart;
        let border_style = if focused {
            Style::new().fg(theme::screen_accent::DATA_VIZ)
        } else {
            theme::content_border()
        };

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Line Chart")
            .title_alignment(Alignment::Center)
            .style(border_style);

        let inner = block.inner(area);
        block.render(area, frame);

        if inner.is_empty() || inner.width < 4 || inner.height < 4 {
            return;
        }

        let sine_points: Vec<(f64, f64)> = self
            .chart_data
            .sine_series
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as f64, v))
            .collect();
        let cos_points: Vec<(f64, f64)> = self
            .chart_data
            .cosine_series
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as f64, v))
            .collect();

        let series = vec![
            Series::new("sin(t)", &sine_points, PackedRgba::rgb(100, 180, 255)),
            Series::new("cos(t)", &cos_points, PackedRgba::rgb(255, 130, 180)),
        ];

        let n = self.chart_data.sine_series.len() as f64;
        let chart = LineChart::new(series)
            .x_bounds(0.0, n.max(1.0))
            .y_bounds(-1.1, 1.1)
            .legend(true)
            .style(Style::new().fg(theme::fg::MUTED));

        chart.render(inner, frame);
    }

    fn render_canvas_panel(&self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == ChartPanel::Canvas;
        let border_style = if focused {
            Style::new().fg(theme::screen_accent::DATA_VIZ)
        } else {
            theme::content_border()
        };

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Canvas (Braille)")
            .title_alignment(Alignment::Center)
            .style(border_style);

        let inner = block.inner(area);
        block.render(area, frame);

        if inner.is_empty() || inner.width < 2 || inner.height < 2 {
            return;
        }

        let mut painter = Painter::for_area(inner, Mode::Braille);
        let (pw, ph) = painter.size();

        // Draw a Lissajous curve that evolves with tick count
        let a_freq = 3.0_f64;
        let b_freq = 2.0_f64;
        let phase = self.tick_count as f64 * 0.05;
        let cx = pw as f64 / 2.0;
        let cy = ph as f64 / 2.0;
        let rx = (pw as f64 / 2.0) - 2.0;
        let ry = (ph as f64 / 2.0) - 2.0;

        let steps = 500;
        for i in 0..steps {
            let t = (i as f64 / steps as f64) * std::f64::consts::TAU;
            let x = cx + rx * (a_freq * t + phase).sin();
            let y = cy + ry * (b_freq * t).sin();

            // Color gradient along the curve
            let hue = (i as f64 / steps as f64 * 360.0) as u32;
            let color = hue_to_rgb(hue);
            painter.point_colored(x as i32, y as i32, color);
        }

        // Draw border box
        painter.rect(0, 0, pw as i32 - 1, ph as i32 - 1);

        Canvas::from_painter(&painter)
            .style(Style::new().fg(theme::fg::PRIMARY))
            .render(inner, frame);
    }
}

/// Convert HSV hue (0-360) to RGB.
fn hue_to_rgb(hue: u32) -> PackedRgba {
    let h = (hue % 360) as f64;
    let s = 0.8_f64;
    let v = 1.0_f64;

    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = match (h as u32) / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    PackedRgba::rgb(
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

impl Screen for DataViz {
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

        // Toggle bar direction with 'd'
        if let Event::Key(KeyEvent {
            code: KeyCode::Char('d'),
            kind: KeyEventKind::Press,
            ..
        }) = event
        {
            self.bar_horizontal = !self.bar_horizontal;
        }

        Cmd::None
    }

    fn tick(&mut self, tick_count: u64) {
        self.tick_count = tick_count;
        self.chart_data.tick(tick_count);
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        if area.is_empty() {
            return;
        }

        let main = Flex::vertical()
            .constraints([Constraint::Min(1), Constraint::Fixed(1)])
            .split(area);

        // 2x2 grid of chart panels
        let rows = Flex::vertical()
            .constraints([Constraint::Percentage(50.0), Constraint::Percentage(50.0)])
            .split(main[0]);

        let top_cols = Flex::horizontal()
            .constraints([Constraint::Percentage(50.0), Constraint::Percentage(50.0)])
            .split(rows[0]);

        let bot_cols = Flex::horizontal()
            .constraints([Constraint::Percentage(50.0), Constraint::Percentage(50.0)])
            .split(rows[1]);

        self.render_sparkline_panel(frame, top_cols[0]);
        self.render_barchart_panel(frame, top_cols[1]);
        self.render_linechart_panel(frame, bot_cols[0]);
        self.render_canvas_panel(frame, bot_cols[1]);

        // Status bar
        let status = format!(
            "Tick: {} | Ctrl+\u{2190}/\u{2192}: panels | d: toggle bar direction",
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
                key: "d",
                action: "Toggle bar direction",
            },
        ]
    }

    fn title(&self) -> &'static str {
        "Data Visualization"
    }

    fn tab_label(&self) -> &'static str {
        "Data Viz"
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
        let screen = DataViz::new();
        assert_eq!(screen.focus, ChartPanel::Sparkline);
        assert_eq!(screen.title(), "Data Visualization");
        assert_eq!(screen.tab_label(), "Data Viz");
    }

    #[test]
    fn panel_navigation() {
        let mut screen = DataViz::new();
        screen.update(&ctrl_press(KeyCode::Right));
        assert_eq!(screen.focus, ChartPanel::BarChart);
        screen.update(&ctrl_press(KeyCode::Right));
        assert_eq!(screen.focus, ChartPanel::LineChart);
        screen.update(&ctrl_press(KeyCode::Left));
        assert_eq!(screen.focus, ChartPanel::BarChart);
    }

    #[test]
    fn tick_updates_data() {
        let mut screen = DataViz::new();
        let initial_len = screen.chart_data.sine_series.len();
        screen.tick(31);
        assert_eq!(screen.chart_data.sine_series.len(), initial_len + 1);
    }

    #[test]
    fn toggle_bar_direction() {
        let mut screen = DataViz::new();
        assert!(!screen.bar_horizontal);
        screen.update(&press(KeyCode::Char('d')));
        assert!(screen.bar_horizontal);
        screen.update(&press(KeyCode::Char('d')));
        assert!(!screen.bar_horizontal);
    }

    #[test]
    fn hue_conversion() {
        let red = hue_to_rgb(0);
        assert_ne!(red, PackedRgba::TRANSPARENT);
        let green = hue_to_rgb(120);
        assert_ne!(green, PackedRgba::TRANSPARENT);
    }
}
