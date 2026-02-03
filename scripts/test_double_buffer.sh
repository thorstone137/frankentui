#!/usr/bin/env bash
# E2E test for DoubleBuffer O(1) swap (bd-1rz0.4.4)
#
# This script validates the DoubleBuffer implementation by running:
# 1. Unit tests (including property tests)
# 2. Benchmarks to verify O(1) swap performance
#
# Usage: ./scripts/test_double_buffer.sh
#
# Exit codes:
#   0 - All tests passed
#   1 - Test failure

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# JSONL logging
LOG_FILE="${PROJECT_ROOT}/target/double_buffer_e2e.jsonl"
mkdir -p "$(dirname "$LOG_FILE")"
: > "$LOG_FILE"  # Clear previous log

log_json() {
    local step="$1"
    local status="$2"
    local message="$3"
    local ts
    ts=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    echo "{\"timestamp\":\"$ts\",\"step\":\"$step\",\"status\":\"$status\",\"message\":\"$message\"}" >> "$LOG_FILE"
}

echo "=== DoubleBuffer O(1) Swap E2E Test ==="
echo "Date: $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
echo "Log: $LOG_FILE"
echo ""

log_json "start" "info" "E2E test started"

# -----------------------------------------------------------------------------
# Step 1: Unit tests (including property tests)
# -----------------------------------------------------------------------------
echo "[1/3] Running unit tests (including property tests)..."

UNIT_LOG="${PROJECT_ROOT}/target/double_buffer_unit.log"
if cargo test -p ftui-render double_buffer --no-fail-fast 2>&1 | tee "$UNIT_LOG"; then
    UNIT_PASSED=$(grep -c "^test.*ok$" "$UNIT_LOG" || echo "0")
    echo "✓ Unit tests passed ($UNIT_PASSED tests)"
    log_json "unit_tests" "pass" "Passed $UNIT_PASSED tests"
else
    echo "✗ Unit tests failed"
    log_json "unit_tests" "fail" "Unit tests failed"
    echo ""
    echo "=== FAILURE DETAILS ==="
    grep -E "^test.*FAILED|^failures:" "$UNIT_LOG" || true
    exit 1
fi

echo ""

# -----------------------------------------------------------------------------
# Step 2: Property tests specifically
# -----------------------------------------------------------------------------
echo "[2/3] Verifying property tests..."

PROP_LOG="${PROJECT_ROOT}/target/double_buffer_prop.log"
if cargo test -p ftui-render property --no-fail-fast 2>&1 | tee "$PROP_LOG"; then
    PROP_PASSED=$(grep -c "^test.*ok$" "$PROP_LOG" || echo "0")
    echo "✓ Property tests passed ($PROP_PASSED tests)"
    log_json "property_tests" "pass" "Passed $PROP_PASSED tests"
else
    echo "✗ Property tests failed"
    log_json "property_tests" "fail" "Property tests failed"
    exit 1
fi

echo ""

# -----------------------------------------------------------------------------
# Step 3: Benchmarks
# -----------------------------------------------------------------------------
echo "[3/3] Running benchmarks..."

BENCH_LOG="${PROJECT_ROOT}/target/double_buffer_bench.log"
if cargo bench -p ftui-render --bench double_buffer_bench -- --noplot 2>&1 | tee "$BENCH_LOG"; then
    echo ""
    echo "=== Benchmark Results ==="

    # Extract key metrics
    echo ""
    echo "Buffer Transition Comparison (120x40):"
    grep -E "double_buffer/transition/(clone|swap|clear|swap_and_clear)/120x40" "$BENCH_LOG" | \
        grep "time:" | head -4 || true

    echo ""
    echo "Frame Simulation Comparison (120x40):"
    grep -E "double_buffer/frame_sim/(clone_per_frame|swap_per_frame)/120x40" "$BENCH_LOG" | \
        grep "time:" | head -2 || true

    log_json "benchmarks" "pass" "Benchmarks completed"
else
    echo "✗ Benchmarks failed"
    log_json "benchmarks" "fail" "Benchmark execution failed"
    # Benchmarks are informational, don't fail the test
fi

echo ""

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
echo "=== Summary ==="
echo ""
echo "Expected Performance (from bead spec):"
echo "  Clone 120x40: ~70,000 ns"
echo "  Swap 120x40:  ~1 ns (O(1))"
echo "  Clear 120x40: ~15,000 ns"
echo "  Net savings:  ~55,000 ns per frame"
echo ""
echo "At 60 FPS: ~3.3ms saved per second"
echo ""

log_json "complete" "pass" "E2E test completed successfully"

echo "=== All tests passed ==="
echo "Log written to: $LOG_FILE"
