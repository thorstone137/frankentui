#![forbid(unsafe_code)]

//! PTY-driven E2E for VisualEffects input handling (bd-l8x9.8.3).
//!
//! Drives real key sequences through a PTY to ensure the VisualEffects screen
//! can cycle effects/palettes without panicking and exits cleanly.

#![cfg(unix)]

use std::sync::mpsc;
use std::time::{Duration, Instant};

use ftui_pty::input_forwarding::{Key, KeyEvent, Modifiers, key_to_sequence};
use ftui_pty::{PtyConfig, spawn_command};
use portable_pty::CommandBuilder;

// ---------------------------------------------------------------------------
// JSONL Logging
// ---------------------------------------------------------------------------

fn log_jsonl(step: &str, data: &[(&str, String)]) {
    let fields: Vec<String> = std::iter::once(format!("\"ts\":\"{}\"", chrono_like_timestamp()))
        .chain(std::iter::once(format!("\"step\":\"{}\"", step)))
        .chain(
            data.iter()
                .map(|(k, v)| format!("\"{}\":\"{}\"", k, v.replace('"', "\\\""))),
        )
        .collect();
    eprintln!("{{{}}}", fields.join(","));
}

fn chrono_like_timestamp() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("T{n:06}")
}

fn tail_output(output: &[u8], max_bytes: usize) -> String {
    let start = output.len().saturating_sub(max_bytes);
    String::from_utf8_lossy(&output[start..]).to_string()
}

fn send_key(
    session: &mut ftui_pty::PtySession,
    label: &str,
    key: Key,
    delay: Duration,
    last_key: &mut String,
) -> std::io::Result<()> {
    let seq = key_to_sequence(KeyEvent::new(key, Modifiers::NONE));
    *last_key = label.to_string();
    session.send_input(&seq)?;
    std::thread::sleep(delay);
    let _ = session.read_output_result();
    Ok(())
}

fn extract_vfx_hashes(output: &[u8]) -> Vec<u64> {
    let text = String::from_utf8_lossy(output);
    text.lines()
        .filter_map(|line| {
            if !line.contains("\"event\":\"vfx_frame\"") {
                return None;
            }
            let key = "\"hash\":";
            let start = line.find(key)? + key.len();
            let rest = &line[start..];
            let end = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            rest[..end].parse::<u64>().ok()
        })
        .collect()
}

fn run_vfx_harness(demo_bin: &str, label: &str) -> Result<Vec<u64>, String> {
    let config = PtyConfig::default()
        .with_size(120, 40)
        .with_test_name(label)
        .logging(false);

    let mut cmd = CommandBuilder::new(demo_bin);
    cmd.arg("--vfx-harness");
    cmd.arg("--vfx-effect=doom-e1m1");
    cmd.arg("--vfx-tick-ms=16");
    cmd.arg("--vfx-frames=6");
    cmd.arg("--vfx-cols=120");
    cmd.arg("--vfx-rows=40");
    cmd.arg("--vfx-jsonl=-");
    cmd.arg("--exit-after-ms=4000");

    let mut session =
        spawn_command(config, cmd).map_err(|err| format!("spawn vfx harness: {err}"))?;
    let status = session
        .wait_and_drain(Duration::from_secs(6))
        .map_err(|err| format!("wait vfx harness: {err}"))?;
    let output = session.output().to_vec();
    let hashes = extract_vfx_hashes(&output);

    if !status.success() {
        let tail = tail_output(&output, 4096);
        return Err(format!(
            "vfx harness exit failure: {status:?}\nTAIL:\n{tail}"
        ));
    }

    Ok(hashes)
}

// ---------------------------------------------------------------------------
// PTY E2E: cycle effects/palettes without panic
// ---------------------------------------------------------------------------

#[test]
fn pty_visual_effects_input_no_panic() -> Result<(), String> {
    let start = Instant::now();
    let demo_bin = std::env::var("CARGO_BIN_EXE_ftui-demo-showcase").map_err(|err| {
        format!("CARGO_BIN_EXE_ftui-demo-showcase must be set for PTY tests: {err}")
    })?;

    log_jsonl(
        "env",
        &[
            ("test", "pty_visual_effects_input_no_panic".to_string()),
            ("bin", demo_bin.clone()),
            ("cols", "120".to_string()),
            ("rows", "40".to_string()),
        ],
    );

    let config = PtyConfig::default()
        .with_size(120, 40)
        .with_test_name("vfx_pty_inputs")
        .with_env("FTUI_DEMO_EXIT_AFTER_MS", "2500")
        .with_env("FTUI_DEMO_SCREEN", "14")
        .logging(false);

    let mut cmd = CommandBuilder::new(demo_bin);
    cmd.arg("--screen=14");

    let mut session =
        spawn_command(config, cmd).map_err(|err| format!("spawn demo in PTY: {err}"))?;
    std::thread::sleep(Duration::from_millis(250));
    let _ = session.read_output_result();

    let mut last_key = "startup".to_string();
    let step_delay = Duration::from_millis(120);

    let steps: [(&str, Key); 7] = [
        ("space", Key::Char(' ')),
        ("right", Key::Right),
        ("right", Key::Right),
        ("left", Key::Left),
        ("palette", Key::Char('p')),
        ("space", Key::Char(' ')),
        ("palette", Key::Char('p')),
    ];

    for (label, key) in steps {
        log_jsonl("input", &[("key", label.to_string())]);
        if let Err(err) = send_key(&mut session, label, key, step_delay, &mut last_key) {
            let output = session.read_output();
            let tail = tail_output(&output, 2048);
            let msg = format!("PTY send failed at key={label}: {err}\nTAIL:\n{tail}");
            eprintln!("{msg}");
            return Err(msg);
        }
    }

    // Request clean exit
    log_jsonl("input", &[("key", "quit".to_string())]);
    let _ = send_key(
        &mut session,
        "quit",
        Key::Char('q'),
        step_delay,
        &mut last_key,
    );

    let output_snapshot = session.output().to_vec();

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let status = session.wait_and_drain(Duration::from_secs(2));
        let output = session.output().to_vec();
        let _ = tx.send((status, output));
    });

    let result = rx.recv_timeout(Duration::from_secs(6));
    match result {
        Ok((Ok(status), output)) if status.success() => {
            log_jsonl(
                "result",
                &[
                    ("case", "pty_visual_effects_input_no_panic".to_string()),
                    ("outcome", "pass".to_string()),
                    ("elapsed_ms", start.elapsed().as_millis().to_string()),
                    ("last_key", last_key),
                    ("output_bytes", output.len().to_string()),
                ],
            );
            Ok(())
        }
        Ok((Ok(status), output)) => {
            let tail = tail_output(&output, 2048);
            let msg = format!("PTY exit status failure: {status:?}\nTAIL:\n{tail}");
            eprintln!("{msg}");
            Err(msg)
        }
        Ok((Err(err), output)) => {
            let tail = tail_output(&output, 2048);
            let msg = format!("PTY wait_and_drain error: {err}\nTAIL:\n{tail}");
            eprintln!("{msg}");
            Err(msg)
        }
        Err(_) => {
            let tail = tail_output(&output_snapshot, 2048);
            let msg = format!("PTY timeout waiting for exit; last_key={last_key}\nTAIL:\n{tail}");
            eprintln!("{msg}");
            Err(msg)
        }
    }
}

// ---------------------------------------------------------------------------
// PTY E2E: deterministic VFX harness hashes
// ---------------------------------------------------------------------------

#[test]
fn pty_vfx_harness_deterministic_hashes() -> Result<(), String> {
    let demo_bin = std::env::var("CARGO_BIN_EXE_ftui-demo-showcase").map_err(|err| {
        format!("CARGO_BIN_EXE_ftui-demo-showcase must be set for PTY tests: {err}")
    })?;

    log_jsonl(
        "env",
        &[
            ("test", "pty_vfx_harness_deterministic_hashes".to_string()),
            ("bin", demo_bin.clone()),
        ],
    );

    let hashes_a = run_vfx_harness(&demo_bin, "vfx_harness_run_a")?;
    let hashes_b = run_vfx_harness(&demo_bin, "vfx_harness_run_b")?;

    if hashes_a.is_empty() {
        return Err("vfx harness produced no hashes".to_string());
    }

    if hashes_a != hashes_b {
        return Err(format!(
            "vfx harness hashes diverged:\nA={:?}\nB={:?}",
            hashes_a, hashes_b
        ));
    }

    log_jsonl(
        "result",
        &[
            ("case", "pty_vfx_harness_deterministic_hashes".to_string()),
            ("frames", hashes_a.len().to_string()),
        ],
    );

    Ok(())
}
