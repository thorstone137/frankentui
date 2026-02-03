#!/bin/bash
set -euo pipefail

# E2E tests for VirtualTerminal quirk profiles (bd-k4lj.4)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB_DIR="$SCRIPT_DIR/../lib"

# shellcheck source=/dev/null
source "$LIB_DIR/common.sh"
# shellcheck source=/dev/null
source "$LIB_DIR/logging.sh"
# shellcheck source=/dev/null
source "$LIB_DIR/pty.sh"

JSONL_FILE="$E2E_RESULTS_DIR/terminal_quirks.jsonl"
RUN_ID="terminal_quirks_$(date +%Y%m%d_%H%M%S)_$$"

jsonl_log() {
    local line="$1"
    mkdir -p "$E2E_RESULTS_DIR"
    printf '%s\n' "$line" >> "$JSONL_FILE"
}

sha256_file() {
    local file="$1"
    if command -v sha256sum >/dev/null 2>&1 && [[ -f "$file" ]]; then
        sha256sum "$file" | awk '{print $1}'
        return 0
    fi
    echo ""
    return 0
}

if ! CANON_BIN="$(resolve_canonicalize_bin)"; then
    LOG_FILE="$E2E_LOG_DIR/terminal_quirks_missing.log"
    for t in screen_immediate_wrap tmux_nested_cursor windows_no_alt_screen; do
        log_test_skip "$t" "pty_canonicalize binary missing"
        record_result "$t" "skipped" 0 "$LOG_FILE" "binary missing"
        jsonl_log "{\"run_id\":\"$RUN_ID\",\"case\":\"$t\",\"status\":\"skipped\",\"reason\":\"binary missing\"}"
    done
    exit 0
fi

run_case() {
    local name="$1"
    local profile="$2"
    local cols="$3"
    local rows="$4"
    local input_bytes="$5"
    local expected_line0="$6"
    local expected_line1="${7:-}"
    local start_ms
    start_ms="$(date +%s%3N)"

    LOG_FILE="$E2E_LOG_DIR/${name}.log"
    local input_file="$E2E_LOG_DIR/${name}.input"
    local output_file="$E2E_LOG_DIR/${name}.out"

    log_test_start "$name"

    printf '%b' "$input_bytes" > "$input_file"

    if "$CANON_BIN" --input "$input_file" --output "$output_file" --cols "$cols" --rows "$rows" --profile "$profile"; then
        local end_ms
        end_ms="$(date +%s%3N)"
        local duration_ms=$((end_ms - start_ms))
        local output_sha
        output_sha="$(sha256_file "$output_file")"
        local actual_line0
        actual_line0="$(head -n 1 "$output_file")"
        local actual_line1
        actual_line1="$(sed -n '2p' "$output_file")"

        if [[ "$actual_line0" != "$expected_line0" ]]; then
            log_test_fail "$name" "line0 mismatch"
            record_result "$name" "failed" "$duration_ms" "$LOG_FILE" "line0 mismatch"
            jsonl_log "{\"run_id\":\"$RUN_ID\",\"case\":\"$name\",\"status\":\"failed\",\"duration_ms\":$duration_ms,\"profile\":\"$profile\",\"cols\":$cols,\"rows\":$rows,\"output_sha256\":\"$output_sha\",\"expected_line0\":\"$expected_line0\",\"actual_line0\":\"$actual_line0\"}"
            return 1
        fi
        if [[ -n "$expected_line1" && "$actual_line1" != "$expected_line1" ]]; then
            log_test_fail "$name" "line1 mismatch"
            record_result "$name" "failed" "$duration_ms" "$LOG_FILE" "line1 mismatch"
            jsonl_log "{\"run_id\":\"$RUN_ID\",\"case\":\"$name\",\"status\":\"failed\",\"duration_ms\":$duration_ms,\"profile\":\"$profile\",\"cols\":$cols,\"rows\":$rows,\"output_sha256\":\"$output_sha\",\"expected_line1\":\"$expected_line1\",\"actual_line1\":\"$actual_line1\"}"
            return 1
        fi

        log_test_pass "$name"
        record_result "$name" "passed" "$duration_ms" "$LOG_FILE"
        jsonl_log "{\"run_id\":\"$RUN_ID\",\"case\":\"$name\",\"status\":\"passed\",\"duration_ms\":$duration_ms,\"profile\":\"$profile\",\"cols\":$cols,\"rows\":$rows,\"output_sha256\":\"$output_sha\",\"actual_line0\":\"$actual_line0\",\"actual_line1\":\"$actual_line1\"}"
        return 0
    fi

    local end_ms
    end_ms="$(date +%s%3N)"
    local duration_ms=$((end_ms - start_ms))
    log_test_fail "$name" "pty_canonicalize failed"
    record_result "$name" "failed" "$duration_ms" "$LOG_FILE" "pty_canonicalize failed"
    jsonl_log "{\"run_id\":\"$RUN_ID\",\"case\":\"$name\",\"status\":\"failed\",\"duration_ms\":$duration_ms,\"profile\":\"$profile\",\"cols\":$cols,\"rows\":$rows,\"error\":\"pty_canonicalize failed\"}"
    return 1
}

FAILURES=0

# Screen immediate wrap: final cursor wraps after last column write.
run_case "screen_immediate_wrap" "screen" 5 3 "ABCDE\rF" "ABCDE" "F" || FAILURES=$((FAILURES + 1))

# tmux nested cursor quirk: save/restore ignored in alt screen.
run_case "tmux_nested_cursor" "tmux_nested" 10 3 $'\x1b[?1049h\x1b[2;2H\x1b7\x1b[1;1H\x1b8X' "X" || FAILURES=$((FAILURES + 1))

# Windows console: alt screen ignored, output stays on main buffer.
run_case "windows_no_alt_screen" "windows_console" 10 3 "Main\x1b[?1049hAlt\x1b[?1049l" "MainAlt" || FAILURES=$((FAILURES + 1))

exit "$FAILURES"
