//! ANSI escape sequence parser using the `vte` crate.
//!
//! This module provides a callback-based ANSI parser that dispatches parsed
//! sequences to an [`AnsiHandler`] implementation.
//!
//! # Invariants
//!
//! 1. **Complete UTF-8**: The parser correctly handles multi-byte UTF-8 sequences.
//! 2. **Sequence isolation**: Each escape sequence is fully parsed before dispatch.
//! 3. **State recovery**: Malformed sequences return to ground state gracefully.
//!
//! # Failure Modes
//!
//! | Failure | Cause | Behavior |
//! |---------|-------|----------|
//! | Invalid UTF-8 | Corrupted input | Replacement character dispatched |
//! | Unknown CSI | Unrecognized sequence | Silently ignored |
//! | Truncated sequence | Incomplete input | Buffered for next parse call |

use vte::{Parser, Perform};

/// Handler trait for ANSI escape sequence events.
///
/// Implement this trait to receive parsed ANSI events from [`AnsiParser`].
///
/// # Example
///
/// ```ignore
/// use ftui_extras::terminal::AnsiHandler;
///
/// struct MyTerminal {
///     cursor_x: u16,
///     cursor_y: u16,
/// }
///
/// impl AnsiHandler for MyTerminal {
///     fn print(&mut self, c: char) {
///         // Handle printable character
///     }
///
///     fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], c: char) {
///         match c {
///             'A' => self.cursor_y = self.cursor_y.saturating_sub(params.first().copied().unwrap_or(1) as u16),
///             'B' => self.cursor_y += params.first().copied().unwrap_or(1) as u16,
///             _ => {}
///         }
///     }
///     // ... other methods
/// }
/// ```
pub trait AnsiHandler {
    /// Handle a printable character.
    ///
    /// Called for each printable Unicode character in the input stream.
    fn print(&mut self, c: char);

    /// Handle a C0/C1 control code.
    ///
    /// Common codes:
    /// - `0x07` (BEL): Bell
    /// - `0x08` (BS): Backspace
    /// - `0x09` (HT): Horizontal tab
    /// - `0x0A` (LF): Line feed
    /// - `0x0D` (CR): Carriage return
    fn execute(&mut self, byte: u8);

    /// Handle a CSI (Control Sequence Introducer) sequence.
    ///
    /// CSI sequences start with `ESC [` and are the primary mechanism for
    /// cursor movement, text styling, and screen manipulation.
    ///
    /// # Arguments
    ///
    /// * `params` - Numeric parameters (semicolon-separated in the sequence)
    /// * `intermediates` - Intermediate bytes (e.g., `?` for DEC private modes)
    /// * `c` - The final byte that identifies the command
    ///
    /// # Common Commands
    ///
    /// | Final | Meaning |
    /// |-------|---------|
    /// | `A` | Cursor up |
    /// | `B` | Cursor down |
    /// | `C` | Cursor forward |
    /// | `D` | Cursor back |
    /// | `H` | Cursor position |
    /// | `J` | Erase display |
    /// | `K` | Erase line |
    /// | `m` | SGR (Select Graphic Rendition) |
    /// | `h` | Set mode (with `?` for DEC private) |
    /// | `l` | Reset mode (with `?` for DEC private) |
    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], c: char);

    /// Handle an OSC (Operating System Command) sequence.
    ///
    /// OSC sequences start with `ESC ]` and are used for things like
    /// setting the window title or clipboard operations.
    ///
    /// # Arguments
    ///
    /// * `params` - The parsed OSC parameters (semicolon-separated strings)
    ///
    /// # Common OSC Commands
    ///
    /// | Code | Meaning |
    /// |------|---------|
    /// | 0 | Set icon name and window title |
    /// | 2 | Set window title |
    /// | 52 | Clipboard operations |
    fn osc_dispatch(&mut self, params: &[&[u8]]);

    /// Handle an ESC sequence (non-CSI, non-OSC).
    ///
    /// # Arguments
    ///
    /// * `intermediates` - Intermediate bytes between ESC and final byte
    /// * `c` - The final byte
    ///
    /// # Common Sequences
    ///
    /// | Sequence | Meaning |
    /// |----------|---------|
    /// | `ESC 7` | Save cursor (DECSC) |
    /// | `ESC 8` | Restore cursor (DECRC) |
    /// | `ESC D` | Index (move down, scroll if needed) |
    /// | `ESC M` | Reverse index (move up, scroll if needed) |
    /// | `ESC c` | Full reset (RIS) |
    fn esc_dispatch(&mut self, intermediates: &[u8], c: char);

    /// Handle a DCS (Device Control String) hook.
    ///
    /// Called when entering a DCS sequence. Override if you need to handle
    /// sixel graphics or other DCS data.
    #[allow(unused_variables)]
    fn hook(&mut self, params: &[i64], intermediates: &[u8], c: char) {
        // Default: ignore
    }

    /// Handle DCS data bytes.
    #[allow(unused_variables)]
    fn put(&mut self, byte: u8) {
        // Default: ignore
    }

    /// Handle DCS sequence end.
    fn unhook(&mut self) {
        // Default: ignore
    }
}

/// Adapter that bridges vte's `Perform` trait to our `AnsiHandler` trait.
struct VteAdapter<'a, H: AnsiHandler> {
    handler: &'a mut H,
}

impl<H: AnsiHandler> Perform for VteAdapter<'_, H> {
    fn print(&mut self, c: char) {
        self.handler.print(c);
    }

    fn execute(&mut self, byte: u8) {
        self.handler.execute(byte);
    }

    fn csi_dispatch(&mut self, params: &vte::Params, intermediates: &[u8], _ignore: bool, c: char) {
        // Convert vte::Params to Vec<i64>
        let params: Vec<i64> = params
            .iter()
            .map(|subparams| {
                // Take the first value of each subparam group (handles colon-separated params)
                subparams.first().copied().map(i64::from).unwrap_or(0)
            })
            .collect();

        self.handler.csi_dispatch(&params, intermediates, c);
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        self.handler.osc_dispatch(params);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        self.handler.esc_dispatch(intermediates, byte as char);
    }

    fn hook(&mut self, params: &vte::Params, intermediates: &[u8], _ignore: bool, c: char) {
        let params: Vec<i64> = params
            .iter()
            .map(|subparams| subparams.first().copied().map(i64::from).unwrap_or(0))
            .collect();
        self.handler.hook(&params, intermediates, c);
    }

    fn put(&mut self, byte: u8) {
        self.handler.put(byte);
    }

    fn unhook(&mut self) {
        self.handler.unhook();
    }
}

/// ANSI escape sequence parser.
///
/// Wraps the `vte` crate's parser and dispatches events to an [`AnsiHandler`].
///
/// # Example
///
/// ```ignore
/// use ftui_extras::terminal::{AnsiParser, AnsiHandler};
///
/// struct MyHandler;
/// impl AnsiHandler for MyHandler {
///     fn print(&mut self, c: char) { print!("{}", c); }
///     fn execute(&mut self, byte: u8) {}
///     fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], c: char) {}
///     fn osc_dispatch(&mut self, params: &[&[u8]]) {}
///     fn esc_dispatch(&mut self, intermediates: &[u8], c: char) {}
/// }
///
/// let mut parser = AnsiParser::new();
/// let mut handler = MyHandler;
/// parser.parse(b"\x1b[31mHello\x1b[0m", &mut handler);
/// ```
pub struct AnsiParser {
    inner: Parser,
}

impl Default for AnsiParser {
    fn default() -> Self {
        Self::new()
    }
}

impl AnsiParser {
    /// Create a new ANSI parser.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Parser::new(),
        }
    }

    /// Parse bytes and dispatch events to the handler.
    ///
    /// This method can be called repeatedly with chunks of data. The parser
    /// maintains state between calls to handle sequences that span chunks.
    pub fn parse<H: AnsiHandler>(&mut self, data: &[u8], handler: &mut H) {
        let mut adapter = VteAdapter { handler };
        for &byte in data {
            self.inner.advance(&mut adapter, byte);
        }
    }

    /// Reset the parser to initial state.
    ///
    /// Call this after a protocol error or when starting a new parsing session.
    pub fn reset(&mut self) {
        self.inner = Parser::new();
    }
}

impl std::fmt::Debug for AnsiParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnsiParser").finish_non_exhaustive()
    }
}

/// SGR (Select Graphic Rendition) parameter values.
///
/// These constants map to the numeric parameters used in `ESC [ ... m` sequences.
pub mod sgr {
    /// Reset all attributes
    pub const RESET: i64 = 0;
    /// Bold/bright
    pub const BOLD: i64 = 1;
    /// Dim/faint
    pub const DIM: i64 = 2;
    /// Italic
    pub const ITALIC: i64 = 3;
    /// Underline
    pub const UNDERLINE: i64 = 4;
    /// Slow blink
    pub const BLINK: i64 = 5;
    /// Reverse video
    pub const REVERSE: i64 = 7;
    /// Hidden/invisible
    pub const HIDDEN: i64 = 8;
    /// Strikethrough
    pub const STRIKETHROUGH: i64 = 9;

    /// Reset bold/dim
    pub const NORMAL_INTENSITY: i64 = 22;
    /// Reset italic
    pub const NO_ITALIC: i64 = 23;
    /// Reset underline
    pub const NO_UNDERLINE: i64 = 24;
    /// Reset blink
    pub const NO_BLINK: i64 = 25;
    /// Reset reverse
    pub const NO_REVERSE: i64 = 27;
    /// Reset hidden
    pub const NO_HIDDEN: i64 = 28;
    /// Reset strikethrough
    pub const NO_STRIKETHROUGH: i64 = 29;

    /// Black foreground
    pub const FG_BLACK: i64 = 30;
    /// Red foreground
    pub const FG_RED: i64 = 31;
    /// Green foreground
    pub const FG_GREEN: i64 = 32;
    /// Yellow foreground
    pub const FG_YELLOW: i64 = 33;
    /// Blue foreground
    pub const FG_BLUE: i64 = 34;
    /// Magenta foreground
    pub const FG_MAGENTA: i64 = 35;
    /// Cyan foreground
    pub const FG_CYAN: i64 = 36;
    /// White foreground
    pub const FG_WHITE: i64 = 37;
    /// Extended foreground color (256 or RGB)
    pub const FG_EXTENDED: i64 = 38;
    /// Default foreground
    pub const FG_DEFAULT: i64 = 39;

    /// Black background
    pub const BG_BLACK: i64 = 40;
    /// Red background
    pub const BG_RED: i64 = 41;
    /// Green background
    pub const BG_GREEN: i64 = 42;
    /// Yellow background
    pub const BG_YELLOW: i64 = 43;
    /// Blue background
    pub const BG_BLUE: i64 = 44;
    /// Magenta background
    pub const BG_MAGENTA: i64 = 45;
    /// Cyan background
    pub const BG_CYAN: i64 = 46;
    /// White background
    pub const BG_WHITE: i64 = 47;
    /// Extended background color (256 or RGB)
    pub const BG_EXTENDED: i64 = 48;
    /// Default background
    pub const BG_DEFAULT: i64 = 49;

    /// Bright black foreground
    pub const FG_BRIGHT_BLACK: i64 = 90;
    /// Bright red foreground
    pub const FG_BRIGHT_RED: i64 = 91;
    /// Bright green foreground
    pub const FG_BRIGHT_GREEN: i64 = 92;
    /// Bright yellow foreground
    pub const FG_BRIGHT_YELLOW: i64 = 93;
    /// Bright blue foreground
    pub const FG_BRIGHT_BLUE: i64 = 94;
    /// Bright magenta foreground
    pub const FG_BRIGHT_MAGENTA: i64 = 95;
    /// Bright cyan foreground
    pub const FG_BRIGHT_CYAN: i64 = 96;
    /// Bright white foreground
    pub const FG_BRIGHT_WHITE: i64 = 97;

    /// Bright black background
    pub const BG_BRIGHT_BLACK: i64 = 100;
    /// Bright red background
    pub const BG_BRIGHT_RED: i64 = 101;
    /// Bright green background
    pub const BG_BRIGHT_GREEN: i64 = 102;
    /// Bright yellow background
    pub const BG_BRIGHT_YELLOW: i64 = 103;
    /// Bright blue background
    pub const BG_BRIGHT_BLUE: i64 = 104;
    /// Bright magenta background
    pub const BG_BRIGHT_MAGENTA: i64 = 105;
    /// Bright cyan background
    pub const BG_BRIGHT_CYAN: i64 = 106;
    /// Bright white background
    pub const BG_BRIGHT_WHITE: i64 = 107;

    /// 256-color mode indicator (used after FG_EXTENDED or BG_EXTENDED)
    pub const COLOR_256: i64 = 5;
    /// RGB color mode indicator (used after FG_EXTENDED or BG_EXTENDED)
    pub const COLOR_RGB: i64 = 2;
}

/// DEC private mode numbers (used with CSI ? h/l sequences).
pub mod dec_mode {
    /// Cursor visible
    pub const CURSOR_VISIBLE: i64 = 25;
    /// Alternate screen buffer
    pub const ALT_SCREEN: i64 = 1049;
    /// Alternate screen (no save/restore)
    pub const ALT_SCREEN_NO_CLEAR: i64 = 1047;
    /// Save cursor before alt screen
    pub const SAVE_CURSOR: i64 = 1048;
    /// Mouse tracking: normal
    pub const MOUSE_TRACKING: i64 = 1000;
    /// Mouse tracking: button events
    pub const MOUSE_BUTTON: i64 = 1002;
    /// Mouse tracking: any event
    pub const MOUSE_ANY: i64 = 1003;
    /// Mouse tracking: SGR extended mode
    pub const MOUSE_SGR: i64 = 1006;
    /// Focus events
    pub const FOCUS: i64 = 1004;
    /// Bracketed paste mode
    pub const BRACKETED_PASTE: i64 = 2004;
}

/// Parse SGR parameters into attribute changes.
///
/// This is a helper for implementing `csi_dispatch` when `c == 'm'`.
///
/// # Returns
///
/// An iterator over `SgrChange` values describing the attribute changes.
pub fn parse_sgr(params: &[i64]) -> impl Iterator<Item = SgrChange> + '_ {
    SgrIterator::new(params)
}

/// A single SGR attribute change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SgrChange {
    /// Reset all attributes
    Reset,
    /// Set bold
    Bold(bool),
    /// Set dim
    Dim(bool),
    /// Set italic
    Italic(bool),
    /// Set underline
    Underline(bool),
    /// Set blink
    Blink(bool),
    /// Set reverse video
    Reverse(bool),
    /// Set hidden
    Hidden(bool),
    /// Set strikethrough
    Strikethrough(bool),
    /// Set foreground to ANSI color (0-7)
    FgAnsi(u8),
    /// Set foreground to bright ANSI color (0-7 maps to 8-15)
    FgBrightAnsi(u8),
    /// Set foreground to 256-color palette
    Fg256(u8),
    /// Set foreground to RGB
    FgRgb(u8, u8, u8),
    /// Reset foreground to default
    FgDefault,
    /// Set background to ANSI color (0-7)
    BgAnsi(u8),
    /// Set background to bright ANSI color (0-7 maps to 8-15)
    BgBrightAnsi(u8),
    /// Set background to 256-color palette
    Bg256(u8),
    /// Set background to RGB
    BgRgb(u8, u8, u8),
    /// Reset background to default
    BgDefault,
}

/// Iterator over SGR changes.
struct SgrIterator<'a> {
    params: &'a [i64],
    index: usize,
}

impl<'a> SgrIterator<'a> {
    fn new(params: &'a [i64]) -> Self {
        Self { params, index: 0 }
    }

    fn next_param(&mut self) -> Option<i64> {
        if self.index < self.params.len() {
            let val = self.params[self.index];
            self.index += 1;
            Some(val)
        } else {
            None
        }
    }

    fn parse_extended_color(&mut self) -> Option<SgrChange> {
        let mode = self.next_param()?;
        match mode {
            5 => {
                // 256-color mode: 38;5;N or 48;5;N
                let color = self.next_param()?;
                Some(SgrChange::Fg256(color as u8))
            }
            2 => {
                // RGB mode: 38;2;R;G;B or 48;2;R;G;B
                let r = self.next_param()?;
                let g = self.next_param()?;
                let b = self.next_param()?;
                Some(SgrChange::FgRgb(r as u8, g as u8, b as u8))
            }
            _ => None,
        }
    }

    fn parse_extended_bg_color(&mut self) -> Option<SgrChange> {
        let mode = self.next_param()?;
        match mode {
            5 => {
                let color = self.next_param()?;
                Some(SgrChange::Bg256(color as u8))
            }
            2 => {
                let r = self.next_param()?;
                let g = self.next_param()?;
                let b = self.next_param()?;
                Some(SgrChange::BgRgb(r as u8, g as u8, b as u8))
            }
            _ => None,
        }
    }
}

impl Iterator for SgrIterator<'_> {
    type Item = SgrChange;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let param = self.next_param()?;

            let change = match param {
                0 => SgrChange::Reset,
                1 => SgrChange::Bold(true),
                2 => SgrChange::Dim(true),
                3 => SgrChange::Italic(true),
                4 => SgrChange::Underline(true),
                5 => SgrChange::Blink(true),
                7 => SgrChange::Reverse(true),
                8 => SgrChange::Hidden(true),
                9 => SgrChange::Strikethrough(true),
                22 => {
                    // Reset both bold and dim
                    return Some(SgrChange::Bold(false));
                }
                23 => SgrChange::Italic(false),
                24 => SgrChange::Underline(false),
                25 => SgrChange::Blink(false),
                27 => SgrChange::Reverse(false),
                28 => SgrChange::Hidden(false),
                29 => SgrChange::Strikethrough(false),
                30..=37 => SgrChange::FgAnsi((param - 30) as u8),
                38 => {
                    if let Some(change) = self.parse_extended_color() {
                        return Some(change);
                    }
                    continue;
                }
                39 => SgrChange::FgDefault,
                40..=47 => SgrChange::BgAnsi((param - 40) as u8),
                48 => {
                    if let Some(change) = self.parse_extended_bg_color() {
                        return Some(change);
                    }
                    continue;
                }
                49 => SgrChange::BgDefault,
                90..=97 => SgrChange::FgBrightAnsi((param - 90) as u8),
                100..=107 => SgrChange::BgBrightAnsi((param - 100) as u8),
                _ => continue, // Unknown SGR parameter
            };

            return Some(change);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Test handler that records all events.
    #[derive(Default)]
    struct TestHandler {
        printed: RefCell<Vec<char>>,
        executed: RefCell<Vec<u8>>,
        csi_calls: RefCell<Vec<(Vec<i64>, Vec<u8>, char)>>,
        osc_calls: RefCell<Vec<Vec<Vec<u8>>>>,
        esc_calls: RefCell<Vec<(Vec<u8>, char)>>,
    }

    impl AnsiHandler for TestHandler {
        fn print(&mut self, c: char) {
            self.printed.borrow_mut().push(c);
        }

        fn execute(&mut self, byte: u8) {
            self.executed.borrow_mut().push(byte);
        }

        fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], c: char) {
            self.csi_calls
                .borrow_mut()
                .push((params.to_vec(), intermediates.to_vec(), c));
        }

        fn osc_dispatch(&mut self, params: &[&[u8]]) {
            self.osc_calls
                .borrow_mut()
                .push(params.iter().map(|p| p.to_vec()).collect());
        }

        fn esc_dispatch(&mut self, intermediates: &[u8], c: char) {
            self.esc_calls
                .borrow_mut()
                .push((intermediates.to_vec(), c));
        }
    }

    #[test]
    fn parse_plain_text() {
        let mut parser = AnsiParser::new();
        let mut handler = TestHandler::default();

        parser.parse(b"Hello", &mut handler);

        let printed: Vec<char> = handler.printed.borrow().clone();
        assert_eq!(printed, vec!['H', 'e', 'l', 'l', 'o']);
    }

    #[test]
    fn parse_control_codes() {
        let mut parser = AnsiParser::new();
        let mut handler = TestHandler::default();

        parser.parse(b"A\nB\rC\tD", &mut handler);

        let printed: Vec<char> = handler.printed.borrow().clone();
        assert_eq!(printed, vec!['A', 'B', 'C', 'D']);

        let executed: Vec<u8> = handler.executed.borrow().clone();
        assert_eq!(executed, vec![b'\n', b'\r', b'\t']);
    }

    #[test]
    fn parse_csi_cursor_up() {
        let mut parser = AnsiParser::new();
        let mut handler = TestHandler::default();

        // ESC [ 5 A - cursor up 5
        parser.parse(b"\x1b[5A", &mut handler);

        let csi_calls = handler.csi_calls.borrow();
        assert_eq!(csi_calls.len(), 1);
        assert_eq!(csi_calls[0].0, vec![5]);
        assert_eq!(csi_calls[0].2, 'A');
    }

    #[test]
    fn parse_csi_cursor_position() {
        let mut parser = AnsiParser::new();
        let mut handler = TestHandler::default();

        // ESC [ 10 ; 20 H - cursor to row 10, col 20
        parser.parse(b"\x1b[10;20H", &mut handler);

        let csi_calls = handler.csi_calls.borrow();
        assert_eq!(csi_calls.len(), 1);
        assert_eq!(csi_calls[0].0, vec![10, 20]);
        assert_eq!(csi_calls[0].2, 'H');
    }

    #[test]
    fn parse_csi_sgr_colors() {
        let mut parser = AnsiParser::new();
        let mut handler = TestHandler::default();

        // ESC [ 1 ; 31 m - bold red
        parser.parse(b"\x1b[1;31m", &mut handler);

        let csi_calls = handler.csi_calls.borrow();
        assert_eq!(csi_calls.len(), 1);
        assert_eq!(csi_calls[0].0, vec![1, 31]);
        assert_eq!(csi_calls[0].2, 'm');
    }

    #[test]
    fn parse_csi_dec_private_mode() {
        let mut parser = AnsiParser::new();
        let mut handler = TestHandler::default();

        // ESC [ ? 25 h - show cursor
        parser.parse(b"\x1b[?25h", &mut handler);

        let csi_calls = handler.csi_calls.borrow();
        assert_eq!(csi_calls.len(), 1);
        assert_eq!(csi_calls[0].1, vec![b'?']);
        assert_eq!(csi_calls[0].2, 'h');
    }

    #[test]
    fn parse_osc_title() {
        let mut parser = AnsiParser::new();
        let mut handler = TestHandler::default();

        // OSC 0 ; title BEL
        parser.parse(b"\x1b]0;My Title\x07", &mut handler);

        let osc_calls = handler.osc_calls.borrow();
        assert_eq!(osc_calls.len(), 1);
        assert_eq!(osc_calls[0].len(), 2);
        assert_eq!(osc_calls[0][0], b"0");
        assert_eq!(osc_calls[0][1], b"My Title");
    }

    #[test]
    fn parse_esc_save_cursor() {
        let mut parser = AnsiParser::new();
        let mut handler = TestHandler::default();

        // ESC 7 - save cursor
        parser.parse(b"\x1b7", &mut handler);

        let esc_calls = handler.esc_calls.borrow();
        assert_eq!(esc_calls.len(), 1);
        assert_eq!(esc_calls[0].1, '7');
    }

    #[test]
    fn parse_mixed_content() {
        let mut parser = AnsiParser::new();
        let mut handler = TestHandler::default();

        parser.parse(b"\x1b[31mHello\x1b[0m World", &mut handler);

        let printed: Vec<char> = handler.printed.borrow().clone();
        assert_eq!(
            printed,
            vec!['H', 'e', 'l', 'l', 'o', ' ', 'W', 'o', 'r', 'l', 'd']
        );

        let csi_calls = handler.csi_calls.borrow();
        assert_eq!(csi_calls.len(), 2); // [31m and [0m
    }

    #[test]
    fn parse_utf8() {
        let mut parser = AnsiParser::new();
        let mut handler = TestHandler::default();

        parser.parse("Hello, 世界! 🎉".as_bytes(), &mut handler);

        let printed: Vec<char> = handler.printed.borrow().clone();
        assert!(printed.contains(&'世'));
        assert!(printed.contains(&'界'));
        assert!(printed.contains(&'🎉'));
    }

    #[test]
    fn parse_incomplete_sequence_buffered() {
        let mut parser = AnsiParser::new();
        let mut handler = TestHandler::default();

        // Send partial sequence
        parser.parse(b"\x1b[1", &mut handler);
        assert!(handler.csi_calls.borrow().is_empty());

        // Complete the sequence
        parser.parse(b";31m", &mut handler);
        assert_eq!(handler.csi_calls.borrow().len(), 1);
    }

    #[test]
    fn reset_clears_state() {
        let mut parser = AnsiParser::new();
        let mut handler = TestHandler::default();

        // Start a sequence
        parser.parse(b"\x1b[1", &mut handler);

        // Reset
        parser.reset();

        // New sequence should work
        parser.parse(b"\x1b[5A", &mut handler);

        let csi_calls = handler.csi_calls.borrow();
        assert_eq!(csi_calls.len(), 1);
        assert_eq!(csi_calls[0].0, vec![5]);
    }

    // ── SGR parsing tests ─────────────────────────────────────────────

    #[test]
    fn sgr_reset() {
        let changes: Vec<_> = parse_sgr(&[0]).collect();
        assert_eq!(changes, vec![SgrChange::Reset]);
    }

    #[test]
    fn sgr_bold_italic() {
        let changes: Vec<_> = parse_sgr(&[1, 3]).collect();
        assert_eq!(
            changes,
            vec![SgrChange::Bold(true), SgrChange::Italic(true)]
        );
    }

    #[test]
    fn sgr_fg_color() {
        let changes: Vec<_> = parse_sgr(&[31]).collect();
        assert_eq!(changes, vec![SgrChange::FgAnsi(1)]);
    }

    #[test]
    fn sgr_256_color() {
        let changes: Vec<_> = parse_sgr(&[38, 5, 196]).collect();
        assert_eq!(changes, vec![SgrChange::Fg256(196)]);
    }

    #[test]
    fn sgr_rgb_color() {
        let changes: Vec<_> = parse_sgr(&[38, 2, 100, 150, 200]).collect();
        assert_eq!(changes, vec![SgrChange::FgRgb(100, 150, 200)]);
    }

    #[test]
    fn sgr_bg_256_color() {
        let changes: Vec<_> = parse_sgr(&[48, 5, 21]).collect();
        assert_eq!(changes, vec![SgrChange::Bg256(21)]);
    }

    #[test]
    fn sgr_bg_rgb_color() {
        let changes: Vec<_> = parse_sgr(&[48, 2, 50, 100, 150]).collect();
        assert_eq!(changes, vec![SgrChange::BgRgb(50, 100, 150)]);
    }

    #[test]
    fn sgr_bright_colors() {
        let changes: Vec<_> = parse_sgr(&[91, 101]).collect();
        assert_eq!(
            changes,
            vec![SgrChange::FgBrightAnsi(1), SgrChange::BgBrightAnsi(1)]
        );
    }

    #[test]
    fn sgr_complex_sequence() {
        // Bold, red fg, blue bg, reset
        let changes: Vec<_> = parse_sgr(&[1, 31, 44, 0]).collect();
        assert_eq!(
            changes,
            vec![
                SgrChange::Bold(true),
                SgrChange::FgAnsi(1),
                SgrChange::BgAnsi(4),
                SgrChange::Reset,
            ]
        );
    }

    #[test]
    fn sgr_empty_treated_as_reset() {
        // Empty params should produce no changes (vte handles this at a higher level)
        let changes: Vec<_> = parse_sgr(&[]).collect();
        assert!(changes.is_empty());
    }

    #[test]
    fn sgr_defaults() {
        let changes: Vec<_> = parse_sgr(&[39, 49]).collect();
        assert_eq!(changes, vec![SgrChange::FgDefault, SgrChange::BgDefault]);
    }
}
