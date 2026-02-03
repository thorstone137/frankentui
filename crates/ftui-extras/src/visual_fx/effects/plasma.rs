#![forbid(unsafe_code)]

//! Plasma backdrop effect (cell-space).
//!
//! Deterministic, no-allocation (steady state), and theme-aware.
//! Uses wave interference patterns for psychedelic visuals.

use crate::visual_fx::{BackdropFx, FxContext, FxQuality, ThemeInputs};
use ftui_render::cell::PackedRgba;

// ---------------------------------------------------------------------------
// Wave Functions
// ---------------------------------------------------------------------------

/// Compute plasma wave value for a single cell.
///
/// Returns a value in `[0.0, 1.0]` representing the wave intensity at the given
/// normalized coordinates `(nx, ny)` where both are in `[0.0, 1.0]`.
///
/// The wave equation uses 6 trigonometric terms:
/// - v1: horizontal wave
/// - v2: vertical wave (slightly offset phase)
/// - v3: diagonal wave
/// - v4: radial wave from center
/// - v5: radial wave with offset center
/// - v6: interference pattern
///
/// # Determinism
///
/// Given identical inputs, this function always returns the same output.
/// No global state or randomness is used.
#[inline]
pub fn plasma_wave(nx: f64, ny: f64, time: f64) -> f64 {
    // Scale normalized coords to wave-space (matches original PlasmaState)
    let x = nx * 6.0;
    let y = ny * 6.0;

    // 6 wave components
    let v1 = (x * 1.5 + time).sin();
    let v2 = (y * 1.8 + time * 0.8).sin();
    let v3 = ((x + y) * 1.2 + time * 0.6).sin();
    let v4 = ((x * x + y * y).sqrt() * 2.0 - time * 1.2).sin();
    let v5 = (((x - 3.0).powi(2) + (y - 3.0).powi(2)).sqrt() * 1.8 + time).cos();
    let v6 = ((x * 2.0).sin() * (y * 2.0).cos() + time * 0.5).sin();

    // Average and normalize from [-1, 1] to [0, 1]
    let value = (v1 + v2 + v3 + v4 + v5 + v6) / 6.0;
    (value + 1.0) / 2.0
}

/// Simplified plasma wave for low-quality rendering.
///
/// Uses only 3 wave components (cheaper but still visually interesting).
#[inline]
pub fn plasma_wave_low(nx: f64, ny: f64, time: f64) -> f64 {
    let x = nx * 6.0;
    let y = ny * 6.0;

    // 3 simplified wave components
    let v1 = (x * 1.5 + time).sin();
    let v2 = (y * 1.8 + time * 0.8).sin();
    let v3 = ((x + y) * 1.2 + time * 0.6).sin();

    let value = (v1 + v2 + v3) / 3.0;
    (value + 1.0) / 2.0
}

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

/// Theme-aware palette presets for plasma.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PlasmaPalette {
    /// Use theme accent colors for the gradient.
    #[default]
    ThemeAccents,
    /// Classic sunset gradient (purple -> pink -> orange -> yellow).
    Sunset,
    /// Ocean gradient (deep blue -> cyan -> seafoam).
    Ocean,
    /// Fire gradient (black -> red -> orange -> yellow -> white).
    Fire,
    /// Neon rainbow (full hue cycle).
    Neon,
    /// Cyberpunk (hot pink -> purple -> cyan).
    Cyberpunk,
}

impl PlasmaPalette {
    /// Map a normalized value [0, 1] to a color.
    pub fn color_at(&self, t: f64, theme: &ThemeInputs) -> PackedRgba {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::ThemeAccents => Self::theme_gradient(t, theme),
            Self::Sunset => Self::sunset(t),
            Self::Ocean => Self::ocean(t),
            Self::Fire => Self::fire(t),
            Self::Neon => Self::neon(t),
            Self::Cyberpunk => Self::cyberpunk(t),
        }
    }

    fn theme_gradient(t: f64, theme: &ThemeInputs) -> PackedRgba {
        // Blend through: bg_base -> accent_primary -> accent_secondary -> fg_primary
        if t < 0.33 {
            let s = t / 0.33;
            Self::lerp_color(theme.bg_surface, theme.accent_primary, s)
        } else if t < 0.66 {
            let s = (t - 0.33) / 0.33;
            Self::lerp_color(theme.accent_primary, theme.accent_secondary, s)
        } else {
            let s = (t - 0.66) / 0.34;
            Self::lerp_color(theme.accent_secondary, theme.fg_primary, s)
        }
    }

    fn sunset(t: f64) -> PackedRgba {
        // Deep purple -> hot pink -> orange -> yellow
        let (r, g, b) = if t < 0.33 {
            let s = t / 0.33;
            Self::lerp_rgb((80, 20, 120), (255, 50, 120), s)
        } else if t < 0.66 {
            let s = (t - 0.33) / 0.33;
            Self::lerp_rgb((255, 50, 120), (255, 150, 50), s)
        } else {
            let s = (t - 0.66) / 0.34;
            Self::lerp_rgb((255, 150, 50), (255, 255, 150), s)
        };
        PackedRgba::rgb(r, g, b)
    }

    fn ocean(t: f64) -> PackedRgba {
        // Deep blue -> cyan -> seafoam
        let (r, g, b) = if t < 0.5 {
            let s = t / 0.5;
            Self::lerp_rgb((10, 30, 100), (30, 180, 220), s)
        } else {
            let s = (t - 0.5) / 0.5;
            Self::lerp_rgb((30, 180, 220), (150, 255, 200), s)
        };
        PackedRgba::rgb(r, g, b)
    }

    fn fire(t: f64) -> PackedRgba {
        // Black -> dark red -> orange -> yellow -> white
        let (r, g, b) = if t < 0.2 {
            let s = t / 0.2;
            Self::lerp_rgb((0, 0, 0), (80, 10, 0), s)
        } else if t < 0.4 {
            let s = (t - 0.2) / 0.2;
            Self::lerp_rgb((80, 10, 0), (200, 50, 0), s)
        } else if t < 0.6 {
            let s = (t - 0.4) / 0.2;
            Self::lerp_rgb((200, 50, 0), (255, 150, 20), s)
        } else if t < 0.8 {
            let s = (t - 0.6) / 0.2;
            Self::lerp_rgb((255, 150, 20), (255, 230, 100), s)
        } else {
            let s = (t - 0.8) / 0.2;
            Self::lerp_rgb((255, 230, 100), (255, 255, 220), s)
        };
        PackedRgba::rgb(r, g, b)
    }

    fn neon(t: f64) -> PackedRgba {
        // Full hue cycle
        let hue = t * 360.0;
        Self::hsv_to_rgb(hue, 1.0, 1.0)
    }

    fn cyberpunk(t: f64) -> PackedRgba {
        // Hot pink -> purple -> cyan
        let (r, g, b) = if t < 0.5 {
            let s = t / 0.5;
            Self::lerp_rgb((255, 20, 150), (150, 50, 200), s)
        } else {
            let s = (t - 0.5) / 0.5;
            Self::lerp_rgb((150, 50, 200), (50, 220, 255), s)
        };
        PackedRgba::rgb(r, g, b)
    }

    #[inline]
    fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
        (
            (a.0 as f64 + (b.0 as f64 - a.0 as f64) * t) as u8,
            (a.1 as f64 + (b.1 as f64 - a.1 as f64) * t) as u8,
            (a.2 as f64 + (b.2 as f64 - a.2 as f64) * t) as u8,
        )
    }

    #[inline]
    fn lerp_color(a: PackedRgba, b: PackedRgba, t: f64) -> PackedRgba {
        let (r, g, blue) = Self::lerp_rgb((a.r(), a.g(), a.b()), (b.r(), b.g(), b.b()), t);
        PackedRgba::rgb(r, g, blue)
    }

    #[inline]
    fn hsv_to_rgb(h: f64, s: f64, v: f64) -> PackedRgba {
        let h = h % 360.0;
        let c = v * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;

        let (r, g, b) = if h < 60.0 {
            (c, x, 0.0)
        } else if h < 120.0 {
            (x, c, 0.0)
        } else if h < 180.0 {
            (0.0, c, x)
        } else if h < 240.0 {
            (0.0, x, c)
        } else if h < 300.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        PackedRgba::rgb(
            ((r + m) * 255.0) as u8,
            ((g + m) * 255.0) as u8,
            ((b + m) * 255.0) as u8,
        )
    }
}

// ---------------------------------------------------------------------------
// PlasmaFx
// ---------------------------------------------------------------------------

/// Procedural plasma backdrop effect.
///
/// Renders a classic plasma effect using multiple overlapping sine waves.
/// The output is deterministic given the same inputs.
///
/// # Quality Tiers
///
/// - `Full`: 6 trigonometric evaluations per cell, full quality
/// - `Reduced`: Same as Full (reserved for future downsampling/skip)
/// - `Minimal`: 3 trigonometric evaluations per cell, simpler waves
/// - `Off`: No rendering (early return)
///
/// # Example
///
/// ```ignore
/// let plasma = PlasmaFx::new(PlasmaPalette::Ocean);
/// let backdrop = Backdrop::new(Box::new(plasma), theme);
/// backdrop.render(area, &mut frame);
/// ```
#[derive(Debug, Clone)]
pub struct PlasmaFx {
    palette: PlasmaPalette,
}

impl PlasmaFx {
    /// Create a new plasma effect with the specified palette.
    #[inline]
    pub const fn new(palette: PlasmaPalette) -> Self {
        Self { palette }
    }

    /// Create a plasma effect using theme colors.
    #[inline]
    pub const fn theme() -> Self {
        Self::new(PlasmaPalette::ThemeAccents)
    }

    /// Create a plasma effect with the sunset palette.
    #[inline]
    pub const fn sunset() -> Self {
        Self::new(PlasmaPalette::Sunset)
    }

    /// Create a plasma effect with the ocean palette.
    #[inline]
    pub const fn ocean() -> Self {
        Self::new(PlasmaPalette::Ocean)
    }

    /// Create a plasma effect with the fire palette.
    #[inline]
    pub const fn fire() -> Self {
        Self::new(PlasmaPalette::Fire)
    }

    /// Create a plasma effect with the neon palette.
    #[inline]
    pub const fn neon() -> Self {
        Self::new(PlasmaPalette::Neon)
    }

    /// Create a plasma effect with the cyberpunk palette.
    #[inline]
    pub const fn cyberpunk() -> Self {
        Self::new(PlasmaPalette::Cyberpunk)
    }

    /// Set the palette.
    #[inline]
    pub fn set_palette(&mut self, palette: PlasmaPalette) {
        self.palette = palette;
    }

    /// Get the current palette.
    #[inline]
    pub const fn palette(&self) -> PlasmaPalette {
        self.palette
    }
}

impl Default for PlasmaFx {
    fn default() -> Self {
        Self::theme()
    }
}

impl BackdropFx for PlasmaFx {
    fn name(&self) -> &'static str {
        "plasma"
    }

    fn render(&mut self, ctx: FxContext<'_>, out: &mut [PackedRgba]) {
        // Early return if quality is Off (decorative effects are non-essential)
        if !ctx.quality.is_enabled() || ctx.is_empty() {
            return;
        }
        debug_assert_eq!(out.len(), ctx.len());

        let w = ctx.width as f64;
        let h = ctx.height as f64;
        let time = ctx.time_seconds;

        // Use simplified wave for Minimal quality
        let use_simplified = ctx.quality == FxQuality::Minimal;

        for dy in 0..ctx.height {
            for dx in 0..ctx.width {
                let idx = dy as usize * ctx.width as usize + dx as usize;

                // Normalized coordinates [0, 1]
                let nx = dx as f64 / w;
                let ny = dy as f64 / h;

                // Compute wave value based on quality
                let wave = if use_simplified {
                    plasma_wave_low(nx, ny, time)
                } else {
                    plasma_wave(nx, ny, time)
                };

                out[idx] = self.palette.color_at(wave, ctx.theme);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(theme: &ThemeInputs) -> FxContext<'_> {
        FxContext {
            width: 24,
            height: 12,
            frame: 1,
            time_seconds: 1.25,
            quality: FxQuality::Full,
            theme,
        }
    }

    #[test]
    fn deterministic_for_fixed_inputs() {
        let theme = ThemeInputs::default_dark();
        let mut fx = PlasmaFx::default();
        let ctx = ctx(&theme);
        let mut out1 = vec![PackedRgba::TRANSPARENT; ctx.len()];
        let mut out2 = vec![PackedRgba::TRANSPARENT; ctx.len()];
        fx.render(ctx, &mut out1);
        fx.render(ctx, &mut out2);
        assert_eq!(out1, out2);
    }

    #[test]
    fn tiny_area_safe() {
        let theme = ThemeInputs::default_dark();
        let mut fx = PlasmaFx::default();

        // Test 0x0
        let ctx = FxContext {
            width: 0,
            height: 0,
            frame: 0,
            time_seconds: 0.0,
            quality: FxQuality::Minimal,
            theme: &theme,
        };
        fx.render(ctx, &mut []);

        // Test 0x10
        let ctx = FxContext {
            width: 0,
            height: 10,
            frame: 0,
            time_seconds: 0.0,
            quality: FxQuality::Minimal,
            theme: &theme,
        };
        fx.render(ctx, &mut []);

        // Test 10x0
        let ctx = FxContext {
            width: 10,
            height: 0,
            frame: 0,
            time_seconds: 0.0,
            quality: FxQuality::Minimal,
            theme: &theme,
        };
        fx.render(ctx, &mut []);

        // Test 1x1
        let ctx = FxContext {
            width: 1,
            height: 1,
            frame: 0,
            time_seconds: 0.0,
            quality: FxQuality::Minimal,
            theme: &theme,
        };
        let mut out = vec![PackedRgba::TRANSPARENT; 1];
        fx.render(ctx, &mut out);
    }

    #[test]
    fn quality_off_does_not_render() {
        let theme = ThemeInputs::default_dark();
        let mut fx = PlasmaFx::default();
        let ctx = FxContext {
            width: 4,
            height: 4,
            frame: 0,
            time_seconds: 1.0,
            quality: FxQuality::Off,
            theme: &theme,
        };
        let mut out = vec![PackedRgba::TRANSPARENT; 16];
        fx.render(ctx, &mut out);
        // Should remain unchanged (TRANSPARENT)
        assert!(out.iter().all(|c| *c == PackedRgba::TRANSPARENT));
    }

    #[test]
    fn all_palettes_render_without_panic() {
        let theme = ThemeInputs::default_dark();
        let ctx = ctx(&theme);
        let mut out = vec![PackedRgba::TRANSPARENT; ctx.len()];

        for palette in [
            PlasmaPalette::ThemeAccents,
            PlasmaPalette::Sunset,
            PlasmaPalette::Ocean,
            PlasmaPalette::Fire,
            PlasmaPalette::Neon,
            PlasmaPalette::Cyberpunk,
        ] {
            let mut fx = PlasmaFx::new(palette);
            fx.render(ctx, &mut out);
        }
    }

    #[test]
    fn wave_output_in_valid_range() {
        // Test that plasma_wave always returns values in [0, 1]
        for nx in [0.0, 0.25, 0.5, 0.75, 1.0] {
            for ny in [0.0, 0.25, 0.5, 0.75, 1.0] {
                for time in [0.0, 1.0, 10.0, 100.0] {
                    let v = plasma_wave(nx, ny, time);
                    assert!(
                        (0.0..=1.0).contains(&v),
                        "plasma_wave({nx}, {ny}, {time}) = {v} out of range"
                    );

                    let v_low = plasma_wave_low(nx, ny, time);
                    assert!(
                        (0.0..=1.0).contains(&v_low),
                        "plasma_wave_low({nx}, {ny}, {time}) = {v_low} out of range"
                    );
                }
            }
        }
    }

    #[test]
    fn minimal_quality_uses_simplified_wave() {
        let theme = ThemeInputs::default_dark();
        let mut fx = PlasmaFx::default();

        // Render with Full quality
        let ctx_full = FxContext {
            width: 8,
            height: 8,
            frame: 0,
            time_seconds: 1.0,
            quality: FxQuality::Full,
            theme: &theme,
        };
        let mut out_full = vec![PackedRgba::TRANSPARENT; 64];
        fx.render(ctx_full, &mut out_full);

        // Render with Minimal quality
        let ctx_min = FxContext {
            width: 8,
            height: 8,
            frame: 0,
            time_seconds: 1.0,
            quality: FxQuality::Minimal,
            theme: &theme,
        };
        let mut out_min = vec![PackedRgba::TRANSPARENT; 64];
        fx.render(ctx_min, &mut out_min);

        // They should differ (different wave functions)
        assert_ne!(out_full, out_min);
    }
}
