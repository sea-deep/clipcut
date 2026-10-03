use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// Progress event phases — structured, not stringly typed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum ProgressEvent {
    Preparing,
    Downloading {
        percent: Option<f64>, // None = indeterminate
        speed: Option<String>,
        eta: Option<String>,
    },
    Processing {
        clip_index: usize,
        clips_total: usize,
        clip_percent: Option<f64>,
        overall_percent: f64,
    },
    Finalizing,
    Complete,
    Cancelled {
        clips_completed: usize,
        clips_total: usize,
    },
    Error {
        message: String,
    },
}

/// Emit a progress event to the frontend window.
pub fn emit(app: &AppHandle, event: ProgressEvent) {
    // Best-effort — if the window has closed, ignore.
    let _ = app.emit("export-progress", &event);
}

/// Parse a yt-dlp progress line like:
/// `[download]  45.2% of ~150.00MiB at 12.50MiB/s ETA 00:06`
pub fn parse_ytdlp_percent(line: &str) -> Option<f64> {
    // Only care about [download] lines with a percent
    if !line.contains("[download]") {
        return None;
    }
    // Find the first number followed by %
    let re = regex::Regex::new(r"(\d+\.?\d*)%").ok()?;
    let caps = re.captures(line)?;
    caps.get(1)?.as_str().parse::<f64>().ok()
}

/// Parse an ffmpeg `-progress pipe:1` output line.
/// Returns `out_time_us` when the key is `out_time_us`.
pub fn parse_ffmpeg_time_us(line: &str) -> Option<i64> {
    let line = line.trim();
    if let Some(val) = line.strip_prefix("out_time_us=") {
        val.parse::<i64>().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ytdlp_normal() {
        let line = "[download]  45.2% of ~150.00MiB at 12.50MiB/s ETA 00:06";
        assert!((parse_ytdlp_percent(line).unwrap() - 45.2).abs() < 0.01);
    }

    #[test]
    fn parse_ytdlp_100() {
        let line = "[download] 100% of 123.00MiB";
        assert!((parse_ytdlp_percent(line).unwrap() - 100.0).abs() < 0.01);
    }

    #[test]
    fn parse_ytdlp_non_download_line() {
        let line = "[info] Downloading video";
        assert!(parse_ytdlp_percent(line).is_none());
    }

    #[test]
    fn parse_ffmpeg_time() {
        let line = "out_time_us=12345678";
        assert_eq!(parse_ffmpeg_time_us(line), Some(12345678));
    }

    #[test]
    fn parse_ffmpeg_other_line() {
        let line = "frame=120";
        assert!(parse_ffmpeg_time_us(line).is_none());
    }
}
