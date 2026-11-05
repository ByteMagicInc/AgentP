//! Detects whether an `ffmpeg` that can actually run is on `PATH`, for MP3 conversion.
//!
//! The TUI starts a check in the background and reads the result at draw time, the same
//! way the banner reads elapsed time, so the 50 ms event loop never waits on a process.
//! The CLI calls [`probe`] directly. Download time does not rely on either: running
//! `ffmpeg` there reports a missing or broken binary on its own.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// What the latest finished check found out about `ffmpeg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfmpegStatus {
    /// No check has finished yet.
    Unknown,
    Available,
    Missing,
}

const UNKNOWN: u8 = 0;
const AVAILABLE: u8 = 1;
const MISSING: u8 = 2;

static LAST_RESULT: AtomicU8 = AtomicU8::new(UNKNOWN);
static CHECK_RUNNING: AtomicBool = AtomicBool::new(false);

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs `ffmpeg -version` and reports whether it started and exited successfully.
///
/// Blocks for at most five seconds; a hung `ffmpeg` is killed and counts as missing.
/// A successful run shows the binary works, not that it was built with the MP3 encoder.
pub fn probe() -> bool {
    let child = Command::new("ffmpeg")
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return false;
    };
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

/// Starts a fresh check on a background thread and returns immediately.
///
/// Does nothing while a check is already running. Until the new check finishes,
/// [`status`] keeps reporting the previous result.
pub fn refresh_in_background() {
    if CHECK_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = thread::Builder::new()
        .name("ffmpeg-probe".into())
        .spawn(|| {
            let result = if probe() { AVAILABLE } else { MISSING };
            LAST_RESULT.store(result, Ordering::SeqCst);
            CHECK_RUNNING.store(false, Ordering::SeqCst);
        });
    if spawned.is_err() {
        CHECK_RUNNING.store(false, Ordering::SeqCst);
    }
}

/// The result of the most recent finished check.
pub fn status() -> FfmpegStatus {
    match LAST_RESULT.load(Ordering::SeqCst) {
        AVAILABLE => FfmpegStatus::Available,
        MISSING => FfmpegStatus::Missing,
        _ => FfmpegStatus::Unknown,
    }
}
