#![forbid(unsafe_code)]

//! VOI debug overlay widget (Galaxy-Brain).

use crate::Widget;
use crate::block::{Alignment, Block};
use crate::borders::{BorderType, Borders};
use crate::paragraph::Paragraph;
use ftui_core::geometry::Rect;
use ftui_render::cell::{Cell, PackedRgba};
use ftui_render::frame::Frame;
use ftui_style::Style;

/// Summary of the VOI posterior.
#[derive(Debug, Clone)]
pub struct VoiPosteriorSummary {
    pub alpha: f64,
    pub beta: f64,
    pub mean: f64,
    pub variance: f64,
    pub expected_variance_after: f64,
    pub voi_gain: f64,
}

/// Summary of the most recent VOI decision.
#[derive(Debug, Clone)]
pub struct VoiDecisionSummary {
    pub event_idx: u64,
    pub should_sample: bool,
    pub reason: String,
    pub score: f64,
    pub cost: f64,
    pub log_bayes_factor: f64,
    pub e_value: f64,
    pub e_threshold: f64,
    pub boundary_score: f64,
}

/// Summary of the most recent VOI observation.
#[derive(Debug, Clone)]
pub struct VoiObservationSummary {
    pub sample_idx: u64,
    pub violated: bool,
    pub posterior_mean: f64,
    pub alpha: f64,
    pub beta: f64,
}

/// Ledger entries for the VOI debug overlay.
#[derive(Debug, Clone)]
pub enum VoiLedgerEntry {
    Decision {
        event_idx: u64,
        should_sample: bool,
        voi_gain: f64,
        log_bayes_factor: f64,
    },
    Observation {
        sample_idx: u64,
        violated: bool,
        posterior_mean: f64,
    },
}

/// Full overlay data payload.
#[derive(Debug, Clone)]
pub struct VoiOverlayData {
    pub title: String,
    pub tick: Option<u64>,
    pub source: Option<String>,
    pub posterior: VoiPosteriorSummary,
    pub decision: Option<VoiDecisionSummary>,
    pub observation: Option<VoiObservationSummary>,
    pub ledger: Vec<VoiLedgerEntry>,
}

/// Styling options for the VOI overlay.
#[derive(Debug, Clone)]
pub struct VoiOverlayStyle {
    pub border: Style,
    pub text: Style,
    pub background: Option<PackedRgba>,
    pub border_type: BorderType,
}

impl Default for VoiOverlayStyle {
    fn default() -> Self {
        Self {
            border: Style::new(),
            text: Style::new(),
            background: None,
            border_type: BorderType::Rounded,
        }
    }
}

/// VOI debug overlay widget.
#[derive(Debug, Clone)]
pub struct VoiDebugOverlay {
    data: VoiOverlayData,
    style: VoiOverlayStyle,
}

impl VoiDebugOverlay {
    /// Create a new VOI overlay widget.
    pub fn new(data: VoiOverlayData) -> Self {
        Self {
            data,
            style: VoiOverlayStyle::default(),
        }
    }

    /// Override styling for the overlay.
    pub fn with_style(mut self, style: VoiOverlayStyle) -> Self {
        self.style = style;
        self
    }

    fn build_lines(&self, line_width: usize) -> Vec<String> {
        let mut lines = Vec::with_capacity(20);
        let divider = "-".repeat(line_width);

        let mut header = self.data.title.clone();
        if let Some(tick) = self.data.tick {
            header.push_str(&format!(" (tick {})", tick));
        }
        if let Some(source) = &self.data.source {
            header.push_str(&format!(" [{source}]"));
        }

        lines.push(header);
        lines.push(divider.clone());

        if let Some(decision) = &self.data.decision {
            let verdict = if decision.should_sample {
                "SAMPLE"
            } else {
                "SKIP"
            };
            lines.push(format!(
                "Decision: {:<6}  reason: {}",
                verdict, decision.reason
            ));
            lines.push(format!(
                "log10 BF: {:+.3}  score/cost",
                decision.log_bayes_factor
            ));
            lines.push(format!(
                "E: {:.3} / {:.2}  boundary: {:.3}",
                decision.e_value, decision.e_threshold, decision.boundary_score
            ));
        } else {
            lines.push("Decision: —".to_string());
        }

        lines.push(String::new());
        lines.push("Posterior Core".to_string());
        lines.push(divider.clone());
        lines.push(format!(
            "p ~ Beta(a,b)  a={:.2}  b={:.2}",
            self.data.posterior.alpha, self.data.posterior.beta
        ));
        lines.push(format!(
            "mu={:.4}  Var={:.6}",
            self.data.posterior.mean, self.data.posterior.variance
        ));
        lines.push("VOI = Var[p] - E[Var|1]".to_string());
        lines.push(format!(
            "VOI = {:.6} - {:.6} = {:.6}",
            self.data.posterior.variance,
            self.data.posterior.expected_variance_after,
            self.data.posterior.voi_gain
        ));

        if let Some(decision) = &self.data.decision {
            lines.push(String::new());
            lines.push("Decision Equation".to_string());
            lines.push(divider.clone());
            lines.push(format!(
                "score={:.6}  cost={:.6}",
                decision.score, decision.cost
            ));
            lines.push(format!(
                "log10 BF = log10({:.6}/{:.6}) = {:+.3}",
                decision.score, decision.cost, decision.log_bayes_factor
            ));
        }

        if let Some(obs) = &self.data.observation {
            lines.push(String::new());
            lines.push("Last Sample".to_string());
            lines.push(divider.clone());
            lines.push(format!(
                "violated: {}  a={:.1}  b={:.1}  mu={:.3}",
                obs.violated, obs.alpha, obs.beta, obs.posterior_mean
            ));
        }

        if !self.data.ledger.is_empty() {
            lines.push(String::new());
            lines.push("Evidence Ledger (Recent)".to_string());
            lines.push(divider);
            for entry in &self.data.ledger {
                match entry {
                    VoiLedgerEntry::Decision {
                        event_idx,
                        should_sample,
                        voi_gain,
                        log_bayes_factor,
                    } => {
                        let verdict = if *should_sample { "S" } else { "-" };
                        lines.push(format!(
                            "D#{:>3} {verdict} VOI={:.5} logBF={:+.2}",
                            event_idx, voi_gain, log_bayes_factor
                        ));
                    }
                    VoiLedgerEntry::Observation {
                        sample_idx,
                        violated,
                        posterior_mean,
                    } => {
                        lines.push(format!(
                            "O#{:>3} viol={} mu={:.3}",
                            sample_idx, violated, posterior_mean
                        ));
                    }
                }
            }
        }

        lines
    }
}

impl Widget for VoiDebugOverlay {
    fn render(&self, area: Rect, frame: &mut Frame) {
        if area.is_empty() || area.width < 20 || area.height < 6 {
            return;
        }

        if let Some(bg) = self.style.background {
            let cell = Cell::default().with_bg(bg);
            frame.buffer.fill(area, cell);
        }

        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(self.style.border_type)
            .border_style(self.style.border)
            .title(&self.data.title)
            .title_alignment(Alignment::Center)
            .style(self.style.text);

        let inner = block.inner(area);
        block.render(area, frame);

        if inner.is_empty() {
            return;
        }

        let line_width = inner.width.saturating_sub(2) as usize;
        let lines = self.build_lines(line_width.max(1));
        let text = lines.join("\n");
        Paragraph::new(text)
            .style(self.style.text)
            .render(inner, frame);
    }
}
