use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::{
    binaries,
    clip::{Clip, OutputFormat},
    error::AppError,
    paths,
    progress::{self, ProgressEvent},
};

/// Download strategy for a given video + clip set.
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadStrategy {
    /// Download only the needed time sections.
    /// Best when clips are a small fraction of video duration.
    Partial,
    /// Download the full video once.
    /// Better when clips are dense or the full source is already cached.
    Full,
}

/// Choose a download strategy.
///
/// The 40% threshold is a starting heuristic — see docs/DECISIONS.md.
/// This is the single place to tune strategy selection.
pub fn choose_strategy(clips: &[Clip], video_duration: f64, cached_full: bool) -> DownloadStrategy {
    // If a full download already exists in cache, always reuse it.
    if cached_full {
        return DownloadStrategy::Full;
    }

    let total_clip_duration: f64 = clips.iter().map(|c| c.duration()).sum();
    let coverage = if video_duration > 0.0 {
        total_clip_duration / video_duration
    } else {
        1.0
    };

    // Many tiny clips spread across a long video → partial is much faster.
    // Dense coverage → full download and clip locally.
    if coverage < 0.40 {
        DownloadStrategy::Partial
    } else {
        DownloadStrategy::Full
    }
}

/// Download a full video to the cache directory.
/// Returns the path to the downloaded file.
pub async fn download_full(
    app: &AppHandle,
    url: &str,
    video_id: &str,
) -> Result<PathBuf, AppError> {
    let cache_dir = paths::video_cache_directory(video_id);
    paths::ensure_dir(&cache_dir)?;

    let output_path = cache_dir.join("source.mp4");

    // Skip if already fully downloaded (trust size > 0 as a basic check)
    if output_path.exists() {
        let meta = std::fs::metadata(&output_path)?;
        if meta.len() > 0 {
            tracing::info!("Reusing cached source for {}", video_id);
            return Ok(output_path);
        }
    }

    tracing::info!("Starting full download for {}", video_id);

    let output_template = output_path.to_string_lossy().to_string();

    let args: Vec<&str> = vec![
        // Format: prefer standard HTTP/HTTPS streams, merge to mp4
        "-f",
        "bv*[protocol^=http]+ba[protocol^=http]/b[protocol^=http]/best",
        "--merge-output-format",
        "mp4",
        "--no-playlist",
        "--newline",          // One progress line per line
        "--no-part",          // Don't write .part files (we manage atomicity ourselves)
        "--js-runtimes",
        "node",
        "-o",
        &output_template,
        url,
    ];

    let cmd = binaries::sidecar_command(app, "yt-dlp")?;
    let (mut rx, child) = cmd
        .args(&args)
        .spawn()
        .map_err(|e| AppError::ToolFailure {
            detail: format!("Failed to spawn yt-dlp: {}", e),
        })?;

    // Track PID for cancellation
    crate::cancel::set_ytdlp_pid(Some(child.pid())).await;

    let mut stderr_buf = String::new();

    use tauri_plugin_shell::process::CommandEvent;
    while let Some(event) = rx.recv().await {
        if crate::cancel::is_cancelled() {
            break;
        }
        match event {
            CommandEvent::Stdout(line) => {
                let text = String::from_utf8_lossy(&line).to_string();
                tracing::debug!("[yt-dlp] {}", text.trim());
                if let Some(pct) = progress::parse_ytdlp_percent(&text) {
                    progress::emit(
                        app,
                        ProgressEvent::Downloading {
                            percent: Some(pct),
                            speed: None,
                            eta: None,
                        },
                    );
                }
            }
            CommandEvent::Stderr(line) => {
                let text = String::from_utf8_lossy(&line).to_string();
                tracing::warn!("[yt-dlp stderr] {}", text.trim());
                stderr_buf.push_str(&text);
            }
            CommandEvent::Error(e) => {
                tracing::error!("[yt-dlp error] {}", e);
            }
            CommandEvent::Terminated(status) => {
                crate::cancel::set_ytdlp_pid(None).await;
                if crate::cancel::is_cancelled() {
                    return Err(AppError::Cancelled);
                }
                if !status.code.map(|c| c == 0).unwrap_or(false) {
                    return Err(AppError::from_ytdlp_stderr(&stderr_buf));
                }
            }
            _ => {}
        }
    }

    crate::cancel::set_ytdlp_pid(None).await;

    if crate::cancel::is_cancelled() {
        return Err(AppError::Cancelled);
    }

    // Verify the file exists and has content
    if !output_path.exists() || std::fs::metadata(&output_path)?.len() == 0 {
        return Err(AppError::ToolFailure {
            detail: "yt-dlp completed but output file is missing or empty.".into(),
        });
    }

    Ok(output_path)
}

/// Download only specific time sections using yt-dlp's --download-sections.
/// Returns paths to each downloaded section file.
pub async fn download_sections(
    app: &AppHandle,
    url: &str,
    video_id: &str,
    clips: &[Clip],
) -> Result<Vec<PathBuf>, AppError> {
    let cache_dir = paths::video_cache_directory(video_id).join("sections");
    paths::ensure_dir(&cache_dir)?;

    let mut section_args: Vec<String> = Vec::new();
    for clip in clips {
        section_args.push("--download-sections".to_string());
        section_args.push(format!("*{}-{}", clip.start_seconds, clip.end_seconds));
    }

    let output_template = cache_dir.join("%(section_start)s-%(section_end)s.%(ext)s");
    let output_str = output_template.to_string_lossy().to_string();

    let mut args: Vec<String> = vec![
        "-f".into(),
        "bestvideo[ext=mp4]+bestaudio[ext=m4a]/bestvideo+bestaudio/best".into(),
        "--merge-output-format".into(),
        "mp4".into(),
        "--force-keyframes-at-cuts".into(),
        "--no-playlist".into(),
        "--newline".into(),
        "-o".into(),
        output_str,
    ];
    args.extend(section_args);
    args.push(url.to_string());

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let cmd = binaries::sidecar_command(app, "yt-dlp")?;
    let (mut rx, child) = cmd
        .args(&arg_refs)
        .spawn()
        .map_err(|e| AppError::ToolFailure {
            detail: format!("Failed to spawn yt-dlp: {}", e),
        })?;

    crate::cancel::set_ytdlp_pid(Some(child.pid())).await;

    let mut stderr_buf = String::new();

    use tauri_plugin_shell::process::CommandEvent;
    while let Some(event) = rx.recv().await {
        if crate::cancel::is_cancelled() {
            break;
        }
        match event {
            CommandEvent::Stdout(line) => {
                let text = String::from_utf8_lossy(&line).to_string();
                tracing::debug!("[yt-dlp sections] {}", text.trim());
                if let Some(pct) = progress::parse_ytdlp_percent(&text) {
                    progress::emit(
                        app,
                        ProgressEvent::Downloading {
                            percent: Some(pct),
                            speed: None,
                            eta: None,
                        },
                    );
                }
            }
            CommandEvent::Stderr(line) => {
                stderr_buf.push_str(&String::from_utf8_lossy(&line));
            }
            CommandEvent::Terminated(status) => {
                crate::cancel::set_ytdlp_pid(None).await;
                if crate::cancel::is_cancelled() {
                    return Err(AppError::Cancelled);
                }
                if !status.code.map(|c| c == 0).unwrap_or(false) {
                    return Err(AppError::from_ytdlp_stderr(&stderr_buf));
                }
            }
            _ => {}
        }
    }

    crate::cancel::set_ytdlp_pid(None).await;

    if crate::cancel::is_cancelled() {
        return Err(AppError::Cancelled);
    }

    // Collect the downloaded section files
    let mut paths: Vec<PathBuf> = Vec::new();
    let rd = std::fs::read_dir(&cache_dir)?;
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().map(|e| e == "mp4").unwrap_or(false) {
            paths.push(p);
        }
    }

    Ok(paths)
}

/// Download a single clip section directly to the target output path using yt-dlp.
/// yt-dlp handles keyframes and ultrafast transcoding at cuts, producing a clean,
/// frame-accurate cut in seconds without downloading the full source video.
pub async fn download_clip_section(
    app: &AppHandle,
    url: &str,
    clip: &Clip,
    output_path: &Path,
    format: &OutputFormat,
    clip_index: usize,
    clips_total: usize,
) -> Result<(), AppError> {
    let output_str = output_path.to_string_lossy().to_string();
    let part_path = output_path.with_extension(format!("{}.part", format.extension()));
    let _ = std::fs::remove_file(output_path);
    let _ = std::fs::remove_file(&part_path);

    let section_arg = format!("*{:.3}-{:.3}", clip.start_seconds, clip.end_seconds);
    let duration = (clip.end_seconds - clip.start_seconds).max(0.1);

    let args: Vec<&str> = vec![
        "-f",
        "bv*[height<=1080][protocol^=http]+ba[protocol^=http]/b[height<=1080][protocol^=http]/best",
        "--merge-output-format",
        format.extension(),
        "--force-keyframes-at-cuts",
        "--downloader-args",
        "ffmpeg:-preset ultrafast",
        "--download-sections",
        &section_arg,
        "--no-playlist",
        "--newline",
        "--js-runtimes",
        "node",
        "-o",
        &output_str,
        url,
    ];

    let cmd = binaries::sidecar_command(app, "yt-dlp")?;
    let (mut rx, child) = cmd
        .args(&args)
        .spawn()
        .map_err(|e| AppError::ToolFailure {
            detail: format!("Failed to spawn yt-dlp: {}", e),
        })?;

    crate::cancel::set_ytdlp_pid(Some(child.pid())).await;

    let initial_overall = (clip_index as f64 / clips_total as f64) * 100.0;
    progress::emit(
        app,
        ProgressEvent::Processing {
            clip_index,
            clips_total,
            clip_percent: Some(0.0),
            overall_percent: initial_overall,
        },
    );

    let mut stderr_buf = String::new();

    use tauri_plugin_shell::process::CommandEvent;
    while let Some(event) = rx.recv().await {
        if crate::cancel::is_cancelled() {
            break;
        }
        match event {
            CommandEvent::Stdout(line) => {
                let text = String::from_utf8_lossy(&line).to_string();
                tracing::debug!("[yt-dlp section stdout] {}", text.trim());
                if let Some(pct) = progress::parse_ytdlp_percent(&text) {
                    let overall = ((clip_index as f64 + (pct / 100.0)) / clips_total as f64) * 100.0;
                    progress::emit(
                        app,
                        ProgressEvent::Processing {
                            clip_index,
                            clips_total,
                            clip_percent: Some(pct),
                            overall_percent: overall.min(100.0),
                        },
                    );
                }
            }
            CommandEvent::Stderr(line) => {
                let text = String::from_utf8_lossy(&line).to_string();
                tracing::debug!("[yt-dlp section stderr] {}", text.trim());
                if let Some(cur_sec) = progress::parse_time_seconds(&text) {
                    let clip_pct = ((cur_sec / duration) * 100.0).clamp(0.0, 100.0);
                    let overall = ((clip_index as f64 + (clip_pct / 100.0)) / clips_total as f64) * 100.0;
                    progress::emit(
                        app,
                        ProgressEvent::Processing {
                            clip_index,
                            clips_total,
                            clip_percent: Some(clip_pct),
                            overall_percent: overall.min(100.0),
                        },
                    );
                }
                stderr_buf.push_str(&text);
            }
            CommandEvent::Terminated(status) => {
                crate::cancel::set_ytdlp_pid(None).await;
                if crate::cancel::is_cancelled() {
                    let _ = std::fs::remove_file(output_path);
                    let _ = std::fs::remove_file(&part_path);
                    return Err(AppError::Cancelled);
                }
                if !status.code.map(|c| c == 0).unwrap_or(false) {
                    let _ = std::fs::remove_file(output_path);
                    let _ = std::fs::remove_file(&part_path);
                    return Err(AppError::from_ytdlp_stderr(&stderr_buf));
                }
            }
            _ => {}
        }
    }

    crate::cancel::set_ytdlp_pid(None).await;

    if crate::cancel::is_cancelled() {
        let _ = std::fs::remove_file(output_path);
        let _ = std::fs::remove_file(&part_path);
        return Err(AppError::Cancelled);
    }

    // Verify output file exists and is not empty
    if !output_path.exists() || std::fs::metadata(output_path)?.len() == 0 {
        let _ = std::fs::remove_file(output_path);
        let _ = std::fs::remove_file(&part_path);
        return Err(AppError::ToolFailure {
            detail: format!(
                "yt-dlp section extraction completed but output file is missing or empty: {}",
                output_str
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::Clip;

    fn clip(s: f64, e: f64) -> Clip {
        Clip {
            start_seconds: s,
            end_seconds: e,
        }
    }

    #[test]
    fn strategy_small_coverage_prefers_partial() {
        let clips = vec![clip(10.0, 40.0), clip(60.0, 90.0)];
        let video_duration = 3600.0; // 1 hour
        assert_eq!(
            choose_strategy(&clips, video_duration, false),
            DownloadStrategy::Partial
        );
    }

    #[test]
    fn strategy_dense_coverage_prefers_full() {
        let clips = vec![clip(0.0, 300.0), clip(350.0, 600.0)];
        let video_duration = 600.0; // 10 min
        assert_eq!(
            choose_strategy(&clips, video_duration, false),
            DownloadStrategy::Full
        );
    }

    #[test]
    fn strategy_cached_full_always_reuses() {
        let clips = vec![clip(10.0, 20.0)]; // tiny coverage
        let video_duration = 3600.0;
        // Even though coverage is tiny, cached full → use full
        assert_eq!(
            choose_strategy(&clips, video_duration, true),
            DownloadStrategy::Full
        );
    }

    #[test]
    fn strategy_borderline() {
        // Exactly 40% coverage → full
        let clips = vec![clip(0.0, 400.0)];
        let video_duration = 1000.0;
        assert_eq!(
            choose_strategy(&clips, video_duration, false),
            DownloadStrategy::Full
        );
    }

    #[test]
    fn strategy_just_below_threshold() {
        // 39% coverage → partial
        let clips = vec![clip(0.0, 390.0)];
        let video_duration = 1000.0;
        assert_eq!(
            choose_strategy(&clips, video_duration, false),
            DownloadStrategy::Partial
        );
    }
}
