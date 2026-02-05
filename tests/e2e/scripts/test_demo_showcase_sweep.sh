#!/bin/bash
set -euo pipefail

# E2E: Demo-showcase screen sweep with JSONL logs (bd-34m9w)
#
# Coverage:
# - Screens 1..38 (configurable)
# - Modes: alt + inline
# - Sizes: 80x24, 120x40
# - Deterministic seeds/time
#
# Logs:
# - E2E JSONL schema events via logging.sh
# - Per-screen hash asserts and stability checks

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB_DIR="$SCRIPT_DIR/../lib"

# shellcheck source=/dev/null
source "$LIB_DIR/common.sh"
# shellcheck source=/dev/null
source "$LIB_DIR/logging.sh"
# shellcheck source=/dev/null
source "$LIB_DIR/pty.sh"

export E2E_DETERMINISTIC="${E2E_DETERMINISTIC:-1}"
export E2E_TIME_STEP_MS="${E2E_TIME_STEP_MS:-100}"

SWEEP_SEED="${SWEEP_SEED:-${E2E_SEED:-0}}"
export E2E_SEED="${E2E_SEED:-$SWEEP_SEED}"

e2e_fixture_init "demo_showcase_sweep" "$E2E_SEED" "$E2E_TIME_STEP_MS"

E2E_LOG_DIR="${E2E_LOG_DIR:-/tmp/ftui_e2e_logs}"
E2E_RESULTS_DIR="${E2E_RESULTS_DIR:-$E2E_LOG_DIR/results}"
LOG_FILE="${LOG_FILE:-$E2E_LOG_DIR/demo_showcase_sweep.log}"
E2E_JSONL_FILE="${E2E_JSONL_FILE:-$E2E_LOG_DIR/e2e.jsonl}"
E2E_RUN_CMD="${E2E_RUN_CMD:-$0 $*}"
export E2E_LOG_DIR E2E_RESULTS_DIR LOG_FILE E2E_JSONL_FILE E2E_RUN_CMD
export E2E_RUN_START_MS="${E2E_RUN_START_MS:-$(e2e_run_start_ms)}"

mkdir -p "$E2E_LOG_DIR" "$E2E_RESULTS_DIR"
jsonl_init
jsonl_assert "artifact_log_dir" "pass" "log_dir=$E2E_LOG_DIR"

if [[ -z "$E2E_PYTHON" ]]; then
    log_error "python3/python is required for PTY helpers"
    exit 1
fi

resolve_demo_bin() {
    if [[ -n "${FTUI_DEMO_BIN:-}" && -x "$FTUI_DEMO_BIN" ]]; then
        echo "$FTUI_DEMO_BIN"
        return 0
    fi
    if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
        local shared_debug="$CARGO_TARGET_DIR/debug/ftui-demo-showcase"
        local shared_release="$CARGO_TARGET_DIR/release/ftui-demo-showcase"
        if [[ -x "$shared_debug" ]]; then
            echo "$shared_debug"
            return 0
        fi
        if [[ -x "$shared_release" ]]; then
            echo "$shared_release"
            return 0
        fi
    fi
    local debug_bin="$PROJECT_ROOT/target/debug/ftui-demo-showcase"
    local release_bin="$PROJECT_ROOT/target/release/ftui-demo-showcase"
    if [[ -x "$debug_bin" ]]; then
        echo "$debug_bin"
        return 0
    fi
    if [[ -x "$release_bin" ]]; then
        echo "$release_bin"
        return 0
    fi
    return 1
}

ensure_demo_bin() {
    local bin=""
    if bin="$(resolve_demo_bin)"; then
        echo "$bin"
        return 0
    fi
    log_info "Building ftui-demo-showcase (debug)..." >&2
    (cd "$PROJECT_ROOT" && cargo build -p ftui-demo-showcase >/dev/null)
    if bin="$(resolve_demo_bin)"; then
        echo "$bin"
        return 0
    fi
    return 1
}

DEMO_BIN="$(ensure_demo_bin || true)"
if [[ -z "$DEMO_BIN" ]]; then
    LOG_FILE="$E2E_LOG_DIR/demo_showcase_missing.log"
    log_test_skip "demo_showcase_sweep" "ftui-demo-showcase binary missing"
    record_result "demo_showcase_sweep" "skipped" 0 "$LOG_FILE" "binary missing"
    exit 0
fi

SWEEP_LOG_DIR="${SWEEP_LOG_DIR:-$E2E_LOG_DIR/demo_showcase_sweep}"
mkdir -p "$SWEEP_LOG_DIR"

SWEEP_EXIT_AFTER_MS="${SWEEP_EXIT_AFTER_MS:-800}"
SWEEP_UI_HEIGHT="${SWEEP_UI_HEIGHT:-12}"
SWEEP_TICK_MS="${SWEEP_TICK_MS:-16}"

modes=("alt" "inline")
sizes=("80x24" "120x40")

ALL_SCREENS=(
    1 2 3 4 5 6 7 8 9 10
    11 12 13 14 15 16 17 18 19 20
    21 22 23 24 25 26 27 28 29 30
    31 32 33 34 35 36 37 38
)

parse_sweep_screens() {
    local raw="${SWEEP_SCREENS:-}"
    if [[ -z "$raw" ]]; then
        printf '%s\n' "${ALL_SCREENS[@]}"
        return 0
    fi
    raw="${raw//,/ }"
    for token in $raw; do
        if [[ "$token" =~ ^[0-9]+$ ]]; then
            printf '%s\n' "$token"
        fi
    done
}

emit_screen_hash_jsonl() {
    local case_id="$1"
    local screen_id="$2"
    local mode="$3"
    local cols="$4"
    local rows="$5"
    local seed="$6"
    local hash="$7"
    local output_file="$8"
    local duration_ms="$9"

    local ts
    ts="$(e2e_timestamp)"
    local seed_json="null"
    if [[ -n "$seed" ]]; then seed_json="$seed"; fi

    if command -v jq >/dev/null 2>&1; then
        jsonl_emit "$(jq -nc \
            --arg schema_version "${E2E_JSONL_SCHEMA_VERSION:-e2e-jsonl-v1}" \
            --arg type "assert" \
            --arg timestamp "$ts" \
            --arg run_id "$E2E_RUN_ID" \
            --arg assertion "demo_screen_hash" \
            --arg status "passed" \
            --arg case_id "$case_id" \
            --arg screen "$screen_id" \
            --arg mode "$mode" \
            --argjson cols "$cols" \
            --argjson rows "$rows" \
            --argjson seed "$seed_json" \
            --arg hash "$hash" \
            --arg output_file "$output_file" \
            --argjson duration_ms "$duration_ms" \
            '{schema_version:$schema_version,type:$type,timestamp:$timestamp,run_id:$run_id,seed:$seed,assertion:$assertion,status:$status,case_id:$case_id,screen:$screen,mode:$mode,cols:$cols,rows:$rows,hash:$hash,output_file:$output_file,duration_ms:$duration_ms}')"
    else
        jsonl_emit "{\"schema_version\":\"${E2E_JSONL_SCHEMA_VERSION:-e2e-jsonl-v1}\",\"type\":\"assert\",\"timestamp\":\"$(json_escape "$ts")\",\"run_id\":\"$(json_escape "$E2E_RUN_ID")\",\"seed\":${seed_json},\"assertion\":\"demo_screen_hash\",\"status\":\"passed\",\"case_id\":\"$(json_escape "$case_id")\",\"screen\":\"$(json_escape "$screen_id")\",\"mode\":\"$(json_escape "$mode")\",\"cols\":${cols},\"rows\":${rows},\"hash\":\"$(json_escape "$hash")\",\"output_file\":\"$(json_escape "$output_file")\",\"duration_ms\":${duration_ms}}"
    fi
}

run_demo_once() {
    local screen_id="$1"
    local mode="$2"
    local cols="$3"
    local rows="$4"
    local seed="$5"
    local out_pty="$6"
    local run_tag="$7"

    local args=(
        "--screen=${screen_id}"
        "--screen-mode=${mode}"
        "--exit-after-ms=${SWEEP_EXIT_AFTER_MS}"
        "--no-mouse"
    )
    if [[ "$mode" == "inline" ]]; then
        args+=("--ui-height=${SWEEP_UI_HEIGHT}")
    fi

    local run_exit=0
    if FTUI_DEMO_DETERMINISTIC=1 \
        FTUI_DEMO_SEED="$seed" \
        FTUI_DEMO_TICK_MS="$SWEEP_TICK_MS" \
        PTY_COLS="$cols" \
        PTY_ROWS="$rows" \
        PTY_TIMEOUT=6 \
        PTY_TEST_NAME="demo_screen_${screen_id}_${mode}_${cols}x${rows}_${run_tag}" \
        pty_run "$out_pty" "$DEMO_BIN" "${args[@]}"; then
        run_exit=0
    else
        run_exit=$?
    fi

    pty_record_metadata "$out_pty" "$run_exit" "$cols" "$rows"
    return "$run_exit"
}

run_case() {
    local screen_id="$1"
    local mode="$2"
    local cols="$3"
    local rows="$4"

    local case_id="demo_screen_${screen_id}_${mode}_${cols}x${rows}"
    LOG_FILE="$SWEEP_LOG_DIR/${case_id}.log"

    export E2E_CONTEXT_MODE="$mode"
    export E2E_CONTEXT_COLS="$cols"
    export E2E_CONTEXT_ROWS="$rows"
    export E2E_CONTEXT_SEED="${E2E_SEED:-0}"

    log_test_start "$case_id"

    local start_ms
    start_ms="$(e2e_now_ms)"

    local out1="$SWEEP_LOG_DIR/${case_id}_run1.pty"
    local out2="$SWEEP_LOG_DIR/${case_id}_run2.pty"

    if ! run_demo_once "$screen_id" "$mode" "$cols" "$rows" "$E2E_SEED" "$out1" "a"; then
        local duration_ms=$(( $(e2e_now_ms) - start_ms ))
        log_test_fail "$case_id" "run1 failed"
        jsonl_assert "demo_screen_run1_${case_id}" "failed" "run1 failed"
        record_result "$case_id" "failed" "$duration_ms" "$LOG_FILE" "run1 failed"
        return 1
    fi

    local hash1
    hash1="$(sha256_file "$out1" || true)"
    if [[ -z "$hash1" ]]; then
        local duration_ms=$(( $(e2e_now_ms) - start_ms ))
        log_test_fail "$case_id" "run1 hash missing"
        jsonl_assert "demo_screen_hash_${case_id}" "failed" "hash missing"
        record_result "$case_id" "failed" "$duration_ms" "$LOG_FILE" "hash missing"
        return 1
    fi

    if ! run_demo_once "$screen_id" "$mode" "$cols" "$rows" "$E2E_SEED" "$out2" "b"; then
        local duration_ms=$(( $(e2e_now_ms) - start_ms ))
        log_test_fail "$case_id" "run2 failed"
        jsonl_assert "demo_screen_run2_${case_id}" "failed" "run2 failed"
        record_result "$case_id" "failed" "$duration_ms" "$LOG_FILE" "run2 failed"
        return 1
    fi

    local hash2
    hash2="$(sha256_file "$out2" || true)"
    if [[ -z "$hash2" ]]; then
        local duration_ms=$(( $(e2e_now_ms) - start_ms ))
        log_test_fail "$case_id" "run2 hash missing"
        jsonl_assert "demo_screen_hash_${case_id}" "failed" "hash missing"
        record_result "$case_id" "failed" "$duration_ms" "$LOG_FILE" "hash missing"
        return 1
    fi

    if [[ "$hash1" != "$hash2" ]]; then
        local duration_ms=$(( $(e2e_now_ms) - start_ms ))
        log_test_fail "$case_id" "hash mismatch"
        jsonl_assert "demo_screen_hash_stability_${case_id}" "failed" "${hash1} != ${hash2}"
        record_result "$case_id" "failed" "$duration_ms" "$LOG_FILE" "hash mismatch"
        return 1
    fi

    local duration_ms=$(( $(e2e_now_ms) - start_ms ))
    jsonl_assert "artifact_demo_pty_${case_id}_run1" "pass" "pty=$out1"
    jsonl_assert "artifact_demo_pty_${case_id}_run2" "pass" "pty=$out2"
    jsonl_assert "demo_screen_hash_stability_${case_id}" "passed" "hashes stable"
    emit_screen_hash_jsonl "$case_id" "$screen_id" "$mode" "$cols" "$rows" "$E2E_SEED" "$hash1" "$out1" "$duration_ms"

    log_test_pass "$case_id"
    record_result "$case_id" "passed" "$duration_ms" "$LOG_FILE"
    return 0
}

FAILURES=0
STOP=0

while IFS= read -r screen_id; do
    if [[ -z "$screen_id" ]]; then
        continue
    fi
    for mode in "${modes[@]}"; do
        for size in "${sizes[@]}"; do
            cols="${size%x*}"
            rows="${size#*x}"
            if ! run_case "$screen_id" "$mode" "$cols" "$rows"; then
                FAILURES=$((FAILURES + 1))
                STOP=1
                break
            fi
        done
        if [[ "$STOP" -eq 1 ]]; then
            break
        fi
    done
    if [[ "$STOP" -eq 1 ]]; then
        break
    fi
done < <(parse_sweep_screens)

run_end_ms="$(e2e_now_ms)"
run_duration_ms=$((run_end_ms - ${E2E_RUN_START_MS:-0}))
if [[ "$FAILURES" -eq 0 ]]; then
    jsonl_run_end "passed" "$run_duration_ms" 0
else
    jsonl_run_end "failed" "$run_duration_ms" "$FAILURES"
fi

exit "$FAILURES"
