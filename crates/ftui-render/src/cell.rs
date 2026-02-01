#![forbid(unsafe_code)]

//! Cell types and invariants.
//!
//! The `Cell` is the fundamental unit of the terminal grid. Each cell occupies
//! exactly **16 bytes** to ensure optimal cache utilization (4 cells per 64-byte
//! cache line) and enable fast SIMD comparisons.
//!
//! # Layout (16 bytes, non-negotiable)
//!
//! ```text
//! Cell {
//!     content: CellContent,  // 4 bytes - char or GraphemeId
//!     fg: PackedRgba,        // 4 bytes - foreground color
//!     bg: PackedRgba,        // 4 bytes - background color
//!     attrs: CellAttrs,      // 4 bytes - style flags + link ID
//! }
//! ```
//!
//! # Why 16 Bytes?
//!
//! - 4 cells per 64-byte cache line (perfect fit)
//! - Single 128-bit SIMD comparison
//! - No heap allocation for 99% of cells
//! - 24 bytes wastes cache, 32 bytes doubles bandwidth

use unicode_width::UnicodeWidthChar;

/// Grapheme ID: reference to an interned string in [`GraphemePool`].
///
/// # Layout
///
/// ```text
/// [30-24: width (7 bits)][23-0: pool slot (24 bits)]
/// ```
///
/// # Capacity
///
/// - Pool slots: 16,777,216 (24 bits = 16M entries)
/// - Width range: 0-127 (7 bits, plenty for any display width)
///
/// # Design Rationale
///
/// - 24 bits for slot allows 16M unique graphemes (far exceeding practical usage)
/// - 7 bits for width allows display widths 0-127 (most graphemes are 1-2)
/// - Embedded width avoids pool lookup for width queries
/// - Total 31 bits leaves bit 31 for `CellContent` type discrimination
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct GraphemeId(u32);

impl GraphemeId {
    /// Maximum slot index (24 bits).
    pub const MAX_SLOT: u32 = 0x00FF_FFFF;

    /// Maximum width (7 bits).
    pub const MAX_WIDTH: u8 = 127;

    /// Create a new `GraphemeId` from slot index and display width.
    ///
    /// # Panics
    ///
    /// Panics in debug mode if `slot > MAX_SLOT` or `width > MAX_WIDTH`.
    #[inline]
    pub const fn new(slot: u32, width: u8) -> Self {
        debug_assert!(slot <= Self::MAX_SLOT, "slot overflow");
        debug_assert!(width <= Self::MAX_WIDTH, "width overflow");
        Self((slot & Self::MAX_SLOT) | ((width as u32) << 24))
    }

    /// Extract the pool slot index (0-16M).
    #[inline]
    pub const fn slot(self) -> usize {
        (self.0 & Self::MAX_SLOT) as usize
    }

    /// Extract the display width (0-127).
    #[inline]
    pub const fn width(self) -> usize {
        ((self.0 >> 24) & 0x7F) as usize
    }

    /// Raw u32 value for storage in `CellContent`.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Reconstruct from a raw u32.
    #[inline]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

impl core::fmt::Debug for GraphemeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GraphemeId")
            .field("slot", &self.slot())
            .field("width", &self.width())
            .finish()
    }
}

/// Cell content: either a direct Unicode char or a reference to a grapheme cluster.
///
/// # Encoding Scheme (4 bytes)
///
/// ```text
/// Bit 31 (type discriminator):
///   0: Direct char (bits 0-20 contain Unicode scalar value, max U+10FFFF)
///   1: GraphemeId reference (bits 0-30 contain slot + width)
/// ```
///
/// This allows:
/// - 99% of cells (ASCII/BMP) to be stored without heap allocation
/// - Complex graphemes (emoji, ZWJ sequences) stored in pool
///
/// # Special Values
///
/// - `EMPTY` (0x0): Empty cell, width 0
/// - `CONTINUATION` (0x1): Placeholder for wide character continuation
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct CellContent(u32);

impl CellContent {
    /// Empty cell content (no character).
    pub const EMPTY: Self = Self(0);

    /// Continuation marker for wide characters.
    ///
    /// When a character has display width > 1, subsequent cells are filled
    /// with this marker to indicate they are part of the previous character.
    pub const CONTINUATION: Self = Self(1);

    /// Create content from a single Unicode character.
    ///
    /// For characters with display width > 1, subsequent cells should be
    /// filled with `CONTINUATION`.
    #[inline]
    pub const fn from_char(c: char) -> Self {
        Self(c as u32)
    }

    /// Create content from a grapheme ID (for multi-codepoint clusters).
    ///
    /// The grapheme ID references an entry in the `GraphemePool`.
    #[inline]
    pub const fn from_grapheme(id: GraphemeId) -> Self {
        Self(0x8000_0000 | id.raw())
    }

    /// Check if this content is a grapheme reference (vs direct char).
    #[inline]
    pub const fn is_grapheme(self) -> bool {
        self.0 & 0x8000_0000 != 0
    }

    /// Check if this is a continuation cell (part of a wide character).
    #[inline]
    pub const fn is_continuation(self) -> bool {
        self.0 == Self::CONTINUATION.0
    }

    /// Check if this cell is empty.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == Self::EMPTY.0
    }

    /// Extract the character if this is a direct char (not a grapheme).
    ///
    /// Returns `None` if this is a grapheme reference.
    #[inline]
    pub fn as_char(self) -> Option<char> {
        if self.is_grapheme() {
            None
        } else {
            char::from_u32(self.0)
        }
    }

    /// Extract the grapheme ID if this is a grapheme reference.
    ///
    /// Returns `None` if this is a direct char.
    #[inline]
    pub const fn grapheme_id(self) -> Option<GraphemeId> {
        if self.is_grapheme() {
            Some(GraphemeId::from_raw(self.0 & !0x8000_0000))
        } else {
            None
        }
    }

    /// Get the display width of this content.
    ///
    /// - Empty: 0
    /// - Continuation: 0
    /// - Grapheme: width embedded in GraphemeId
    /// - Char: requires external width lookup (returns 1 as default for ASCII)
    ///
    /// Note: For accurate char width, use unicode-width crate externally.
    /// This method provides a fast path for known cases.
    #[inline]
    pub const fn width_hint(self) -> usize {
        if self.is_empty() || self.is_continuation() {
            0
        } else if self.is_grapheme() {
            ((self.0 >> 24) & 0x7F) as usize
        } else {
            // For direct chars, assume width 1 (fast path for ASCII)
            // Callers should use unicode-width for accurate measurement
            1
        }
    }

    /// Get the display width of this content with Unicode width semantics.
    ///
    /// This is the accurate (but slower) width computation for direct chars.
    #[inline]
    pub fn width(self) -> usize {
        if self.is_empty() || self.is_continuation() {
            0
        } else if self.is_grapheme() {
            ((self.0 >> 24) & 0x7F) as usize
        } else {
            self.as_char()
                .and_then(UnicodeWidthChar::width)
                .unwrap_or(1)
        }
    }

    /// Raw u32 value.
    #[inline]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl Default for CellContent {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl core::fmt::Debug for CellContent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.is_empty() {
            write!(f, "CellContent::EMPTY")
        } else if self.is_continuation() {
            write!(f, "CellContent::CONTINUATION")
        } else if let Some(c) = self.as_char() {
            write!(f, "CellContent::Char({c:?})")
        } else if let Some(id) = self.grapheme_id() {
            write!(f, "CellContent::Grapheme({id:?})")
        } else {
            write!(f, "CellContent(0x{:08x})", self.0)
        }
    }
}

/// A single terminal cell (16 bytes).
///
/// # Layout
///
/// ```text
/// #[repr(C, align(16))]
/// Cell {
///     content: CellContent,  // 4 bytes
///     fg: PackedRgba,        // 4 bytes
///     bg: PackedRgba,        // 4 bytes
///     attrs: CellAttrs,      // 4 bytes
/// }
/// ```
///
/// # Invariants
///
/// - Size is exactly 16 bytes (verified by compile-time assert)
/// - All fields are valid (no uninitialized memory)
/// - Continuation cells should not have meaningful fg/bg (they inherit from parent)
///
/// # Default
///
/// The default cell is empty with transparent background, white foreground,
/// and no style attributes.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C, align(16))]
pub struct Cell {
    /// Character or grapheme content.
    pub content: CellContent,
    /// Foreground color.
    pub fg: PackedRgba,
    /// Background color.
    pub bg: PackedRgba,
    /// Style flags and hyperlink ID.
    pub attrs: CellAttrs,
}

// Compile-time size check
const _: () = assert!(core::mem::size_of::<Cell>() == 16);

impl Cell {
    /// A continuation cell (placeholder for wide characters).
    ///
    /// When a character has display width > 1, subsequent cells are filled
    /// with this to indicate they are "owned" by the previous cell.
    pub const CONTINUATION: Self = Self {
        content: CellContent::CONTINUATION,
        fg: PackedRgba::TRANSPARENT,
        bg: PackedRgba::TRANSPARENT,
        attrs: CellAttrs::NONE,
    };

    /// Create a new cell with the given content and default colors.
    #[inline]
    pub const fn new(content: CellContent) -> Self {
        Self {
            content,
            fg: PackedRgba::WHITE,
            bg: PackedRgba::TRANSPARENT,
            attrs: CellAttrs::NONE,
        }
    }

    /// Create a cell from a single character.
    #[inline]
    pub const fn from_char(c: char) -> Self {
        Self::new(CellContent::from_char(c))
    }

    /// Check if this is a continuation cell.
    #[inline]
    pub const fn is_continuation(&self) -> bool {
        self.content.is_continuation()
    }

    /// Check if this cell is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Get the display width hint for this cell.
    ///
    /// See [`CellContent::width_hint`] for details.
    #[inline]
    pub const fn width_hint(&self) -> usize {
        self.content.width_hint()
    }

    /// Bitwise equality comparison (fast path for diffing).
    ///
    /// This compares the raw bytes of two cells, which is faster than
    /// field-by-field comparison for bulk operations.
    #[inline]
    pub fn bits_eq(&self, other: &Self) -> bool {
        // Safe because Cell is repr(C) with no padding
        self.content.raw() == other.content.raw()
            && self.fg == other.fg
            && self.bg == other.bg
            && self.attrs == other.attrs
    }

    /// Set the foreground color.
    #[inline]
    pub const fn with_fg(mut self, fg: PackedRgba) -> Self {
        self.fg = fg;
        self
    }

    /// Set the background color.
    #[inline]
    pub const fn with_bg(mut self, bg: PackedRgba) -> Self {
        self.bg = bg;
        self
    }

    /// Set the style attributes.
    #[inline]
    pub const fn with_attrs(mut self, attrs: CellAttrs) -> Self {
        self.attrs = attrs;
        self
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            content: CellContent::EMPTY,
            fg: PackedRgba::WHITE,
            bg: PackedRgba::TRANSPARENT,
            attrs: CellAttrs::NONE,
        }
    }
}

impl core::fmt::Debug for Cell {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Cell")
            .field("content", &self.content)
            .field("fg", &self.fg)
            .field("bg", &self.bg)
            .field("attrs", &self.attrs)
            .finish()
    }
}

/// A compact RGBA color.
///
/// - **Size:** 4 bytes (fits within the `Cell` 16-byte budget).
/// - **Layout:** `0xRRGGBBAA` (R in bits 31..24, A in bits 7..0).
///
/// Notes
/// -----
/// This is **straight alpha** storage (RGB channels are not pre-multiplied).
/// Compositing uses Porter-Duff **SourceOver** (`src over dst`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(transparent)]
pub struct PackedRgba(pub u32);

impl PackedRgba {
    pub const TRANSPARENT: Self = Self(0);
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    #[inline]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }

    #[inline]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self(((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | (a as u32))
    }

    #[inline]
    pub const fn r(self) -> u8 {
        (self.0 >> 24) as u8
    }

    #[inline]
    pub const fn g(self) -> u8 {
        (self.0 >> 16) as u8
    }

    #[inline]
    pub const fn b(self) -> u8 {
        (self.0 >> 8) as u8
    }

    #[inline]
    pub const fn a(self) -> u8 {
        self.0 as u8
    }

    #[inline]
    const fn div_round_u8(numer: u64, denom: u64) -> u8 {
        debug_assert!(denom != 0);
        let v = (numer + (denom / 2)) / denom;
        if v > 255 { 255 } else { v as u8 }
    }

    /// Porter-Duff SourceOver: `src over dst`.
    ///
    /// Stored as straight alpha, so we compute the exact rational form and round at the end
    /// (avoids accumulating rounding error across intermediate steps).
    #[inline]
    pub fn over(self, dst: Self) -> Self {
        let s_a = self.a() as u64;
        if s_a == 255 {
            return self;
        }
        if s_a == 0 {
            return dst;
        }

        let d_a = dst.a() as u64;
        let inv_s_a = 255 - s_a;

        // out_a = s_a + d_a*(1 - s_a)  (all in [0,1], scaled by 255)
        // We compute numer_a in the "255^2 domain" to keep channels exact:
        // numer_a = 255*s_a + d_a*(255 - s_a)
        // out_a_u8 = round(numer_a / 255)
        let numer_a = 255 * s_a + d_a * inv_s_a;
        if numer_a == 0 {
            return Self::TRANSPARENT;
        }

        let out_a = Self::div_round_u8(numer_a, 255);

        // For straight alpha, the exact rational (scaled to [0,255]) is:
        // out_c_u8 = round( (src_c*s_a*255 + dst_c*d_a*(255 - s_a)) / numer_a )
        let r = Self::div_round_u8(
            (self.r() as u64) * s_a * 255 + (dst.r() as u64) * d_a * inv_s_a,
            numer_a,
        );
        let g = Self::div_round_u8(
            (self.g() as u64) * s_a * 255 + (dst.g() as u64) * d_a * inv_s_a,
            numer_a,
        );
        let b = Self::div_round_u8(
            (self.b() as u64) * s_a * 255 + (dst.b() as u64) * d_a * inv_s_a,
            numer_a,
        );

        Self::rgba(r, g, b, out_a)
    }

    /// Apply uniform opacity in `[0.0, 1.0]` by scaling alpha.
    #[inline]
    pub fn with_opacity(self, opacity: f32) -> Self {
        let opacity = opacity.clamp(0.0, 1.0);
        let a = ((self.a() as f32) * opacity).round().clamp(0.0, 255.0) as u8;
        Self::rgba(self.r(), self.g(), self.b(), a)
    }
}

bitflags::bitflags! {
    /// 8-bit cell style flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct StyleFlags: u8 {
        const BOLD          = 0b0000_0001;
        const DIM           = 0b0000_0010;
        const ITALIC        = 0b0000_0100;
        const UNDERLINE     = 0b0000_1000;
        const BLINK         = 0b0001_0000;
        const REVERSE       = 0b0010_0000;
        const STRIKETHROUGH = 0b0100_0000;
        const HIDDEN        = 0b1000_0000;
    }
}

/// Packed cell attributes:
/// - bits 31..24: `StyleFlags` (8 bits)
/// - bits 23..0: `link_id` (24 bits)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[repr(transparent)]
pub struct CellAttrs(u32);

impl CellAttrs {
    pub const NONE: Self = Self(0);

    pub const LINK_ID_NONE: u32 = 0;
    pub const LINK_ID_MAX: u32 = 0x00FF_FFFE;

    #[inline]
    pub fn new(flags: StyleFlags, link_id: u32) -> Self {
        debug_assert!(
            link_id <= Self::LINK_ID_MAX,
            "link_id overflow: {link_id} (max={})",
            Self::LINK_ID_MAX
        );
        Self(((flags.bits() as u32) << 24) | (link_id & 0x00FF_FFFF))
    }

    #[inline]
    pub fn flags(self) -> StyleFlags {
        StyleFlags::from_bits_truncate((self.0 >> 24) as u8)
    }

    #[inline]
    pub fn link_id(self) -> u32 {
        self.0 & 0x00FF_FFFF
    }

    #[inline]
    pub fn with_flags(self, flags: StyleFlags) -> Self {
        Self((self.0 & 0x00FF_FFFF) | ((flags.bits() as u32) << 24))
    }

    #[inline]
    pub fn with_link(self, link_id: u32) -> Self {
        debug_assert!(
            link_id <= Self::LINK_ID_MAX,
            "link_id overflow: {link_id} (max={})",
            Self::LINK_ID_MAX
        );
        Self((self.0 & 0xFF00_0000) | (link_id & 0x00FF_FFFF))
    }

    #[inline]
    pub fn has_flag(self, flag: StyleFlags) -> bool {
        self.flags().contains(flag)
    }
}

#[cfg(test)]
mod tests {
    use super::{Cell, CellAttrs, CellContent, GraphemeId, PackedRgba, StyleFlags};

    fn reference_over(src: PackedRgba, dst: PackedRgba) -> PackedRgba {
        let sr = src.r() as f64 / 255.0;
        let sg = src.g() as f64 / 255.0;
        let sb = src.b() as f64 / 255.0;
        let sa = src.a() as f64 / 255.0;

        let dr = dst.r() as f64 / 255.0;
        let dg = dst.g() as f64 / 255.0;
        let db = dst.b() as f64 / 255.0;
        let da = dst.a() as f64 / 255.0;

        let out_a = sa + da * (1.0 - sa);
        if out_a <= 0.0 {
            return PackedRgba::TRANSPARENT;
        }

        let out_r = (sr * sa + dr * da * (1.0 - sa)) / out_a;
        let out_g = (sg * sa + dg * da * (1.0 - sa)) / out_a;
        let out_b = (sb * sa + db * da * (1.0 - sa)) / out_a;

        let to_u8 = |x: f64| -> u8 { (x * 255.0).round().clamp(0.0, 255.0) as u8 };
        PackedRgba::rgba(to_u8(out_r), to_u8(out_g), to_u8(out_b), to_u8(out_a))
    }

    #[test]
    fn packed_rgba_is_4_bytes() {
        assert_eq!(core::mem::size_of::<PackedRgba>(), 4);
    }

    #[test]
    fn rgb_sets_alpha_to_255() {
        let c = PackedRgba::rgb(1, 2, 3);
        assert_eq!(c.r(), 1);
        assert_eq!(c.g(), 2);
        assert_eq!(c.b(), 3);
        assert_eq!(c.a(), 255);
    }

    #[test]
    fn rgba_round_trips_components() {
        let c = PackedRgba::rgba(10, 20, 30, 40);
        assert_eq!(c.r(), 10);
        assert_eq!(c.g(), 20);
        assert_eq!(c.b(), 30);
        assert_eq!(c.a(), 40);
    }

    #[test]
    fn over_with_opaque_src_returns_src() {
        let src = PackedRgba::rgba(1, 2, 3, 255);
        let dst = PackedRgba::rgba(9, 8, 7, 200);
        assert_eq!(src.over(dst), src);
    }

    #[test]
    fn over_with_transparent_src_returns_dst() {
        let src = PackedRgba::TRANSPARENT;
        let dst = PackedRgba::rgba(9, 8, 7, 200);
        assert_eq!(src.over(dst), dst);
    }

    #[test]
    fn over_blends_correctly_for_half_alpha_over_opaque() {
        // 50% red over opaque blue -> purple-ish, and resulting alpha stays opaque.
        let src = PackedRgba::rgba(255, 0, 0, 128);
        let dst = PackedRgba::rgba(0, 0, 255, 255);
        assert_eq!(src.over(dst), PackedRgba::rgba(128, 0, 127, 255));
    }

    #[test]
    fn over_matches_reference_for_partial_alpha_cases() {
        let cases = [
            (
                PackedRgba::rgba(200, 10, 10, 64),
                PackedRgba::rgba(10, 200, 10, 128),
            ),
            (
                PackedRgba::rgba(1, 2, 3, 1),
                PackedRgba::rgba(250, 251, 252, 254),
            ),
            (
                PackedRgba::rgba(100, 0, 200, 200),
                PackedRgba::rgba(0, 120, 30, 50),
            ),
        ];

        for (src, dst) in cases {
            assert_eq!(src.over(dst), reference_over(src, dst));
        }
    }

    #[test]
    fn with_opacity_scales_alpha() {
        let c = PackedRgba::rgba(10, 20, 30, 255);
        assert_eq!(c.with_opacity(0.5).a(), 128);
        assert_eq!(c.with_opacity(-1.0).a(), 0);
        assert_eq!(c.with_opacity(2.0).a(), 255);
    }

    #[test]
    fn cell_attrs_is_4_bytes() {
        assert_eq!(core::mem::size_of::<CellAttrs>(), 4);
    }

    #[test]
    fn cell_attrs_none_has_no_flags_and_no_link() {
        assert!(CellAttrs::NONE.flags().is_empty());
        assert_eq!(CellAttrs::NONE.link_id(), 0);
    }

    #[test]
    fn cell_attrs_new_stores_flags_and_link() {
        let flags = StyleFlags::BOLD | StyleFlags::ITALIC;
        let a = CellAttrs::new(flags, 42);
        assert_eq!(a.flags(), flags);
        assert_eq!(a.link_id(), 42);
    }

    #[test]
    fn cell_attrs_with_flags_preserves_link_id() {
        let a = CellAttrs::new(StyleFlags::BOLD, 123);
        let b = a.with_flags(StyleFlags::UNDERLINE);
        assert_eq!(b.flags(), StyleFlags::UNDERLINE);
        assert_eq!(b.link_id(), 123);
    }

    #[test]
    fn cell_attrs_with_link_preserves_flags() {
        let a = CellAttrs::new(StyleFlags::BOLD | StyleFlags::ITALIC, 1);
        let b = a.with_link(999);
        assert_eq!(b.flags(), StyleFlags::BOLD | StyleFlags::ITALIC);
        assert_eq!(b.link_id(), 999);
    }

    #[test]
    fn cell_attrs_flag_combinations_work() {
        let flags = StyleFlags::BOLD | StyleFlags::ITALIC;
        let a = CellAttrs::new(flags, 0);
        assert!(a.has_flag(StyleFlags::BOLD));
        assert!(a.has_flag(StyleFlags::ITALIC));
        assert!(!a.has_flag(StyleFlags::UNDERLINE));
    }

    #[test]
    fn cell_attrs_link_id_max_boundary() {
        let a = CellAttrs::new(StyleFlags::empty(), CellAttrs::LINK_ID_MAX);
        assert_eq!(a.link_id(), CellAttrs::LINK_ID_MAX);
    }

    // ====== GraphemeId tests ======

    #[test]
    fn grapheme_id_is_4_bytes() {
        assert_eq!(core::mem::size_of::<GraphemeId>(), 4);
    }

    #[test]
    fn grapheme_id_encoding_roundtrip() {
        let id = GraphemeId::new(12345, 2);
        assert_eq!(id.slot(), 12345);
        assert_eq!(id.width(), 2);
    }

    #[test]
    fn grapheme_id_max_values() {
        let id = GraphemeId::new(GraphemeId::MAX_SLOT, GraphemeId::MAX_WIDTH);
        assert_eq!(id.slot(), 0x00FF_FFFF);
        assert_eq!(id.width(), 127);
    }

    #[test]
    fn grapheme_id_zero_values() {
        let id = GraphemeId::new(0, 0);
        assert_eq!(id.slot(), 0);
        assert_eq!(id.width(), 0);
    }

    #[test]
    fn grapheme_id_raw_roundtrip() {
        let id = GraphemeId::new(999, 5);
        let raw = id.raw();
        let restored = GraphemeId::from_raw(raw);
        assert_eq!(restored.slot(), 999);
        assert_eq!(restored.width(), 5);
    }

    // ====== CellContent tests ======

    #[test]
    fn cell_content_is_4_bytes() {
        assert_eq!(core::mem::size_of::<CellContent>(), 4);
    }

    #[test]
    fn cell_content_empty_properties() {
        assert!(CellContent::EMPTY.is_empty());
        assert!(!CellContent::EMPTY.is_continuation());
        assert!(!CellContent::EMPTY.is_grapheme());
        assert_eq!(CellContent::EMPTY.width_hint(), 0);
    }

    #[test]
    fn cell_content_continuation_properties() {
        assert!(CellContent::CONTINUATION.is_continuation());
        assert!(!CellContent::CONTINUATION.is_empty());
        assert!(!CellContent::CONTINUATION.is_grapheme());
        assert_eq!(CellContent::CONTINUATION.width_hint(), 0);
    }

    #[test]
    fn cell_content_from_char_ascii() {
        let c = CellContent::from_char('A');
        assert!(!c.is_grapheme());
        assert!(!c.is_empty());
        assert!(!c.is_continuation());
        assert_eq!(c.as_char(), Some('A'));
        assert_eq!(c.width_hint(), 1);
    }

    #[test]
    fn cell_content_from_char_unicode() {
        // BMP character
        let c = CellContent::from_char('日');
        assert_eq!(c.as_char(), Some('日'));
        assert!(!c.is_grapheme());

        // Supplementary plane character (emoji)
        let c2 = CellContent::from_char('🎉');
        assert_eq!(c2.as_char(), Some('🎉'));
        assert!(!c2.is_grapheme());
    }

    #[test]
    fn cell_content_from_grapheme() {
        let id = GraphemeId::new(42, 2);
        let c = CellContent::from_grapheme(id);

        assert!(c.is_grapheme());
        assert!(!c.is_empty());
        assert!(!c.is_continuation());
        assert_eq!(c.grapheme_id(), Some(id));
        assert_eq!(c.as_char(), None);
        assert_eq!(c.width_hint(), 2);
    }

    #[test]
    fn cell_content_width_for_chars() {
        let ascii = CellContent::from_char('A');
        assert_eq!(ascii.width(), 1);

        let wide = CellContent::from_char('日');
        assert_eq!(wide.width(), 2);

        let emoji = CellContent::from_char('🎉');
        assert_eq!(emoji.width(), 2);
    }

    #[test]
    fn cell_content_width_for_grapheme() {
        let id = GraphemeId::new(7, 3);
        let c = CellContent::from_grapheme(id);
        assert_eq!(c.width(), 3);
    }

    #[test]
    fn cell_content_width_empty_is_zero() {
        assert_eq!(CellContent::EMPTY.width(), 0);
        assert_eq!(CellContent::CONTINUATION.width(), 0);
    }

    #[test]
    fn cell_content_grapheme_discriminator_bit() {
        // Chars should have bit 31 = 0
        let char_content = CellContent::from_char('X');
        assert_eq!(char_content.raw() & 0x8000_0000, 0);

        // Graphemes should have bit 31 = 1
        let grapheme_content = CellContent::from_grapheme(GraphemeId::new(1, 1));
        assert_ne!(grapheme_content.raw() & 0x8000_0000, 0);
    }

    // ====== Cell tests ======

    #[test]
    fn cell_is_16_bytes() {
        assert_eq!(core::mem::size_of::<Cell>(), 16);
    }

    #[test]
    fn cell_alignment_is_16() {
        assert_eq!(core::mem::align_of::<Cell>(), 16);
    }

    #[test]
    fn cell_default_properties() {
        let cell = Cell::default();
        assert!(cell.is_empty());
        assert!(!cell.is_continuation());
        assert_eq!(cell.fg, PackedRgba::WHITE);
        assert_eq!(cell.bg, PackedRgba::TRANSPARENT);
        assert_eq!(cell.attrs, CellAttrs::NONE);
    }

    #[test]
    fn cell_continuation_constant() {
        assert!(Cell::CONTINUATION.is_continuation());
        assert!(!Cell::CONTINUATION.is_empty());
    }

    #[test]
    fn cell_from_char() {
        let cell = Cell::from_char('X');
        assert_eq!(cell.content.as_char(), Some('X'));
        assert_eq!(cell.fg, PackedRgba::WHITE);
        assert_eq!(cell.bg, PackedRgba::TRANSPARENT);
    }

    #[test]
    fn cell_builder_methods() {
        let cell = Cell::from_char('A')
            .with_fg(PackedRgba::rgb(255, 0, 0))
            .with_bg(PackedRgba::rgb(0, 0, 255))
            .with_attrs(CellAttrs::new(StyleFlags::BOLD, 0));

        assert_eq!(cell.content.as_char(), Some('A'));
        assert_eq!(cell.fg, PackedRgba::rgb(255, 0, 0));
        assert_eq!(cell.bg, PackedRgba::rgb(0, 0, 255));
        assert!(cell.attrs.has_flag(StyleFlags::BOLD));
    }

    #[test]
    fn cell_bits_eq_same_cells() {
        let cell1 = Cell::from_char('X').with_fg(PackedRgba::rgb(1, 2, 3));
        let cell2 = Cell::from_char('X').with_fg(PackedRgba::rgb(1, 2, 3));
        assert!(cell1.bits_eq(&cell2));
    }

    #[test]
    fn cell_bits_eq_different_cells() {
        let cell1 = Cell::from_char('X');
        let cell2 = Cell::from_char('Y');
        assert!(!cell1.bits_eq(&cell2));

        let cell3 = Cell::from_char('X').with_fg(PackedRgba::rgb(1, 2, 3));
        assert!(!cell1.bits_eq(&cell3));
    }

    #[test]
    fn cell_width_hint() {
        let empty = Cell::default();
        assert_eq!(empty.width_hint(), 0);

        let cont = Cell::CONTINUATION;
        assert_eq!(cont.width_hint(), 0);

        let ascii = Cell::from_char('A');
        assert_eq!(ascii.width_hint(), 1);
    }
}
