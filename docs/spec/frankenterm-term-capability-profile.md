# FrankenTerm TERM/Capability Profile + Terminfo Specification

**Status**: DRAFT
**Bead**: bd-lff4p.10.7
**Author**: ChartreuseStream (claude-code / opus-4.6)
**Date**: 2026-02-08
**References**: frankenterm-vt-support-matrix.md, frankenterm-websocket-protocol.md

## Overview

This document defines how FrankenTerm identifies itself to PTY-hosted
applications, what terminal capabilities it declares, and how the terminfo
database entry is structured. The goal is to ensure that CLI applications
(vim, tmux, htop, fzf, fish, etc.) detect the correct feature set and render
correctly in FrankenTerm remote sessions.

## 1. TERM Strategy

### 1.1 Decision: Use `xterm-256color` by Default

FrankenTerm sets `TERM=xterm-256color` in the PTY environment. Rationale:

1. **Maximum compatibility**: virtually every terminal application handles
   `xterm-256color` correctly. A custom `TERM=frankenterm` would require
   shipping a terminfo entry to every server, which is fragile for remote
   sessions.
2. **Accurate enough**: FrankenTerm implements a strict superset of the
   capabilities described by `xterm-256color`. The few additional features
   (kitty keyboard, synchronized output, OSC 8 hyperlinks) are negotiated
   via escape sequences, not terminfo.
3. **Precedent**: Kitty, Alacritty, and WezTerm all default to
   `xterm-256color` (or `xterm-kitty`) for remote sessions despite having
   custom terminfo entries locally.

### 1.2 Supplementary Environment Variables

In addition to `TERM`, FrankenTerm sets these environment variables:

| Variable               | Value                | Purpose                            |
|------------------------|----------------------|------------------------------------|
| `TERM`                 | `xterm-256color`     | Terminfo lookup key                |
| `COLORTERM`            | `truecolor`          | Signal 24-bit color support        |
| `FRANKENTERM`          | `1`                  | Identify FrankenTerm sessions      |
| `FRANKENTERM_VERSION`  | `0.1`                | Protocol/capability version        |
| `LANG`                 | `en_US.UTF-8`        | UTF-8 locale (or client-specified) |

Applications that want to detect FrankenTerm specifically (e.g., to enable
advanced features) check `FRANKENTERM=1` rather than relying on `TERM`.

### 1.3 Optional Custom Terminfo (`xterm-frankenterm`)

For advanced use cases, FrankenTerm ships an optional `xterm-frankenterm`
terminfo entry that extends `xterm-256color` with additional capabilities.
This is NOT the default but can be opted into via:

```
TERM=xterm-frankenterm
```

The custom entry is installed by:
- The FrankenTerm server package (installs to system terminfo).
- A standalone `tic` source file (`terminfo/xterm-frankenterm.ti`) that users
  can compile manually: `tic terminfo/xterm-frankenterm.ti`.

## 2. Capability Profile

### 2.1 Core Capabilities (Must)

These capabilities are always present and applications can rely on them
unconditionally when `FRANKENTERM=1`.

| Capability           | Terminfo Field     | Value / Behavior                        |
|----------------------|--------------------|-----------------------------------------|
| 256 colors           | `colors#256`       | Standard 256-color palette              |
| Truecolor (24-bit)   | (no terminfo)      | `COLORTERM=truecolor`; SGR 38;2/48;2    |
| UTF-8                | (no terminfo)      | All input/output is UTF-8               |
| Cursor addressing    | `cup`              | CSI row;col H                           |
| Cursor visibility    | `civis`/`cnorm`    | CSI ?25 l / CSI ?25 h                   |
| Auto-wrap            | `am`               | DECAWM enabled by default               |
| Scroll regions       | `csr`              | DECSTBM                                 |
| Insert/delete line   | `il1`/`dl1`        | CSI L / CSI M                           |
| Insert/delete char   | `ich1`/`dch1`      | CSI @ / CSI P                           |
| Clear screen         | `clear`            | CSI 2 J                                 |
| Clear to EOL         | `el`               | CSI K                                   |
| Clear to EOS         | `ed`               | CSI J                                   |
| Alt screen           | `smcup`/`rmcup`    | CSI ?1049 h / CSI ?1049 l               |
| Bold                 | `bold`             | SGR 1                                   |
| Dim                  | `dim`              | SGR 2                                   |
| Italic               | `sitm`/`ritm`      | SGR 3 / SGR 23                          |
| Underline            | `smul`/`rmul`      | SGR 4 / SGR 24                          |
| Reverse              | `rev`              | SGR 7                                   |
| Strikethrough        | `smxx`/`rmxx`      | SGR 9 / SGR 29                          |
| SGR reset            | `sgr0`             | SGR 0                                   |
| Application cursor   | `smkx`/`rmkx`      | CSI ?1 h / CSI ?1 l (DECCKM)           |
| Save/restore cursor  | `sc`/`rc`          | ESC 7 / ESC 8                           |
| Synchronized output  | (no terminfo)      | CSI ?2026 h / CSI ?2026 l               |

### 2.2 Feature-Negotiated Capabilities (Must, When Enabled)

These capabilities are present but require explicit activation via escape
sequences. The WebSocket protocol handshake negotiates which ones the client
supports.

| Capability           | Activation                  | Behavior                      |
|----------------------|-----------------------------|-------------------------------|
| Mouse (SGR mode)     | CSI ?1000;1002;1006 h       | Button/motion events          |
| Bracketed paste      | CSI ?2004 h                 | Paste delimiters              |
| Focus reporting      | CSI ?1004 h                 | Focus in/out events           |
| Kitty keyboard       | CSI >flags u                | Progressive enhancement       |
| OSC 8 hyperlinks     | OSC 8;params;uri ST         | Clickable links               |
| OSC 52 clipboard     | OSC 52;c;data ST            | Read/write clipboard          |

### 2.3 Best-Effort Capabilities (Should)

These capabilities are implemented when the cost is low but applications
should degrade gracefully if they are absent or incorrect.

| Capability           | Notes                                                    |
|----------------------|----------------------------------------------------------|
| Overline (SGR 53)    | Supported; not all applications use it                   |
| Double underline     | SGR 4:2; may render as single underline on some displays |
| Curly underline      | SGR 4:3; best-effort rendering                           |
| Underline color      | SGR 58;2;r;g;b; may be ignored by older applications    |
| Cursor blink         | CSI ?12 h/l; timing is client-dependent                  |
| Tab stops            | HTS/TBC; default 8-column stops always available         |
| Character sets       | G0/G1 (DEC Special Graphics for line drawing)            |
| Window title         | OSC 0/2; stored and surfaced via callback                |
| Shell integration    | OSC 133; best-effort prompt/command marking               |
| Grapheme clustering  | CSI ?2027 h; depends on client Unicode version           |

### 2.4 Unsupported Capabilities (Won't)

Applications MUST NOT rely on these. FrankenTerm silently ignores or discards
related sequences.

| Capability           | Reason                                                   |
|----------------------|----------------------------------------------------------|
| Sixel graphics       | Prefer image protocols in future (Kitty/iTerm2)          |
| ReGIS/Tektronix      | Obsolete                                                 |
| 8-bit C1 controls    | Conflicts with UTF-8; use 7-bit ESC equivalents          |
| VT52 mode            | No modern use                                            |
| DRCS (custom chars)  | Rarely used                                              |
| Selective erase      | DECSED/DECSEL add complexity for minimal benefit         |

## 3. Terminal Reply Behavior

The terminal reply engine (bd-lff4p.10.3) generates deterministic responses to
application queries. Replies are based on the negotiated capability profile
and never leak implementation details.

### 3.1 Device Attributes

**Primary DA (CSI c)**:

Reply: `CSI ?64;1;2;4;6;9;15;18;21;22 c`

Decoded: VT420-class terminal with capabilities:
- 1: 132-column mode (conceptually; grid is resizable)
- 2: Printer port (not implemented; declared for compatibility)
- 4: Sixel graphics (not implemented; may be removed from reply)
- 6: Selective erase (not implemented; declared for compatibility)
- 9: National Replacement Character Sets
- 15: DEC Technical Character Set
- 18: Windowing capability
- 21: Horizontal scrolling
- 22: Color text

Note: the exact DA reply is chosen for compatibility with applications that
fingerprint terminal type from DA responses (e.g., vim, tmux). The values
match what xterm reports.

**Secondary DA (CSI > c)**:

Reply: `CSI >1;10;0 c`

- 1: VT220-class
- 10: firmware version (arbitrary; matches xterm convention)
- 0: ROM cartridge (always 0)

### 3.2 Device Status Report

**DSR (CSI 5 n)**: Reply `CSI 0 n` (terminal OK).

**CPR (CSI 6 n)**: Reply `CSI row;col R` (current cursor position, 1-indexed).

**Extended CPR (CSI ?6 n)**: Reply `CSI ?row;col R`.

### 3.3 Terminal Parameters

**DECREQTPARM (CSI x)**: Reply `CSI 2;1;1;112;112;1;0 x`

This reports communication parameters. The values are conventional
(no parity, 9600 baud equivalent) and match xterm.

### 3.4 Operating Status

**DECRQSS (DCS $ q ... ST)**: Reply with the current value of the requested
setting. Supported for:

| Request          | Response                                          |
|------------------|---------------------------------------------------|
| `m` (SGR)        | Current SGR attributes as parameter string        |
| `r` (DECSTBM)    | Current scroll region as `top;bottom`             |
| `" p` (DECSCL)   | Conformance level (64 for VT420)                  |

Unsupported requests receive `DCS 0 $ r ST` (not recognized).

## 4. Terminfo Source

The optional `xterm-frankenterm` terminfo entry. Compile with:
`tic -x terminfo/xterm-frankenterm.ti`

```terminfo
# FrankenTerm terminal description.
# Extends xterm-256color with additional capabilities.
xterm-frankenterm|FrankenTerm terminal emulator,
    use=xterm-256color,
    # Colors
    colors#256,
    # Synchronized output (DEC mode 2026)
    Sync=\E[?2026h,
    Se=\E[?2026l,
    # Styled underlines
    Smulx=\E[4:%p1%dm,
    # Underline color (colon-separated SGR 58)
    Setulc=\E[58:2::%p1%{65536}%/%d:%p1%{256}%/%{255}%&%d:%p1%{255}%&%d%;m,
    # Overline
    Smol=\E[53m,
    Rmol=\E[55m,
    # Strikethrough (already in xterm-256color but explicit)
    smxx=\E[9m,
    rmxx=\E[29m,
    # Kitty keyboard protocol (extended)
    # Applications should use CSI >flags u directly rather than terminfo
    # but we document the presence for tooling.
```

### 4.1 Terminfo Distribution

The terminfo source file is included in the repository at:
```
terminfo/xterm-frankenterm.ti
```

The PTY bridge server (bd-lff4p.10.4) compiles and installs it to a
session-local terminfo directory (`$HOME/.terminfo/` or a tmpdir) when
`TERM=xterm-frankenterm` is requested. This avoids requiring system-level
terminfo installation.

## 5. Capability Detection by Applications

### 5.1 How Applications Detect Features

Applications use multiple mechanisms to detect capabilities:

| Mechanism              | What It Detects                              |
|------------------------|----------------------------------------------|
| `TERM` + terminfo      | Core terminal capabilities (cursor, color)   |
| `COLORTERM=truecolor`  | 24-bit color support                         |
| Primary DA response    | Terminal class and feature set                |
| `FRANKENTERM=1`        | FrankenTerm-specific features                |
| Feature probing (CSI)  | Runtime capability testing                   |

### 5.2 Application Compatibility Matrix

This matrix documents expected behavior for key applications:

| Application | TERM Requirement | Additional Detection  | Expected Behavior       |
|-------------|------------------|-----------------------|-------------------------|
| vim/neovim  | xterm-256color   | DA, COLORTERM         | Full color, mouse, kitty keyboard |
| tmux        | xterm-256color   | DA                    | Full color, mouse, nested sessions |
| htop        | xterm-256color   | terminfo              | Full TUI rendering      |
| fzf         | xterm-256color   | COLORTERM             | Full color, mouse       |
| fish        | xterm-256color   | CSI u query           | Kitty keyboard, hyperlinks |
| less        | xterm-256color   | terminfo              | Color, line drawing     |
| git (delta) | xterm-256color   | COLORTERM             | Truecolor diffs         |
| bat         | xterm-256color   | COLORTERM             | Syntax highlighting     |
| zellij      | xterm-256color   | COLORTERM, kitty      | Full TUI, kitty keyboard |

### 5.3 Known Incompatibilities

| Application | Issue                                        | Workaround              |
|-------------|----------------------------------------------|-------------------------|
| screen      | Does not pass COLORTERM through               | Set COLORTERM inside    |
| mosh        | Limited mouse/color support                   | N/A (mosh limitation)   |
| emacs -nw   | May not detect truecolor without config       | Set `COLORTERM`         |

## 6. Validation

### 6.1 E2E Test Scripts

Remote session E2E tests validate application rendering under the FrankenTerm
capability profile:

```bash
# Launch a FrankenTerm remote session and run validation scripts
tests/e2e/scripts/remote_term_profile.sh
```

The test script:
1. Starts a PTY bridge server.
2. Connects a FrankenTerm client.
3. Runs a battery of commands that probe terminal capabilities:
   - `tput colors` (expect 256)
   - `echo $COLORTERM` (expect `truecolor`)
   - `echo $FRANKENTERM` (expect `1`)
   - `infocmp` (dump terminfo for the session)
   - Cursor position query/response round-trip
   - DA query/response verification
4. Runs a representative set of applications (vim, htop, fzf) and captures
   the output stream for golden comparison.

### 6.2 JSONL Logging

Each remote session logs capability-related events:

```json
{"event": "term_profile", "ts": "...", "session_id": "...",
 "term": "xterm-256color", "colorterm": "truecolor",
 "frankenterm_version": "0.1",
 "effective_capabilities": {
   "mouse_sgr": true, "bracketed_paste": true,
   "focus_events": true, "kitty_keyboard": true,
   "osc_hyperlinks": true, "clipboard": true,
   "truecolor": true
 }}
```

### 6.3 Conformance Fixtures

Capability probe sequences and expected replies are defined as conformance
fixtures in `tests/fixtures/vt-conformance/device_status/`:

```json
{
  "name": "primary_da",
  "input_bytes_hex": "1b5b63",
  "expected_reply_hex": "1b5b3f36343b313b323b343b363b393b31353b31383b32313b323220630d",
  "description": "Primary DA query -> VT420 response"
}
```

## 7. Future: Custom TERM Profile

If the FrankenTerm ecosystem grows large enough that a custom TERM value
becomes valuable (e.g., for terminfo capabilities not expressible via
xterm-256color), the migration path is:

1. Ship `xterm-frankenterm` terminfo widely (package managers, SSH auto-install).
2. Default `TERM=xterm-frankenterm` only when the terminfo is confirmed present
   (probe via `infocmp xterm-frankenterm 2>/dev/null`).
3. Fall back to `TERM=xterm-256color` if the custom terminfo is missing.

This ensures backward compatibility and avoids the "unknown terminal type"
errors that plague custom TERM values.

## 8. References

- docs/spec/frankenterm-vt-support-matrix.md (bd-lff4p.1.1)
- docs/spec/frankenterm-websocket-protocol.md (bd-lff4p.10.1)
- docs/spec/frankenterm-architecture.md (bd-lff4p.6)
- [terminfo(5) man page](https://man7.org/linux/man-pages/man5/terminfo.5.html)
- [xterm terminfo source](https://invisible-island.net/xterm/terminfo.html)
- [Kitty terminfo discussion](https://sw.kovidgoyal.net/kitty/faq/#i-get-errors-about-the-terminal-being-unknown)
