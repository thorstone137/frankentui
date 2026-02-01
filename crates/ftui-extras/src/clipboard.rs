#![forbid(unsafe_code)]

use std::env;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ftui_core::terminal_capabilities::TerminalCapabilities;

const ENV_CLIPBOARD_BACKEND: &str = "FTUI_CLIPBOARD_BACKEND";

/// OSC 52 clipboard selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardSelection {
    /// System clipboard.
    Clipboard,
    /// Primary selection (X11).
    Primary,
    /// Secondary selection (X11).
    Secondary,
    /// Cut buffer 0..=7.
    CutBuffer(u8),
}

impl ClipboardSelection {
    fn osc52_code(self) -> Result<char, ClipboardError> {
        match self {
            Self::Clipboard => Ok('c'),
            Self::Primary => Ok('p'),
            Self::Secondary => Ok('s'),
            Self::CutBuffer(index) if index <= 7 => Ok((b'0' + index) as char),
            Self::CutBuffer(index) => Err(ClipboardError::InvalidInput(format!(
                "cut buffer index must be 0..=7 (got {index})",
            ))),
        }
    }
}

/// Clipboard backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardBackend {
    Osc52,
    External(ExternalBackend),
    Unavailable,
}

/// External clipboard mechanisms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalBackend {
    MacOS,
    Windows,
    Wayland,
    X11,
}

/// Clipboard errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    NotAvailable,
    InvalidInput(String),
    WriteError(String),
    ReadError(String),
    Timeout,
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAvailable => write!(f, "clipboard not available"),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::WriteError(msg) => write!(f, "clipboard write failed: {msg}"),
            Self::ReadError(msg) => write!(f, "clipboard read failed: {msg}"),
            Self::Timeout => write!(f, "clipboard read timed out"),
        }
    }
}

impl std::error::Error for ClipboardError {}

/// Cross-terminal clipboard helper.
#[derive(Debug, Clone)]
pub struct Clipboard {
    caps: TerminalCapabilities,
    backend: ClipboardBackend,
    fallback: Option<ExternalBackend>,
    max_payload: usize,
    osc52_timeout: Duration,
}

impl Clipboard {
    /// Common OSC 52 size limit (base64 payload bytes).
    pub const DEFAULT_MAX_OSC52_PAYLOAD: usize = 74_994;
    /// Default OSC 52 response timeout.
    pub const DEFAULT_OSC52_TIMEOUT: Duration = Duration::from_millis(100);

    /// Create a clipboard helper using OSC 52 only.
    ///
    /// For auto-detection across terminals, use [`detect`](Self::detect).
    #[must_use]
    pub const fn new(caps: TerminalCapabilities) -> Self {
        let backend = if caps.osc52_clipboard {
            ClipboardBackend::Osc52
        } else {
            ClipboardBackend::Unavailable
        };
        Self {
            caps,
            backend,
            fallback: None,
            max_payload: Self::DEFAULT_MAX_OSC52_PAYLOAD,
            osc52_timeout: Self::DEFAULT_OSC52_TIMEOUT,
        }
    }

    /// Auto-detect the best available clipboard backend.
    #[must_use]
    pub fn detect(caps: TerminalCapabilities) -> Self {
        let external = detect_external_backend();
        let (backend, fallback) = if caps.osc52_clipboard {
            (ClipboardBackend::Osc52, external)
        } else if let Some(ext) = external {
            (ClipboardBackend::External(ext), None)
        } else {
            (ClipboardBackend::Unavailable, None)
        };

        let (backend, fallback) = apply_backend_override(caps, backend, fallback);

        log_detected(backend);

        Self {
            caps,
            backend,
            fallback,
            max_payload: Self::DEFAULT_MAX_OSC52_PAYLOAD,
            osc52_timeout: Self::DEFAULT_OSC52_TIMEOUT,
        }
    }

    /// Create a clipboard helper with a custom OSC 52 payload limit.
    #[must_use]
    pub fn with_max_payload(caps: TerminalCapabilities, max_payload: usize) -> Self {
        let mut clipboard = Self::new(caps);
        clipboard.max_payload = if max_payload == 0 {
            Self::DEFAULT_MAX_OSC52_PAYLOAD
        } else {
            max_payload
        };
        clipboard
    }

    /// Return the selected backend.
    #[must_use]
    pub const fn backend(&self) -> ClipboardBackend {
        self.backend
    }

    /// Return true when any clipboard backend is available.
    #[must_use]
    pub const fn is_available(&self) -> bool {
        !matches!(self.backend, ClipboardBackend::Unavailable)
    }

    /// Return true if OSC 52 is supported.
    #[must_use]
    pub const fn supports_osc52(&self) -> bool {
        self.caps.osc52_clipboard
    }

    /// Return the maximum allowed OSC 52 base64 payload size.
    #[must_use]
    pub const fn max_payload(&self) -> usize {
        self.max_payload
    }

    /// Set clipboard content.
    pub fn set(
        &self,
        content: &str,
        selection: ClipboardSelection,
        writer: &mut impl Write,
    ) -> Result<(), ClipboardError> {
        match self.backend {
            ClipboardBackend::Osc52 => {
                let result = self.write_osc52(content, selection, writer);
                if result.is_err() {
                    if let Some(fallback) = self.fallback {
                        log_fallback(self.backend, ClipboardBackend::External(fallback), "osc52");
                        return set_external_backend(fallback, content, selection);
                    }
                }
                log_write(self.backend, content.len());
                result
            }
            ClipboardBackend::External(backend) => {
                log_write(self.backend, content.len());
                set_external_backend(backend, content, selection)
            }
            ClipboardBackend::Unavailable => Err(ClipboardError::NotAvailable),
        }
    }

    /// Set clipboard content with the fallback chain enabled.
    #[cfg(feature = "clipboard-fallback")]
    pub fn set_with_fallback(
        &self,
        content: &str,
        selection: ClipboardSelection,
        writer: &mut impl Write,
    ) -> Result<(), ClipboardError> {
        self.set(content, selection, writer)
    }

    /// Clear clipboard content.
    pub fn clear(
        &self,
        selection: ClipboardSelection,
        writer: &mut impl Write,
    ) -> Result<(), ClipboardError> {
        match self.backend {
            ClipboardBackend::Osc52 => {
                let code = selection.osc52_code()?;
                write!(writer, "\x1b]52;{};\x07", code)
                    .map_err(|err| ClipboardError::WriteError(err.to_string()))?;
                writer
                    .flush()
                    .map_err(|err| ClipboardError::WriteError(err.to_string()))
            }
            ClipboardBackend::External(backend) => set_external_backend(backend, "", selection),
            ClipboardBackend::Unavailable => Err(ClipboardError::NotAvailable),
        }
    }

    /// Read clipboard content with a default timeout for OSC 52.
    pub fn get(&self) -> Result<String, ClipboardError> {
        self.get_with_timeout(self.osc52_timeout)
    }

    /// Read clipboard content with a custom timeout for OSC 52.
    ///
    /// Note: OSC 52 read responses require terminal support. If unavailable, we
    /// fall back to external clipboard tools when possible.
    pub fn get_with_timeout(&self, _timeout: Duration) -> Result<String, ClipboardError> {
        match self.backend {
            ClipboardBackend::External(backend) => get_external_backend(backend),
            ClipboardBackend::Osc52 => {
                if let Some(fallback) = self.fallback {
                    log_fallback(self.backend, ClipboardBackend::External(fallback), "timeout");
                    return get_external_backend(fallback);
                }
                Err(ClipboardError::Timeout)
            }
            ClipboardBackend::Unavailable => Err(ClipboardError::NotAvailable),
        }
    }

    fn write_osc52(
        &self,
        content: &str,
        selection: ClipboardSelection,
        writer: &mut impl Write,
    ) -> Result<(), ClipboardError> {
        if !self.supports_osc52() {
            return Err(ClipboardError::NotAvailable);
        }
        let code = selection.osc52_code()?;
        let encoded = STANDARD.encode(content.as_bytes());
        if encoded.len() > self.max_payload {
            return Err(ClipboardError::InvalidInput(format!(
                "OSC 52 payload too large ({} > {})",
                encoded.len(),
                self.max_payload
            )));
        }
        write!(writer, "\x1b]52;{};{}\x07", code, encoded)
            .map_err(|err| ClipboardError::WriteError(err.to_string()))?;
        writer
            .flush()
            .map_err(|err| ClipboardError::WriteError(err.to_string()))
    }
}

fn apply_backend_override(
    caps: TerminalCapabilities,
    backend: ClipboardBackend,
    fallback: Option<ExternalBackend>,
) -> (ClipboardBackend, Option<ExternalBackend>) {
    let override_val = match env::var(ENV_CLIPBOARD_BACKEND) {
        Ok(val) => val,
        Err(_) => return (backend, fallback),
    };

    let override_val = override_val.to_ascii_lowercase();
    match override_val.as_str() {
        "osc52" => {
            if caps.osc52_clipboard {
                (ClipboardBackend::Osc52, fallback)
            } else {
                (ClipboardBackend::Unavailable, None)
            }
        }
        "macos" => resolve_external_override(ExternalBackend::MacOS),
        "windows" => resolve_external_override(ExternalBackend::Windows),
        "wayland" => resolve_external_override(ExternalBackend::Wayland),
        "x11" => resolve_external_override(ExternalBackend::X11),
        "none" => (ClipboardBackend::Unavailable, None),
        _ => (backend, fallback),
    }
}

fn resolve_external_override(
    backend: ExternalBackend,
) -> (ClipboardBackend, Option<ExternalBackend>) {
    if external_backend_available(backend) {
        (ClipboardBackend::External(backend), None)
    } else {
        (ClipboardBackend::Unavailable, None)
    }
}

fn detect_external_backend() -> Option<ExternalBackend> {
    if external_backend_available(ExternalBackend::MacOS) {
        return Some(ExternalBackend::MacOS);
    }
    if external_backend_available(ExternalBackend::Windows) {
        return Some(ExternalBackend::Windows);
    }
    if external_backend_available(ExternalBackend::Wayland) {
        return Some(ExternalBackend::Wayland);
    }
    if external_backend_available(ExternalBackend::X11) {
        return Some(ExternalBackend::X11);
    }
    None
}

fn external_backend_available(backend: ExternalBackend) -> bool {
    #[cfg(feature = "clipboard-fallback")]
    {
        match backend {
            ExternalBackend::MacOS => cfg!(target_os = "macos")
                && command_exists("pbcopy")
                && command_exists("pbpaste"),
            ExternalBackend::Windows => cfg!(target_os = "windows") && command_exists("clip"),
            ExternalBackend::Wayland => {
                env::var_os("WAYLAND_DISPLAY").is_some()
                    && command_exists("wl-copy")
                    && command_exists("wl-paste")
            }
            ExternalBackend::X11 => {
                env::var_os("DISPLAY").is_some()
                    && (command_exists("xclip") || command_exists("xsel"))
            }
        }
    }
    #[cfg(not(feature = "clipboard-fallback"))]
    {
        let _ = backend;
        false
    }
}

fn command_exists(command: &str) -> bool {
    if command.contains(std::path::MAIN_SEPARATOR) {
        return Path::new(command).is_file();
    }

    let path_var = match env::var_os("PATH") {
        Some(path) => path,
        None => return false,
    };

    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return true;
        }
        if cfg!(target_os = "windows") {
            let candidate = dir.join(format!("{command}.exe"));
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

fn set_external_backend(
    backend: ExternalBackend,
    content: &str,
    selection: ClipboardSelection,
) -> Result<(), ClipboardError> {
    if selection != ClipboardSelection::Clipboard {
        return Err(ClipboardError::InvalidInput(
            "external clipboard supports only Clipboard selection".to_string(),
        ));
    }
    #[cfg(feature = "clipboard-fallback")]
    {
        match backend {
            ExternalBackend::MacOS => run_command_with_input("pbcopy", &[], content),
            ExternalBackend::Windows => run_command_with_input("clip", &[], content),
            ExternalBackend::Wayland => run_command_with_input("wl-copy", &[], content),
            ExternalBackend::X11 => {
                if run_command_with_input("xclip", &["-selection", "clipboard"], content).is_ok()
                {
                    Ok(())
                } else {
                    run_command_with_input("xsel", &["--clipboard", "--input"], content)
                }
            }
        }
    }
    #[cfg(not(feature = "clipboard-fallback"))]
    {
        let _ = backend;
        let _ = content;
        Err(ClipboardError::NotAvailable)
    }
}

fn get_external_backend(backend: ExternalBackend) -> Result<String, ClipboardError> {
    #[cfg(feature = "clipboard-fallback")]
    {
        match backend {
            ExternalBackend::MacOS => run_command_output("pbpaste", &[]),
            ExternalBackend::Windows => run_command_output(
                "powershell",
                &["-NoProfile", "-Command", "Get-Clipboard"],
            ),
            ExternalBackend::Wayland => run_command_output("wl-paste", &["--no-newline"]),
            ExternalBackend::X11 => {
                if let Ok(output) = run_command_output(
                    "xclip",
                    &["-selection", "clipboard", "-o"],
                ) {
                    Ok(output)
                } else {
                    run_command_output("xsel", &["--clipboard", "--output"])
                }
            }
        }
    }
    #[cfg(not(feature = "clipboard-fallback"))]
    {
        let _ = backend;
        Err(ClipboardError::NotAvailable)
    }
}

#[cfg(feature = "clipboard-fallback")]
fn run_command_with_input(cmd: &str, args: &[&str], content: &str) -> Result<(), ClipboardError> {
    use std::process::{Command, Stdio};

    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| ClipboardError::WriteError(err.to_string()))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(content.as_bytes())
            .map_err(|err| ClipboardError::WriteError(err.to_string()))?;
    }

    let status = child
        .wait()
        .map_err(|err| ClipboardError::WriteError(err.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(ClipboardError::WriteError(format!(
            "clipboard command failed: {cmd}"
        )))
    }
}

#[cfg(feature = "clipboard-fallback")]
fn run_command_output(cmd: &str, args: &[&str]) -> Result<String, ClipboardError> {
    use std::process::Command;

    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|err| ClipboardError::ReadError(err.to_string()))?;

    if !output.status.success() {
        return Err(ClipboardError::ReadError(format!(
            "clipboard command failed: {cmd}"
        )));
    }

    String::from_utf8(output.stdout).map_err(|err| ClipboardError::ReadError(err.to_string()))
}

#[cfg(feature = "clipboard-logging")]
fn log_detected(backend: ClipboardBackend) {
    tracing::info!(backend = ?backend, "Clipboard backend detected");
}

#[cfg(not(feature = "clipboard-logging"))]
fn log_detected(_backend: ClipboardBackend) {}

#[cfg(feature = "clipboard-logging")]
fn log_write(backend: ClipboardBackend, bytes: usize) {
    tracing::debug!(backend = ?backend, bytes, "Clipboard write");
}

#[cfg(not(feature = "clipboard-logging"))]
fn log_write(_backend: ClipboardBackend, _bytes: usize) {}

#[cfg(feature = "clipboard-logging")]
fn log_fallback(primary: ClipboardBackend, fallback: ClipboardBackend, reason: &str) {
    tracing::warn!(primary = ?primary, fallback = ?fallback, reason, "Clipboard fallback triggered");
}

#[cfg(not(feature = "clipboard-logging"))]
fn log_fallback(_primary: ClipboardBackend, _fallback: ClipboardBackend, _reason: &str) {}

#[cfg(test)]
mod tests {
    use super::{Clipboard, ClipboardError, ClipboardSelection, TerminalCapabilities};

    fn caps_with_clipboard() -> TerminalCapabilities {
        let mut caps = TerminalCapabilities::basic();
        caps.osc52_clipboard = true;
        caps
    }

    #[test]
    fn selection_cut_buffer_bounds() {
        let ok = ClipboardSelection::CutBuffer(7).osc52_code();
        assert!(ok.is_ok());

        let err = ClipboardSelection::CutBuffer(8).osc52_code();
        assert!(matches!(err, Err(ClipboardError::InvalidInput(_))));
    }

    #[test]
    fn set_writes_osc52_sequence() {
        let clipboard = Clipboard::new(caps_with_clipboard());
        let mut out = Vec::new();
        clipboard
            .set("hi", ClipboardSelection::Clipboard, &mut out)
            .unwrap();

        let expected = format!(
            "\x1b]52;c;{}\x07",
            base64::engine::general_purpose::STANDARD.encode("hi")
        );
        assert_eq!(String::from_utf8(out).unwrap(), expected);
    }

    #[test]
    fn clear_writes_empty_osc52_sequence() {
        let clipboard = Clipboard::new(caps_with_clipboard());
        let mut out = Vec::new();
        clipboard
            .clear(ClipboardSelection::Clipboard, &mut out)
            .unwrap();
        assert_eq!(out, b"\x1b]52;c;\x07");
    }

    #[test]
    fn size_limit_is_enforced() {
        let mut clipboard = Clipboard::new(caps_with_clipboard());
        clipboard.max_payload = 4;
        let mut out = Vec::new();
        let err = clipboard.set("hello", ClipboardSelection::Clipboard, &mut out);
        assert!(matches!(err, Err(ClipboardError::InvalidInput(_))));
    }

    #[test]
    fn set_fails_when_unavailable() {
        let clipboard = Clipboard::new(TerminalCapabilities::basic());
        let mut out = Vec::new();
        let err = clipboard.set("hi", ClipboardSelection::Clipboard, &mut out);
        assert!(matches!(err, Err(ClipboardError::NotAvailable)));
    }
}
