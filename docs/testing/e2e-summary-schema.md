# E2E Summary JSON Schema

The E2E runner writes a structured summary to:

```
<E2E_RESULTS_DIR>/summary.json
```

## Top-level fields

- `timestamp`: ISO-8601 timestamp for the run.
- `total`, `passed`, `failed`, `skipped`: counts by status.
- `duration_ms`: total suite duration in milliseconds.
- `run`:
  - `command`: command used to invoke the suite (or null if unknown).
  - `log_dir`: root log directory for this run.
  - `results_dir`: directory containing per-test result JSON files.
  - `cases_dir`: directory containing per-case bundles.
- `environment`:
  - `date`, `user`, `hostname`, `cwd`, `rustc`, `cargo`, `git_status`, `git_commit`.
- `tests`: array of per-test records (see below).

## Per-test fields

- `name`: case name (string).
- `status`: `passed` | `failed` | `skipped`.
- `duration_ms`: case duration in milliseconds.
- `log_file`: primary log file for the case.
- `case_dir`: per-case bundle directory.
- `bundled_log`: copied case log inside the bundle.
- `pty_file`: original PTY capture path.
- `bundled_pty`: copied PTY capture inside the bundle.
- `pty_hex`: full hex dump of the PTY capture.
- `pty_text`: decoded/printable text from the PTY capture.
- `pty_head_hex`: first N bytes (hex) for failed cases (null otherwise).
- `pty_tail_text`: last N lines of decoded text for failed cases (null otherwise).
- `failure_summary`: summary file for failed cases (null otherwise).
- `env_log`: environment log text file.
- `env_json`: environment JSON file.
- `repro_cmd`: reproduction command for the case (null if unknown).
- `error`: failure reason (null for pass/skip).

## Per-case bundle layout

Each case has a bundle directory under `cases_dir` containing:

- `case.log` (test logs)
- `capture.pty` (raw PTY output)
- `capture.hex` (full hex dump)
- `capture.txt` (decoded text)
- `capture.head.hex` (failed cases, first N bytes)
- `capture.tail.txt` (failed cases, last N lines)
- `failure_summary.txt` (failed cases)

## E2E Event JSONL (Target Schema)

The suite and related scripts should converge on a per-event JSONL log for
deterministic E2E analysis. This is a schema contract; not every script emits
every event yet.

Default location: `<E2E_LOG_DIR>/e2e.jsonl` (scripts may override).
One JSON object per line.

Machine-readable schema:

```
tests/e2e/lib/e2e_jsonl_schema.json
```

The logger sets `schema_version` from `E2E_JSONL_SCHEMA_VERSION`
(default `e2e-jsonl-v1`).

Validator:

```bash
python3 tests/e2e/lib/validate_jsonl.py <path/to/e2e.jsonl> --schema tests/e2e/lib/e2e_jsonl_schema.json --warn
```

Example fixture (handy for local verification):

```
tests/e2e/lib/e2e_jsonl_examples.jsonl
```

### Common Fields (required)

| Field | Type | Description |
|-------|------|-------------|
| `schema_version` | string | Schema version (current: `e2e-jsonl-v1`) |
| `type` | string | Event type (see below) |
| `timestamp` | string | ISO-8601 timestamp |
| `run_id` | string | Stable run id for correlation |
| `seed` | number | Determinism seed (required, may be `0`) |

Additional fields are defined per-event in
`tests/e2e/lib/e2e_jsonl_schema.json`; not every script emits every field.
Common optional fields (depending on event type) include `mode`, `cols`,
`rows`, `hash_key`, `git_commit`, `git_dirty`, `term`, `colorterm`,
`no_color`, and `host`.

### Validation Behavior

- In CI, validation is **strict** (fail on first schema violation).
- Locally, validation **warns** by default.
- Override with `E2E_JSONL_VALIDATE=1` (strict) or
  `E2E_JSONL_VALIDATE_MODE=warn|strict`.
- The logger uses `tests/e2e/lib/validate_jsonl.py` when `E2E_PYTHON` is set and
  the schema file is present; otherwise it falls back to a lightweight `jq`
  check of required fields.

### Event Types

| Type | Purpose | Key Fields |
|------|---------|-----------|
| `env` | Environment snapshot | `platform`, `arch`, `rust_version`, `cargo_version` |
| `run_start` | Suite start | `command`, `log_dir`, `results_dir` |
| `run_end` | Suite end | `status`, `duration_ms`, `failed_count` |
| `step_start` | Step begin | `step`, `description` |
| `step_end` | Step end | `step`, `status`, `duration_ms` |
| `case_step_start` | Case step begin | `case`, `step`, `action`, `details` |
| `case_step_end` | Case step end | `case`, `step`, `status`, `duration_ms` |
| `input` | Input injection | `input_type`, `encoding`, `bytes_b64`, `input_hash` |
| `frame` | Render frame | `frame_idx`, `frame_hash`, `hash_algo`, `render_ms`, `present_ms` |
| `pty_capture` | PTY metadata | `output_file`, `output_sha256`, `output_bytes` |
| `assert` | Assertion result | `assertion`, `status`, `details` |
| `artifact` | Artifact reference | `path`, `artifact_type`, `status`, `sha256`, `bytes` |
| `error` | Error detail | `message`, `exit_code`, `stack` |

`artifact` events use `status` = `present` or `missing`. In CI strict mode,
missing required artifacts fail the run with a clear error.

### Hashing Requirements

`frame_hash` and `input_hash` should be stable across runs with the same seed
and terminal size. Use `sha256` unless a different algorithm is explicitly
recorded in `hash_algo`.
