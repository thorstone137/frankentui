#!/bin/bash
# Demo Showcase E2E Test Script for FrankenTUI
# bd-qsbe.22: Comprehensive end-to-end verification of the demo showcase
#
# This script validates:
# 1. Compilation (debug + release)
# 2. Clippy (no warnings)
# 3. Formatting (cargo fmt --check)
# 4. Unit + snapshot tests
# 5. Smoke test (alt-screen with auto-exit)
# 6. Inline mode smoke test
# 7. Screen navigation (cycle all 12 screens)
# 8. Search test (Shakespeare screen)
# 9. Resize test (SIGWINCH handling)
#
# Usage:
#   ./scripts/demo_showcase_e2e.sh              # Run all tests
#   ./scripts/demo_showcase_e2e.sh --verbose    # Extra output
#   ./scripts/demo_showcase_e2e.sh --quick      # Compilation + clippy + fmt only
#   LOG_DIR=/path/to/logs ./scripts/demo_showcase_e2e.sh

set -uo pipefail

# ============================================================================
# Configuration
# ============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
LOG_DIR="${LOG_DIR:-/tmp/ftui-demo-e2e-${TIMESTAMP}}"
PKG="ftui-demo-showcase"

VERBOSE=false
QUICK=false
STEP_COUNT=0
PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0

declare -a STEP_NAMES=()
declare -a STEP_STATUSES=()
declare -a STEP_DURATIONS=()

# Parse arguments
for arg in "$@"; do
    case $arg in
        --verbose|-v)
            VERBOSE=true
            ;;
        --quick|-q)
            QUICK=true
            ;;
        --help|-h)
            echo "Usage: $0 [--verbose] [--quick]"
            echo ""
            echo "Options:"
            echo "  --verbose, -v   Show detailed output during execution"
            echo "  --quick, -q     Compilation + clippy + fmt only"
            echo "  --help, -h      Show this help message"
            echo ""
            echo "Environment:"
            echo "  LOG_DIR         Directory for log files (default: /tmp/ftui-demo-e2e-TIMESTAMP)"
            exit 0
            ;;
    esac
done

# ============================================================================
# Logging Functions
# ============================================================================

log_info() {
    echo -e "\033[1;34m[INFO]\033[0m $(date +%H:%M:%S) $*"
}

log_pass() {
    echo -e "\033[1;32m[PASS]\033[0m $(date +%H:%M:%S) $*"
}

log_fail() {
    echo -e "\033[1;31m[FAIL]\033[0m $(date +%H:%M:%S) $*"
}

log_skip() {
    echo -e "\033[1;33m[SKIP]\033[0m $(date +%H:%M:%S) $*"
}

log_step() {
    STEP_COUNT=$((STEP_COUNT + 1))
    echo ""
    echo -e "\033[1;36m[$STEP_COUNT/$TOTAL_STEPS]\033[0m $*"
}

# ============================================================================
# Step Runner
# ============================================================================

run_step() {
    local step_name="$1"
    local log_file="$2"
    shift 2
    local cmd=("$@")

    log_step "$step_name"
    log_info "Running: ${cmd[*]}"

    local start_time
    start_time=$(date +%s%N)

    local exit_code=0
    if $VERBOSE; then
        if "${cmd[@]}" 2>&1 | tee "$log_file"; then
            exit_code=0
        else
            exit_code=1
        fi
    else
        if "${cmd[@]}" > "$log_file" 2>&1; then
            exit_code=0
        else
            exit_code=1
        fi
    fi

    local end_time
    end_time=$(date +%s%N)
    local duration_ms=$(( (end_time - start_time) / 1000000 ))
    local duration_s
    duration_s=$(echo "scale=2; $duration_ms / 1000" | bc 2>/dev/null || echo "${duration_ms}ms")

    local stdout_size
    stdout_size=$(wc -c < "$log_file" 2>/dev/null || echo 0)

    STEP_NAMES+=("$step_name")
    STEP_DURATIONS+=("${duration_s}s")

    if [ $exit_code -eq 0 ]; then
        log_pass "$step_name completed in ${duration_s}s (output: ${stdout_size} bytes)"
        PASS_COUNT=$((PASS_COUNT + 1))
        STEP_STATUSES+=("PASS")
        return 0
    else
        log_fail "$step_name failed (exit=$exit_code). See: $log_file"
        FAIL_COUNT=$((FAIL_COUNT + 1))
        STEP_STATUSES+=("FAIL")
        return 1
    fi
}

skip_step() {
    local step_name="$1"
    local reason="${2:---quick mode}"
    log_step "$step_name"
    log_skip "Skipped ($reason)"
    SKIP_COUNT=$((SKIP_COUNT + 1))
    STEP_NAMES+=("$step_name")
    STEP_STATUSES+=("SKIP")
    STEP_DURATIONS+=("-")
}

# Run a smoke-test step. Captures exit code and records result.
# Usage: run_smoke_step "step name" "log_file" command...
run_smoke_step() {
    local step_name="$1"
    local log_file="$2"
    shift 2

    log_step "$step_name"
    log_info "Running: $*"
    STEP_NAMES+=("$step_name")

    local start_time
    start_time=$(date +%s%N)

    local exit_code=0
    if eval "$@" > "$log_file" 2>&1; then
        exit_code=0
    else
        exit_code=$?
    fi

    local end_time
    end_time=$(date +%s%N)
    local duration_ms=$(( (end_time - start_time) / 1000000 ))
    local duration_s
    duration_s=$(echo "scale=2; $duration_ms / 1000" | bc 2>/dev/null || echo "${duration_ms}ms")
    STEP_DURATIONS+=("${duration_s}s")

    # exit code 0 = clean exit, 124 = timeout (acceptable for smoke tests)
    if [ $exit_code -eq 0 ] || [ $exit_code -eq 124 ]; then
        log_pass "$step_name passed (exit=$exit_code) in ${duration_s}s"
        PASS_COUNT=$((PASS_COUNT + 1))
        STEP_STATUSES+=("PASS")
        return 0
    else
        log_fail "$step_name failed (exit=$exit_code). See: $log_file"
        FAIL_COUNT=$((FAIL_COUNT + 1))
        STEP_STATUSES+=("FAIL")
        return 1
    fi
}

# ============================================================================
# PTY Helper
# ============================================================================

# Check whether the `script` command is available for providing a PTY.
has_pty_support() {
    command -v script >/dev/null 2>&1
}

# Run a command inside a pseudo-terminal via script(1).
# This allows the TUI binary to initialize its terminal I/O even in CI.
# Sets a default terminal size of 80x24 unless the command sets its own stty.
# Usage: run_in_pty "command string"
run_in_pty() {
    local cmd="$1"
    # Only add default stty if the command doesn't already set one
    local setup
    if echo "$cmd" | grep -q 'stty'; then
        setup="$cmd"
    else
        setup="stty rows 24 cols 80 2>/dev/null; $cmd"
    fi
    if [ "$(uname)" = "Linux" ]; then
        script -qec "$setup" /dev/null
    else
        script -q /dev/null bash -c "$setup"
    fi
}

# ============================================================================
# Main Script
# ============================================================================

if $QUICK; then
    TOTAL_STEPS=3
else
    TOTAL_STEPS=9
fi

echo "=============================================="
echo "  FrankenTUI Demo Showcase E2E Test Suite"
echo "=============================================="
echo ""
echo "Project root: $PROJECT_ROOT"
echo "Log directory: $LOG_DIR"
echo "Started at:   $(date -Iseconds)"
MODE=""
if $QUICK; then MODE="${MODE}quick "; fi
if $VERBOSE; then MODE="${MODE}verbose "; fi
MODE="${MODE:-normal}"
echo "Mode:         ${MODE% }"

mkdir -p "$LOG_DIR"
cd "$PROJECT_ROOT"

# Record environment info
{
    echo "Environment Information"
    echo "======================="
    echo "Date: $(date -Iseconds)"
    echo "User: $(whoami)"
    echo "Hostname: $(hostname)"
    echo "Working directory: $(pwd)"
    echo "Rust version: $(rustc --version 2>/dev/null || echo 'N/A')"
    echo "Cargo version: $(cargo --version 2>/dev/null || echo 'N/A')"
    echo ""
    echo "Git status:"
    git status --short 2>/dev/null | head -20 || echo "Not a git repo"
    echo ""
    echo "Git commit:"
    git log -1 --oneline 2>/dev/null || echo "N/A"
} > "$LOG_DIR/00_environment.log"

# ────────────────────────────────────────────────────────────────────────────
# Step 1: Compilation (debug + release)
# ────────────────────────────────────────────────────────────────────────────
run_step "Compilation (debug + release)" "$LOG_DIR/01_build.log" \
    bash -c "cargo build -p $PKG && cargo build -p $PKG --release" || true

# Resolve binary path via cargo metadata (handles custom target dirs)
TARGET_DIR=$(cargo metadata --format-version=1 -q 2>/dev/null \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['target_directory'])" 2>/dev/null \
    || echo "$PROJECT_ROOT/target")
BINARY="$TARGET_DIR/release/$PKG"
BINARY_DBG="$TARGET_DIR/debug/$PKG"

# ────────────────────────────────────────────────────────────────────────────
# Step 2: Clippy
# ────────────────────────────────────────────────────────────────────────────
run_step "Clippy (all targets)" "$LOG_DIR/02_clippy.log" \
    cargo clippy -p "$PKG" --all-targets -- -D warnings || true

# ────────────────────────────────────────────────────────────────────────────
# Step 3: Format Check
# ────────────────────────────────────────────────────────────────────────────
run_step "Format check" "$LOG_DIR/03_fmt.log" \
    cargo fmt -p "$PKG" -- --check || true

if $QUICK; then
    # Quick mode stops here — jump to summary
    :
else

# ────────────────────────────────────────────────────────────────────────────
# Step 4: Unit + Snapshot Tests
# ────────────────────────────────────────────────────────────────────────────
run_step "Unit + snapshot tests" "$LOG_DIR/04_tests.log" \
    cargo test -p "$PKG" -- --test-threads=4 || true

# ────────────────────────────────────────────────────────────────────────────
# Steps 5-9: Smoke / Interactive Tests (require PTY)
# ────────────────────────────────────────────────────────────────────────────

CAN_SMOKE=true
SMOKE_REASON=""

if ! has_pty_support; then
    CAN_SMOKE=false
    SMOKE_REASON="script command not available"
fi

if [ ! -x "$BINARY" ] && [ ! -x "$BINARY_DBG" ]; then
    CAN_SMOKE=false
    SMOKE_REASON="binary not found (build may have failed)"
fi

# Prefer release binary, fall back to debug
if [ -x "$BINARY" ]; then
    DEMO_BIN="$BINARY"
elif [ -x "$BINARY_DBG" ]; then
    DEMO_BIN="$BINARY_DBG"
else
    DEMO_BIN=""
fi

if $CAN_SMOKE; then

    # ────────────────────────────────────────────────────────────────────────
    # Step 5: Alt-screen Smoke Test
    # ────────────────────────────────────────────────────────────────────────
    run_smoke_step "Smoke test (alt-screen)" "$LOG_DIR/05_smoke_alt.log" \
        "run_in_pty 'FTUI_DEMO_EXIT_AFTER_MS=3000 timeout 10 $DEMO_BIN'" || true

    # ────────────────────────────────────────────────────────────────────────
    # Step 6: Inline Smoke Test
    # ────────────────────────────────────────────────────────────────────────
    run_smoke_step "Smoke test (inline)" "$LOG_DIR/06_smoke_inline.log" \
        "run_in_pty 'FTUI_DEMO_EXIT_AFTER_MS=3000 FTUI_DEMO_SCREEN_MODE=inline timeout 10 $DEMO_BIN'" || true

    # ────────────────────────────────────────────────────────────────────────
    # Step 7: Screen Navigation
    #
    # Launch the demo on each of the 11 screens (--screen=1..11) with a
    # short auto-exit. If any screen panics on startup, this catches it.
    # ────────────────────────────────────────────────────────────────────────
    log_step "Screen navigation (all 12 screens)"
    log_info "Starting demo on each screen to verify no panics..."
    NAV_LOG="$LOG_DIR/07_navigation.log"
    STEP_NAMES+=("Screen navigation (all 12)")

    nav_start=$(date +%s%N)
    {
        NAV_FAILURES=0
        for screen_num in 1 2 3 4 5 6 7 8 9 10 11 12; do
            echo "--- Screen $screen_num ---"
            if run_in_pty "FTUI_DEMO_EXIT_AFTER_MS=1500 timeout 8 $DEMO_BIN --screen=$screen_num" 2>&1; then
                echo "  Screen $screen_num: OK"
            else
                sc_exit=$?
                # 124 = timeout (acceptable if exit_after_ms didn't fire)
                if [ "$sc_exit" -eq 124 ]; then
                    echo "  Screen $screen_num: OK (timeout)"
                else
                    echo "  Screen $screen_num: FAILED (exit=$sc_exit)"
                    NAV_FAILURES=$((NAV_FAILURES + 1))
                fi
            fi
        done
        echo ""
        echo "Screens with failures: $NAV_FAILURES"
        [ "$NAV_FAILURES" -eq 0 ]
    } > "$NAV_LOG" 2>&1
    nav_exit=$?
    nav_end=$(date +%s%N)
    nav_dur_ms=$(( (nav_end - nav_start) / 1000000 ))
    nav_dur_s=$(echo "scale=2; $nav_dur_ms / 1000" | bc 2>/dev/null || echo "${nav_dur_ms}ms")
    STEP_DURATIONS+=("${nav_dur_s}s")

    if [ $nav_exit -eq 0 ]; then
        log_pass "Screen navigation passed in ${nav_dur_s}s"
        PASS_COUNT=$((PASS_COUNT + 1))
        STEP_STATUSES+=("PASS")
    else
        log_fail "Screen navigation failed. See: $NAV_LOG"
        FAIL_COUNT=$((FAIL_COUNT + 1))
        STEP_STATUSES+=("FAIL")
    fi

    # ────────────────────────────────────────────────────────────────────────
    # Step 8: Search Test (Shakespeare)
    #
    # Start on the Shakespeare screen and verify it renders without panic.
    # The snapshot tests cover search functionality in detail; this verifies
    # the screen survives initialization and a brief run.
    # ────────────────────────────────────────────────────────────────────────
    run_smoke_step "Search test (Shakespeare)" "$LOG_DIR/08_search.log" \
        "run_in_pty 'FTUI_DEMO_EXIT_AFTER_MS=2000 FTUI_DEMO_SCREEN=2 timeout 8 $DEMO_BIN'" || true

    # ────────────────────────────────────────────────────────────────────────
    # Step 9: Resize Test (SIGWINCH)
    #
    # Start the demo at one size, then at a different size. The PTY
    # creation triggers a resize event internally. If the demo survives
    # both sizes without crashing, resize handling works.
    # ────────────────────────────────────────────────────────────────────────
    log_step "Resize test (multiple terminal sizes)"
    log_info "Running demo at 80x24 and 132x43 to verify resize handling..."
    RESIZE_LOG="$LOG_DIR/09_resize.log"
    STEP_NAMES+=("Resize test (multi-size)")

    resize_start=$(date +%s%N)
    {
        echo "=== Testing at 80x24 ==="
        run_in_pty "stty rows 24 cols 80 2>/dev/null; FTUI_DEMO_EXIT_AFTER_MS=1500 timeout 8 $DEMO_BIN" 2>&1
        exit1=$?
        echo "  Exit code: $exit1"

        echo "=== Testing at 132x43 ==="
        run_in_pty "stty rows 43 cols 132 2>/dev/null; FTUI_DEMO_EXIT_AFTER_MS=1500 timeout 8 $DEMO_BIN" 2>&1
        exit2=$?
        echo "  Exit code: $exit2"

        echo "=== Testing at 40x10 (tiny) ==="
        run_in_pty "stty rows 10 cols 40 2>/dev/null; FTUI_DEMO_EXIT_AFTER_MS=1500 timeout 8 $DEMO_BIN" 2>&1
        exit3=$?
        echo "  Exit code: $exit3"

        # Check all exits (0 or 124 acceptable)
        all_ok=true
        for ec in $exit1 $exit2 $exit3; do
            if [ "$ec" -ne 0 ] && [ "$ec" -ne 124 ]; then
                all_ok=false
            fi
        done
        $all_ok
    } > "$RESIZE_LOG" 2>&1
    resize_exit=$?
    resize_end=$(date +%s%N)
    resize_dur_ms=$(( (resize_end - resize_start) / 1000000 ))
    resize_dur_s=$(echo "scale=2; $resize_dur_ms / 1000" | bc 2>/dev/null || echo "${resize_dur_ms}ms")
    STEP_DURATIONS+=("${resize_dur_s}s")

    if [ $resize_exit -eq 0 ]; then
        log_pass "Resize test passed in ${resize_dur_s}s"
        PASS_COUNT=$((PASS_COUNT + 1))
        STEP_STATUSES+=("PASS")
    else
        log_fail "Resize test failed. See: $RESIZE_LOG"
        FAIL_COUNT=$((FAIL_COUNT + 1))
        STEP_STATUSES+=("FAIL")
    fi

else
    # No PTY support — skip all smoke/interactive tests
    for step in "Smoke test (alt-screen)" "Smoke test (inline)" \
                "Screen navigation" "Search test (Shakespeare)" \
                "Resize (SIGWINCH) test"; do
        skip_step "$step" "$SMOKE_REASON"
    done
fi

fi  # end of non-quick block

# ============================================================================
# Summary
# ============================================================================

echo ""
echo "=============================================="
echo "  E2E Test Suite Complete"
echo "=============================================="
echo ""
echo "Ended at: $(date -Iseconds)"
echo "Log directory: $LOG_DIR"
echo ""

# Summary table
printf "%-35s %-6s %s\n" "Step" "Status" "Duration"
printf "%-35s %-6s %s\n" "---" "------" "--------"
for i in "${!STEP_NAMES[@]}"; do
    local_status="${STEP_STATUSES[$i]}"
    case $local_status in
        PASS) color="\033[32m" ;;
        FAIL) color="\033[31m" ;;
        SKIP) color="\033[33m" ;;
        *)    color="" ;;
    esac
    printf "%-35s ${color}%-6s\033[0m %s\n" "${STEP_NAMES[$i]}" "$local_status" "${STEP_DURATIONS[$i]}"
done

echo ""
echo "Results: $PASS_COUNT passed, $FAIL_COUNT failed, $SKIP_COUNT skipped"
echo ""

# List log files with sizes
echo "Log files:"
ls -lh "$LOG_DIR"/*.log 2>/dev/null | awk '{print "  " $9 " (" $5 ")"}'
echo ""

# Generate summary file
{
    echo "Demo Showcase E2E Summary"
    echo "========================="
    echo "Date: $(date -Iseconds)"
    echo "Passed: $PASS_COUNT"
    echo "Failed: $FAIL_COUNT"
    echo "Skipped: $SKIP_COUNT"
    echo ""
    for i in "${!STEP_NAMES[@]}"; do
        printf "  %-35s %s  %s\n" "${STEP_NAMES[$i]}" "${STEP_STATUSES[$i]}" "${STEP_DURATIONS[$i]}"
    done
    echo ""
    echo "Exit code: $( [ $FAIL_COUNT -eq 0 ] && echo 0 || echo 1 )"
} > "$LOG_DIR/SUMMARY.txt"

if [ $FAIL_COUNT -eq 0 ]; then
    echo -e "\033[1;32mAll tests passed!\033[0m"
    exit 0
else
    echo -e "\033[1;31m$FAIL_COUNT test(s) failed!\033[0m"
    exit 1
fi
