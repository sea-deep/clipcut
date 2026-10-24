use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex};

use once_cell::sync::Lazy;
use tauri::AppHandle;
use tokio::sync::Mutex as TokioMutex;

use crate::{
    binaries,
    clip::{Clip, OutputFormat, QualityOption},
    error::AppError,
    paths,
    progress::{self, ProgressEvent},
};

static DOWNLOAD_LOCKS: Lazy<StdMutex<HashMap<String, Arc<TokioMutex<()>>>>> =
    Lazy::new(|| StdMutex::new(HashMap::new()));

fn get_download_lock(video_id: &str) -> Arc<TokioMutex<()>> {
    let mut map = DOWNLOAD_LOCKS.lock().unwrap();
    map.entry(video_id.to_string())
        .or_insert_with(|| Arc::new(TokioMutex::new(())))
        .clone()
}

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

    // For short videos (<= 10 mins / 600s), full download is fast, pre-warmed in background,
    // and enables instant ~0.05s ffmpeg cuts.
    if video_duration > 0.0 && video_duration <= 600.0 {
        return DownloadStrategy::Full;
    }

    let total_clip_duration: f64 = clips.iter().map(|c| c.duration()).sum();
    let coverage = if video_duration > 0.0 {
        total_clip_duration / video_duration
    } else {
        1.0
    };

    // Many tiny clips spread across a long video (> 10 mins) → partial is much faster.
    // Dense coverage → full download and clip locally.
    if coverage < 0.40 {
        DownloadStrategy::Partial
    } else {
        DownloadStrategy::Full
    }
}

fn base_ytdlp_args(app: &AppHandle, cache_dir_str: &str) -> Vec<String> {
    let mut args = vec![
        "--cache-dir".to_string(),
        cache_dir_str.to_string(),
        "-4".to_string(),
        "--no-warnings".to_string(),
    ];
    if let Some(ffmpeg_loc) = binaries::ffmpeg_location(app) {
        args.push("--ffmpeg-location".to_string());
        args.push(ffmpeg_loc.to_string_lossy().to_string());
    }
    if let Some(js_runtime) = binaries::js_runtime_arg() {
        args.push("--js-runtimes".to_string());
        args.push(js_runtime);
    }
    args
}

/// Download a full video to the cache directory.
/// Returns the path to the downloaded file.
pub async fn download_full(
    app: &AppHandle,
    url: &str,
    video_id: &str,
    quality: &QualityOption,
) -> Result<PathBuf, AppError> {
    let cache_dir = paths::video_cache_directory(video_id);
    paths::ensure_dir(&cache_dir)?;

    let output_path = cache_dir.join("source.mp4");

    // Coordinate through per-video lock so concurrent / pre-warm downloads don't duplicate work
    let lock = get_download_lock(video_id);
    let _guard = lock.lock().await;

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
    let ytdlp_cache = paths::cache_directory().join("ytdlp");
    let cache_dir_str = ytdlp_cache.to_string_lossy().to_string();

    let info_json_path = cache_dir.join("info.json");
    let has_info_json = info_json_path.exists()
        && std::fs::metadata(&info_json_path)
            .map(|m| m.len() > 0)
            .unwrap_or(false);
    let info_json_str = info_json_path.to_string_lossy().to_string();

    // Fast path: use pre-fetched info.json to eliminate YouTube network handshake
    if has_info_json {
        let mut args = base_ytdlp_args(app, &cache_dir_str);
        args.extend([
            "--load-info-json".to_string(),
            info_json_str,
            "-f".to_string(),
            quality.format_selector().to_string(),
            "--merge-output-format".to_string(),
            "mp4".to_string(),
            "-N".to_string(),
            "4".to_string(),
            "--no-playlist".to_string(),
            "--newline".to_string(),
            "-o".to_string(),
            output_template.clone(),
        ]);

        let res = run_ytdlp_download(app, &args, &output_path).await;
        if res.is_ok() || crate::cancel::is_cancelled() {
            return res.map(|_| output_path);
        }
        tracing::warn!(
            "Pre-fetched info.json full download failed, falling back to direct URL fetch: {:?}",
            res.err()
        );
    }

    // Fallback: direct URL fetch
    let mut args = base_ytdlp_args(app, &cache_dir_str);
    args.extend([
        "-f".to_string(),
        quality.format_selector().to_string(),
        "--merge-output-format".to_string(),
        "mp4".to_string(),
        "-N".to_string(),
        "4".to_string(),
        "--no-playlist".to_string(),
        "--newline".to_string(),
        "-o".to_string(),
        output_template,
        url.to_string(),
    ]);

    run_ytdlp_download(app, &args, &output_path).await?;
    Ok(output_path)
}

async fn run_ytdlp_download(
    app: &AppHandle,
    args: &[String],
    output_path: &Path,
) -> Result<(), AppError> {
    let cmd = binaries::sidecar_command(app, "yt-dlp")?;
    let (mut rx, child) = cmd.args(args).spawn().map_err(|e| AppError::ToolFailure {
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
                    let _ = std::fs::remove_file(output_path);
                    let _ = std::fs::remove_file(output_path.with_extension("mp4.part"));
                    return Err(AppError::Cancelled);
                }
                if !status.code.map(|c| c == 0).unwrap_or(false) {
                    let _ = std::fs::remove_file(output_path);
                    let _ = std::fs::remove_file(output_path.with_extension("mp4.part"));
                    return Err(AppError::from_ytdlp_stderr(&stderr_buf));
                }
            }
            _ => {}
        }
    }

    crate::cancel::set_ytdlp_pid(None).await;

    if crate::cancel::is_cancelled() {
        let _ = std::fs::remove_file(output_path);
        let _ = std::fs::remove_file(output_path.with_extension("mp4.part"));
        return Err(AppError::Cancelled);
    }

    // Verify the file exists and has content
    if !output_path.exists() || std::fs::metadata(output_path)?.len() == 0 {
        return Err(AppError::ToolFailure {
            detail: "yt-dlp completed but output file is missing or empty.".into(),
        });
    }

    Ok(())
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

    let ytdlp_cache = paths::cache_directory().join("ytdlp");
    let cache_dir_str = ytdlp_cache.to_string_lossy().to_string();

    let mut args = base_ytdlp_args(app, &cache_dir_str);
    args.extend([
        "-f".into(),
        "bestvideo[ext=mp4]+bestaudio[ext=m4a]/bestvideo+bestaudio/best".into(),
        "--merge-output-format".into(),
        "mp4".into(),
        "--force-keyframes-at-cuts".into(),
        "--downloader-args".into(),
        "ffmpeg:-preset ultrafast".into(),
        "-N".into(),
        "4".into(),
        "--no-playlist".into(),
        "--newline".into(),
        "-o".into(),
        output_str,
    ]);
    args.extend(section_args);
    args.push(url.to_string());

    let cmd = binaries::sidecar_command(app, "yt-dlp")?;
    let (mut rx, child) = cmd.args(&args).spawn().map_err(|e| AppError::ToolFailure {
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
#[allow(clippy::too_many_arguments)]
async fn execute_ytdlp_cut(
    app: &AppHandle,
    args: &[String],
    duration: f64,
    output_path: &Path,
    part_path: &Path,
    output_str: &str,
    clip_index: usize,
    clips_total: usize,
) -> Result<(), AppError> {
    let cmd = binaries::sidecar_command(app, "yt-dlp")?;
    let (mut rx, child) = cmd.args(args).spawn().map_err(|e| AppError::ToolFailure {
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
                    let overall =
                        ((clip_index as f64 + (pct / 100.0)) / clips_total as f64) * 100.0;
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
                    let overall =
                        ((clip_index as f64 + (clip_pct / 100.0)) / clips_total as f64) * 100.0;
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
                    let _ = std::fs::remove_file(part_path);
                    return Err(AppError::Cancelled);
                }
                if !status.code.map(|c| c == 0).unwrap_or(false) {
                    let _ = std::fs::remove_file(output_path);
                    let _ = std::fs::remove_file(part_path);
                    return Err(AppError::from_ytdlp_stderr(&stderr_buf));
                }
            }
            _ => {}
        }
    }

    crate::cancel::set_ytdlp_pid(None).await;

    if crate::cancel::is_cancelled() {
        let _ = std::fs::remove_file(output_path);
        let _ = std::fs::remove_file(part_path);
        return Err(AppError::Cancelled);
    }

    // Verify output file exists and is not empty
    if !output_path.exists() || std::fs::metadata(output_path)?.len() == 0 {
        let _ = std::fs::remove_file(output_path);
        let _ = std::fs::remove_file(part_path);
        return Err(AppError::ToolFailure {
            detail: format!(
                "yt-dlp section extraction completed but output file is missing or empty: {}",
                output_str
            ),
        });
    }

    Ok(())
}

/// Download a single clip section directly to the target output path using yt-dlp.
/// Reuses pre-fetched info.json to skip YouTube HTML and JS challenge handshakes,
/// downloading and transcoding the cut in just a few seconds.
#[allow(clippy::too_many_arguments)]
pub async fn download_clip_section(
    app: &AppHandle,
    url: &str,
    video_id: &str,
    clip: &Clip,
    output_path: &Path,
    format: &OutputFormat,
    quality: &QualityOption,
    clip_index: usize,
    clips_total: usize,
) -> Result<(), AppError> {
    let output_str = output_path.to_string_lossy().to_string();
    let part_path = output_path.with_extension(format!("{}.part", format.extension()));
    let _ = std::fs::remove_file(output_path);
    let _ = std::fs::remove_file(&part_path);

    let section_arg = format!("*{:.3}-{:.3}", clip.start_seconds, clip.end_seconds);
    let duration = (clip.end_seconds - clip.start_seconds).max(0.1);

    let ytdlp_cache = paths::cache_directory().join("ytdlp");
    let cache_dir_str = ytdlp_cache.to_string_lossy().to_string();

    let info_json_path = paths::video_cache_directory(video_id).join("info.json");
    let has_info_json = info_json_path.exists()
        && std::fs::metadata(&info_json_path)
            .map(|m| m.len() > 0)
            .unwrap_or(false);
    let info_json_str = info_json_path.to_string_lossy().to_string();

    // Fast path: reuse pre-fetched video metadata to eliminate network handshake
    if has_info_json {
        let mut args = base_ytdlp_args(app, &cache_dir_str);
        args.extend([
            "--load-info-json".to_string(),
            info_json_str,
            "-f".to_string(),
            quality.format_selector().to_string(),
            "--merge-output-format".to_string(),
            format.extension().to_string(),
            "--force-keyframes-at-cuts".to_string(),
            "--downloader-args".to_string(),
            "ffmpeg:-preset ultrafast".to_string(),
            "-N".to_string(),
            "4".to_string(),
            "--download-sections".to_string(),
            section_arg.clone(),
            "--no-playlist".to_string(),
            "--newline".to_string(),
            "-o".to_string(),
            output_str.clone(),
        ]);

        let res = execute_ytdlp_cut(
            app,
            &args,
            duration,
            output_path,
            &part_path,
            &output_str,
            clip_index,
            clips_total,
        )
        .await;

        if res.is_ok() || crate::cancel::is_cancelled() {
            return res;
        }

        tracing::warn!(
            "Pre-fetched info.json cut failed or expired, retrying with direct URL fetch: {:?}",
            res.err()
        );
        let _ = std::fs::remove_file(&info_json_path);
        let _ = std::fs::remove_file(output_path);
        let _ = std::fs::remove_file(&part_path);
    }

    // Fallback: direct URL fetch
    let mut args = base_ytdlp_args(app, &cache_dir_str);
    args.extend([
        "-f".to_string(),
        quality.format_selector().to_string(),
        "--merge-output-format".to_string(),
        format.extension().to_string(),
        "--force-keyframes-at-cuts".to_string(),
        "--downloader-args".to_string(),
        "ffmpeg:-preset ultrafast".to_string(),
        "-N".to_string(),
        "4".to_string(),
        "--download-sections".to_string(),
        section_arg,
        "--no-playlist".to_string(),
        "--newline".to_string(),
        "-o".to_string(),
        output_str.clone(),
        url.to_string(),
    ]);

    execute_ytdlp_cut(
        app,
        &args,
        duration,
        output_path,
        &part_path,
        &output_str,
        clip_index,
        clips_total,
    )
    .await
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
