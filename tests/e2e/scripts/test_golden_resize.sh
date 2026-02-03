#!/bin/bash
set -euo pipefail

# E2E tests for Golden Output Harness with resize scenarios.
#
# Generates golden outputs and checksums for resize scenarios to prove isomorphism.
# Supports deterministic mode via GOLDEN_SEED environment variable.
#
# JSONL Schema:
# {"event":"start","run_id":"...","case":"...","env":{...},"seed":N,"timestamp":"..."}
# {"event":"frame","frame_id":N,"width":N,"height":N,"checksum":"sha256:...","timing_ms":N}
# {"event":"resize","from":"WxH","to":"WxH","timing_ms":N}
# {"event":"complete","outcome":"pass|fail|skip","checksums":[...],"total_ms":N}
#
# Usage:
#   ./test_golden_resize.sh                    # Run all scenarios
#   BLESS=1 ./test_golden_resize.sh            # Update golden checksums
#   GOLDEN_SEED=42 ./test_golden_resize.sh     # Deterministic mode

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB_DIR="$SCRIPT_DIR/../lib"

# shellcheck source=/dev/null
source "$LIB_DIR/common.sh"
# shellcheck source=/dev/null
source "$LIB_DIR/logging.sh"
# shellcheck source=/dev/null
source "$LIB_DIR/pty.sh"

# Configuration
GOLDEN_SEED="${GOLDEN_SEED:-0}"
GOLDEN_LOG_DIR="${GOLDEN_LOG_DIR:-$E2E_LOG_DIR/golden}"
GOLDEN_CHECKSUMS_DIR="${GOLDEN_CHECKSUMS_DIR:-$SCRIPT_DIR/../../golden_checksums}"
BLESS="${BLESS:-0}"

mkdir -p "$GOLDEN_LOG_DIR"
mkdir -p "$GOLDEN_CHECKSUMS_DIR"

# Master JSONL log
GOLDEN_JSONL="$GOLDEN_LOG_DIR/golden_resize_$(date +%Y%m%d_%H%M%S).jsonl"

# Log environment header
log_golden_env() {
    local run_id="$1"
    cat >> "$GOLDEN_JSONL" <<EOF
{"event":"env","run_id":"$run_id","timestamp":"$(date -Iseconds)","seed":$GOLDEN_SEED,"term":"${TERM:-}","colorterm":"${COLORTERM:-}","bless":$BLESS}
{"event":"git","run_id":"$run_id","commit":"$(git rev-parse HEAD 2>/dev/null || echo 'N/A')","branch":"$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo 'N/A')"}
EOF
}

# Compute checksum of PTY output (deterministic hash)
compute_checksum() {
    local file="$1"
    if [[ -f "$file" ]]; then
        # Use sha256sum if available, otherwise md5sum, otherwise wc-based hash
        if command -v sha256sum >/dev/null 2>&1; then
            sha256sum "$file" | cut -d' ' -f1 | head -c 16
        elif command -v md5sum >/dev/null 2>&1; then
            md5sum "$file" | cut -d' ' -f1 | head -c 16
        else
            # Fallback: size + first bytes hash
            local size
            size=$(wc -c < "$file")
            printf "%08x%08x" "$size" "$(head -c 64 "$file" | cksum | cut -d' ' -f1)"
        fi
    else
        echo "0000000000000000"
    fi
}

# Log frame capture
log_frame() {
    local run_id="$1"
    local frame_id="$2"
    local width="$3"
    local height="$4"
    local checksum="$5"
    local timing_ms="$6"
    cat >> "$GOLDEN_JSONL" <<EOF
{"event":"frame","run_id":"$run_id","frame_id":$frame_id,"width":$width,"height":$height,"checksum":"sha256:$checksum","timing_ms":$timing_ms}
EOF
}

# Log resize event
log_resize_event() {
    local run_id="$1"
    local from_size="$2"
    local to_size="$3"
    local timing_ms="$4"
    cat >> "$GOLDEN_JSONL" <<EOF
{"event":"resize","run_id":"$run_id","from":"$from_size","to":"$to_size","timing_ms":$timing_ms}
EOF
}

# Log completion
log_complete() {
    local run_id="$1"
    local outcome="$2"
    local checksums="$3"
    local total_ms="$4"
    cat >> "$GOLDEN_JSONL" <<EOF
{"event":"complete","run_id":"$run_id","outcome":"$outcome","checksums":[$checksums],"total_ms":$total_ms}
EOF
}

# Check if harness binary exists
if [[ ! -x "${E2E_HARNESS_BIN:-}" ]]; then
    LOG_FILE="$GOLDEN_LOG_DIR/golden_missing.log"
    for scenario in fixed_80x24 fixed_120x40 fixed_60x15 resize_80x24_to_120x40 resize_120x40_to_80x24; do
        log_test_skip "golden_$scenario" "ftui-harness binary missing"
        record_result "golden_$scenario" "skipped" 0 "$LOG_FILE" "binary missing"
    done
    exit 0
fi

# Run a golden test case
run_golden_case() {
    local name="$1"
    local initial_cols="$2"
    local initial_rows="$3"
    local resize_cols="${4:-}"
    local resize_rows="${5:-}"
    local resize_delay_ms="${6:-400}"

    local run_id
    run_id="$(date +%s%N | head -c 16)"
    LOG_FILE="$GOLDEN_LOG_DIR/golden_${name}.log"
    local output_file="$GOLDEN_LOG_DIR/golden_${name}.pty"
    local checksum_file="$GOLDEN_CHECKSUMS_DIR/${name}.checksums"

    log_test_start "golden_$name"
    log_golden_env "$run_id"

    local start_ms
    start_ms="$(date +%s%3N)"

    # Build environment
    local pty_env=(
        PTY_COLS="$initial_cols"
        PTY_ROWS="$initial_rows"
        FTUI_HARNESS_EXIT_AFTER_MS=1200
        FTUI_HARNESS_LOG_LINES=5
        FTUI_HARNESS_SUPPRESS_WELCOME=1
        GOLDEN_SEED="$GOLDEN_SEED"
        PTY_TIMEOUT=4
        PTY_CANONICALIZE=1
        PTY_TEST_NAME="golden_$name"
        PTY_JSONL="$GOLDEN_LOG_DIR/golden_pty.jsonl"
    )

    # Add resize parameters if specified
    if [[ -n "$resize_cols" && -n "$resize_rows" ]]; then
        pty_env+=(
            PTY_RESIZE_COLS="$resize_cols"
            PTY_RESIZE_ROWS="$resize_rows"
            PTY_RESIZE_DELAY_MS="$resize_delay_ms"
            FTUI_HARNESS_EXIT_AFTER_MS=1800
        )
    fi

    # Run harness
    env "${pty_env[@]}" pty_run "$output_file" "$E2E_HARNESS_BIN"

    local end_ms
    end_ms="$(date +%s%3N)"
    local duration_ms=$((end_ms - start_ms))

    # Compute checksum of output
    local checksum
    checksum="$(compute_checksum "$output_file")"

    # Log frame(s)
    log_frame "$run_id" 0 "$initial_cols" "$initial_rows" "$checksum" "$duration_ms"

    if [[ -n "$resize_cols" && -n "$resize_rows" ]]; then
        log_resize_event "$run_id" "${initial_cols}x${initial_rows}" "${resize_cols}x${resize_rows}" "$resize_delay_ms"
        # After resize, we'd ideally capture another frame, but PTY capture is single-shot
        # The checksum reflects the final state after resize
    fi

    local checksums_json="\"sha256:$checksum\""

    # Verify against golden checksums if they exist
    local outcome="pass"
    if [[ -f "$checksum_file" && "$BLESS" != "1" ]]; then
        local expected_checksum
        expected_checksum="$(grep -v '^#' "$checksum_file" | head -1 | tr -d ' \n')"
        if [[ -n "$expected_checksum" && "$expected_checksum" != "sha256:$checksum" ]]; then
            log_error "Checksum mismatch for $name"
            log_error "  expected: $expected_checksum"
            log_error "  actual:   sha256:$checksum"
            outcome="fail"
        fi
    fi

    # Save golden checksums if in BLESS mode
    if [[ "$BLESS" == "1" ]]; then
        mkdir -p "$(dirname "$checksum_file")"
        cat > "$checksum_file" <<EOF
# Golden checksum for $name
# Generated: $(date -Iseconds)
# Seed: $GOLDEN_SEED
# Size: ${initial_cols}x${initial_rows}$(if [[ -n "$resize_cols" ]]; then echo " -> ${resize_cols}x${resize_rows}"; fi)
sha256:$checksum
EOF
        log_info "Updated golden checksum: $checksum_file"
    fi

    log_complete "$run_id" "$outcome" "$checksums_json" "$duration_ms"

    if [[ "$outcome" == "pass" ]]; then
        log_test_pass "golden_$name"
        record_result "golden_$name" "passed" "$duration_ms" "$LOG_FILE"
        return 0
    else
        log_test_fail "golden_$name" "checksum mismatch"
        record_result "golden_$name" "failed" "$duration_ms" "$LOG_FILE" "checksum mismatch"
        return 1
    fi
}

run_case() {
    local name="$1"
    shift
    local start_ms
    start_ms="$(date +%s%3N)"

    if "$@"; then
        local end_ms
        end_ms="$(date +%s%3N)"
        local duration_ms=$((end_ms - start_ms))
        log_test_pass "$name"
        record_result "$name" "passed" "$duration_ms" "$LOG_FILE"
        return 0
    fi

    local end_ms
    end_ms="$(date +%s%3N)"
    local duration_ms=$((end_ms - start_ms))
    log_test_fail "$name" "assertion failed"
    record_result "$name" "failed" "$duration_ms" "$LOG_FILE" "assertion failed"
    return 1
}

# Log summary header
log_info "=========================================="
log_info "Golden Output Harness - Resize Scenarios"
log_info "=========================================="
log_info "GOLDEN_SEED: $GOLDEN_SEED"
log_info "BLESS mode: $BLESS"
log_info "Log directory: $GOLDEN_LOG_DIR"
log_info "Checksums directory: $GOLDEN_CHECKSUMS_DIR"
log_info "JSONL log: $GOLDEN_JSONL"
log_info ""

FAILURES=0

# Fixed size scenarios
run_case "golden_fixed_80x24" run_golden_case "fixed_80x24" 80 24 || FAILURES=$((FAILURES + 1))
run_case "golden_fixed_120x40" run_golden_case "fixed_120x40" 120 40 || FAILURES=$((FAILURES + 1))
run_case "golden_fixed_60x15" run_golden_case "fixed_60x15" 60 15 || FAILURES=$((FAILURES + 1))
run_case "golden_fixed_40x10" run_golden_case "fixed_40x10" 40 10 || FAILURES=$((FAILURES + 1))

# Resize scenarios
run_case "golden_resize_80x24_to_120x40" run_golden_case "resize_80x24_to_120x40" 80 24 120 40 400 || FAILURES=$((FAILURES + 1))
run_case "golden_resize_120x40_to_80x24" run_golden_case "resize_120x40_to_80x24" 120 40 80 24 400 || FAILURES=$((FAILURES + 1))
run_case "golden_resize_80x24_to_40x10" run_golden_case "resize_80x24_to_40x10" 80 24 40 10 400 || FAILURES=$((FAILURES + 1))

# Summary
log_info ""
log_info "=========================================="
log_info "Golden Output Harness Complete"
log_info "=========================================="
log_info "Failures: $FAILURES"
log_info "JSONL log: $GOLDEN_JSONL"
if [[ "$BLESS" == "1" ]]; then
    log_info "Golden checksums updated in: $GOLDEN_CHECKSUMS_DIR"
fi

# Print reproduction command on failure
if [[ "$FAILURES" -gt 0 ]]; then
    log_error ""
    log_error "Reproduction command:"
    log_error "  GOLDEN_SEED=$GOLDEN_SEED ./tests/e2e/scripts/test_golden_resize.sh"
    log_error ""
    log_error "To update golden checksums:"
    log_error "  BLESS=1 GOLDEN_SEED=$GOLDEN_SEED ./tests/e2e/scripts/test_golden_resize.sh"
fi

exit "$FAILURES"
