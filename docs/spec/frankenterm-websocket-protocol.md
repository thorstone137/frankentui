# FrankenTerm WebSocket Protocol Specification

**Status**: DRAFT
**Bead**: bd-lff4p.10.1
**Author**: ChartreuseStream (claude-code / opus-4.6)
**Date**: 2026-02-08

## Overview

This document specifies the bidirectional WebSocket transport protocol between a
FrankenTerm browser client and a PTY bridge server. The protocol carries terminal
I/O (ANSI byte streams downstream, input events upstream) plus control messages
for session lifecycle, resize, capability negotiation, and flow control.

Design constraints (inherited from the FrankenTerm architecture spec):

- **Deterministic replay**: identical input sequences produce identical output
  given the same seed, clock model, and capability profile.
- **Safe Rust**: `#![forbid(unsafe_code)]` in all new crates.
- **Bounded memory**: no unbounded queues; explicit backpressure policy.
- **Audit-friendly**: every session emits structured JSONL logs.

## 1. Transport Layer

### 1.1 WebSocket Endpoint

```
wss://{host}/ws/terminal?session={session_id}&version={protocol_version}
```

- **MUST** use `wss://` (TLS) in production. Plain `ws://` is permitted only
  for local development (`localhost` / `127.0.0.1`).
- Query parameters:
  - `session`: opaque session identifier (UUID v7 recommended). If omitted, the
    server creates a new session and returns the ID in the handshake response.
  - `version`: requested protocol version (e.g., `frankenterm-ws-v1`). If
    omitted, the server selects the latest supported version.

### 1.2 Subprotocol Negotiation

The client SHOULD include `Sec-WebSocket-Protocol: frankenterm-ws-v1` in the
upgrade request. The server MUST echo the negotiated subprotocol in the response.
If the server does not support the requested version, it MUST reject the upgrade
with HTTP 400 and a JSON body:

```json
{
  "error": "unsupported_protocol",
  "supported": ["frankenterm-ws-v1"],
  "requested": "frankenterm-ws-v99"
}
```

### 1.3 Frame Format

All protocol messages use **WebSocket binary frames** containing a
length-prefixed envelope:

```
┌─────────┬──────────┬────────────────────────────┐
│ type(1) │ len(3)   │ payload(len bytes)          │
└─────────┴──────────┴────────────────────────────┘
```

- **type** (1 byte): message type discriminator (see Section 2).
- **len** (3 bytes, big-endian unsigned): payload length in bytes. Maximum
  payload size is 16 MiB (0xFF_FF_FF). Messages exceeding this MUST be
  rejected.
- **payload**: type-specific content.

WebSocket text frames are reserved for error diagnostics and MUST NOT carry
protocol messages.

### 1.4 Byte Order

All multi-byte integers in binary payloads are big-endian unless explicitly
noted. Strings are UTF-8.

## 2. Message Types

### 2.1 Type Table

| Code | Name              | Direction       | Payload Format |
|------|-------------------|-----------------|----------------|
| 0x01 | `Handshake`       | Client → Server | JSON           |
| 0x02 | `HandshakeAck`    | Server → Client | JSON           |
| 0x03 | `Input`           | Client → Server | Binary         |
| 0x04 | `Output`          | Server → Client | Binary         |
| 0x05 | `Resize`          | Client → Server | Binary (4B)    |
| 0x06 | `ResizeAck`       | Server → Client | Binary (4B)    |
| 0x07 | `TerminalQuery`   | Bidirectional   | Binary         |
| 0x08 | `TerminalReply`   | Bidirectional   | Binary         |
| 0x09 | `FeatureToggle`   | Client → Server | Binary (4B)    |
| 0x0A | `FeatureAck`      | Server → Client | Binary (4B)    |
| 0x0B | `Clipboard`       | Bidirectional   | JSON           |
| 0x0C | `Keepalive`       | Bidirectional   | Binary (8B)    |
| 0x0D | `KeepaliveAck`    | Bidirectional   | Binary (8B)    |
| 0x0E | `FlowControl`     | Bidirectional   | Binary (8B)    |
| 0x0F | `SessionEnd`      | Bidirectional   | JSON           |
| 0x10 | `Error`           | Bidirectional   | JSON           |

Codes 0x00 and 0x11-0xFF are reserved for future use. Receivers MUST ignore
unknown message types (log and skip, do not disconnect).

### 2.2 Handshake (0x01)

Sent by the client immediately after the WebSocket connection opens. The server
MUST NOT send `Output` frames before receiving and acknowledging the handshake.

```json
{
  "protocol_version": "frankenterm-ws-v1",
  "client_id": "frankenterm-web/0.1.0",
  "capabilities": {
    "clipboard": true,
    "osc_hyperlinks": true,
    "kitty_keyboard": true,
    "sixel": false,
    "truecolor": true
  },
  "initial_size": { "cols": 120, "rows": 40 },
  "dpr": 2.0,
  "auth_token": "<bearer token or null>",
  "seed": 0,
  "trace_mode": false
}
```

Fields:

| Field              | Required | Description                                       |
|--------------------|----------|---------------------------------------------------|
| `protocol_version` | yes      | Must match negotiated subprotocol.                 |
| `client_id`        | yes      | Client implementation identifier.                  |
| `capabilities`     | yes      | Client-supported terminal features (see 2.2.1).   |
| `initial_size`     | yes      | Initial terminal dimensions (cols, rows).          |
| `dpr`              | no       | Device pixel ratio (default 1.0).                  |
| `auth_token`       | no       | Bearer token for authenticated sessions.           |
| `seed`             | no       | RNG seed for deterministic replay (default 0).     |
| `trace_mode`       | no       | If true, server includes frame checksums in output.|

#### 2.2.1 Capability Object

The capability object declares what the client can handle. The server uses this
to configure the PTY's TERM environment and mode settings.

```json
{
  "clipboard": true,
  "osc_hyperlinks": true,
  "kitty_keyboard": true,
  "sixel": false,
  "truecolor": true,
  "bracketed_paste": true,
  "focus_events": true,
  "mouse_sgr": true,
  "unicode_version": "15.1"
}
```

The server MUST NOT enable features the client did not declare support for.

### 2.3 HandshakeAck (0x02)

```json
{
  "protocol_version": "frankenterm-ws-v1",
  "session_id": "01958c3a-...",
  "server_id": "ftui-remote/0.1.0",
  "effective_capabilities": { ... },
  "term_profile": "xterm-256color",
  "pty_pid": 12345,
  "flow_control": {
    "output_window": 65536,
    "input_window": 8192,
    "coalesce_resize_ms": 50,
    "coalesce_mouse_move_ms": 16
  }
}
```

Fields:

| Field                      | Required | Description                                     |
|----------------------------|----------|-------------------------------------------------|
| `protocol_version`         | yes      | Negotiated version (echo).                       |
| `session_id`               | yes      | Server-assigned session identifier.              |
| `server_id`                | yes      | Server implementation identifier.                |
| `effective_capabilities`   | yes      | Intersection of client + server capabilities.    |
| `term_profile`             | yes      | TERM value set for the PTY.                      |
| `pty_pid`                  | no       | PID of the spawned shell (omitted if sandboxed). |
| `flow_control`             | yes      | Initial flow control parameters (see Section 4). |

### 2.4 Input (0x03)

Client-to-server input events. Two sub-formats selected by the first byte of
the payload:

**Raw bytes** (sub-type 0x00):
```
┌──────────┬────────────────────────┐
│ 0x00 (1) │ bytes (variable)       │
└──────────┴────────────────────────┘
```

Used for keyboard input that maps cleanly to byte sequences (e.g., printable
characters, Ctrl+key). The server writes these directly to the PTY stdin.

**Semantic event** (sub-type 0x01):
```
┌──────────┬──────────┬──────┬──────┬────────────────┐
│ 0x01 (1) │ kind (1) │ mods │ data │ (variable)     │
│          │          │ (1)  │ len  │                 │
│          │          │      │ (2)  │                 │
└──────────┴──────────┴──────┴──────┴────────────────┘
```

Kind values:

| Kind | Name         | Data                                          |
|------|--------------|-----------------------------------------------|
| 0x01 | `KeyDown`    | UTF-8 key code string (DOM `code` field)       |
| 0x02 | `KeyUp`      | UTF-8 key code string                          |
| 0x03 | `MouseDown`  | `button(1) + col(2) + row(2)`                  |
| 0x04 | `MouseUp`    | `button(1) + col(2) + row(2)`                  |
| 0x05 | `MouseMove`  | `col(2) + row(2)`                              |
| 0x06 | `MouseDrag`  | `button(1) + col(2) + row(2)`                  |
| 0x07 | `Wheel`      | `dx(2,signed) + dy(2,signed) + col(2) + row(2)`|
| 0x08 | `Paste`      | UTF-8 paste content                            |
| 0x09 | `FocusIn`    | (empty)                                        |
| 0x0A | `FocusOut`   | (empty)                                        |

Modifier byte (bitfield):

| Bit | Modifier |
|-----|----------|
| 0   | Shift    |
| 1   | Ctrl     |
| 2   | Alt      |
| 3   | Super    |

The server translates semantic events into the appropriate byte sequences for
the PTY based on the negotiated capability profile (e.g., kitty keyboard
protocol, SGR mouse encoding).

### 2.5 Output (0x04)

Server-to-client terminal output. The payload is raw bytes from the PTY stdout.
The client feeds these into `frankenterm-core`'s VT parser.

When `trace_mode` is enabled in the handshake, the server MAY append a 32-byte
SHA-256 checksum of the cumulative output stream after each output frame. This
is indicated by setting bit 0 of a flags byte prepended to the payload:

```
┌───────────┬────────────────────────┬──────────────────────┐
│ flags (1) │ pty_bytes (variable)   │ checksum (32, if     │
│           │                        │ flags & 0x01)        │
└───────────┴────────────────────────┴──────────────────────┘
```

If `flags == 0x00`, the payload is raw PTY bytes with no checksum suffix.

### 2.6 Resize (0x05)

```
┌──────────┬──────────┐
│ cols (2) │ rows (2) │
└──────────┴──────────┘
```

Client sends when the terminal viewport changes size. The server:
1. Sends `SIGWINCH` to the PTY.
2. Replies with `ResizeAck` echoing the applied dimensions.

The server MUST coalesce resize storms (see Section 4.2).

### 2.7 ResizeAck (0x06)

```
┌──────────┬──────────┐
│ cols (2) │ rows (2) │
└──────────┴──────────┘
```

Echoes the dimensions actually applied to the PTY. May differ from the request
if the server clamps to min/max bounds (e.g., minimum 1x1, maximum 500x200).

### 2.8 TerminalQuery (0x07) / TerminalReply (0x08)

Carries DSR (Device Status Report), DA (Device Attributes), and other
bidirectional terminal queries.

```
┌──────────┬────────────────────────┐
│ seq_id(2)│ query_bytes (variable) │
└──────────┴────────────────────────┘
```

- `seq_id`: monotonically increasing sequence number for request/response
  correlation. Replies echo the `seq_id` of the originating query.
- `query_bytes`: raw VT/ANSI query sequence (e.g., `\x1b[6n` for cursor
  position, `\x1b[c` for primary DA).

The reply engine (bd-lff4p.10.3) generates deterministic responses based on the
negotiated capability profile.

### 2.9 FeatureToggle (0x09) / FeatureAck (0x0A)

Runtime feature toggle (e.g., enabling mouse capture mid-session).

```
┌───────────────────────────────┐
│ features (4, bitfield)        │
└───────────────────────────────┘
```

Bitfield layout (matches `BackendFeatures`):

| Bit | Feature           |
|-----|-------------------|
| 0   | mouse_capture     |
| 1   | bracketed_paste   |
| 2   | focus_events      |
| 3   | kitty_keyboard    |

The server applies the requested features and replies with `FeatureAck`
containing the features actually enabled (may differ if the PTY or TERM profile
doesn't support a requested feature).

### 2.10 Clipboard (0x0B)

```json
{
  "action": "copy" | "paste" | "paste_request",
  "mime": "text/plain",
  "data_b64": "<base64-encoded content>",
  "source": "selection" | "clipboard" | "primary"
}
```

- `copy`: client notifies server of a copy operation (informational; server MAY
  log but MUST NOT execute commands based on clipboard content).
- `paste`: client sends paste content to be fed to the PTY (subject to
  bracketed paste wrapping if enabled).
- `paste_request`: server requests clipboard content from the client (OSC 52).

Maximum clipboard payload: 1 MiB base64-encoded (768 KiB decoded). Larger
payloads MUST be rejected with an `Error` message.

### 2.11 Keepalive (0x0C) / KeepaliveAck (0x0D)

```
┌──────────────────────┐
│ timestamp_ns (8)     │
└──────────────────────┘
```

Either side may send a keepalive. The receiver MUST reply with `KeepaliveAck`
echoing the timestamp. This enables round-trip latency measurement.

Default keepalive interval: 30 seconds. If no message (of any type) is received
for 90 seconds, the connection is considered stale and SHOULD be closed.

### 2.12 FlowControl (0x0E)

```
┌──────────────────────┬──────────────────────┐
│ direction (1)        │ window_bytes (4)     │
│ 0x00=output          │                      │
│ 0x01=input           │                      │
└──────────────────────┴──────────────────────┘
```

Updates the flow control window (see Section 4). The receiver adjusts its send
rate to stay within the advertised window.

### 2.13 SessionEnd (0x0F)

```json
{
  "reason": "client_close" | "server_close" | "pty_exit" | "timeout" | "error",
  "exit_code": 0,
  "message": "optional human-readable detail"
}
```

Either side may initiate session end. After sending `SessionEnd`, the sender
MUST NOT send further messages and SHOULD close the WebSocket with code 1000
(normal closure).

### 2.14 Error (0x10)

```json
{
  "code": "auth_failed" | "rate_limited" | "payload_too_large" | "invalid_message" | "internal",
  "message": "human-readable description",
  "fatal": true
}
```

If `fatal` is true, the sender will close the connection after sending the error.
Non-fatal errors are informational (e.g., a coalesced resize that was clamped).

## 3. Security Model

### 3.1 Authentication

Sessions MUST be authenticated. Supported mechanisms:

1. **Bearer token** in handshake `auth_token` field (JWT recommended).
2. **HTTP cookie** on the WebSocket upgrade request (for browser-initiated
   sessions sharing an existing authenticated HTTP session).

The server MUST validate the token/cookie before spawning a PTY. Failed
authentication results in an `Error` message with `code: "auth_failed"` and
WebSocket close code 4001.

### 3.2 Origin Restrictions

The server MUST validate the `Origin` header on WebSocket upgrade requests:

- Allow only explicitly configured origins (no wildcards in production).
- Reject requests with missing or disallowed `Origin` headers.
- Log all rejected origins for audit.

### 3.3 Rate Limiting

Per-session and per-IP rate limits:

| Resource          | Default Limit      | Action on Exceed       |
|-------------------|--------------------|------------------------|
| Input messages    | 1000/sec           | Drop + `FlowControl`   |
| Resize messages   | 20/sec             | Coalesce               |
| Clipboard paste   | 10/sec, 1 MiB/msg  | Reject + `Error`       |
| New sessions      | 5/min per IP       | Reject upgrade (429)   |
| Concurrent sessions | 10 per user      | Reject upgrade (429)   |

Limits are configurable. The server MUST log rate limit events.

### 3.4 Command Execution

The protocol MUST NOT provide an API for arbitrary command execution. The server
spawns a single shell process per session (configured at server startup, not per
client request). The client can only send input to this shell via `Input`
messages.

The server MUST NOT:
- Execute commands based on clipboard content.
- Allow the client to specify the shell binary or arguments.
- Allow the client to access the host filesystem directly.

### 3.5 Threat Model

#### 3.5.1 Threat Matrix

| Threat                        | Impact | Mitigation                                              |
|-------------------------------|--------|---------------------------------------------------------|
| Unauthenticated PTY access    | Critical | Bearer token / cookie auth required before PTY spawn. |
| Cross-origin WebSocket hijack | High   | Origin header validation; no wildcard origins.          |
| Output flood (DoS client)     | High   | Server-side output window; coalescing; max frame size.  |
| Input flood (DoS server)      | High   | Per-session input rate limit; bounded input queue.      |
| Clipboard exfiltration        | Medium | Clipboard messages are opt-in; logged; size-limited.    |
| Session hijack via token theft| High   | Short-lived tokens; token binding to IP/session.        |
| PTY escape / sandbox escape   | Critical | PTY runs in sandboxed environment (namespaces, seccomp).|
| Memory exhaustion             | High   | Bounded queues; max message size; session limits.       |
| Timing side-channels          | Low    | Keepalive intervals are fixed; no secret-dependent timing.|

#### 3.5.2 Loss Matrix

| Decision          | False Allow (threat succeeds)          | False Block (legitimate use blocked)     |
|-------------------|----------------------------------------|------------------------------------------|
| Auth check        | Unauthorized PTY access                | User locked out; must re-authenticate    |
| Origin check      | XS-WebSocket from malicious site       | Legitimate multi-origin deploy blocked   |
| Rate limit        | Resource exhaustion possible            | Fast typist or automated test throttled  |
| Size limit        | Large paste truncated silently          | Legitimate large paste rejected          |

**Policy**: Prefer false block over false allow for auth and origin checks.
Prefer false allow over false block for rate limits and size limits (degrade
gracefully with warnings rather than hard disconnects).

### 3.6 Encryption

All production deployments MUST use WSS (TLS 1.2+). The server SHOULD support
TLS 1.3 and SHOULD disable weak cipher suites.

## 4. Flow Control and Backpressure

### 4.1 Window-Based Flow Control

The protocol uses a credit-based flow control scheme inspired by HTTP/2:

- **Output window**: server may send at most `output_window` bytes of `Output`
  messages before the client sends a `FlowControl` message replenishing the
  window.
- **Input window**: client may send at most `input_window` bytes of `Input`
  messages before the server replenishes.

Initial windows are set in `HandshakeAck.flow_control`. Either side replenishes
by sending `FlowControl` with the number of bytes consumed.

**Stall detection**: if a sender has exhausted its window and receives no
replenishment for 30 seconds, it SHOULD send a `Keepalive` and log a stall
warning. After 60 seconds of stall, the sender MAY close the connection with
`SessionEnd(reason: "timeout")`.

### 4.2 Coalescing Policies

To prevent flooding, the server and client MUST coalesce bursty events:

| Event Type   | Coalesce Window | Strategy                                    |
|--------------|-----------------|---------------------------------------------|
| Resize       | 50 ms           | Keep only the latest resize in the window.  |
| Mouse move   | 16 ms (60 fps)  | Keep only the latest position in the window.|
| Output       | 1 ms            | Batch PTY reads into single Output frames.  |

Coalescing parameters are negotiated in the handshake and MAY be updated via
`FlowControl` messages.

### 4.3 Fairness

Interactive input MUST NOT be starved by output. The server MUST:

1. Process `Input` messages with higher priority than generating `Output`.
2. Limit output batch size to at most 64 KiB per event loop iteration.
3. Interleave input processing with output sending (no "drain output then
   read input" pattern).

### 4.4 Bounded Queues

| Queue              | Max Size | Eviction Policy                         |
|--------------------|----------|-----------------------------------------|
| Server output      | 256 KiB  | Drop oldest bytes; send `FlowControl`   |
| Server input       | 16 KiB   | Drop newest; send `Error` (non-fatal)   |
| Client render      | 2 frames | Drop oldest frame; render latest         |

## 5. Session Lifecycle

### 5.1 Connection Sequence

```
Client                                  Server
  │                                       │
  │──── WebSocket Upgrade ───────────────>│
  │<─── 101 Switching Protocols ──────────│
  │                                       │
  │──── Handshake (0x01) ───────────────>│
  │                                       │ (validate auth, spawn PTY)
  │<─── HandshakeAck (0x02) ─────────────│
  │                                       │
  │<─── Output (0x04) ──────────────────>│ (shell prompt)
  │──── Input (0x03) ───────────────────>│
  │     ...bidirectional I/O...           │
  │                                       │
  │──── SessionEnd (0x0F) ──────────────>│ (or server sends first)
  │<─── SessionEnd (0x0F) ───────────────│
  │                                       │
  │──── WebSocket Close (1000) ─────────>│
```

### 5.2 Reconnection

If the WebSocket connection drops unexpectedly:

1. The server keeps the PTY alive for a configurable grace period (default: 60
   seconds).
2. The client reconnects and sends a `Handshake` with the same `session_id`.
3. The server replays buffered output since the last acknowledged byte offset
   (tracked via `FlowControl` messages).

If the grace period expires, the server sends `SIGHUP` to the PTY and cleans up
the session.

### 5.3 Concurrent Connections

Only one WebSocket connection per session is active at a time. If a second
connection attempts to join an existing session:

- If the first connection is still active, reject the second with `Error`
  (`code: "session_in_use"`).
- If the first connection is stale (no messages for >30s), close the first
  and accept the second (session takeover).

## 6. Capability Profiles

### 6.1 TERM Mapping

The server sets the PTY's `TERM` environment variable based on the negotiated
capabilities:

| Capability Set                           | TERM Value         |
|------------------------------------------|--------------------|
| truecolor + kitty_keyboard + osc_hyperlinks | `xterm-kitty`   |
| truecolor + osc_hyperlinks               | `xterm-256color`   |
| 256-color only                           | `xterm-256color`   |
| Basic (no truecolor)                     | `xterm`            |
| Minimal (dumb terminal)                  | `dumb`             |

Additional environment variables set by the server:

```
COLORTERM=truecolor     (if truecolor capability)
LANG=en_US.UTF-8        (or client-specified locale)
FRANKENTERM=1            (identifies FrankenTerm sessions)
FRANKENTERM_VERSION=0.1  (protocol version)
```

### 6.2 Capability Evolution

New capabilities are added as optional fields in the handshake capability
object. Unknown capabilities MUST be ignored by both sides.

## 7. Logging

### 7.1 Session JSONL

Every session emits a JSONL log file with the following record types:

**Session start**:
```json
{
  "event": "session_start",
  "ts": "2026-02-08T19:00:00.000Z",
  "run_id": "01958c3a-...",
  "session_id": "01958c3b-...",
  "git_sha": "abc123",
  "build_id": "ftui-remote/0.1.0",
  "client_id": "frankenterm-web/0.1.0",
  "initial_size": { "cols": 120, "rows": 40 },
  "term_profile": "xterm-256color",
  "capabilities": { ... }
}
```

**Wire counters** (periodic, every 10 seconds):
```json
{
  "event": "wire_stats",
  "ts": "2026-02-08T19:00:10.000Z",
  "session_id": "...",
  "interval_ms": 10000,
  "output_bytes": 45231,
  "input_bytes": 127,
  "output_messages": 342,
  "input_messages": 15,
  "resize_count": 0,
  "keepalive_rtt_ms": { "p50": 12, "p95": 25, "p99": 48 }
}
```

**Latency histogram** (periodic):
```json
{
  "event": "latency_histogram",
  "ts": "...",
  "session_id": "...",
  "input_to_output_ms": { "p50": 8, "p95": 22, "p99": 45, "max": 120 },
  "output_queue_depth": { "avg": 2048, "max": 16384 }
}
```

**Flow control event**:
```json
{
  "event": "flow_control",
  "ts": "...",
  "session_id": "...",
  "direction": "output",
  "action": "stall" | "replenish" | "coalesce",
  "window_bytes": 65536,
  "queued_bytes": 65000
}
```

**Session end**:
```json
{
  "event": "session_end",
  "ts": "...",
  "session_id": "...",
  "reason": "client_close",
  "duration_ms": 300000,
  "total_output_bytes": 1234567,
  "total_input_bytes": 4567,
  "total_messages": 12345,
  "exit_code": 0
}
```

### 7.2 Trace Integration

When `trace_mode` is enabled, the session log also includes golden trace records
compatible with `frankenterm-golden-trace-format.md`:

- `frame` records with `frame_hash` and `checksum_chain`.
- `input` records with `ts_ns` relative to session start.
- `resize` records.

This enables deterministic replay of remote sessions using the same trace
replayer infrastructure (bd-lff4p.5.2).

## 8. Schema Versioning

### 8.1 Protocol Version String

Format: `frankenterm-ws-v{N}` where `N` is a positive integer.

### 8.2 Evolution Rules

Within a version (`vN`):
- New optional fields MAY be added to JSON messages.
- New message types MAY be added (receivers ignore unknown types).
- Existing field semantics MUST NOT change.
- Existing required fields MUST NOT be removed.

A new version (`v(N+1)`) is required for:
- Removing or renaming existing fields.
- Changing the binary frame envelope format.
- Changing the semantics of existing message types.

### 8.3 Negotiation

The client requests a version via the `Sec-WebSocket-Protocol` header. The
server selects the highest mutually supported version. If no common version
exists, the connection is rejected at the HTTP level.

## 9. Conformance Testing

### 9.1 Test Categories

1. **Handshake conformance**: valid/invalid handshakes, capability negotiation
   edge cases, version mismatch handling.
2. **Message round-trip**: each message type sent and received correctly.
3. **Flow control**: window exhaustion, stall detection, replenishment.
4. **Coalescing**: resize storms, mouse move floods.
5. **Security**: auth failure, origin rejection, rate limiting, oversized
   payloads.
6. **Reconnection**: graceful reconnect with output replay.
7. **Deterministic replay**: trace-mode sessions produce reproducible checksums.

### 9.2 Golden Transcripts

Conformance test sessions produce golden transcripts in the format defined by
`frankenterm-golden-trace-format.md`. These transcripts are committed to the
repository under `tests/fixtures/ws-protocol/` and validated in CI.

### 9.3 E2E JSONL Validation

Session JSONL logs validate against `tests/e2e/lib/e2e_jsonl_schema.json` with
the event types defined in Section 7.1.

## 10. References

- `docs/spec/frankenterm-architecture.md` (bd-lff4p.6)
- `docs/spec/frankenterm-golden-trace-format.md` (bd-lff4p.5.1)
- `docs/adr/ADR-008-terminal-backend-strategy.md` (bd-lff4p.9)
- `docs/adr/ADR-009-webgpu-renderer-architecture.md` (bd-lff4p.2.1)
- bd-lff4p.10.3: Terminal reply engine (DSR/DA/DEC queries)
- bd-lff4p.10.4: PTY bridge server (Rust) with websocket transport
- bd-lff4p.10.7: TERM/capability profile + terminfo
- bd-lff4p.10.8: Queueing-theoretic backpressure + fairness policy
