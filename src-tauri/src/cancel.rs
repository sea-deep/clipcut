use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use once_cell::sync::Lazy;
use tokio::sync::Mutex;

/// Application-wide cancellation token.
/// Set by `cancel_export`; checked at every subprocess boundary.
static CANCEL_FLAG: Lazy<Arc<AtomicBool>> = Lazy::new(|| Arc::new(AtomicBool::new(false)));

/// Handle to the currently running yt-dlp child, if any.
/// Stored so cancellation can kill the process immediately.
static ACTIVE_YTDLP: Lazy<Mutex<Option<u32>>> = Lazy::new(|| Mutex::new(None));

/// Handles to active ffmpeg children.
static ACTIVE_FFMPEG: Lazy<Mutex<Vec<u32>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub fn cancel_flag() -> Arc<AtomicBool> {
    CANCEL_FLAG.clone()
}

pub fn is_cancelled() -> bool {
    CANCEL_FLAG.load(Ordering::Relaxed)
}

pub fn reset_cancel() {
    CANCEL_FLAG.store(false, Ordering::Relaxed);
}

pub async fn set_ytdlp_pid(pid: Option<u32>) {
    *ACTIVE_YTDLP.lock().await = pid;
}

pub async fn add_ffmpeg_pid(pid: u32) {
    ACTIVE_FFMPEG.lock().await.push(pid);
}

pub async fn remove_ffmpeg_pid(pid: u32) {
    ACTIVE_FFMPEG.lock().await.retain(|&p| p != pid);
}

/// Signal cancellation and kill all tracked subprocesses.
pub async fn cancel_all() {
    CANCEL_FLAG.store(true, Ordering::Relaxed);

    // Kill yt-dlp
    if let Some(pid) = *ACTIVE_YTDLP.lock().await {
        kill_pid(pid);
    }

    // Kill ffmpeg instances
    let pids: Vec<u32> = ACTIVE_FFMPEG.lock().await.clone();
    for pid in pids {
        kill_pid(pid);
    }
}

#[cfg(unix)]
fn kill_pid(pid: u32) {
    // SAFETY: SIGTERM is safe to send to a known child process PID
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    // Use taskkill with /T to kill the entire process tree (including any sidecar children)
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F", "/T"])
        .output();
}

#[cfg(not(any(unix, windows)))]
fn kill_pid(_pid: u32) {
    // No-op on unsupported platforms
}
