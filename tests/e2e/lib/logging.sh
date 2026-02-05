#!/bin/bash
set -euo pipefail

LOG_LEVEL="${LOG_LEVEL:-INFO}"
E2E_LOG_DIR="${E2E_LOG_DIR:-/tmp/ftui_e2e_logs}"
E2E_RESULTS_DIR="${E2E_RESULTS_DIR:-/tmp/ftui_e2e_results}"
LOG_FILE="${LOG_FILE:-$E2E_LOG_DIR/e2e.log}"
E2E_JSONL_FILE="${E2E_JSONL_FILE:-$E2E_LOG_DIR/e2e.jsonl}"
E2E_JSONL_DISABLE="${E2E_JSONL_DISABLE:-0}"
E2E_JSONL_SCHEMA_VERSION="${E2E_JSONL_SCHEMA_VERSION:-e2e-jsonl-v1}"
E2E_JSONL_VALIDATE="${E2E_JSONL_VALIDATE:-}"
E2E_JSONL_VALIDATE_MODE="${E2E_JSONL_VALIDATE_MODE:-}"
E2E_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
E2E_JSONL_SCHEMA_FILE="${E2E_JSONL_SCHEMA_FILE:-$E2E_LIB_DIR/e2e_jsonl_schema.json}"
E2E_JSONL_VALIDATOR="${E2E_JSONL_VALIDATOR:-$E2E_LIB_DIR/validate_jsonl.py}"
E2E_DETERMINISTIC="${E2E_DETERMINISTIC:-1}"
E2E_SEED="${E2E_SEED:-0}"
E2E_TIME_STEP_MS="${E2E_TIME_STEP_MS:-100}"

e2e_is_deterministic() {
    [[ "${E2E_DETERMINISTIC:-1}" == "1" ]]
}

e2e_state_dir() {
    local dir="${E2E_STATE_DIR:-$E2E_LOG_DIR}"
    if [[ -z "$dir" ]]; then
        dir="/tmp/ftui_e2e_state"
    fi
    mkdir -p "$dir"
    printf '%s' "$dir"
}

e2e_counter_file() {
    local name="$1"
    local dir
    dir="$(e2e_state_dir)"
    printf '%s/.e2e_%s_counter' "$dir" "$name"
}

e2e_counter_read() {
    local name="$1"
    local env_var="${2:-}"
    local default="${3:-0}"
    local file value
    file="$(e2e_counter_file "$name")"
    value=""
    if [[ -f "$file" ]]; then
        value="$(cat "$file" 2>/dev/null || true)"
    fi
    if [[ -z "$value" && -n "$env_var" && -n "${!env_var:-}" ]]; then
        value="${!env_var}"
    fi
    if [[ -z "$value" || ! "$value" =~ ^[0-9]+$ ]]; then
        value="$default"
    fi
    printf '%s' "$value"
}

e2e_counter_set() {
    local name="$1"
    local value="$2"
    local env_var="${3:-}"
    local file
    file="$(e2e_counter_file "$name")"
    printf '%s' "$value" > "$file"
    if [[ -n "$env_var" ]]; then
        export "$env_var"="$value"
    fi
}

e2e_counter_next() {
    local name="$1"
    local step="${2:-1}"
    local env_var="${3:-}"
    local default="${4:-0}"
    local value
    value="$(e2e_counter_read "$name" "$env_var" "$default")"
    if [[ -z "$step" || ! "$step" =~ ^[0-9]+$ ]]; then
        step=1
    fi
    value=$((value + step))
    e2e_counter_set "$name" "$value" "$env_var"
    printf '%s' "$value"
}

e2e_timestamp() {
    if e2e_is_deterministic; then
        local seq
        seq="$(e2e_counter_next "ts" 1 "E2E_TS_COUNTER" 0)"
        printf 'T%06d' "$seq"
        return 0
    fi
    date -Iseconds
}

e2e_run_id() {
    if [[ -n "${E2E_RUN_ID:-}" ]]; then
        printf '%s' "$E2E_RUN_ID"
        return 0
    fi
    if e2e_is_deterministic; then
        local seed="${E2E_SEED:-0}"
        local seq
        seq="$(e2e_counter_next "run_seq" 1 "E2E_RUN_SEQ" 0)"
        printf 'det_%s_%s' "$seed" "$seq"
        return 0
    fi
    printf 'run_%s_%s' "$(date +%Y%m%d_%H%M%S)" "$$"
}

e2e_determinism_self_test() {
    local had_det="${E2E_DETERMINISTIC+x}"
    local had_seed="${E2E_SEED+x}"
    local had_run_id="${E2E_RUN_ID+x}"
    local had_run_seq="${E2E_RUN_SEQ+x}"
    local had_ts="${E2E_TS_COUNTER+x}"
    local had_ms="${E2E_MS_COUNTER+x}"
    local prev_det="${E2E_DETERMINISTIC:-}"
    local prev_seed="${E2E_SEED:-}"
    local prev_run_id="${E2E_RUN_ID:-}"
    local prev_run_seq="${E2E_RUN_SEQ:-}"
    local prev_ts="${E2E_TS_COUNTER:-}"
    local prev_ms="${E2E_MS_COUNTER:-}"
    local prev_run_seq_file prev_ts_file prev_ms_file
    prev_run_seq_file="$(e2e_counter_read "run_seq" "E2E_RUN_SEQ" 0)"
    prev_ts_file="$(e2e_counter_read "ts" "E2E_TS_COUNTER" 0)"
    prev_ms_file="$(e2e_counter_read "ms" "E2E_MS_COUNTER" 0)"

    export E2E_DETERMINISTIC="1"
    export E2E_SEED="${E2E_SEED:-0}"
    unset E2E_RUN_ID
    export E2E_RUN_SEQ="0"
    export E2E_TS_COUNTER="0"
    export E2E_MS_COUNTER="0"
    e2e_counter_set "run_seq" 0 "E2E_RUN_SEQ"
    e2e_counter_set "ts" 0 "E2E_TS_COUNTER"
    e2e_counter_set "ms" 0 "E2E_MS_COUNTER"

    local run1 run2 ts1 ts2 ms1 ms2 step
    run1="$(e2e_run_id)"
    run2="$(e2e_run_id)"
    ts1="$(e2e_timestamp)"
    ts2="$(e2e_timestamp)"
    ms1="$(e2e_now_ms)"
    ms2="$(e2e_now_ms)"

    step="${E2E_TIME_STEP_MS:-100}"
    local status=0
    if [[ "$run1" == "$run2" ]]; then
        echo "E2E determinism self-test failed: run_id did not advance ($run1)" >&2
        status=1
    fi
    if [[ "$ts1" != "T000001" || "$ts2" != "T000002" ]]; then
        echo "E2E determinism self-test failed: timestamp did not advance ($ts1/$ts2)" >&2
        status=1
    fi
    if [[ "$ms1" != "$step" || "$ms2" != "$((step * 2))" ]]; then
        echo "E2E determinism self-test failed: ms counter not step-aligned ($ms1/$ms2, step=$step)" >&2
        status=1
    fi

    if [[ -n "$had_det" ]]; then export E2E_DETERMINISTIC="$prev_det"; else unset E2E_DETERMINISTIC; fi
    if [[ -n "$had_seed" ]]; then export E2E_SEED="$prev_seed"; else unset E2E_SEED; fi
    if [[ -n "$had_run_id" ]]; then export E2E_RUN_ID="$prev_run_id"; else unset E2E_RUN_ID; fi
    if [[ -n "$had_run_seq" ]]; then
        export E2E_RUN_SEQ="$prev_run_seq"
        e2e_counter_set "run_seq" "$prev_run_seq_file" "E2E_RUN_SEQ"
    else
        unset E2E_RUN_SEQ
        e2e_counter_set "run_seq" "$prev_run_seq_file"
    fi
    if [[ -n "$had_ts" ]]; then
        export E2E_TS_COUNTER="$prev_ts"
        e2e_counter_set "ts" "$prev_ts_file" "E2E_TS_COUNTER"
    else
        unset E2E_TS_COUNTER
        e2e_counter_set "ts" "$prev_ts_file"
    fi
    if [[ -n "$had_ms" ]]; then
        export E2E_MS_COUNTER="$prev_ms"
        e2e_counter_set "ms" "$prev_ms_file" "E2E_MS_COUNTER"
    else
        unset E2E_MS_COUNTER
        e2e_counter_set "ms" "$prev_ms_file"
    fi

    return $status
}

e2e_run_start_ms() {
    if e2e_is_deterministic; then
        printf '0'
        return 0
    fi
    date +%s%3N
}

e2e_now_ms() {
    if e2e_is_deterministic; then
        local step="${E2E_TIME_STEP_MS:-100}"
        local seq
        seq="$(e2e_counter_next "ms" "$step" "E2E_MS_COUNTER" 0)"
        printf '%s' "$seq"
        return 0
    fi
    date +%s%3N
}

e2e_log_stamp() {
    if e2e_is_deterministic; then
        local seed="${E2E_SEED:-0}"
        printf 'det_%s' "$seed"
        return 0
    fi
    date +%Y%m%d_%H%M%S
}

e2e_hash_key() {
    local mode="$1"
    local cols="$2"
    local rows="$3"
    local seed="${4:-${E2E_SEED:-0}}"
    printf '%s-%sx%s-seed%s' "$mode" "$cols" "$rows" "$seed"
}

json_escape() {
    printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'
}

jsonl_emit() {
    local json="$1"
    if [[ "$E2E_JSONL_DISABLE" == "1" ]]; then
        return 0
    fi
    mkdir -p "$(dirname "$E2E_JSONL_FILE")"
    echo "$json" >> "$E2E_JSONL_FILE"
}

jsonl_should_validate() {
    if [[ "${E2E_JSONL_VALIDATE:-}" == "1" ]]; then
        return 0
    fi
    if [[ "${E2E_JSONL_VALIDATE_MODE:-}" == "strict" || "${E2E_JSONL_VALIDATE_MODE:-}" == "warn" ]]; then
        return 0
    fi
    if [[ -n "${CI:-}" ]]; then
        return 0
    fi
    return 1
}

jsonl_validate_line() {
    local line="$1"
    local type
    type="$(jq -r '.type // .event // empty' <<<"$line" 2>/dev/null || true)"
    if [[ -z "$type" ]]; then
        return 1
    fi
    local ts
    ts="$(jq -r '.timestamp // .ts // empty' <<<"$line" 2>/dev/null || true)"
    if [[ -z "$ts" ]]; then
        return 1
    fi
    local run_id
    run_id="$(jq -r '.run_id // empty' <<<"$line" 2>/dev/null || true)"
    if [[ -z "$run_id" ]]; then
        return 1
    fi

    if ! jq -e 'has("schema_version")' >/dev/null <<<"$line"; then
        return 1
    fi

    case "$type" in
        env)
            jq -e 'has("seed") and has("deterministic") and has("term") and has("colorterm") and has("no_color")' >/dev/null <<<"$line"
            ;;
        run_start)
            jq -e 'has("seed") and has("command") and has("log_dir") and has("results_dir")' >/dev/null <<<"$line"
            ;;
        run_end)
            jq -e 'has("seed") and has("status") and has("duration_ms") and has("failed_count")' >/dev/null <<<"$line"
            ;;
        step_start)
            jq -e 'has("step") and has("mode") and has("cols") and has("rows") and has("seed")' >/dev/null <<<"$line"
            ;;
        step_end)
            jq -e 'has("step") and has("status") and has("duration_ms") and has("mode") and has("cols") and has("rows") and has("seed")' >/dev/null <<<"$line"
            ;;
        pty_capture)
            jq -e 'has("seed") and has("output_sha256") and has("output_bytes") and has("cols") and has("rows") and has("exit_code")' >/dev/null <<<"$line"
            ;;
        assert)
            jq -e 'has("seed") and has("assertion") and has("status")' >/dev/null <<<"$line"
            ;;
        *)
            return 0
            ;;
    esac
}

jsonl_validate_file() {
    local jsonl_file="$1"
    local mode="${2:-}"
    if [[ ! -f "$jsonl_file" ]]; then
        return 0
    fi
    if ! command -v jq >/dev/null 2>&1; then
        if [[ "$mode" == "strict" ]] || jsonl_should_validate; then
            echo "WARN: jq not available; skipping JSONL validation for $jsonl_file" >&2
        fi
        return 0
    fi
    local line_no=0
    while IFS= read -r line || [[ -n "$line" ]]; do
        line_no=$((line_no + 1))
        if [[ -z "$line" ]]; then
            continue
        fi
        if ! jsonl_validate_line "$line"; then
            echo "JSONL schema violation at line $line_no: $line" >&2
            if [[ "$mode" == "strict" ]]; then
                return 1
            fi
            if [[ -z "$mode" ]] && jsonl_should_validate; then
                return 1
            fi
        fi
    done < "$jsonl_file"
    return 0
}

jsonl_validate_current() {
    if [[ "$E2E_JSONL_DISABLE" == "1" ]]; then
        return 0
    fi
    if [[ ! -f "$E2E_JSONL_FILE" ]]; then
        return 0
    fi

    local mode="${E2E_JSONL_VALIDATE_MODE:-}"
    if [[ -z "$mode" ]]; then
        if [[ -n "${CI:-}" || "${E2E_JSONL_VALIDATE:-}" == "1" ]]; then
            mode="strict"
        else
            mode="warn"
        fi
    fi

    if [[ -n "${E2E_PYTHON:-}" && -f "$E2E_JSONL_VALIDATOR" && -f "$E2E_JSONL_SCHEMA_FILE" ]]; then
        local flag="--warn"
        if [[ "$mode" == "strict" ]]; then
            flag="--strict"
        fi
        if ! "$E2E_PYTHON" "$E2E_JSONL_VALIDATOR" "$E2E_JSONL_FILE" --schema "$E2E_JSONL_SCHEMA_FILE" "$flag"; then
            log_error "JSONL schema validation failed for $E2E_JSONL_FILE"
            return 1
        fi
        return 0
    fi

    if [[ "$mode" == "strict" || "$mode" == "warn" ]]; then
        jsonl_validate_file "$E2E_JSONL_FILE" "$mode"
        return $?
    fi

    if jsonl_should_validate; then
        jsonl_validate_file "$E2E_JSONL_FILE"
    fi
}

jsonl_init() {
    if [[ "${E2E_JSONL_INIT:-}" == "1" ]]; then
        return 0
    fi
    export E2E_JSONL_INIT=1
    e2e_seed >/dev/null
    export E2E_RUN_ID="${E2E_RUN_ID:-$(e2e_run_id)}"
    export E2E_RUN_START_MS="${E2E_RUN_START_MS:-$(e2e_run_start_ms)}"
    jsonl_env
    jsonl_run_start "${E2E_RUN_CMD:-}"
    jsonl_assert "artifact_log_dir" "pass" "log_dir=$E2E_LOG_DIR"
}

jsonl_env() {
    local ts host rustc cargo git_commit git_dirty
    ts="$(e2e_timestamp)"
    host="$(hostname 2>/dev/null || echo unknown)"
    rustc="$(rustc --version 2>/dev/null || echo unknown)"
    cargo="$(cargo --version 2>/dev/null || echo unknown)"
    git_commit="$(git rev-parse HEAD 2>/dev/null || echo "")"
    if git diff --quiet --ignore-submodules -- 2>/dev/null; then
        git_dirty="false"
    else
        git_dirty="true"
    fi

    local seed_json="null"
    local deterministic_json="false"
    if e2e_is_deterministic; then deterministic_json="true"; fi
    if [[ -n "${E2E_SEED:-}" ]]; then seed_json="${E2E_SEED}"; fi

    if command -v jq >/dev/null 2>&1; then
        jsonl_emit "$(jq -nc \
            --arg schema_version "$E2E_JSONL_SCHEMA_VERSION" \
            --arg type "env" \
            --arg timestamp "$ts" \
            --arg run_id "$E2E_RUN_ID" \
            --arg host "$host" \
            --arg rustc "$rustc" \
            --arg cargo "$cargo" \
            --arg git_commit "$git_commit" \
            --argjson git_dirty "$git_dirty" \
            --argjson seed "$seed_json" \
            --argjson deterministic "$deterministic_json" \
            --arg term "${TERM:-}" \
            --arg colorterm "${COLORTERM:-}" \
            --arg no_color "${NO_COLOR:-}" \
            '{schema_version:$schema_version,type:$type,timestamp:$timestamp,run_id:$run_id,host:$host,rustc:$rustc,cargo:$cargo,git_commit:$git_commit,git_dirty:$git_dirty,seed:$seed,deterministic:$deterministic,term:$term,colorterm:$colorterm,no_color:$no_color}')"
    else
        jsonl_emit "{\"schema_version\":\"${E2E_JSONL_SCHEMA_VERSION}\",\"type\":\"env\",\"timestamp\":\"$(json_escape "$ts")\",\"run_id\":\"$(json_escape "$E2E_RUN_ID")\",\"host\":\"$(json_escape "$host")\",\"rustc\":\"$(json_escape "$rustc")\",\"cargo\":\"$(json_escape "$cargo")\",\"git_commit\":\"$(json_escape "$git_commit")\",\"git_dirty\":${git_dirty},\"seed\":${seed_json},\"deterministic\":${deterministic_json},\"term\":\"$(json_escape "${TERM:-}")\",\"colorterm\":\"$(json_escape "${COLORTERM:-}")\",\"no_color\":\"$(json_escape "${NO_COLOR:-}")\"}"
    fi
}

jsonl_run_start() {
    local cmd="$1"
    local ts
    ts="$(e2e_timestamp)"
    local seed_json="null"
    if [[ -n "${E2E_SEED:-}" ]]; then seed_json="${E2E_SEED}"; fi
    if command -v jq >/dev/null 2>&1; then
        jsonl_emit "$(jq -nc \
            --arg schema_version "$E2E_JSONL_SCHEMA_VERSION" \
            --arg type "run_start" \
            --arg timestamp "$ts" \
            --arg run_id "$E2E_RUN_ID" \
            --arg command "$cmd" \
            --arg log_dir "$E2E_LOG_DIR" \
            --arg results_dir "$E2E_RESULTS_DIR" \
            --argjson seed "$seed_json" \
            '{schema_version:$schema_version,type:$type,timestamp:$timestamp,run_id:$run_id,seed:$seed,command:$command,log_dir:$log_dir,results_dir:$results_dir}')"
    else
        jsonl_emit "{\"schema_version\":\"${E2E_JSONL_SCHEMA_VERSION}\",\"type\":\"run_start\",\"timestamp\":\"$(json_escape "$ts")\",\"run_id\":\"$(json_escape "$E2E_RUN_ID")\",\"seed\":${seed_json},\"command\":\"$(json_escape "$cmd")\",\"log_dir\":\"$(json_escape "$E2E_LOG_DIR")\",\"results_dir\":\"$(json_escape "$E2E_RESULTS_DIR")\"}"
    fi
}

jsonl_run_end() {
    local status="$1"
    local duration_ms="$2"
    local failed_count="$3"
    local ts
    ts="$(e2e_timestamp)"
    local seed_json="null"
    if [[ -n "${E2E_SEED:-}" ]]; then seed_json="${E2E_SEED}"; fi
    if command -v jq >/dev/null 2>&1; then
        jsonl_emit "$(jq -nc \
            --arg schema_version "$E2E_JSONL_SCHEMA_VERSION" \
            --arg type "run_end" \
            --arg timestamp "$ts" \
            --arg run_id "$E2E_RUN_ID" \
            --arg status "$status" \
            --argjson seed "$seed_json" \
            --argjson duration_ms "$duration_ms" \
            --argjson failed_count "$failed_count" \
            '{schema_version:$schema_version,type:$type,timestamp:$timestamp,run_id:$run_id,seed:$seed,status:$status,duration_ms:$duration_ms,failed_count:$failed_count}')"
    else
        jsonl_emit "{\"schema_version\":\"${E2E_JSONL_SCHEMA_VERSION}\",\"type\":\"run_end\",\"timestamp\":\"$(json_escape "$ts")\",\"run_id\":\"$(json_escape "$E2E_RUN_ID")\",\"seed\":${seed_json},\"status\":\"$(json_escape "$status")\",\"duration_ms\":${duration_ms},\"failed_count\":${failed_count}}"
    fi
    jsonl_validate_current
}

jsonl_set_context() {
    export E2E_CONTEXT_MODE="${1:-${E2E_CONTEXT_MODE:-}}"
    export E2E_CONTEXT_COLS="${2:-${E2E_CONTEXT_COLS:-}}"
    export E2E_CONTEXT_ROWS="${3:-${E2E_CONTEXT_ROWS:-}}"
    export E2E_CONTEXT_SEED="${4:-${E2E_CONTEXT_SEED:-}}"
}

e2e_seed() {
    local seed="${E2E_SEED:-0}"
    export E2E_SEED="$seed"
    if e2e_is_deterministic; then
        if [[ -z "${FTUI_TEST_DETERMINISTIC:-}" ]]; then
            export FTUI_TEST_DETERMINISTIC="1"
        fi
        if [[ -z "${FTUI_SEED:-}" ]]; then
            export FTUI_SEED="$seed"
        fi
        if [[ -z "${FTUI_HARNESS_SEED:-}" ]]; then
            export FTUI_HARNESS_SEED="$seed"
        fi
        if [[ -z "${FTUI_DEMO_SEED:-}" ]]; then
            export FTUI_DEMO_SEED="$seed"
        fi
        if [[ -z "${FTUI_TEST_SEED:-}" ]]; then
            export FTUI_TEST_SEED="$seed"
        fi
        if [[ -z "${FTUI_DEMO_DETERMINISTIC:-}" ]]; then
            export FTUI_DEMO_DETERMINISTIC="1"
        fi
        if [[ -n "${E2E_TIME_STEP_MS:-}" && -z "${FTUI_TEST_TIME_STEP_MS:-}" ]]; then
            export FTUI_TEST_TIME_STEP_MS="$E2E_TIME_STEP_MS"
        fi
    fi
    if [[ -z "${E2E_CONTEXT_SEED:-}" ]]; then
        export E2E_CONTEXT_SEED="$seed"
    fi
    if [[ -z "${STORM_SEED:-}" ]]; then
        export STORM_SEED="$seed"
    fi
    printf '%s' "$seed"
}

if [[ "${E2E_AUTO_SEED:-1}" == "1" ]]; then
    e2e_seed >/dev/null 2>&1 || true
fi

jsonl_step_start() {
    local step="$1"
    local ts
    ts="$(e2e_timestamp)"
    local mode="${E2E_CONTEXT_MODE:-}"
    local cols="${E2E_CONTEXT_COLS:-}"
    local rows="${E2E_CONTEXT_ROWS:-}"
    local seed="${E2E_CONTEXT_SEED:-}"
    local hash_key=""
    if [[ -n "$mode" && -n "$cols" && -n "$rows" ]]; then
        hash_key="$(e2e_hash_key "$mode" "$cols" "$rows" "$seed")"
    fi
    local cols_json="null"
    local rows_json="null"
    local seed_json="null"
    if [[ -n "$cols" ]]; then cols_json="$cols"; fi
    if [[ -n "$rows" ]]; then rows_json="$rows"; fi
    if [[ -n "$seed" ]]; then seed_json="$seed"; fi
    if command -v jq >/dev/null 2>&1; then
        jsonl_emit "$(jq -nc \
            --arg schema_version "$E2E_JSONL_SCHEMA_VERSION" \
            --arg type "step_start" \
            --arg timestamp "$ts" \
            --arg run_id "$E2E_RUN_ID" \
            --arg step "$step" \
            --arg mode "$mode" \
            --arg hash_key "$hash_key" \
            --argjson cols "$cols_json" \
            --argjson rows "$rows_json" \
            --argjson seed "$seed_json" \
            '{schema_version:$schema_version,type:$type,timestamp:$timestamp,run_id:$run_id,step:$step,mode:$mode,hash_key:$hash_key,cols:$cols,rows:$rows,seed:$seed}')"
    else
        jsonl_emit "{\"schema_version\":\"${E2E_JSONL_SCHEMA_VERSION}\",\"type\":\"step_start\",\"timestamp\":\"$(json_escape "$ts")\",\"run_id\":\"$(json_escape "$E2E_RUN_ID")\",\"step\":\"$(json_escape "$step")\",\"mode\":\"$(json_escape "$mode")\",\"hash_key\":\"$(json_escape "$hash_key")\",\"cols\":${cols_json},\"rows\":${rows_json},\"seed\":${seed_json}}"
    fi
}

jsonl_step_end() {
    local step="$1"
    local status="$2"
    local duration_ms="$3"
    local ts
    ts="$(e2e_timestamp)"
    local mode="${E2E_CONTEXT_MODE:-}"
    local cols="${E2E_CONTEXT_COLS:-}"
    local rows="${E2E_CONTEXT_ROWS:-}"
    local seed="${E2E_CONTEXT_SEED:-}"
    local hash_key=""
    if [[ -n "$mode" && -n "$cols" && -n "$rows" ]]; then
        hash_key="$(e2e_hash_key "$mode" "$cols" "$rows" "$seed")"
    fi
    local cols_json="null"
    local rows_json="null"
    local seed_json="null"
    if [[ -n "$cols" ]]; then cols_json="$cols"; fi
    if [[ -n "$rows" ]]; then rows_json="$rows"; fi
    if [[ -n "$seed" ]]; then seed_json="$seed"; fi
    if command -v jq >/dev/null 2>&1; then
        jsonl_emit "$(jq -nc \
            --arg schema_version "$E2E_JSONL_SCHEMA_VERSION" \
            --arg type "step_end" \
            --arg timestamp "$ts" \
            --arg run_id "$E2E_RUN_ID" \
            --arg step "$step" \
            --arg status "$status" \
            --argjson duration_ms "$duration_ms" \
            --arg mode "$mode" \
            --arg hash_key "$hash_key" \
            --argjson cols "$cols_json" \
            --argjson rows "$rows_json" \
            --argjson seed "$seed_json" \
            '{schema_version:$schema_version,type:$type,timestamp:$timestamp,run_id:$run_id,step:$step,status:$status,duration_ms:$duration_ms,mode:$mode,hash_key:$hash_key,cols:$cols,rows:$rows,seed:$seed}')"
    else
        jsonl_emit "{\"schema_version\":\"${E2E_JSONL_SCHEMA_VERSION}\",\"type\":\"step_end\",\"timestamp\":\"$(json_escape "$ts")\",\"run_id\":\"$(json_escape "$E2E_RUN_ID")\",\"step\":\"$(json_escape "$step")\",\"status\":\"$(json_escape "$status")\",\"duration_ms\":${duration_ms},\"mode\":\"$(json_escape "$mode")\",\"hash_key\":\"$(json_escape "$hash_key")\",\"cols\":${cols_json},\"rows\":${rows_json},\"seed\":${seed_json}}"
    fi
}

jsonl_case_step_start() {
    local case_name="$1"
    local step="$2"
    local action="$3"
    local details="${4:-}"
    local ts
    ts="$(e2e_timestamp)"
    local mode="${E2E_CONTEXT_MODE:-}"
    local cols="${E2E_CONTEXT_COLS:-}"
    local rows="${E2E_CONTEXT_ROWS:-}"
    local seed="${E2E_CONTEXT_SEED:-}"
    local hash_key=""
    if [[ -n "$mode" && -n "$cols" && -n "$rows" ]]; then
        hash_key="$(e2e_hash_key "$mode" "$cols" "$rows" "$seed")"
    fi
    local cols_json="null"
    local rows_json="null"
    local seed_json="null"
    if [[ -n "$cols" ]]; then cols_json="$cols"; fi
    if [[ -n "$rows" ]]; then rows_json="$rows"; fi
    if [[ -n "$seed" ]]; then seed_json="$seed"; fi
    if command -v jq >/dev/null 2>&1; then
        jsonl_emit "$(jq -nc \
            --arg schema_version "$E2E_JSONL_SCHEMA_VERSION" \
            --arg type "case_step_start" \
            --arg timestamp "$ts" \
            --arg run_id "$E2E_RUN_ID" \
            --arg case "$case_name" \
            --arg step "$step" \
            --arg action "$action" \
            --arg details "$details" \
            --arg mode "$mode" \
            --arg hash_key "$hash_key" \
            --argjson cols "$cols_json" \
            --argjson rows "$rows_json" \
            --argjson seed "$seed_json" \
            '{schema_version:$schema_version,type:$type,timestamp:$timestamp,run_id:$run_id,case:$case,step:$step,action:$action,details:$details,mode:$mode,hash_key:$hash_key,cols:$cols,rows:$rows,seed:$seed}')"
    else
        jsonl_emit "{\"schema_version\":\"${E2E_JSONL_SCHEMA_VERSION}\",\"type\":\"case_step_start\",\"timestamp\":\"$(json_escape "$ts")\",\"run_id\":\"$(json_escape "$E2E_RUN_ID")\",\"case\":\"$(json_escape "$case_name")\",\"step\":\"$(json_escape "$step")\",\"action\":\"$(json_escape "$action")\",\"details\":\"$(json_escape "$details")\",\"mode\":\"$(json_escape "$mode")\",\"hash_key\":\"$(json_escape "$hash_key")\",\"cols\":${cols_json},\"rows\":${rows_json},\"seed\":${seed_json}}"
    fi
}

jsonl_case_step_end() {
    local case_name="$1"
    local step="$2"
    local status="$3"
    local duration_ms="$4"
    local action="${5:-}"
    local details="${6:-}"
    local ts
    ts="$(e2e_timestamp)"
    local mode="${E2E_CONTEXT_MODE:-}"
    local cols="${E2E_CONTEXT_COLS:-}"
    local rows="${E2E_CONTEXT_ROWS:-}"
    local seed="${E2E_CONTEXT_SEED:-}"
    local hash_key=""
    if [[ -n "$mode" && -n "$cols" && -n "$rows" ]]; then
        hash_key="$(e2e_hash_key "$mode" "$cols" "$rows" "$seed")"
    fi
    local cols_json="null"
    local rows_json="null"
    local seed_json="null"
    if [[ -n "$cols" ]]; then cols_json="$cols"; fi
    if [[ -n "$rows" ]]; then rows_json="$rows"; fi
    if [[ -n "$seed" ]]; then seed_json="$seed"; fi
    if command -v jq >/dev/null 2>&1; then
        jsonl_emit "$(jq -nc \
            --arg schema_version "$E2E_JSONL_SCHEMA_VERSION" \
            --arg type "case_step_end" \
            --arg timestamp "$ts" \
            --arg run_id "$E2E_RUN_ID" \
            --arg case "$case_name" \
            --arg step "$step" \
            --arg status "$status" \
            --argjson duration_ms "$duration_ms" \
            --arg action "$action" \
            --arg details "$details" \
            --arg mode "$mode" \
            --arg hash_key "$hash_key" \
            --argjson cols "$cols_json" \
            --argjson rows "$rows_json" \
            --argjson seed "$seed_json" \
            '{schema_version:$schema_version,type:$type,timestamp:$timestamp,run_id:$run_id,case:$case,step:$step,status:$status,duration_ms:$duration_ms,action:$action,details:$details,mode:$mode,hash_key:$hash_key,cols:$cols,rows:$rows,seed:$seed}')"
    else
        jsonl_emit "{\"schema_version\":\"${E2E_JSONL_SCHEMA_VERSION}\",\"type\":\"case_step_end\",\"timestamp\":\"$(json_escape "$ts")\",\"run_id\":\"$(json_escape "$E2E_RUN_ID")\",\"case\":\"$(json_escape "$case_name")\",\"step\":\"$(json_escape "$step")\",\"status\":\"$(json_escape "$status")\",\"duration_ms\":${duration_ms},\"action\":\"$(json_escape "$action")\",\"details\":\"$(json_escape "$details")\",\"mode\":\"$(json_escape "$mode")\",\"hash_key\":\"$(json_escape "$hash_key")\",\"cols\":${cols_json},\"rows\":${rows_json},\"seed\":${seed_json}}"
    fi
}

jsonl_pty_capture() {
    local output_file="$1"
    local cols="$2"
    local rows="$3"
    local exit_code="$4"
    local canonical_file="${5:-}"
    jsonl_init
    local ts output_sha output_bytes canonical_sha canonical_bytes
    ts="$(e2e_timestamp)"
    local seed_json="null"
    if [[ -n "${E2E_SEED:-}" ]]; then seed_json="${E2E_SEED}"; fi
    output_sha="$(sha256_file "$output_file")"
    output_bytes=$(wc -c < "$output_file" 2>/dev/null | tr -d ' ')
    canonical_sha=""
    canonical_bytes=0
    if [[ -n "$canonical_file" && -f "$canonical_file" ]]; then
        canonical_sha="$(sha256_file "$canonical_file")"
        canonical_bytes=$(wc -c < "$canonical_file" 2>/dev/null | tr -d ' ')
    fi
    if command -v jq >/dev/null 2>&1; then
        jsonl_emit "$(jq -nc \
            --arg schema_version "$E2E_JSONL_SCHEMA_VERSION" \
            --arg type "pty_capture" \
            --arg timestamp "$ts" \
            --arg run_id "$E2E_RUN_ID" \
            --arg output_file "$output_file" \
            --arg canonical_file "$canonical_file" \
            --arg output_sha256 "$output_sha" \
            --arg canonical_sha256 "$canonical_sha" \
            --argjson output_bytes "${output_bytes:-0}" \
            --argjson canonical_bytes "${canonical_bytes:-0}" \
            --argjson cols "$cols" \
            --argjson rows "$rows" \
            --argjson exit_code "$exit_code" \
            --argjson seed "$seed_json" \
            '{schema_version:$schema_version,type:$type,timestamp:$timestamp,run_id:$run_id,seed:$seed,output_file:$output_file,canonical_file:$canonical_file,output_sha256:$output_sha256,canonical_sha256:$canonical_sha256,output_bytes:$output_bytes,canonical_bytes:$canonical_bytes,cols:$cols,rows:$rows,exit_code:$exit_code}')"
    else
        local seed_json="null"
        if [[ -n "${E2E_SEED:-}" ]]; then seed_json="${E2E_SEED}"; fi
        jsonl_emit "{\"schema_version\":\"${E2E_JSONL_SCHEMA_VERSION}\",\"type\":\"pty_capture\",\"timestamp\":\"$(json_escape "$ts")\",\"run_id\":\"$(json_escape "$E2E_RUN_ID")\",\"seed\":${seed_json},\"output_file\":\"$(json_escape "$output_file")\",\"canonical_file\":\"$(json_escape "$canonical_file")\",\"output_sha256\":\"$(json_escape "$output_sha")\",\"canonical_sha256\":\"$(json_escape "$canonical_sha")\",\"output_bytes\":${output_bytes:-0},\"canonical_bytes\":${canonical_bytes:-0},\"cols\":${cols},\"rows\":${rows},\"exit_code\":${exit_code}}"
    fi
}

artifact_strict_mode() {
    local mode="${E2E_JSONL_VALIDATE_MODE:-}"
    if [[ -z "$mode" ]]; then
        if [[ -n "${CI:-}" || "${E2E_JSONL_VALIDATE:-}" == "1" ]]; then
            mode="strict"
        else
            mode="warn"
        fi
    fi
    [[ "$mode" == "strict" ]]
}

artifact_path_from_details() {
    local details="$1"
    if [[ -z "$details" ]]; then
        printf ''
        return 0
    fi
    if [[ "$details" == *"="* ]]; then
        local value="${details#*=}"
        printf '%s' "${value%% *}"
        return 0
    fi
    printf '%s' "${details%% *}"
}

jsonl_artifact() {
    local artifact_type="$1"
    local path="$2"
    local status="${3:-present}"
    local ts sha bytes
    ts="$(e2e_timestamp)"
    sha=""
    bytes=0
    if [[ -n "$path" && -e "$path" ]]; then
        if [[ -f "$path" ]]; then
            sha="$(sha256_file "$path" 2>/dev/null || true)"
            bytes=$(wc -c < "$path" 2>/dev/null | tr -d ' ')
        fi
    else
        status="missing"
    fi
    local seed_json="null"
    if [[ -n "${E2E_SEED:-}" ]]; then seed_json="${E2E_SEED}"; fi
    if command -v jq >/dev/null 2>&1; then
        jsonl_emit "$(jq -nc \
            --arg schema_version "$E2E_JSONL_SCHEMA_VERSION" \
            --arg type "artifact" \
            --arg timestamp "$ts" \
            --arg run_id "$E2E_RUN_ID" \
            --arg artifact_type "$artifact_type" \
            --arg path "$path" \
            --arg status "$status" \
            --arg sha256 "$sha" \
            --argjson bytes "${bytes:-0}" \
            --argjson seed "$seed_json" \
            '{schema_version:$schema_version,type:$type,timestamp:$timestamp,run_id:$run_id,seed:$seed,artifact_type:$artifact_type,path:$path,status:$status,sha256:$sha256,bytes:$bytes}')"
    else
        jsonl_emit "{\"schema_version\":\"${E2E_JSONL_SCHEMA_VERSION}\",\"type\":\"artifact\",\"timestamp\":\"$(json_escape "$ts")\",\"run_id\":\"$(json_escape "$E2E_RUN_ID")\",\"seed\":${seed_json},\"artifact_type\":\"$(json_escape "$artifact_type")\",\"path\":\"$(json_escape "$path")\",\"status\":\"$(json_escape "$status")\",\"sha256\":\"$(json_escape "$sha")\",\"bytes\":${bytes:-0}}"
    fi
}

jsonl_assert() {
    local name="$1"
    local status="$2"
    local details="${3:-}"
    local assert_status="$status"
    if [[ "$name" == artifact_* ]]; then
        local artifact_type="${name#artifact_}"
        local path
        path="$(artifact_path_from_details "$details")"
        if [[ -z "$path" || ! -e "$path" ]]; then
            assert_status="failed"
            jsonl_artifact "$artifact_type" "$path" "missing"
            if artifact_strict_mode; then
                log_error "Missing required artifact: ${artifact_type} (${path:-no path})"
                return 1
            fi
        else
            jsonl_artifact "$artifact_type" "$path" "present"
        fi
    fi
    local ts
    ts="$(e2e_timestamp)"
    local seed_json="null"
    if [[ -n "${E2E_SEED:-}" ]]; then seed_json="${E2E_SEED}"; fi
    if command -v jq >/dev/null 2>&1; then
        jsonl_emit "$(jq -nc \
            --arg schema_version "$E2E_JSONL_SCHEMA_VERSION" \
            --arg type "assert" \
            --arg timestamp "$ts" \
            --arg run_id "$E2E_RUN_ID" \
            --arg assertion "$name" \
            --arg status "$assert_status" \
            --arg details "$details" \
            --argjson seed "$seed_json" \
            '{schema_version:$schema_version,type:$type,timestamp:$timestamp,run_id:$run_id,seed:$seed,assertion:$assertion,status:$status,details:$details}')"
    else
        local seed_json="null"
        if [[ -n "${E2E_SEED:-}" ]]; then seed_json="${E2E_SEED}"; fi
        jsonl_emit "{\"schema_version\":\"${E2E_JSONL_SCHEMA_VERSION}\",\"type\":\"assert\",\"timestamp\":\"$(json_escape "$ts")\",\"run_id\":\"$(json_escape "$E2E_RUN_ID")\",\"seed\":${seed_json},\"assertion\":\"$(json_escape "$name")\",\"status\":\"$(json_escape "$assert_status")\",\"details\":\"$(json_escape "$details")\"}"
    fi
}

sha256_file() {
    local file="$1"
    if command -v sha256sum >/dev/null 2>&1 && [[ -f "$file" ]]; then
        sha256sum "$file" | awk '{print $1}'
        return 0
    fi
    return 1
}

verify_sha256() {
    local file="$1"
    local expected="$2"
    local label="${3:-sha256_match}"
    local actual=""
    actual="$(sha256_file "$file" || true)"
    if [[ -z "$actual" ]]; then
        jsonl_assert "$label" "skipped" "sha256sum unavailable or file missing"
        return 2
    fi
    if [[ "$actual" == "$expected" ]]; then
        jsonl_assert "$label" "passed" "sha256 match"
        return 0
    fi
    jsonl_assert "$label" "failed" "expected ${expected}, got ${actual}"
    return 1
}

log() {
    local level="$1"
    shift
    local ts
    ts="$(date +"%Y-%m-%d %H:%M:%S.%3N")"
    echo "[$ts] [$level] $*" | tee -a "$LOG_FILE"
}

log_debug() {
    if [[ "$LOG_LEVEL" == "DEBUG" ]]; then
        log "DEBUG" "$@"
    fi
}

log_info() {
    log "INFO" "$@"
}

log_warn() {
    log "WARN" "$@"
}

log_error() {
    log "ERROR" "$@"
}

log_test_start() {
    local name="$1"
    jsonl_init
    jsonl_step_start "$name"
    log_info "========================================"
    log_info "STARTING TEST: $name"
    log_info "========================================"
}

log_test_pass() {
    local name="$1"
    log_info "PASS: $name"
}

log_test_fail() {
    local name="$1"
    local reason="$2"
    log_error "FAIL: $name"
    log_error "  Reason: $reason"
    log_error "  Log file: $LOG_FILE"
}

log_test_skip() {
    local name="$1"
    local reason="$2"
    log_warn "SKIP: $name"
    log_warn "  Reason: $reason"
}

record_result() {
    local name="$1"
    local status="$2"
    local duration_ms="$3"
    local log_file="$4"
    local error_msg="${5:-}"
    jsonl_init

    mkdir -p "$E2E_RESULTS_DIR"

    local result_file
    result_file="$E2E_RESULTS_DIR/${name}_$(date +%s%N)_$$.json"

    if command -v jq >/dev/null 2>&1; then
        if [[ -n "$error_msg" ]]; then
            jq -n \
                --arg name "$name" \
                --arg status "$status" \
                --argjson duration_ms "$duration_ms" \
                --arg log_file "$log_file" \
                --arg error "$error_msg" \
                '{name:$name,status:$status,duration_ms:$duration_ms,log_file:$log_file,error:$error}' \
                > "$result_file"
        else
            jq -n \
                --arg name "$name" \
                --arg status "$status" \
                --argjson duration_ms "$duration_ms" \
                --arg log_file "$log_file" \
                '{name:$name,status:$status,duration_ms:$duration_ms,log_file:$log_file}' \
                > "$result_file"
        fi
    else
        local safe_error
        safe_error="$(printf '%s' "$error_msg" | sed 's/"/\\"/g')"
        if [[ -n "$safe_error" ]]; then
            printf '{"name":"%s","status":"%s","duration_ms":%s,"log_file":"%s","error":"%s"}\n' \
                "$name" "$status" "$duration_ms" "$log_file" "$safe_error" \
                > "$result_file"
        else
            printf '{"name":"%s","status":"%s","duration_ms":%s,"log_file":"%s"}\n' \
                "$name" "$status" "$duration_ms" "$log_file" \
                > "$result_file"
        fi
    fi
    jsonl_assert "artifact_case_log" "pass" "case_log=$log_file"
    jsonl_step_end "$name" "$status" "$duration_ms"
}

finalize_summary() {
    local summary_file="$1"
    local end_ms
    end_ms="$(e2e_now_ms)"
    local start_ms="${E2E_RUN_START_MS:-$end_ms}"
    local duration_ms=$((end_ms - start_ms))

    if command -v jq >/dev/null 2>&1; then
        jq -s \
            --arg timestamp "$(e2e_timestamp)" \
            --argjson duration_ms "$duration_ms" \
            '{
                timestamp: $timestamp,
                total: length,
                passed: (map(select(.status=="passed")) | length),
                failed: (map(select(.status=="failed")) | length),
                skipped: (map(select(.status=="skipped")) | length),
                duration_ms: $duration_ms,
                tests: .
            }' \
            "$E2E_RESULTS_DIR"/*.json > "$summary_file"
    else
        local total
        total=$(ls -1 "$E2E_RESULTS_DIR"/*.json 2>/dev/null | wc -l | tr -d ' ')
        cat > "$summary_file" <<EOF_SUM
{"timestamp":"$(e2e_timestamp)","total":${total},"passed":0,"failed":0,"skipped":0,"duration_ms":${duration_ms},"tests":[]}
EOF_SUM
    fi
    local failed_count=0
    if command -v jq >/dev/null 2>&1; then
        failed_count=$(jq '.failed // 0' "$summary_file" 2>/dev/null || echo 0)
    fi
    if [[ "$failed_count" -gt 0 ]]; then
        jsonl_run_end "failed" "$duration_ms" "$failed_count"
    else
        jsonl_run_end "complete" "$duration_ms" "$failed_count"
    fi
    jsonl_assert "artifact_summary_json" "pass" "summary_json=$summary_file"
    jsonl_assert "artifact_e2e_jsonl" "pass" "e2e_jsonl=$E2E_JSONL_FILE"
}
