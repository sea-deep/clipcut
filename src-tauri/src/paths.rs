use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Resolve the user's Videos directory, cross-platform.
/// Never hardcodes paths or separators — uses OS APIs throughout.
pub fn default_video_directory() -> PathBuf {
    dirs::video_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Videos")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Default ClipCut output root: <Videos>/ClipCut
pub fn default_output_directory() -> PathBuf {
    default_video_directory().join("ClipCut")
}

/// OS-appropriate directory for ClipCut's temporary cache.
/// Uses the OS cache/temp dir — never pollutes the output directory.
pub fn cache_directory() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("clipcut")
}

/// Per-video cache directory, keyed by stable video ID (not title).
pub fn video_cache_directory(video_id: &str) -> PathBuf {
    cache_directory().join(video_id)
}

/// OS log directory for ClipCut.
pub fn log_directory() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("ClipCut")
        .join("logs")
}

/// Sanitize a string for use as a filename component, safe on both Windows and Linux.
///
/// Handles:
/// - Windows reserved characters: / \ : * ? " < > |
/// - Windows reserved names: CON, PRN, AUX, NUL, COM1–COM9, LPT1–LPT9
/// - Trailing dots and spaces (Windows restriction)
/// - Very long names (truncated to 200 chars)
/// - Unicode is preserved where safe
pub fn sanitize_filename(name: &str) -> String {
    // Replace forbidden characters
    let replaced: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();

    // Trim leading/trailing whitespace and dots
    let trimmed = replaced.trim().trim_end_matches('.').trim();

    // Truncate to 200 characters (UTF-8 char boundary safe)
    let truncated: String = trimmed.chars().take(200).collect();

    // Reject/replace Windows reserved device names (case-insensitive)
    let lower = truncated.to_lowercase();
    let reserved_names = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
        "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    if reserved_names.contains(&lower.as_str()) {
        format!("_{}", truncated)
    } else if truncated.is_empty() {
        "clip".to_string()
    } else {
        truncated
    }
}

/// Build the output filename for a single clip.
/// Format: `01_00-15_00-42.mp4`
pub fn clip_filename(index: usize, start_seconds: f64, end_seconds: f64, extension: &str) -> String {
    format!(
        "{:02}_{}.{}",
        index + 1,
        format_time_range(start_seconds, end_seconds),
        extension
    )
}

/// Format a time range as `MM-SS_MM-SS` or `HH-MM-SS_HH-MM-SS`.
fn format_time_range(start: f64, end: f64) -> String {
    format!("{}_{}", format_secs(start), format_secs(end))
}

/// Format seconds as `MM-SS` or `HH-MM-SS` (using `-` instead of `:` for filenames).
fn format_secs(secs: f64) -> String {
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{:02}-{:02}-{:02}", h, m, s)
    } else {
        format!("{:02}-{:02}", m, s)
    }
}

/// Resolve a non-conflicting output path. If the file already exists,
/// appends ` (2)`, ` (3)`, etc. before the extension.
pub fn resolve_output_path(dir: &Path, filename: &str) -> PathBuf {
    let path = dir.join(filename);
    if !path.exists() {
        return path;
    }
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("clip");
    let ext = Path::new(filename)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("mp4");
    for n in 2..=999 {
        let candidate = dir.join(format!("{} ({}).{}", stem, n, ext));
        if !candidate.exists() {
            return candidate;
        }
    }
    // Fallback with UUID suffix
    dir.join(format!("{}_{}.{}", stem, uuid::Uuid::new_v4(), ext))
}

/// Create a directory and all parents, returning AppError on failure.
pub fn ensure_dir(path: &Path) -> Result<(), AppError> {
    std::fs::create_dir_all(path).map_err(AppError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_normal() {
        assert_eq!(sanitize_filename("Hello World"), "Hello World");
    }

    #[test]
    fn sanitize_forbidden_chars() {
        assert_eq!(sanitize_filename("a/b\\c:d*e?f\"g<h>i|j"), "a_b_c_d_e_f_g_h_i_j");
    }

    #[test]
    fn sanitize_trailing_dot() {
        assert_eq!(sanitize_filename("file."), "file");
    }

    #[test]
    fn sanitize_reserved_name() {
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("nul"), "_nul");
    }

    #[test]
    fn sanitize_empty() {
        assert_eq!(sanitize_filename(""), "clip");
    }

    #[test]
    fn sanitize_unicode() {
        assert_eq!(sanitize_filename("日本語タイトル"), "日本語タイトル");
    }

    #[test]
    fn sanitize_emoji() {
        assert_eq!(sanitize_filename("🎬 Clip"), "🎬 Clip");
    }

    #[test]
    fn sanitize_very_long() {
        let long = "a".repeat(300);
        assert_eq!(sanitize_filename(&long).len(), 200);
    }

    #[test]
    fn clip_filename_format() {
        let name = clip_filename(0, 15.0, 42.0, "mp4");
        assert_eq!(name, "01_00-15_00-42.mp4");
    }

    #[test]
    fn clip_filename_hours() {
        let name = clip_filename(2, 3735.0, 3800.0, "mp4");
        assert_eq!(name, "03_01-02-15_01-03-20.mp4");
    }
}
