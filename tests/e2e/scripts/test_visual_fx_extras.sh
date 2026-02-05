#!/bin/bash
set -euo pipefail

# E2E: ftui-extras visual_fx sweep (sampling/plasma/metaballs) with JSONL logs.
# bd-1qyn3
#
# Coverage:
# - Effects: sampling (alias), plasma, metaballs
# - Modes: alt + inline
# - Sizes: 80x24, 120x40
# - Deterministic seeds/time
#
# JSONL (E2E schema) emits:
# - env/run_start/run_end/step_start/step_end
# - per-frame assert entries with effect, frame_idx, hash, timing, mode, dims
# - error asserts on mismatches

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

VFX_SEED="${VFX_SEED:-${E2E_SEED:-0}}"
export E2E_SEED="${E2E_SEED:-$VFX_SEED}"

e2e_fixture_init "vfx_extras" "$E2E_SEED" "$E2E_TIME_STEP_MS"

E2E_LOG_DIR="${E2E_LOG_DIR:-/tmp/ftui_e2e_logs}"
E2E_RESULTS_DIR="${E2E_RESULTS_DIR:-$E2E_LOG_DIR/results}"
LOG_FILE="${LOG_FILE:-$E2E_LOG_DIR/vfx_extras.log}"
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
    LOG_FILE="$E2E_LOG_DIR/vfx_extras_missing.log"
    for t in vfx_sampling_alt_80x24 vfx_sampling_inline_80x24 vfx_sampling_alt_120x40 vfx_sampling_inline_120x40 \
             vfx_plasma_alt_80x24 vfx_plasma_inline_80x24 vfx_plasma_alt_120x40 vfx_plasma_inline_120x40 \
             vfx_metaballs_alt_80x24 vfx_metaballs_inline_80x24 vfx_metaballs_alt_120x40 vfx_metaballs_inline_120x40; do
        log_test_skip "$t" "ftui-demo-showcase binary missing"
        record_result "$t" "skipped" 0 "$LOG_FILE" "binary missing"
    done
    exit 0
fi

VFX_LOG_DIR="${VFX_LOG_DIR:-$E2E_LOG_DIR/visual_fx}"
mkdir -p "$VFX_LOG_DIR"

VFX_TICK_MS="${VFX_TICK_MS:-16}"
VFX_FRAMES="${VFX_FRAMES:-12}"
VFX_UI_HEIGHT="${VFX_UI_HEIGHT:-12}"
VFX_PERF="${VFX_PERF:-0}"

modes=("alt" "inline")
sizes=("80x24" "120x40")

effect_actual_for() {
    local label="$1"
    case "$label" in
        sampling)
            # Sampling API is exercised by Metaballs/Plasma. Alias to metaballs for harness.
            echo "metaballs"
            ;;
        *)
            echo "$label"
            ;;
    esac
}

emit_vfx_frame_jsonl() {
    local case_id="$1"
    local effect_label="$2"
    local effect_actual="$3"
    local mode="$4"
    local cols="$5"
    local rows="$6"
    local seed="$7"
    local frame_idx="$8"
    local hash="$9"
    local sim_time="${10}"
    local tick_ms="${11}"

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
            --arg assertion "vfx_frame" \
            --arg status "passed" \
            --arg case_id "$case_id" \
            --arg effect "$effect_label" \
            --arg effect_actual "$effect_actual" \
            --arg mode "$mode" \
            --argjson cols "$cols" \
            --argjson rows "$rows" \
            --argjson seed "$seed_json" \
            --argjson frame_idx "$frame_idx" \
            --arg hash "$hash" \
            --argjson sim_time "$sim_time" \
            --argjson tick_ms "$tick_ms" \
            '{schema_version:$schema_version,type:$type,timestamp:$timestamp,run_id:$run_id,seed:$seed,assertion:$assertion,status:$status,case_id:$case_id,effect:$effect,effect_actual:$effect_actual,mode:$mode,cols:$cols,rows:$rows,frame_idx:$frame_idx,hash:$hash,sim_time:$sim_time,tick_ms:$tick_ms}')"
    else
        jsonl_emit "{\"schema_version\":\"${E2E_JSONL_SCHEMA_VERSION:-e2e-jsonl-v1}\",\"type\":\"assert\",\"timestamp\":\"$(json_escape "$ts")\",\"run_id\":\"$(json_escape "$E2E_RUN_ID")\",\"seed\":${seed_json},\"assertion\":\"vfx_frame\",\"status\":\"passed\",\"case_id\":\"$(json_escape "$case_id")\",\"effect\":\"$(json_escape "$effect_label")\",\"effect_actual\":\"$(json_escape "$effect_actual")\",\"mode\":\"$(json_escape "$mode")\",\"cols\":${cols},\"rows\":${rows},\"frame_idx\":${frame_idx},\"hash\":\"$(json_escape "$hash")\",\"sim_time\":${sim_time},\"tick_ms\":${tick_ms}}"
    fi
}

parse_vfx_frames() {
    local jsonl_path="$1"
    local out_path="$2"
    local expected_frames="$3"
    local expect_cols="$4"
    local expect_rows="$5"
    local expect_seed="$6"
    local expect_effect="$7"

    "$E2E_PYTHON" - "$jsonl_path" "$out_path" "$expected_frames" \
        "$expect_cols" "$expect_rows" "$expect_seed" "$expect_effect" <<'PY'
import json
import sys

path, out_path = sys.argv[1], sys.argv[2]
expected = int(sys.argv[3])
expect_cols = int(sys.argv[4])
expect_rows = int(sys.argv[5])
expect_seed = int(sys.argv[6])
expect_effect = sys.argv[7]

frames = []
with open(path, "r", encoding="utf-8") as handle:
    for line in handle:
        if '"event":"vfx_frame"' not in line:
            continue
        obj = json.loads(line)
        if obj.get("event") != "vfx_frame":
            continue
        frames.append(obj)

if not frames:
    print("no vfx_frame entries found", file=sys.stderr)
    sys.exit(2)

if expected > 0 and len(frames) != expected:
    print(f"frame count mismatch: expected {expected}, got {len(frames)}", file=sys.stderr)
    sys.exit(3)

prev = -1
for idx, frame in enumerate(frames):
    frame_idx = frame.get("frame_idx")
    if not isinstance(frame_idx, int):
        print(f"frame_idx missing or invalid at index {idx}", file=sys.stderr)
        sys.exit(4)
    if frame_idx <= prev:
        print(f"frame_idx not monotonic: {prev} -> {frame_idx}", file=sys.stderr)
        sys.exit(5)
    prev = frame_idx

    for key in ("hash", "time", "cols", "rows", "tick_ms", "seed", "effect"):
        if key not in frame:
            print(f"vfx_frame missing {key}: {frame}", file=sys.stderr)
            sys.exit(6)

    if int(frame["cols"]) != expect_cols or int(frame["rows"]) != expect_rows:
        print(f"dims mismatch: expected {expect_cols}x{expect_rows}, got {frame['cols']}x{frame['rows']}", file=sys.stderr)
        sys.exit(7)
    if int(frame["seed"]) != expect_seed:
        print(f"seed mismatch: expected {expect_seed}, got {frame['seed']}", file=sys.stderr)
        sys.exit(8)
    if str(frame["effect"]) != expect_effect:
        print(f"effect mismatch: expected {expect_effect}, got {frame['effect']}", file=sys.stderr)
        sys.exit(9)

with open(out_path, "w", encoding="utf-8") as out:
    for frame in frames:
        out.write(
            f"{frame['frame_idx']}|{frame['hash']}|{frame['time']}|{frame['effect']}|{frame['cols']}|"
            f"{frame['rows']}|{frame['tick_ms']}|{frame['seed']}\n"
        )

print(len(frames))
PY
}

run_vfx_once() {
    local case_id="$1"
    local effect_actual="$2"
    local mode="$3"
    local cols="$4"
    local rows="$5"
    local seed="$6"
    local run_tag="$7"
    local out_jsonl="$8"
    local out_pty="$9"

    local timeout_sec=6

    local args=(
        "--vfx-harness"
        "--vfx-effect=${effect_actual}"
        "--vfx-tick-ms=${VFX_TICK_MS}"
        "--vfx-frames=${VFX_FRAMES}"
        "--vfx-cols=${cols}"
        "--vfx-rows=${rows}"
        "--vfx-seed=${seed}"
        "--vfx-jsonl=${out_jsonl}"
        "--vfx-run-id=${case_id}-${run_tag}"
        "--screen-mode=${mode}"
    )

    if [[ "$mode" == "inline" ]]; then
        args+=("--ui-height=${VFX_UI_HEIGHT}")
    fi
    if [[ "$VFX_PERF" == "1" ]]; then
        args+=("--vfx-perf")
    fi

    local run_exit=0
    if FTUI_DEMO_DETERMINISTIC=1 \
        FTUI_DEMO_SEED="$seed" \
        FTUI_DEMO_VFX_SEED="$seed" \
        PTY_COLS="$cols" \
        PTY_ROWS="$rows" \
        PTY_TIMEOUT="$timeout_sec" \
        PTY_TEST_NAME="${case_id}-${run_tag}" \
        pty_run "$out_pty" "$DEMO_BIN" "${args[@]}"; then
        run_exit=0
    else
        run_exit=$?
    fi

    pty_record_metadata "$out_pty" "$run_exit" "$cols" "$rows"
    return "$run_exit"
}

run_case() {
    local effect_label="$1"
    local effect_actual="$2"
    local mode="$3"
    local cols="$4"
    local rows="$5"

    local case_id="vfx_${effect_label}_${mode}_${cols}x${rows}"
    LOG_FILE="$VFX_LOG_DIR/${case_id}.log"

    export E2E_CONTEXT_MODE="$mode"
    export E2E_CONTEXT_COLS="$cols"
    export E2E_CONTEXT_ROWS="$rows"
    export E2E_CONTEXT_SEED="${E2E_SEED:-0}"

    log_test_start "$case_id"

    if [[ "$effect_label" != "$effect_actual" ]]; then
        jsonl_assert "vfx_effect_alias_${case_id}" "passed" "${effect_label} -> ${effect_actual}"
    fi

    local run1_jsonl="$VFX_LOG_DIR/${case_id}_run1.jsonl"
    local run1_pty="$VFX_LOG_DIR/${case_id}_run1.pty"
    local run2_jsonl="$VFX_LOG_DIR/${case_id}_run2.jsonl"
    local run2_pty="$VFX_LOG_DIR/${case_id}_run2.pty"
    local frames1="$VFX_LOG_DIR/${case_id}_frames1.txt"
    local frames2="$VFX_LOG_DIR/${case_id}_frames2.txt"

    local start_ms
    start_ms="$(e2e_now_ms)"

    if ! run_vfx_once "$case_id" "$effect_actual" "$mode" "$cols" "$rows" "$E2E_SEED" "a" "$run1_jsonl" "$run1_pty"; then
        local end_ms
        end_ms="$(e2e_now_ms)"
        local duration_ms=$((end_ms - start_ms))
        log_test_fail "$case_id" "vfx harness run1 failed"
        jsonl_assert "vfx_harness_${case_id}" "failed" "run1 failed"
        record_result "$case_id" "failed" "$duration_ms" "$LOG_FILE" "run1 failed"
        return 1
    fi

    if ! parse_vfx_frames "$run1_jsonl" "$frames1" "$VFX_FRAMES" "$cols" "$rows" "$E2E_SEED" "$effect_actual"; then
        local end_ms
        end_ms="$(e2e_now_ms)"
        local duration_ms=$((end_ms - start_ms))
        log_test_fail "$case_id" "vfx jsonl parse failed (run1)"
        jsonl_assert "vfx_jsonl_parse_${case_id}" "failed" "run1 parse error"
        record_result "$case_id" "failed" "$duration_ms" "$LOG_FILE" "run1 parse failed"
        return 1
    fi

    if ! run_vfx_once "$case_id" "$effect_actual" "$mode" "$cols" "$rows" "$E2E_SEED" "b" "$run2_jsonl" "$run2_pty"; then
        local end_ms
        end_ms="$(e2e_now_ms)"
        local duration_ms=$((end_ms - start_ms))
        log_test_fail "$case_id" "vfx harness run2 failed"
        jsonl_assert "vfx_harness_${case_id}" "failed" "run2 failed"
        record_result "$case_id" "failed" "$duration_ms" "$LOG_FILE" "run2 failed"
        return 1
    fi

    if ! parse_vfx_frames "$run2_jsonl" "$frames2" "$VFX_FRAMES" "$cols" "$rows" "$E2E_SEED" "$effect_actual"; then
        local end_ms
        end_ms="$(e2e_now_ms)"
        local duration_ms=$((end_ms - start_ms))
        log_test_fail "$case_id" "vfx jsonl parse failed (run2)"
        jsonl_assert "vfx_jsonl_parse_${case_id}" "failed" "run2 parse error"
        record_result "$case_id" "failed" "$duration_ms" "$LOG_FILE" "run2 parse failed"
        return 1
    fi

    # Emit per-frame JSONL entries (run1)
    while IFS='|' read -r frame_idx hash sim_time effect cols_read rows_read tick_ms seed; do
        if [[ -z "$frame_idx" ]]; then
            continue
        fi
        emit_vfx_frame_jsonl "$case_id" "$effect_label" "$effect_actual" "$mode" "$cols_read" "$rows_read" "$seed" \
            "$frame_idx" "$hash" "$sim_time" "$tick_ms"
    done < "$frames1"

    # Compare hash sequences between runs
    if ! diff -q <(cut -d'|' -f2 "$frames1") <(cut -d'|' -f2 "$frames2") >/dev/null 2>&1; then
        local end_ms
        end_ms="$(e2e_now_ms)"
        local duration_ms=$((end_ms - start_ms))
        log_test_fail "$case_id" "hash mismatch between runs"
        jsonl_assert "vfx_hash_stability_${case_id}" "failed" "hash mismatch between runs"
        record_result "$case_id" "failed" "$duration_ms" "$LOG_FILE" "hash mismatch"
        return 1
    fi

    jsonl_assert "vfx_hash_stability_${case_id}" "passed" "hashes stable across runs"
    jsonl_assert "artifact_vfx_jsonl_${case_id}_run1" "pass" "vfx_jsonl=$run1_jsonl"
    jsonl_assert "artifact_vfx_jsonl_${case_id}_run2" "pass" "vfx_jsonl=$run2_jsonl"

    local end_ms
    end_ms="$(e2e_now_ms)"
    local duration_ms=$((end_ms - start_ms))
    log_test_pass "$case_id"
    record_result "$case_id" "passed" "$duration_ms" "$LOG_FILE"
    return 0
}

FAILURES=0

for effect_label in sampling plasma metaballs; do
    effect_actual="$(effect_actual_for "$effect_label")"
    for mode in "${modes[@]}"; do
        for size in "${sizes[@]}"; do
            cols="${size%x*}"
            rows="${size#*x}"
            if ! run_case "$effect_label" "$effect_actual" "$mode" "$cols" "$rows"; then
                FAILURES=$((FAILURES + 1))
            fi
        done
    done
done

run_end_ms="$(e2e_now_ms)"
run_duration_ms=$((run_end_ms - ${E2E_RUN_START_MS:-0}))
if [[ "$FAILURES" -eq 0 ]]; then
    jsonl_run_end "passed" "$run_duration_ms" 0
else
    jsonl_run_end "failed" "$run_duration_ms" "$FAILURES"
fi

exit "$FAILURES"
