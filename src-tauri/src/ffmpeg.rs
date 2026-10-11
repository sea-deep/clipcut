use std::path::Path;

use tauri::AppHandle;

use crate::{
    binaries,
    clip::{Clip, ExportMode, OutputFormat},
    error::AppError,
    progress::{self, ProgressEvent},
};

/// Extract a single clip from a source file using ffmpeg.
///
/// `part_path` is the temporary .part path written during extraction.
/// After success, it is renamed atomically to `final_path`.
#[allow(clippy::too_many_arguments)]
pub async fn extract_clip(
    app: &AppHandle,
    source: &Path,
    clip: &Clip,
    part_path: &Path,
    final_path: &Path,
    format: &OutputFormat,
    _mode: &ExportMode,
    clip_index: usize,
    clips_total: usize,
) -> Result<(), AppError> {
    let source_str = source.to_string_lossy().to_string();
    let output_str = part_path.to_string_lossy().to_string();

    let start_str = format!("{:.3}", clip.start_seconds);
    let duration_str = format!("{:.3}", clip.end_seconds - clip.start_seconds);
    let duration_us = ((clip.end_seconds - clip.start_seconds) * 1_000_000.0) as i64;

    // Input-seeking (-ss before -i) jumps instantly to nearest keyframe,
    // then accurately decodes from there to the exact frame.
    // Near-instant seek + frame-accurate cut.
    let (vcodec, acodec, extra) = match format {
        OutputFormat::Mp4 | OutputFormat::Mkv => (
            "libx264",
            "aac",
            vec![
                "-preset".to_string(),
                "veryfast".to_string(),
                "-crf".to_string(),
                "18".to_string(),
                "-b:a".to_string(),
                "192k".to_string(),
                "-movflags".to_string(),
                "+faststart".to_string(),
            ],
        ),
        OutputFormat::Webm => (
            "libvpx-vp9",
            "libopus",
            vec![
                "-deadline".to_string(),
                "realtime".to_string(),
                "-cpu-used".to_string(),
                "4".to_string(),
                "-crf".to_string(),
                "30".to_string(),
                "-b:v".to_string(),
                "0".to_string(),
                "-b:a".to_string(),
                "128k".to_string(),
            ],
        ),
    };

    let mut args = vec![
        "-y".to_string(),
        "-ss".to_string(),
        start_str,
        "-i".to_string(),
        source_str,
        "-t".to_string(),
        duration_str,
        "-c:v".to_string(),
        vcodec.to_string(),
        "-c:a".to_string(),
        acodec.to_string(),
    ];
    args.extend(extra);
    let muxer_format = match format {
        OutputFormat::Mp4 => "mp4",
        OutputFormat::Webm => "webm",
        OutputFormat::Mkv => "matroska",
    };
    args.push("-f".to_string());
    args.push(muxer_format.to_string());
    args.push("-progress".to_string());
    args.push("pipe:1".to_string());
    args.push(output_str);

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let cmd = binaries::sidecar_command(app, "ffmpeg")?;
    let (mut rx, child) = cmd
        .args(&arg_refs)
        .spawn()
        .map_err(|e| AppError::ToolFailure {
            detail: format!("Failed to spawn ffmpeg: {}", e),
        })?;

    let pid = child.pid();
    crate::cancel::add_ffmpeg_pid(pid).await;

    let mut stderr_buf = String::new();

    use tauri_plugin_shell::process::CommandEvent;
    while let Some(event) = rx.recv().await {
        if crate::cancel::is_cancelled() {
            break;
        }
        match event {
            CommandEvent::Stdout(line) => {
                let text = String::from_utf8_lossy(&line).to_string();
                if let Some(us) = progress::parse_ffmpeg_time_us(&text) {
                    let clip_pct = if duration_us > 0 {
                        Some(((us as f64 / duration_us as f64) * 100.0).min(100.0))
                    } else {
                        None
                    };
                    // Overall: downloading = 60%, clipping = 40%
                    // Each clip gets an equal share of the 40%.
                    let clip_share = 40.0 / clips_total as f64;
                    let overall = 60.0
                        + (clip_index as f64 * clip_share)
                        + (clip_pct.unwrap_or(0.0) * clip_share / 100.0);
                    progress::emit(
                        app,
                        ProgressEvent::Processing {
                            clip_index: clip_index + 1,
                            clips_total,
                            clip_percent: clip_pct,
                            overall_percent: overall,
                        },
                    );
                }
            }
            CommandEvent::Stderr(line) => {
                stderr_buf.push_str(&String::from_utf8_lossy(&line));
            }
            CommandEvent::Terminated(status) => {
                crate::cancel::remove_ffmpeg_pid(pid).await;
                if crate::cancel::is_cancelled() {
                    // Clean up partial file
                    let _ = std::fs::remove_file(part_path);
                    return Err(AppError::Cancelled);
                }
                if !status.code.map(|c| c == 0).unwrap_or(false) {
                    let _ = std::fs::remove_file(part_path);
                    return Err(AppError::from_ffmpeg_stderr(&stderr_buf));
                }
            }
            _ => {}
        }
    }

    crate::cancel::remove_ffmpeg_pid(pid).await;

    if crate::cancel::is_cancelled() {
        let _ = std::fs::remove_file(part_path);
        return Err(AppError::Cancelled);
    }

    // Verify the output file is non-empty
    if !part_path.exists() || std::fs::metadata(part_path)?.len() == 0 {
        return Err(AppError::ToolFailure {
            detail: format!(
                "ffmpeg completed but output is missing or empty: {}",
                part_path.display()
            ),
        });
    }

    // Atomic rename: .part → final
    std::fs::rename(part_path, final_path)?;

    tracing::info!("Clip {} written: {}", clip_index + 1, final_path.display());
    Ok(())
}

/// Run ffprobe on a file and return its duration in seconds.
/// Used to verify output files after extraction.
pub async fn probe_duration(app: &AppHandle, path: &Path) -> Result<f64, AppError> {
    let path_str = path.to_string_lossy().to_string();
    let output = binaries::sidecar_command(app, "ffprobe")?
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_entries",
            "format=duration",
            &path_str,
        ])
        .output()
        .await
        .map_err(|e| AppError::ToolFailure {
            detail: format!("ffprobe failed: {}", e),
        })?;

    if !output.status.success() {
        return Err(AppError::ToolFailure {
            detail: format!(
                "ffprobe exited with error: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| AppError::Unknown(e.to_string()))?;

    json["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| AppError::ToolFailure {
            detail: "ffprobe returned no duration".into(),
        })
}
