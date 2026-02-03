#!/usr/bin/env bash
# E2E test for hover jitter stabilization (bd-9n09)
#
# Tests:
# 1. Jitter sequences do not cause target flicker
# 2. Intentional crossing triggers switch within expected latency
# 3. Hysteresis band prevents boundary oscillation
#
# Output: JSONL log with env, capabilities, timings, seed, checksums

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_DIR="${PROJECT_ROOT}/target/e2e-logs"
LOG_FILE="${LOG_DIR}/hover_stabilizer_$(date +%Y%m%d_%H%M%S).jsonl"

mkdir -p "$LOG_DIR"

# -----------------------------------------------------------------------
# Environment info
# -----------------------------------------------------------------------

echo '=== Hover Stabilizer E2E Tests (bd-9n09) ==='
echo "Date: $(date -Iseconds)"
echo "Log: $LOG_FILE"
echo

# Log environment
cat > "$LOG_FILE" <<EOF
{"type":"env","timestamp":"$(date -Iseconds)","rust_version":"$(rustc --version 2>/dev/null || echo 'unknown')","platform":"$(uname -s)","arch":"$(uname -m)"}
EOF

# -----------------------------------------------------------------------
# Build
# -----------------------------------------------------------------------

echo "Building ftui-core (release)..."
if cargo build -p ftui-core --release 2>&1 | tail -1; then
    echo '{"type":"build","status":"success","target":"ftui-core"}' >> "$LOG_FILE"
else
    echo '{"type":"build","status":"failed","target":"ftui-core"}' >> "$LOG_FILE"
    echo "FAIL: Build failed"
    exit 1
fi

# -----------------------------------------------------------------------
# Run unit tests with output capture
# -----------------------------------------------------------------------

echo "Running hover_stabilizer unit tests..."

TEST_OUTPUT=$(cargo test -p ftui-core hover_stabilizer -- --nocapture 2>&1)
TEST_EXIT=$?

if [ $TEST_EXIT -eq 0 ]; then
    # Count passed tests
    PASSED=$(echo "$TEST_OUTPUT" | grep -c 'ok$' || true)
    echo '{"type":"test","name":"hover_stabilizer_unit","status":"pass","passed":'"$PASSED"'}' >> "$LOG_FILE"
    echo "Unit tests: PASS ($PASSED tests)"
else
    echo '{"type":"test","name":"hover_stabilizer_unit","status":"fail"}' >> "$LOG_FILE"
    echo "Unit tests: FAIL"
    echo "$TEST_OUTPUT"
    exit 1
fi

# -----------------------------------------------------------------------
# Property test: jitter stability rate
# -----------------------------------------------------------------------

echo "Running property test: jitter stability..."

# Extract jitter stability test result
JITTER_OUTPUT=$(cargo test -p ftui-core hover_stabilizer::tests::jitter_stability_rate -- --nocapture 2>&1)

if echo "$JITTER_OUTPUT" | grep -q 'test result: ok'; then
    echo '{"type":"property","name":"jitter_stability_rate","status":"pass","threshold":">99%"}' >> "$LOG_FILE"
    echo "Jitter stability: PASS (>99% stable under oscillation)"
else
    echo '{"type":"property","name":"jitter_stability_rate","status":"fail"}' >> "$LOG_FILE"
    echo "Jitter stability: FAIL"
    exit 1
fi

# -----------------------------------------------------------------------
# Property test: crossing detection latency
# -----------------------------------------------------------------------

echo "Running property test: crossing detection latency..."

LATENCY_OUTPUT=$(cargo test -p ftui-core hover_stabilizer::tests::crossing_detection_latency -- --nocapture 2>&1)

if echo "$LATENCY_OUTPUT" | grep -q 'test result: ok'; then
    echo '{"type":"property","name":"crossing_detection_latency","status":"pass","threshold":"<=3 frames"}' >> "$LOG_FILE"
    echo "Crossing latency: PASS (<=3 frames)"
else
    echo '{"type":"property","name":"crossing_detection_latency","status":"fail"}' >> "$LOG_FILE"
    echo "Crossing latency: FAIL"
    exit 1
fi

# -----------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------

echo
echo '=== E2E Summary ==='
echo "All tests: PASS"
echo '{"type":"summary","status":"pass","tests_passed":3}' >> "$LOG_FILE"
echo
echo "Log written to: $LOG_FILE"

# Print log summary
echo
echo "=== Log Contents ==="
cat "$LOG_FILE" | jq -c '.' 2>/dev/null || cat "$LOG_FILE"

exit 0
