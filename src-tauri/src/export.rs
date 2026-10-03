use std::path::PathBuf;

use crate::{
    clip::{Clip, ExportOptions, ExportResult, VideoInfo},
    download::{self, DownloadStrategy},
    error::AppError,
    ffmpeg,
    paths,
    progress::{self, ProgressEvent},
};
use tauri::AppHandle;

/// Top-level export pipeline.
///
/// Steps:
/// 1. Validate all clips
/// 2. Create output directory
/// 3. Inspect cache
/// 4. Choose download strategy
/// 5. Download (or reuse cached source)
/// 6. Extract clips sequentially (parallel is a future optimisation once benchmarked)
/// 7. Verify outputs
/// 8. Report
pub async fn run_export(
    app: &AppHandle,
    video: &VideoInfo,
    clips: &[Clip],
    options: &ExportOptions,
) -> Result<ExportResult, AppError> {
    // --- 1. Validate ---
    for clip in clips {
        clip.validate(video.duration_seconds)?;
    }

    // --- 2. Create output directory ---
    let title_safe = paths::sanitize_filename(&video.title);
    let output_dir = PathBuf::from(&options.output_directory).join(&title_safe);
    paths::ensure_dir(&output_dir)?;

    // --- 3. Cache check ---
    let cache_dir = paths::video_cache_directory(&video.id);
    let cached_source = cache_dir.join("source.mp4");
    let cached_full = cached_source.exists()
        && std::fs::metadata(&cached_source)
            .map(|m| m.len() > 0)
            .unwrap_or(false);

    // --- 4. Strategy ---
    let strategy = download::choose_strategy(clips, video.duration_seconds, cached_full);
    tracing::info!("Export strategy: {:?} for video {}", strategy, video.id);

    progress::emit(app, ProgressEvent::Preparing);

    let mut completed_files: Vec<String> = Vec::new();

    match strategy {
        DownloadStrategy::Partial => {
            // Direct fast section download for each clip:
            // yt-dlp only downloads the exact bytes needed, transcodes cuts with ultrafast preset,
            // and saves directly into the target clip filename.
            for (i, clip) in clips.iter().enumerate() {
                if crate::cancel::is_cancelled() {
                    let n = completed_files.len();
                    progress::emit(
                        app,
                        ProgressEvent::Cancelled {
                            clips_completed: n,
                            clips_total: clips.len(),
                        },
                    );
                    return Ok(ExportResult {
                        files: completed_files,
                        output_directory: output_dir.to_string_lossy().to_string(),
                        cancelled: true,
                        clips_completed: n,
                        clips_total: clips.len(),
                    });
                }

                let filename = paths::clip_filename(i, clip.start_seconds, clip.end_seconds, options.format.extension());
                let final_path = paths::resolve_output_path(&output_dir, &filename);

                download::download_clip_section(
                    app,
                    &video.url,
                    &video.id,
                    clip,
                    &final_path,
                    &options.format,
                    &options.quality,
                    i,
                    clips.len(),
                )
                .await?;

                completed_files.push(final_path.to_string_lossy().to_string());
            }
        }
        DownloadStrategy::Full => {
            let source_path = if cached_full {
                cached_source
            } else {
                progress::emit(
                    app,
                    ProgressEvent::Downloading {
                        percent: None,
                        speed: None,
                        eta: None,
                    },
                );
                download::download_full(app, &video.url, &video.id, &options.quality).await?
            };

            if crate::cancel::is_cancelled() {
                return Err(AppError::Cancelled);
            }

            for (i, clip) in clips.iter().enumerate() {
                if crate::cancel::is_cancelled() {
                    let n = completed_files.len();
                    progress::emit(
                        app,
                        ProgressEvent::Cancelled {
                            clips_completed: n,
                            clips_total: clips.len(),
                        },
                    );
                    return Ok(ExportResult {
                        files: completed_files,
                        output_directory: output_dir.to_string_lossy().to_string(),
                        cancelled: true,
                        clips_completed: n,
                        clips_total: clips.len(),
                    });
                }

                let filename = paths::clip_filename(i, clip.start_seconds, clip.end_seconds, options.format.extension());
                let final_path = paths::resolve_output_path(&output_dir, &filename);
                let part_path = final_path.with_extension(format!("{}.part", options.format.extension()));

                ffmpeg::extract_clip(
                    app,
                    &source_path,
                    clip,
                    &part_path,
                    &final_path,
                    &options.format,
                    &options.mode,
                    i,
                    clips.len(),
                )
                .await?;

                completed_files.push(final_path.to_string_lossy().to_string());
            }
        }
    }

    progress::emit(app, ProgressEvent::Finalizing);

    // --- 7. Verify at least one output file ---
    for file in &completed_files {
        let p = PathBuf::from(file);
        if !p.exists() || std::fs::metadata(&p).map(|m| m.len() == 0).unwrap_or(true) {
            return Err(AppError::ToolFailure {
                detail: format!("Output file missing or empty: {}", file),
            });
        }
    }

    progress::emit(app, ProgressEvent::Complete);

    Ok(ExportResult {
        output_directory: output_dir.to_string_lossy().to_string(),
        clips_completed: completed_files.len(),
        clips_total: clips.len(),
        files: completed_files,
        cancelled: false,
    })
}
