#![forbid(unsafe_code)]

//! Performance HUD + Render Budget Visualizer screen.
//!
//! Demonstrates real-time performance monitoring with:
//! - Tick interval measurement (ring buffer, 120 samples)
//! - Estimated FPS via views-per-tick × TPS
//! - Latency percentiles (p50 / p95 / p99)
//! - Braille-encoded sparkline of tick intervals
//! - Render budget tracking with degradation tier indicators
//! - Resettable counters and pauseable collection

use std::cell::Cell;
use std::collections::VecDeque;
use std::env;
use std::time::Instant;

use ftui_core::event::{Event, KeyCode, KeyEvent, KeyEventKind, Modifiers};
use ftui_core::geometry::Rect;
use ftui_layout::{Constraint, Flex};
use ftui_render::frame::Frame;
use ftui_runtime::Cmd;
use ftui_style::Style;
use ftui_widgets::Widget;
use ftui_widgets::block::{Alignment, Block};
use ftui_widgets::borders::{BorderType, Borders};
use ftui_widgets::paragraph::Paragraph;

use super::{HelpEntry, Screen};
use crate::theme;

/// Maximum number of tick-interval samples in the ring buffer.
const MAX_SAMPLES: usize = 120;

/// Braille characters for sparkline rows (each encodes 4 vertical dots).
/// Index maps to dot pattern for 0..=8 inclusive height levels.
const BRAILLE_BLOCKS: [char; 9] = [
    ' ', '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
    '\u{2588}',
];

/// Degradation tier based on estimated FPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DegradationTier {
    /// ≥50 FPS — full fidelity.
    Full,
    /// 20–49 FPS — reduce animations.
    Reduced,
    /// 5–19 FPS — minimal rendering.
    Minimal,
    /// <5 FPS — safety mode (text only).
    Safety,
}

impl DegradationTier {
    fn from_fps(fps: f64) -> Self {
        if fps >= 50.0 {
            Self::Full
        } else if fps >= 20.0 {
            Self::Reduced
        } else if fps >= 5.0 {
            Self::Minimal
        } else {
            Self::Safety
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Full => "Full Fidelity",
            Self::Reduced => "Reduced (no FX)",
            Self::Minimal => "Minimal",
            Self::Safety => "SAFETY MODE",
        }
    }

    fn bar(self) -> &'static str {
        match self {
            Self::Full => "\u{2588}\u{2588}\u{2588}\u{2588}",
            Self::Reduced => "\u{2588}\u{2588}\u{2588}\u{2591}",
            Self::Minimal => "\u{2588}\u{2588}\u{2591}\u{2591}",
            Self::Safety => "\u{2588}\u{2591}\u{2591}\u{2591}",
        }
    }
}

/// Display mode for the sparkline panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SparklineMode {
    /// Show raw tick intervals in microseconds.
    Intervals,
    /// Show estimated FPS over time.
    Fps,
}

pub struct PerformanceHud {
    /// Ring buffer of tick intervals in microseconds.
    tick_times_us: VecDeque<u64>,
    /// When we last received a tick.
    last_tick: Option<Instant>,
    /// Number of view() calls (interior mutability for &self).
    view_counter: Cell<u64>,
    /// Previous view count snapshot (for views-per-tick).
    prev_view_count: u64,
    /// EMA-smoothed views per tick.
    views_per_tick: f64,
    /// Global tick counter.
    tick_count: u64,
    /// Whether metric collection is paused.
    paused: bool,
    /// Sparkline display mode.
    sparkline_mode: SparklineMode,
    /// Render budget target in milliseconds.
    budget_ms: f64,
    /// Fixed tick interval for deterministic fixtures (microseconds).
    deterministic_tick_us: Option<u64>,
    /// Optional override for views-per-tick in deterministic fixtures.
    forced_views_per_tick: Option<f64>,
}

impl Default for PerformanceHud {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceHud {
    pub fn new() -> Self {
        let forced_views_per_tick = env::var("FTUI_DEMO_PERF_HUD_VIEWS_PER_TICK")
            .ok()
            .and_then(|value| value.parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0);
        Self {
            tick_times_us: VecDeque::with_capacity(MAX_SAMPLES),
            last_tick: None,
            view_counter: Cell::new(0),
            prev_view_count: 0,
            views_per_tick: 0.0,
            tick_count: 0,
            paused: false,
            sparkline_mode: SparklineMode::Intervals,
            budget_ms: 16.67, // ~60 FPS target
            deterministic_tick_us: None,
            forced_views_per_tick,
        }
    }

    pub fn enable_deterministic_mode(&mut self, tick_ms: u64) {
        let tick_us = tick_ms.max(1) * 1000;
        if self.deterministic_tick_us == Some(tick_us) {
            return;
        }
        self.deterministic_tick_us = Some(tick_us);
        self.tick_times_us.clear();
        self.last_tick = None;
    }

    fn reset(&mut self) {
        self.tick_times_us.clear();
        self.last_tick = None;
        self.view_counter.set(0);
        self.prev_view_count = 0;
        self.views_per_tick = self.forced_views_per_tick.unwrap_or(0.0);
    }

    fn record_tick(&mut self) {
        if self.paused {
            if self.deterministic_tick_us.is_none() {
                self.last_tick = Some(Instant::now());
            }
            return;
        }
        if let Some(dt_us) = self.deterministic_tick_us {
            if self.tick_times_us.len() >= MAX_SAMPLES {
                self.tick_times_us.pop_front();
            }
            self.tick_times_us.push_back(dt_us);
            self.last_tick = None;
        } else {
            let now = Instant::now();
            if let Some(last) = self.last_tick {
                let dt_us = now.duration_since(last).as_micros() as u64;
                if self.tick_times_us.len() >= MAX_SAMPLES {
                    self.tick_times_us.pop_front();
                }
                self.tick_times_us.push_back(dt_us);
            }
            self.last_tick = Some(now);
        }

        // EMA for views per tick
        let current = self.view_counter.get();
        let delta = current.saturating_sub(self.prev_view_count);
        self.prev_view_count = current;
        self.views_per_tick = 0.7 * self.views_per_tick + 0.3 * delta as f64;

        if self.deterministic_tick_us.is_some()
            && let Some(forced) = self.forced_views_per_tick
        {
            self.views_per_tick = forced;
        }
    }

    /// Compute (tps, avg_ms, p50_ms, p95_ms, p99_ms, min_ms, max_ms).
    fn stats(&self) -> (f64, f64, f64, f64, f64, f64, f64) {
        let n = self.tick_times_us.len();
        if n == 0 {
            return (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        }
        let sum: u64 = self.tick_times_us.iter().sum();
        let avg_us = sum as f64 / n as f64;
        let tps = 1_000_000.0 / avg_us;
        let avg_ms = avg_us / 1000.0;

        let mut sorted: Vec<u64> = self.tick_times_us.iter().copied().collect();
        sorted.sort_unstable();

        let percentile = |p: f64| -> f64 {
            let idx = ((n as f64 * p) as usize).min(n.saturating_sub(1));
            sorted[idx] as f64 / 1000.0
        };

        let p50_ms = percentile(0.50);
        let p95_ms = percentile(0.95);
        let p99_ms = percentile(0.99);
        let min_ms = sorted[0] as f64 / 1000.0;
        let max_ms = sorted[n - 1] as f64 / 1000.0;

        (tps, avg_ms, p50_ms, p95_ms, p99_ms, min_ms, max_ms)
    }

    fn estimated_fps(&self) -> f64 {
        let (tps, ..) = self.stats();
        self.views_per_tick * tps
    }

    fn render_metrics_panel(&self, frame: &mut Frame, area: Rect) {
        let accent = theme::screen_accent::PERFORMANCE;
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Real-Time Metrics")
            .title_alignment(Alignment::Center)
            .style(Style::new().fg(accent));

        let inner = block.inner(area);
        block.render(area, frame);
        if inner.is_empty() {
            return;
        }

        let (tps, avg_ms, p50_ms, p95_ms, p99_ms, min_ms, max_ms) = self.stats();
        let est_fps = self.views_per_tick * tps;
        let tier = DegradationTier::from_fps(est_fps);
        let views = self.view_counter.get();

        let fps_color = match tier {
            DegradationTier::Full => theme::accent::SUCCESS,
            DegradationTier::Reduced => theme::accent::WARNING,
            _ => theme::accent::ERROR,
        };

        let paused_tag = if self.paused { " [PAUSED]" } else { "" };

        let p95_style = if p95_ms > 16.67 {
            Style::new().fg(theme::accent::WARNING)
        } else {
            Style::new().fg(theme::fg::PRIMARY)
        };
        let p99_style = if p99_ms > 16.67 {
            Style::new().fg(theme::accent::ERROR)
        } else {
            Style::new().fg(theme::fg::PRIMARY)
        };

        let lines: Vec<(String, Style)> = vec![
            (
                format!("  Est. FPS:   {est_fps:>8.1}{paused_tag}"),
                Style::new().fg(fps_color),
            ),
            (
                format!("  Tick Rate:  {tps:>8.1} tps"),
                Style::new().fg(theme::fg::PRIMARY),
            ),
            (String::new(), Style::new()),
            (
                "  Latency (ms)".into(),
                Style::new().fg(theme::fg::SECONDARY),
            ),
            (
                format!("  avg:  {avg_ms:>8.2}"),
                Style::new().fg(theme::fg::PRIMARY),
            ),
            (
                format!("  p50:  {p50_ms:>8.2}"),
                Style::new().fg(theme::fg::PRIMARY),
            ),
            (format!("  p95:  {p95_ms:>8.2}"), p95_style),
            (format!("  p99:  {p99_ms:>8.2}"), p99_style),
            (
                format!("  min:  {min_ms:>8.2}"),
                Style::new().fg(theme::fg::MUTED),
            ),
            (
                format!("  max:  {max_ms:>8.2}"),
                Style::new().fg(theme::fg::MUTED),
            ),
            (String::new(), Style::new()),
            ("  Counters".into(), Style::new().fg(theme::fg::SECONDARY)),
            (
                format!("  Views:      {views:>8}"),
                Style::new().fg(theme::fg::PRIMARY),
            ),
            (
                format!("  Ticks:      {:>8}", self.tick_count),
                Style::new().fg(theme::fg::PRIMARY),
            ),
            (
                format!("  Samples:    {:>8}", self.tick_times_us.len()),
                Style::new().fg(theme::fg::MUTED),
            ),
            (
                format!("  V/Tick:     {:>8.2}", self.views_per_tick),
                Style::new().fg(theme::fg::MUTED),
            ),
        ];

        for (i, (text, style)) in lines.iter().enumerate() {
            if i >= inner.height as usize {
                break;
            }
            let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
            Paragraph::new(text.as_str())
                .style(*style)
                .render(row, frame);
        }
    }

    fn render_sparkline_panel(&self, frame: &mut Frame, area: Rect) {
        let mode_label = match self.sparkline_mode {
            SparklineMode::Intervals => "Tick Intervals (\u{00b5}s)",
            SparklineMode::Fps => "FPS Estimate",
        };

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(mode_label)
            .title_alignment(Alignment::Center)
            .style(theme::content_border());

        let inner = block.inner(area);
        block.render(area, frame);
        if inner.is_empty() || self.tick_times_us.is_empty() {
            return;
        }

        // Compute values for sparkline
        let values: Vec<f64> = match self.sparkline_mode {
            SparklineMode::Intervals => self.tick_times_us.iter().map(|&v| v as f64).collect(),
            SparklineMode::Fps => {
                // Rolling FPS estimate: 1_000_000 / interval_us
                self.tick_times_us
                    .iter()
                    .map(|&v| if v > 0 { 1_000_000.0 / v as f64 } else { 0.0 })
                    .collect()
            }
        };

        // Take only the last `width` samples
        let width = inner.width as usize;
        let start = values.len().saturating_sub(width);
        let visible = &values[start..];

        let v_min = visible.iter().copied().fold(f64::INFINITY, f64::min);
        let v_max = visible.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let range = (v_max - v_min).max(1.0);

        // Render sparkline using block characters
        let height = inner.height.saturating_sub(2) as usize; // leave room for axis labels
        if height == 0 {
            return;
        }

        // Build columns
        for (col, &val) in visible.iter().enumerate() {
            if col >= inner.width as usize {
                break;
            }
            let normalized = ((val - v_min) / range * height as f64).round() as usize;
            let filled = normalized.min(height);

            for row in 0..height {
                let y = inner.y + (height - 1 - row) as u16;
                let x = inner.x + col as u16;
                let ch = if row < filled {
                    let level = if row == filled.saturating_sub(1) {
                        // Top of bar: partial fill
                        let frac = ((val - v_min) / range * height as f64)
                            - (filled.saturating_sub(1)) as f64;
                        (frac * 8.0).round() as usize
                    } else {
                        8
                    };
                    BRAILLE_BLOCKS[level.min(8)]
                } else {
                    ' '
                };

                let style = if row < filled {
                    let ratio = row as f64 / height as f64;
                    if ratio > 0.8 {
                        Style::new().fg(theme::accent::ERROR)
                    } else if ratio > 0.5 {
                        Style::new().fg(theme::accent::WARNING)
                    } else {
                        Style::new().fg(theme::accent::SUCCESS)
                    }
                } else {
                    Style::new()
                };

                let cell_area = Rect::new(x, y, 1, 1);
                let s = String::from(ch);
                Paragraph::new(&*s).style(style).render(cell_area, frame);
            }
        }

        // Axis labels
        let max_label = match self.sparkline_mode {
            SparklineMode::Intervals => format!("{v_max:.0}\u{00b5}s"),
            SparklineMode::Fps => format!("{v_max:.0}fps"),
        };
        let min_label = match self.sparkline_mode {
            SparklineMode::Intervals => format!("{v_min:.0}\u{00b5}s"),
            SparklineMode::Fps => format!("{v_min:.0}fps"),
        };

        let label_y_top = inner.y + height as u16;
        let label_y_bot = inner.y + height as u16 + 1;

        if label_y_top < inner.y + inner.height {
            let lbl_area = Rect::new(inner.x, label_y_top, inner.width, 1);
            Paragraph::new(&*max_label)
                .style(Style::new().fg(theme::fg::MUTED))
                .render(lbl_area, frame);
        }
        if label_y_bot < inner.y + inner.height {
            let lbl_area = Rect::new(inner.x, label_y_bot, inner.width, 1);
            Paragraph::new(&*min_label)
                .style(Style::new().fg(theme::fg::MUTED))
                .render(lbl_area, frame);
        }
    }

    fn render_budget_panel(&self, frame: &mut Frame, area: Rect) {
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Render Budget")
            .title_alignment(Alignment::Center)
            .style(theme::content_border());

        let inner = block.inner(area);
        block.render(area, frame);
        if inner.is_empty() {
            return;
        }

        let (_, avg_ms, _, p95_ms, p99_ms, _, _) = self.stats();
        let est_fps = self.estimated_fps();
        let tier = DegradationTier::from_fps(est_fps);

        let budget_ratio = if self.budget_ms > 0.0 {
            avg_ms / self.budget_ms
        } else {
            0.0
        };

        let tier_color = match tier {
            DegradationTier::Full => theme::accent::SUCCESS,
            DegradationTier::Reduced => theme::accent::WARNING,
            DegradationTier::Minimal => theme::accent::ERROR,
            DegradationTier::Safety => theme::accent::ERROR,
        };

        let lines: Vec<(String, Style)> = vec![
            (
                format!(
                    "  Budget: {:.2}ms ({:.0}fps target)",
                    self.budget_ms,
                    1000.0 / self.budget_ms
                ),
                Style::new().fg(theme::fg::SECONDARY),
            ),
            (
                format!("  Actual: {avg_ms:.2}ms avg"),
                if budget_ratio > 1.0 {
                    Style::new().fg(theme::accent::ERROR)
                } else {
                    Style::new().fg(theme::fg::PRIMARY)
                },
            ),
            (
                format!("  Usage:  {:.0}%", budget_ratio * 100.0),
                if budget_ratio > 1.0 {
                    Style::new().fg(theme::accent::ERROR)
                } else if budget_ratio > 0.8 {
                    Style::new().fg(theme::accent::WARNING)
                } else {
                    Style::new().fg(theme::accent::SUCCESS)
                },
            ),
            (String::new(), Style::new()),
            ("  Budget Bar".into(), Style::new().fg(theme::fg::SECONDARY)),
        ];

        for (i, (text, style)) in lines.iter().enumerate() {
            if i >= inner.height as usize {
                break;
            }
            let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
            Paragraph::new(text.as_str())
                .style(*style)
                .render(row, frame);
        }

        // Budget progress bar
        let bar_row = 5;
        if bar_row < inner.height as usize {
            let bar_width = inner.width.saturating_sub(4) as usize;
            let filled = ((budget_ratio.min(2.0) / 2.0) * bar_width as f64) as usize;
            let threshold = (0.5 * bar_width as f64) as usize; // 100% mark
            let mut bar = String::with_capacity(bar_width + 4);
            bar.push_str("  ");
            for i in 0..bar_width {
                if i < filled {
                    if i >= threshold {
                        bar.push('\u{2588}'); // over budget
                    } else {
                        bar.push('\u{2588}');
                    }
                } else if i == threshold {
                    bar.push('|');
                } else {
                    bar.push('\u{2591}');
                }
            }

            let bar_style = if budget_ratio > 1.0 {
                Style::new().fg(theme::accent::ERROR)
            } else if budget_ratio > 0.8 {
                Style::new().fg(theme::accent::WARNING)
            } else {
                Style::new().fg(theme::accent::SUCCESS)
            };

            let bar_area = Rect::new(inner.x, inner.y + bar_row as u16, inner.width, 1);
            Paragraph::new(&*bar)
                .style(bar_style)
                .render(bar_area, frame);
        }

        // Degradation tier info
        let tier_start = 7;
        let tier_lines: Vec<(String, Style)> = vec![
            (String::new(), Style::new()),
            (
                "  Degradation Tier".into(),
                Style::new().fg(theme::fg::SECONDARY),
            ),
            (
                format!("  {} {}", tier.bar(), tier.label()),
                Style::new().fg(tier_color),
            ),
            (String::new(), Style::new()),
            ("  Thresholds".into(), Style::new().fg(theme::fg::MUTED)),
            (
                format!(
                    "  {} Full     \u{2265}50fps",
                    if tier == DegradationTier::Full {
                        "\u{25c6}"
                    } else {
                        "\u{25c7}"
                    }
                ),
                if tier == DegradationTier::Full {
                    Style::new().fg(theme::accent::SUCCESS)
                } else {
                    Style::new().fg(theme::fg::MUTED)
                },
            ),
            (
                format!(
                    "  {} Reduced  20\u{2013}49fps",
                    if tier == DegradationTier::Reduced {
                        "\u{25c6}"
                    } else {
                        "\u{25c7}"
                    }
                ),
                if tier == DegradationTier::Reduced {
                    Style::new().fg(theme::accent::WARNING)
                } else {
                    Style::new().fg(theme::fg::MUTED)
                },
            ),
            (
                format!(
                    "  {} Minimal  5\u{2013}19fps",
                    if tier == DegradationTier::Minimal {
                        "\u{25c6}"
                    } else {
                        "\u{25c7}"
                    }
                ),
                if tier == DegradationTier::Minimal {
                    Style::new().fg(theme::accent::ERROR)
                } else {
                    Style::new().fg(theme::fg::MUTED)
                },
            ),
            (
                format!(
                    "  {} Safety   <5fps",
                    if tier == DegradationTier::Safety {
                        "\u{25c6}"
                    } else {
                        "\u{25c7}"
                    }
                ),
                if tier == DegradationTier::Safety {
                    Style::new().fg(theme::accent::ERROR)
                } else {
                    Style::new().fg(theme::fg::MUTED)
                },
            ),
        ];

        for (i, (text, style)) in tier_lines.iter().enumerate() {
            let row_idx = tier_start + i;
            if row_idx >= inner.height as usize {
                break;
            }
            let row = Rect::new(inner.x, inner.y + row_idx as u16, inner.width, 1);
            Paragraph::new(text.as_str())
                .style(*style)
                .render(row, frame);
        }

        // P95/P99 warnings at bottom
        let warn_start = tier_start + tier_lines.len();
        if p95_ms > self.budget_ms && warn_start < inner.height as usize {
            let warn = format!("  \u{26a0} p95 ({p95_ms:.1}ms) exceeds budget");
            let row = Rect::new(inner.x, inner.y + warn_start as u16, inner.width, 1);
            Paragraph::new(&*warn)
                .style(Style::new().fg(theme::accent::WARNING))
                .render(row, frame);
        }
        if p99_ms > self.budget_ms && warn_start + 1 < inner.height as usize {
            let warn = format!("  \u{26a0} p99 ({p99_ms:.1}ms) exceeds budget");
            let row = Rect::new(inner.x, inner.y + (warn_start + 1) as u16, inner.width, 1);
            Paragraph::new(&*warn)
                .style(Style::new().fg(theme::accent::ERROR))
                .render(row, frame);
        }
    }
}

impl Screen for PerformanceHud {
    type Message = Event;

    fn update(&mut self, event: &Event) -> Cmd<Self::Message> {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            modifiers,
            ..
        }) = event
        {
            match (*code, *modifiers) {
                (KeyCode::Char('r'), Modifiers::NONE) => self.reset(),
                (KeyCode::Char('p'), Modifiers::NONE) => self.paused = !self.paused,
                (KeyCode::Char('m'), Modifiers::NONE) => {
                    self.sparkline_mode = match self.sparkline_mode {
                        SparklineMode::Intervals => SparklineMode::Fps,
                        SparklineMode::Fps => SparklineMode::Intervals,
                    };
                }
                _ => {}
            }
        }
        Cmd::None
    }

    fn tick(&mut self, tick_count: u64) {
        self.tick_count = tick_count;
        self.record_tick();
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        self.view_counter.set(self.view_counter.get() + 1);

        if area.is_empty() {
            return;
        }

        // Three-row layout: title, content panels, status bar
        let rows = Flex::vertical()
            .constraints([
                Constraint::Fixed(1),
                Constraint::Min(8),
                Constraint::Fixed(1),
            ])
            .split(area);

        // Title bar
        let title = "PERFORMANCE HUD + RENDER BUDGET VISUALIZER";
        Paragraph::new(title)
            .style(
                Style::new()
                    .fg(theme::fg::PRIMARY)
                    .bg(theme::alpha::SURFACE),
            )
            .render(rows[0], frame);

        // Content: metrics (left) | sparkline (center) | budget (right)
        let cols = Flex::horizontal()
            .constraints([
                Constraint::Fixed(32),
                Constraint::Min(20),
                Constraint::Fixed(36),
            ])
            .split(rows[1]);

        self.render_metrics_panel(frame, cols[0]);
        self.render_sparkline_panel(frame, cols[1]);
        self.render_budget_panel(frame, cols[2]);

        // Status bar
        let mode_label = match self.sparkline_mode {
            SparklineMode::Intervals => "intervals",
            SparklineMode::Fps => "FPS",
        };
        let pause_label = if self.paused { "resume" } else { "pause" };
        let status = format!(
            "r:reset | p:{pause_label} | m:mode({mode_label}) | samples:{}/{}",
            self.tick_times_us.len(),
            MAX_SAMPLES,
        );
        Paragraph::new(&*status)
            .style(Style::new().fg(theme::fg::MUTED).bg(theme::alpha::SURFACE))
            .render(rows[2], frame);
    }

    fn keybindings(&self) -> Vec<HelpEntry> {
        vec![
            HelpEntry {
                key: "r",
                action: "Reset all metrics",
            },
            HelpEntry {
                key: "p",
                action: "Pause/resume collection",
            },
            HelpEntry {
                key: "m",
                action: "Toggle sparkline mode (intervals/FPS)",
            },
        ]
    }

    fn title(&self) -> &'static str {
        "Performance HUD"
    }

    fn tab_label(&self) -> &'static str {
        "PerfHUD"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ftui_render::grapheme_pool::GraphemePool;

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: Modifiers::empty(),
            kind: KeyEventKind::Press,
        })
    }

    #[test]
    fn initial_state() {
        let hud = PerformanceHud::new();
        assert_eq!(hud.tick_count, 0);
        assert!(hud.tick_times_us.is_empty());
        assert!(!hud.paused);
        assert_eq!(hud.sparkline_mode, SparklineMode::Intervals);
        assert_eq!(hud.title(), "Performance HUD");
        assert_eq!(hud.tab_label(), "PerfHUD");
    }

    #[test]
    fn stats_empty() {
        let hud = PerformanceHud::new();
        let (tps, avg, p50, p95, p99, min, max) = hud.stats();
        assert_eq!(tps, 0.0);
        assert_eq!(avg, 0.0);
        assert_eq!(p50, 0.0);
        assert_eq!(p95, 0.0);
        assert_eq!(p99, 0.0);
        assert_eq!(min, 0.0);
        assert_eq!(max, 0.0);
    }

    #[test]
    fn stats_with_samples() {
        let mut hud = PerformanceHud::new();
        // Simulate 100ms tick intervals (100_000 us)
        for _ in 0..10 {
            hud.tick_times_us.push_back(100_000);
        }
        let (tps, avg_ms, _, _, _, _, _) = hud.stats();
        assert!((tps - 10.0).abs() < 0.1);
        assert!((avg_ms - 100.0).abs() < 0.1);
    }

    #[test]
    fn reset_clears_state() {
        let mut hud = PerformanceHud::new();
        hud.tick_times_us.push_back(100_000);
        hud.view_counter.set(42);
        hud.views_per_tick = 1.5;
        hud.reset();
        assert!(hud.tick_times_us.is_empty());
        assert_eq!(hud.view_counter.get(), 0);
        assert_eq!(hud.views_per_tick, 0.0);
    }

    #[test]
    fn pause_toggle() {
        let mut hud = PerformanceHud::new();
        assert!(!hud.paused);
        hud.update(&press(KeyCode::Char('p')));
        assert!(hud.paused);
        hud.update(&press(KeyCode::Char('p')));
        assert!(!hud.paused);
    }

    #[test]
    fn mode_toggle() {
        let mut hud = PerformanceHud::new();
        assert_eq!(hud.sparkline_mode, SparklineMode::Intervals);
        hud.update(&press(KeyCode::Char('m')));
        assert_eq!(hud.sparkline_mode, SparklineMode::Fps);
        hud.update(&press(KeyCode::Char('m')));
        assert_eq!(hud.sparkline_mode, SparklineMode::Intervals);
    }

    #[test]
    fn reset_via_key() {
        let mut hud = PerformanceHud::new();
        hud.tick_times_us.push_back(50_000);
        hud.update(&press(KeyCode::Char('r')));
        assert!(hud.tick_times_us.is_empty());
    }

    #[test]
    fn ring_buffer_caps_at_max() {
        let mut hud = PerformanceHud::new();
        for i in 0..MAX_SAMPLES + 20 {
            hud.tick_times_us.push_back(i as u64 * 1000);
            if hud.tick_times_us.len() > MAX_SAMPLES {
                hud.tick_times_us.pop_front();
            }
        }
        assert_eq!(hud.tick_times_us.len(), MAX_SAMPLES);
    }

    #[test]
    fn degradation_tiers() {
        assert_eq!(DegradationTier::from_fps(60.0), DegradationTier::Full);
        assert_eq!(DegradationTier::from_fps(50.0), DegradationTier::Full);
        assert_eq!(DegradationTier::from_fps(49.9), DegradationTier::Reduced);
        assert_eq!(DegradationTier::from_fps(20.0), DegradationTier::Reduced);
        assert_eq!(DegradationTier::from_fps(19.9), DegradationTier::Minimal);
        assert_eq!(DegradationTier::from_fps(5.0), DegradationTier::Minimal);
        assert_eq!(DegradationTier::from_fps(4.9), DegradationTier::Safety);
        assert_eq!(DegradationTier::from_fps(0.0), DegradationTier::Safety);
    }

    #[test]
    fn keybindings_not_empty() {
        let hud = PerformanceHud::new();
        assert!(!hud.keybindings().is_empty());
    }

    #[test]
    fn render_no_panic_empty_area() {
        let hud = PerformanceHud::new();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(1, 1, &mut pool);
        let area = Rect::new(0, 0, 0, 0);
        hud.view(&mut frame, area);
    }

    #[test]
    fn render_no_panic_small_area() {
        let hud = PerformanceHud::new();
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(40, 10, &mut pool);
        let area = Rect::new(0, 0, 40, 10);
        hud.view(&mut frame, area);
    }

    #[test]
    fn render_no_panic_standard_area() {
        let mut hud = PerformanceHud::new();
        // Add some samples
        for _ in 0..60 {
            hud.tick_times_us.push_back(100_000);
        }
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(120, 40, &mut pool);
        let area = Rect::new(0, 0, 120, 40);
        hud.view(&mut frame, area);
    }

    #[test]
    fn render_fps_mode_no_panic() {
        let mut hud = PerformanceHud::new();
        hud.sparkline_mode = SparklineMode::Fps;
        for i in 0..30 {
            hud.tick_times_us.push_back(80_000 + i * 1000);
        }
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(80, 24, &mut pool);
        let area = Rect::new(0, 0, 80, 24);
        hud.view(&mut frame, area);
    }

    #[test]
    fn view_counter_increments() {
        let hud = PerformanceHud::new();
        assert_eq!(hud.view_counter.get(), 0);
        let mut pool = GraphemePool::new();
        let mut frame = Frame::new(80, 24, &mut pool);
        let area = Rect::new(0, 0, 80, 24);
        hud.view(&mut frame, area);
        assert_eq!(hud.view_counter.get(), 1);
        hud.view(&mut frame, area);
        assert_eq!(hud.view_counter.get(), 2);
    }

    #[test]
    fn estimated_fps_zero_when_empty() {
        let hud = PerformanceHud::new();
        assert_eq!(hud.estimated_fps(), 0.0);
    }

    #[test]
    fn tick_records_interval() {
        let mut hud = PerformanceHud::new();
        hud.tick(0);
        // First tick establishes baseline, no sample recorded
        assert!(hud.tick_times_us.is_empty());
        // Give a small delay for a nonzero interval
        std::thread::sleep(std::time::Duration::from_millis(1));
        hud.tick(1);
        assert_eq!(hud.tick_times_us.len(), 1);
    }

    #[test]
    fn paused_does_not_record() {
        let mut hud = PerformanceHud::new();
        hud.tick(0);
        hud.paused = true;
        std::thread::sleep(std::time::Duration::from_millis(1));
        hud.tick(1);
        assert!(hud.tick_times_us.is_empty());
    }
}
