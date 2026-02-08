//! VT/ANSI parser (API skeleton).
//!
//! This parser is a minimal, deterministic state machine that converts an
//! output byte stream into a sequence of actions for the terminal engine.
//!
//! In the full implementation, this will cover CSI/OSC/DCS/APC and a VT support
//! matrix. For the crate skeleton, we focus on:
//!
//! - printable ASCII -> `Action::Print`
//! - a small set of C0 controls -> dedicated actions
//! - capture of raw escape sequences as `Action::Escape` for later decoding

/// Parser output actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Print a single character.
    Print(char),
    /// Line feed / newline (`\n`).
    Newline,
    /// Carriage return (`\r`).
    CarriageReturn,
    /// Horizontal tab (`\t`).
    Tab,
    /// Backspace (`\x08`).
    Backspace,
    /// Bell (`\x07`).
    Bell,
    /// A raw escape/CSI/OSC sequence captured verbatim (starts with ESC).
    Escape(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Esc,
    Csi,
    Osc,
    OscEsc,
}

/// VT/ANSI parser state.
#[derive(Debug, Clone)]
pub struct Parser {
    state: State,
    buf: Vec<u8>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    /// Create a new parser in ground state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            buf: Vec::new(),
        }
    }

    /// Feed a chunk of bytes and return parsed actions.
    #[must_use]
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Action> {
        let mut out = Vec::new();
        for &b in bytes {
            if let Some(action) = self.advance(b) {
                out.push(action);
            }
        }
        out
    }

    /// Advance the parser by one byte.
    ///
    /// Returns an action when a complete token is recognized.
    pub fn advance(&mut self, b: u8) -> Option<Action> {
        match self.state {
            State::Ground => self.advance_ground(b),
            State::Esc => self.advance_esc(b),
            State::Csi => self.advance_csi(b),
            State::Osc => self.advance_osc(b),
            State::OscEsc => self.advance_osc_esc(b),
        }
    }

    fn advance_ground(&mut self, b: u8) -> Option<Action> {
        match b {
            b'\n' => Some(Action::Newline),
            b'\r' => Some(Action::CarriageReturn),
            b'\t' => Some(Action::Tab),
            0x08 => Some(Action::Backspace),
            0x07 => Some(Action::Bell),
            0x1b => {
                self.state = State::Esc;
                self.buf.clear();
                self.buf.push(0x1b);
                None
            }
            0x20..=0x7E => Some(Action::Print(b as char)),
            _ => None, // ignore other control bytes in the skeleton
        }
    }

    fn advance_esc(&mut self, b: u8) -> Option<Action> {
        self.buf.push(b);
        match b {
            b'[' => {
                self.state = State::Csi;
                None
            }
            b']' => {
                self.state = State::Osc;
                None
            }
            _ => {
                self.state = State::Ground;
                Some(Action::Escape(self.take_buf()))
            }
        }
    }

    fn advance_csi(&mut self, b: u8) -> Option<Action> {
        self.buf.push(b);
        // Final byte for CSI is in the 0x40..=0x7E range (ECMA-48).
        if (0x40..=0x7E).contains(&b) {
            self.state = State::Ground;
            return Some(Action::Escape(self.take_buf()));
        }
        None
    }

    fn advance_osc(&mut self, b: u8) -> Option<Action> {
        self.buf.push(b);
        match b {
            0x07 => {
                // BEL terminator.
                self.state = State::Ground;
                Some(Action::Escape(self.take_buf()))
            }
            0x1b => {
                // ESC, possibly starting ST terminator (ESC \).
                self.state = State::OscEsc;
                None
            }
            _ => None,
        }
    }

    fn advance_osc_esc(&mut self, b: u8) -> Option<Action> {
        self.buf.push(b);
        if b == b'\\' {
            // ST terminator.
            self.state = State::Ground;
            return Some(Action::Escape(self.take_buf()));
        }
        // False alarm; continue OSC.
        self.state = State::Osc;
        None
    }

    fn take_buf(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        core::mem::swap(&mut out, &mut self.buf);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printable_ascii_emits_print() {
        let mut p = Parser::new();
        let actions = p.feed(b"hi");
        assert_eq!(actions, vec![Action::Print('h'), Action::Print('i')]);
    }

    #[test]
    fn c0_controls_emit_actions() {
        let mut p = Parser::new();
        let actions = p.feed(b"\t\r\n");
        assert_eq!(
            actions,
            vec![Action::Tab, Action::CarriageReturn, Action::Newline]
        );
    }

    #[test]
    fn csi_sequence_is_captured_as_escape() {
        let mut p = Parser::new();
        let actions = p.feed(b"\x1b[31m");
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            Action::Escape(seq) if seq.as_slice() == b"\x1b[31m"
        ));
    }

    #[test]
    fn osc_sequence_bel_terminated_is_captured() {
        let mut p = Parser::new();
        let actions = p.feed(b"\x1b]0;title\x07");
        assert_eq!(actions.len(), 1);
        assert!(matches!(
            &actions[0],
            Action::Escape(seq) if seq.starts_with(b"\x1b]0;")
        ));
    }
}
