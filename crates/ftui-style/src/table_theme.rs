#![forbid(unsafe_code)]

//! TableTheme core types and preset definitions.

use crate::Style;
use crate::color::{Ansi16, Color, ColorProfile};
use ftui_render::cell::PackedRgba;

#[inline]
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let a = a as f32;
    let b = b as f32;
    (a + (b - a) * t).round().clamp(0.0, 255.0) as u8
}

#[inline]
fn lerp_color(a: PackedRgba, b: PackedRgba, t: f32) -> PackedRgba {
    let t = t.clamp(0.0, 1.0);
    PackedRgba::rgba(
        lerp_u8(a.r(), b.r(), t),
        lerp_u8(a.g(), b.g(), t),
        lerp_u8(a.b(), b.b(), t),
        lerp_u8(a.a(), b.a(), t),
    )
}

/// Built-in TableTheme preset identifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TablePresetId {
    Aurora,
    Graphite,
    Neon,
    Slate,
    Solar,
    Orchard,
    Paper,
    Midnight,
    TerminalClassic,
}

/// Semantic table sections.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TableSection {
    Header,
    Body,
    Footer,
}

/// Target selection for a table effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TableEffectTarget {
    Section(TableSection),
    Row(usize),
    RowRange {
        start: usize,
        end: usize,
    },
    Column(usize),
    ColumnRange {
        start: usize,
        end: usize,
    },
    /// Body rows only.
    AllRows,
    /// Header + body.
    AllCells,
}

/// Scope used to resolve table effects without per-cell work.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TableEffectScope {
    pub section: TableSection,
    pub row: Option<usize>,
    pub column: Option<usize>,
}

impl TableEffectScope {
    /// Scope for a whole section (no row/column specificity).
    #[must_use]
    pub const fn section(section: TableSection) -> Self {
        Self {
            section,
            row: None,
            column: None,
        }
    }

    /// Scope for a specific row within a section.
    #[must_use]
    pub const fn row(section: TableSection, row: usize) -> Self {
        Self {
            section,
            row: Some(row),
            column: None,
        }
    }

    /// Scope for a specific column within a section.
    #[must_use]
    pub const fn column(section: TableSection, column: usize) -> Self {
        Self {
            section,
            row: None,
            column: Some(column),
        }
    }
}

impl TableEffectTarget {
    /// Determine whether this target applies to the given scope.
    #[must_use]
    pub fn matches_scope(&self, scope: TableEffectScope) -> bool {
        match *self {
            TableEffectTarget::Section(section) => scope.section == section,
            TableEffectTarget::Row(row) => scope.row == Some(row),
            TableEffectTarget::RowRange { start, end } => {
                scope.row.is_some_and(|row| row >= start && row <= end)
            }
            TableEffectTarget::Column(column) => scope.column == Some(column),
            TableEffectTarget::ColumnRange { start, end } => scope
                .column
                .is_some_and(|column| column >= start && column <= end),
            TableEffectTarget::AllRows => {
                scope.section == TableSection::Body && scope.row.is_some()
            }
            TableEffectTarget::AllCells => {
                matches!(scope.section, TableSection::Header | TableSection::Body)
                    && (scope.row.is_some() || scope.column.is_some())
            }
        }
    }
}

/// A multi-stop gradient for table effects.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    stops: Vec<(f32, PackedRgba)>,
}

impl Gradient {
    /// Create a new gradient with stops in the range [0, 1].
    pub fn new(stops: Vec<(f32, PackedRgba)>) -> Self {
        let mut stops = stops;
        stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        Self { stops }
    }

    /// Access the gradient stops (sorted by position).
    #[must_use]
    pub fn stops(&self) -> &[(f32, PackedRgba)] {
        &self.stops
    }

    /// Sample the gradient at a normalized position in [0, 1].
    #[must_use]
    pub fn sample(&self, t: f32) -> PackedRgba {
        let t = t.clamp(0.0, 1.0);
        let Some(first) = self.stops.first() else {
            return PackedRgba::TRANSPARENT;
        };
        if t <= first.0 {
            return first.1;
        }
        let Some(last) = self.stops.last() else {
            return first.1;
        };
        if t >= last.0 {
            return last.1;
        }

        for window in self.stops.windows(2) {
            let (p0, c0) = window[0];
            let (p1, c1) = window[1];
            if t <= p1 {
                let denom = p1 - p0;
                if denom <= f32::EPSILON {
                    return c1;
                }
                let local = (t - p0) / denom;
                return lerp_color(c0, c1, local);
            }
        }

        last.1
    }
}

/// Effect definitions applied to table styles.
#[derive(Clone, Debug)]
pub enum TableEffect {
    Pulse {
        fg_a: PackedRgba,
        fg_b: PackedRgba,
        bg_a: PackedRgba,
        bg_b: PackedRgba,
        speed: f32,
        phase_offset: f32,
    },
    BreathingGlow {
        fg: PackedRgba,
        bg: PackedRgba,
        intensity: f32,
        speed: f32,
        phase_offset: f32,
        asymmetry: f32,
    },
    GradientSweep {
        gradient: Gradient,
        speed: f32,
        phase_offset: f32,
    },
}

/// How effect colors blend with the base style.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Default)]
pub enum BlendMode {
    #[default]
    Replace,
    Additive,
    Multiply,
    Screen,
}

/// Mask for which style channels effects are allowed to override.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct StyleMask {
    pub fg: bool,
    pub bg: bool,
    pub attrs: bool,
}

impl StyleMask {
    /// Mask that allows only foreground and background changes.
    #[must_use]
    pub const fn fg_bg() -> Self {
        Self {
            fg: true,
            bg: true,
            attrs: false,
        }
    }

    /// Mask that allows all channels.
    #[must_use]
    pub const fn all() -> Self {
        Self {
            fg: true,
            bg: true,
            attrs: true,
        }
    }

    /// Mask that blocks all channels.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            fg: false,
            bg: false,
            attrs: false,
        }
    }
}

impl Default for StyleMask {
    fn default() -> Self {
        Self::fg_bg()
    }
}

/// A single effect rule applied to a table target.
#[derive(Clone, Debug)]
pub struct TableEffectRule {
    pub target: TableEffectTarget,
    pub effect: TableEffect,
    pub priority: u8,
    pub blend_mode: BlendMode,
    pub style_mask: StyleMask,
}

impl TableEffectRule {
    /// Create a new effect rule with default blending and masking.
    #[must_use]
    pub fn new(target: TableEffectTarget, effect: TableEffect) -> Self {
        Self {
            target,
            effect,
            priority: 0,
            blend_mode: BlendMode::default(),
            style_mask: StyleMask::default(),
        }
    }

    /// Set rule priority (higher applies later).
    #[must_use]
    pub fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// Set blend mode.
    #[must_use]
    pub fn blend_mode(mut self, blend_mode: BlendMode) -> Self {
        self.blend_mode = blend_mode;
        self
    }

    /// Set style mask.
    #[must_use]
    pub fn style_mask(mut self, style_mask: StyleMask) -> Self {
        self.style_mask = style_mask;
        self
    }
}

/// Resolve table effects for a given scope and phase.
///
/// The resolver is designed to run once per row/column/section (not per cell).
pub struct TableEffectResolver<'a> {
    theme: &'a TableTheme,
}

impl<'a> TableEffectResolver<'a> {
    /// Create a resolver for a given theme.
    #[must_use]
    pub const fn new(theme: &'a TableTheme) -> Self {
        Self { theme }
    }

    /// Resolve effects for a specific scope at the provided phase.
    #[must_use]
    pub fn resolve(&self, base: Style, scope: TableEffectScope, phase: f32) -> Style {
        resolve_effects_for_scope(self.theme, base, scope, phase)
    }
}

/// Shared theme for all table render paths.
#[derive(Clone, Debug)]
pub struct TableTheme {
    pub border: Style,
    pub header: Style,
    pub row: Style,
    pub row_alt: Style,
    pub row_selected: Style,
    pub row_hover: Style,
    pub divider: Style,
    pub padding: u8,
    pub column_gap: u8,
    pub row_height: u8,
    pub effects: Vec<TableEffectRule>,
    pub preset_id: Option<TablePresetId>,
}

struct ThemeStyles {
    border: Style,
    header: Style,
    row: Style,
    row_alt: Style,
    row_selected: Style,
    row_hover: Style,
    divider: Style,
}

impl TableTheme {
    /// Create a resolver that applies this theme's effects.
    #[must_use]
    pub const fn effect_resolver(&self) -> TableEffectResolver<'_> {
        TableEffectResolver::new(self)
    }

    /// Build a theme from a preset identifier.
    #[must_use]
    pub fn preset(preset: TablePresetId) -> Self {
        match preset {
            TablePresetId::Aurora => Self::aurora(),
            TablePresetId::Graphite => Self::graphite(),
            TablePresetId::Neon => Self::neon(),
            TablePresetId::Slate => Self::slate(),
            TablePresetId::Solar => Self::solar(),
            TablePresetId::Orchard => Self::orchard(),
            TablePresetId::Paper => Self::paper(),
            TablePresetId::Midnight => Self::midnight(),
            TablePresetId::TerminalClassic => Self::terminal_classic(),
        }
    }

    /// Luminous header with cool zebra rows.
    #[must_use]
    pub fn aurora() -> Self {
        Self::build(
            TablePresetId::Aurora,
            ThemeStyles {
                border: Style::new().fg(PackedRgba::rgb(130, 170, 210)),
                header: Style::new()
                    .fg(PackedRgba::rgb(250, 250, 255))
                    .bg(PackedRgba::rgb(70, 100, 140))
                    .bold(),
                row: Style::new().fg(PackedRgba::rgb(230, 235, 245)),
                row_alt: Style::new()
                    .fg(PackedRgba::rgb(230, 235, 245))
                    .bg(PackedRgba::rgb(28, 36, 54)),
                row_selected: Style::new()
                    .fg(PackedRgba::rgb(255, 255, 255))
                    .bg(PackedRgba::rgb(50, 90, 140))
                    .bold(),
                row_hover: Style::new()
                    .fg(PackedRgba::rgb(240, 245, 255))
                    .bg(PackedRgba::rgb(40, 70, 110)),
                divider: Style::new().fg(PackedRgba::rgb(90, 120, 160)),
            },
        )
    }

    /// Monochrome, maximum legibility at dense data.
    #[must_use]
    pub fn graphite() -> Self {
        Self::build(
            TablePresetId::Graphite,
            ThemeStyles {
                border: Style::new().fg(PackedRgba::rgb(140, 140, 140)),
                header: Style::new()
                    .fg(PackedRgba::rgb(240, 240, 240))
                    .bg(PackedRgba::rgb(70, 70, 70))
                    .bold(),
                row: Style::new().fg(PackedRgba::rgb(220, 220, 220)),
                row_alt: Style::new()
                    .fg(PackedRgba::rgb(220, 220, 220))
                    .bg(PackedRgba::rgb(35, 35, 35)),
                row_selected: Style::new()
                    .fg(PackedRgba::rgb(255, 255, 255))
                    .bg(PackedRgba::rgb(90, 90, 90)),
                row_hover: Style::new()
                    .fg(PackedRgba::rgb(245, 245, 245))
                    .bg(PackedRgba::rgb(60, 60, 60)),
                divider: Style::new().fg(PackedRgba::rgb(100, 100, 100)),
            },
        )
    }

    /// Neon accent header with vivid highlights.
    #[must_use]
    pub fn neon() -> Self {
        Self::build(
            TablePresetId::Neon,
            ThemeStyles {
                border: Style::new().fg(PackedRgba::rgb(120, 255, 230)),
                header: Style::new()
                    .fg(PackedRgba::rgb(10, 10, 15))
                    .bg(PackedRgba::rgb(0, 255, 200))
                    .bold(),
                row: Style::new().fg(PackedRgba::rgb(210, 255, 245)),
                row_alt: Style::new()
                    .fg(PackedRgba::rgb(210, 255, 245))
                    .bg(PackedRgba::rgb(10, 20, 30)),
                row_selected: Style::new()
                    .fg(PackedRgba::rgb(5, 5, 10))
                    .bg(PackedRgba::rgb(255, 0, 200))
                    .bold(),
                row_hover: Style::new()
                    .fg(PackedRgba::rgb(0, 10, 15))
                    .bg(PackedRgba::rgb(0, 200, 255)),
                divider: Style::new().fg(PackedRgba::rgb(80, 220, 200)),
            },
        )
    }

    /// Subtle slate tones for neutral dashboards.
    #[must_use]
    pub fn slate() -> Self {
        Self::build(
            TablePresetId::Slate,
            ThemeStyles {
                border: Style::new().fg(PackedRgba::rgb(120, 130, 140)),
                header: Style::new()
                    .fg(PackedRgba::rgb(230, 235, 240))
                    .bg(PackedRgba::rgb(60, 70, 80))
                    .bold(),
                row: Style::new().fg(PackedRgba::rgb(210, 215, 220)),
                row_alt: Style::new()
                    .fg(PackedRgba::rgb(210, 215, 220))
                    .bg(PackedRgba::rgb(30, 35, 40)),
                row_selected: Style::new()
                    .fg(PackedRgba::rgb(255, 255, 255))
                    .bg(PackedRgba::rgb(80, 90, 110)),
                row_hover: Style::new()
                    .fg(PackedRgba::rgb(235, 240, 245))
                    .bg(PackedRgba::rgb(50, 60, 70)),
                divider: Style::new().fg(PackedRgba::rgb(90, 100, 110)),
            },
        )
    }

    /// Warm, sunlight-forward palette.
    #[must_use]
    pub fn solar() -> Self {
        Self::build(
            TablePresetId::Solar,
            ThemeStyles {
                border: Style::new().fg(PackedRgba::rgb(200, 170, 120)),
                header: Style::new()
                    .fg(PackedRgba::rgb(30, 25, 10))
                    .bg(PackedRgba::rgb(255, 200, 90))
                    .bold(),
                row: Style::new().fg(PackedRgba::rgb(240, 220, 180)),
                row_alt: Style::new()
                    .fg(PackedRgba::rgb(240, 220, 180))
                    .bg(PackedRgba::rgb(60, 40, 20)),
                row_selected: Style::new()
                    .fg(PackedRgba::rgb(20, 10, 0))
                    .bg(PackedRgba::rgb(255, 140, 60))
                    .bold(),
                row_hover: Style::new()
                    .fg(PackedRgba::rgb(20, 10, 0))
                    .bg(PackedRgba::rgb(220, 120, 40)),
                divider: Style::new().fg(PackedRgba::rgb(170, 140, 90)),
            },
        )
    }

    /// Orchard greens with soft depth.
    #[must_use]
    pub fn orchard() -> Self {
        Self::build(
            TablePresetId::Orchard,
            ThemeStyles {
                border: Style::new().fg(PackedRgba::rgb(140, 180, 120)),
                header: Style::new()
                    .fg(PackedRgba::rgb(20, 40, 20))
                    .bg(PackedRgba::rgb(120, 200, 120))
                    .bold(),
                row: Style::new().fg(PackedRgba::rgb(210, 235, 210)),
                row_alt: Style::new()
                    .fg(PackedRgba::rgb(210, 235, 210))
                    .bg(PackedRgba::rgb(30, 60, 40)),
                row_selected: Style::new()
                    .fg(PackedRgba::rgb(15, 30, 15))
                    .bg(PackedRgba::rgb(160, 230, 140))
                    .bold(),
                row_hover: Style::new()
                    .fg(PackedRgba::rgb(15, 30, 15))
                    .bg(PackedRgba::rgb(130, 210, 120)),
                divider: Style::new().fg(PackedRgba::rgb(100, 150, 100)),
            },
        )
    }

    /// Light, paper-like styling for documentation tables.
    #[must_use]
    pub fn paper() -> Self {
        Self::build(
            TablePresetId::Paper,
            ThemeStyles {
                border: Style::new().fg(PackedRgba::rgb(120, 110, 100)),
                header: Style::new()
                    .fg(PackedRgba::rgb(30, 30, 30))
                    .bg(PackedRgba::rgb(230, 220, 200))
                    .bold(),
                row: Style::new()
                    .fg(PackedRgba::rgb(40, 40, 40))
                    .bg(PackedRgba::rgb(245, 240, 230)),
                row_alt: Style::new()
                    .fg(PackedRgba::rgb(40, 40, 40))
                    .bg(PackedRgba::rgb(235, 230, 220)),
                row_selected: Style::new()
                    .fg(PackedRgba::rgb(10, 10, 10))
                    .bg(PackedRgba::rgb(255, 245, 210))
                    .bold(),
                row_hover: Style::new()
                    .fg(PackedRgba::rgb(20, 20, 20))
                    .bg(PackedRgba::rgb(245, 235, 205)),
                divider: Style::new().fg(PackedRgba::rgb(140, 130, 120)),
            },
        )
    }

    /// Deep, nocturnal palette with high contrast accents.
    #[must_use]
    pub fn midnight() -> Self {
        Self::build(
            TablePresetId::Midnight,
            ThemeStyles {
                border: Style::new().fg(PackedRgba::rgb(80, 100, 130)),
                header: Style::new()
                    .fg(PackedRgba::rgb(220, 230, 255))
                    .bg(PackedRgba::rgb(30, 40, 70))
                    .bold(),
                row: Style::new().fg(PackedRgba::rgb(200, 210, 230)),
                row_alt: Style::new()
                    .fg(PackedRgba::rgb(200, 210, 230))
                    .bg(PackedRgba::rgb(15, 20, 35)),
                row_selected: Style::new()
                    .fg(PackedRgba::rgb(255, 255, 255))
                    .bg(PackedRgba::rgb(60, 80, 120))
                    .bold(),
                row_hover: Style::new()
                    .fg(PackedRgba::rgb(240, 240, 255))
                    .bg(PackedRgba::rgb(45, 60, 90)),
                divider: Style::new().fg(PackedRgba::rgb(60, 80, 110)),
            },
        )
    }

    /// ANSI-16 baseline with richer palettes on 256/truecolor terminals.
    #[must_use]
    pub fn terminal_classic() -> Self {
        Self::terminal_classic_for(ColorProfile::detect())
    }

    /// ANSI-16 baseline with richer palettes on 256/truecolor terminals.
    #[must_use]
    pub fn terminal_classic_for(profile: ColorProfile) -> Self {
        let border = classic_color(profile, (160, 160, 160), Ansi16::BrightBlack);
        let header_fg = classic_color(profile, (245, 245, 245), Ansi16::BrightWhite);
        let header_bg = classic_color(profile, (0, 90, 140), Ansi16::Blue);
        let row_fg = classic_color(profile, (230, 230, 230), Ansi16::White);
        let row_alt_bg = classic_color(profile, (30, 30, 30), Ansi16::Black);
        let selected_bg = classic_color(profile, (160, 90, 10), Ansi16::Yellow);
        let hover_bg = classic_color(profile, (90, 90, 90), Ansi16::BrightBlack);
        let divider = classic_color(profile, (120, 120, 120), Ansi16::BrightBlack);

        Self::build(
            TablePresetId::TerminalClassic,
            ThemeStyles {
                border: Style::new().fg(border),
                header: Style::new().fg(header_fg).bg(header_bg).bold(),
                row: Style::new().fg(row_fg),
                row_alt: Style::new().fg(row_fg).bg(row_alt_bg),
                row_selected: Style::new().fg(PackedRgba::BLACK).bg(selected_bg).bold(),
                row_hover: Style::new().fg(PackedRgba::WHITE).bg(hover_bg),
                divider: Style::new().fg(divider),
            },
        )
    }

    fn build(preset_id: TablePresetId, styles: ThemeStyles) -> Self {
        Self {
            border: styles.border,
            header: styles.header,
            row: styles.row,
            row_alt: styles.row_alt,
            row_selected: styles.row_selected,
            row_hover: styles.row_hover,
            divider: styles.divider,
            padding: 1,
            column_gap: 1,
            row_height: 1,
            effects: Vec::new(),
            preset_id: Some(preset_id),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct EffectSample {
    fg: Option<PackedRgba>,
    bg: Option<PackedRgba>,
    alpha: f32,
}

#[inline]
fn resolve_effects_for_scope(
    theme: &TableTheme,
    base: Style,
    scope: TableEffectScope,
    phase: f32,
) -> Style {
    if theme.effects.is_empty() {
        return base;
    }

    let mut min_priority = u8::MAX;
    let mut max_priority = 0;
    for rule in &theme.effects {
        min_priority = min_priority.min(rule.priority);
        max_priority = max_priority.max(rule.priority);
    }
    if min_priority == u8::MAX {
        return base;
    }

    let mut resolved = base;
    for priority in min_priority..=max_priority {
        for rule in &theme.effects {
            if rule.priority != priority {
                continue;
            }
            if !rule.target.matches_scope(scope) {
                continue;
            }
            resolved = apply_effect_rule(resolved, rule, phase);
        }
    }

    resolved
}

#[inline]
fn apply_effect_rule(mut base: Style, rule: &TableEffectRule, phase: f32) -> Style {
    let sample = sample_effect(&rule.effect, phase);
    let alpha = sample.alpha.clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return base;
    }

    if rule.style_mask.fg {
        base.fg = apply_channel(base.fg, sample.fg, alpha, rule.blend_mode);
    }
    if rule.style_mask.bg {
        base.bg = apply_channel(base.bg, sample.bg, alpha, rule.blend_mode);
    }
    base
}

#[inline]
fn apply_channel(
    base: Option<PackedRgba>,
    effect: Option<PackedRgba>,
    alpha: f32,
    blend_mode: BlendMode,
) -> Option<PackedRgba> {
    let effect = effect?;
    let alpha = alpha.clamp(0.0, 1.0);
    let result = match base {
        Some(base) => blend_with_alpha(base, effect, alpha, blend_mode),
        None => with_alpha(effect, alpha),
    };
    Some(result)
}

#[inline]
fn blend_with_alpha(
    base: PackedRgba,
    effect: PackedRgba,
    alpha: f32,
    blend_mode: BlendMode,
) -> PackedRgba {
    let alpha = alpha.clamp(0.0, 1.0);
    match blend_mode {
        BlendMode::Replace => lerp_color(base, effect, alpha),
        BlendMode::Additive => blend_additive(with_alpha(effect, alpha), base),
        BlendMode::Multiply => blend_multiply(with_alpha(effect, alpha), base),
        BlendMode::Screen => blend_screen(with_alpha(effect, alpha), base),
    }
}

#[inline]
fn sample_effect(effect: &TableEffect, phase: f32) -> EffectSample {
    match *effect {
        TableEffect::Pulse {
            fg_a,
            fg_b,
            bg_a,
            bg_b,
            speed,
            phase_offset,
        } => {
            let t = normalize_phase(phase * speed + phase_offset);
            let alpha = pulse_curve(t);
            EffectSample {
                fg: Some(lerp_color(fg_a, fg_b, alpha)),
                bg: Some(lerp_color(bg_a, bg_b, alpha)),
                alpha: 1.0,
            }
        }
        TableEffect::BreathingGlow {
            fg,
            bg,
            intensity,
            speed,
            phase_offset,
            asymmetry,
        } => {
            let t = normalize_phase(phase * speed + phase_offset);
            let alpha = (breathing_curve(t, asymmetry) * intensity).clamp(0.0, 1.0);
            EffectSample {
                fg: Some(fg),
                bg: Some(bg),
                alpha,
            }
        }
        TableEffect::GradientSweep {
            ref gradient,
            speed,
            phase_offset,
        } => {
            let t = normalize_phase(phase * speed + phase_offset);
            let color = gradient.sample(t);
            EffectSample {
                fg: Some(color),
                bg: Some(color),
                alpha: 1.0,
            }
        }
    }
}

#[inline]
fn normalize_phase(phase: f32) -> f32 {
    phase.rem_euclid(1.0)
}

#[inline]
fn pulse_curve(t: f32) -> f32 {
    0.5 - 0.5 * (std::f32::consts::TAU * t).cos()
}

#[inline]
fn breathing_curve(t: f32, asymmetry: f32) -> f32 {
    let t = skew_phase(t, asymmetry);
    0.5 - 0.5 * (std::f32::consts::TAU * t).cos()
}

#[inline]
fn skew_phase(t: f32, asymmetry: f32) -> f32 {
    let skew = asymmetry.clamp(-0.9, 0.9);
    if skew == 0.0 {
        return t;
    }
    if skew > 0.0 {
        t.powf(1.0 + skew * 2.0)
    } else {
        1.0 - (1.0 - t).powf(1.0 - skew * 2.0)
    }
}

#[inline]
fn with_alpha(color: PackedRgba, alpha: f32) -> PackedRgba {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    PackedRgba::rgba(color.r(), color.g(), color.b(), a)
}

#[inline]
fn blend_additive(top: PackedRgba, bottom: PackedRgba) -> PackedRgba {
    let ta = top.a() as f32 / 255.0;
    let r = (bottom.r() as f32 + top.r() as f32 * ta).min(255.0) as u8;
    let g = (bottom.g() as f32 + top.g() as f32 * ta).min(255.0) as u8;
    let b = (bottom.b() as f32 + top.b() as f32 * ta).min(255.0) as u8;
    let a = bottom.a().max(top.a());
    PackedRgba::rgba(r, g, b, a)
}

#[inline]
fn blend_multiply(top: PackedRgba, bottom: PackedRgba) -> PackedRgba {
    let ta = top.a() as f32 / 255.0;
    let mr = (top.r() as f32 * bottom.r() as f32 / 255.0) as u8;
    let mg = (top.g() as f32 * bottom.g() as f32 / 255.0) as u8;
    let mb = (top.b() as f32 * bottom.b() as f32 / 255.0) as u8;
    let r = (bottom.r() as f32 * (1.0 - ta) + mr as f32 * ta) as u8;
    let g = (bottom.g() as f32 * (1.0 - ta) + mg as f32 * ta) as u8;
    let b = (bottom.b() as f32 * (1.0 - ta) + mb as f32 * ta) as u8;
    let a = bottom.a().max(top.a());
    PackedRgba::rgba(r, g, b, a)
}

#[inline]
fn blend_screen(top: PackedRgba, bottom: PackedRgba) -> PackedRgba {
    let ta = top.a() as f32 / 255.0;
    let sr = 255 - ((255 - top.r()) as u16 * (255 - bottom.r()) as u16 / 255) as u8;
    let sg = 255 - ((255 - top.g()) as u16 * (255 - bottom.g()) as u16 / 255) as u8;
    let sb = 255 - ((255 - top.b()) as u16 * (255 - bottom.b()) as u16 / 255) as u8;
    let r = (bottom.r() as f32 * (1.0 - ta) + sr as f32 * ta) as u8;
    let g = (bottom.g() as f32 * (1.0 - ta) + sg as f32 * ta) as u8;
    let b = (bottom.b() as f32 * (1.0 - ta) + sb as f32 * ta) as u8;
    let a = bottom.a().max(top.a());
    PackedRgba::rgba(r, g, b, a)
}

impl Default for TableTheme {
    fn default() -> Self {
        Self::graphite()
    }
}

#[inline]
fn classic_color(profile: ColorProfile, rgb: (u8, u8, u8), ansi16: Ansi16) -> PackedRgba {
    let color = match profile {
        ColorProfile::Ansi16 => Color::Ansi16(ansi16),
        _ => Color::rgb(rgb.0, rgb.1, rgb.2).downgrade(profile),
    };
    let rgb = color.to_rgb();
    PackedRgba::rgb(rgb.r, rgb.g, rgb.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_mask_default_is_fg_bg() {
        let mask = StyleMask::default();
        assert!(mask.fg);
        assert!(mask.bg);
        assert!(!mask.attrs);
    }

    #[test]
    fn presets_set_preset_id() {
        let theme = TableTheme::aurora();
        assert_eq!(theme.preset_id, Some(TablePresetId::Aurora));
    }

    #[test]
    fn terminal_classic_keeps_profile() {
        let theme = TableTheme::terminal_classic_for(ColorProfile::Ansi16);
        assert_eq!(theme.preset_id, Some(TablePresetId::TerminalClassic));
        assert!(theme.column_gap > 0);
    }
}
