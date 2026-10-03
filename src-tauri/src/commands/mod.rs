use std::sync::Mutex;

use once_cell::sync::Lazy;
use tauri::AppHandle;

use crate::{
    binaries,
    clip::{Clip, ExportOptions, ExportResult, VideoInfo},
    error::AppError,
    export,
    paths,
    progress,
};

/// Generation counter to prevent stale metadata from overwriting newer requests.
/// Incremented on every `load_video` call.
static LOAD_GENERATION: Lazy<Mutex<u64>> = Lazy::new(|| Mutex::new(0));

/// Load video metadata from a YouTube URL using yt-dlp --dump-json.
/// Returns only the fields ClipCut needs — not the full yt-dlp payload.
#[tauri::command]
pub async fn load_video(app: AppHandle, url: String) -> Result<VideoInfo, AppError> {
    // Validate URL shape before hitting the network
    if !is_youtube_url(&url) {
        return Err(AppError::InvalidUrl);
    }

    // Increment generation
    let generation = {
        let mut g = LOAD_GENERATION.lock().unwrap();
        *g += 1;
        *g
    };

    tracing::info!("Loading video metadata (gen {}): {}", generation, url);

    let output = binaries::sidecar_command(&app, "yt-dlp")
        .map_err(|e| e)?
        .args(&[
            "--dump-json",
            "--no-download",
            "--no-playlist",
            &url,
        ])
        .output()
        .await
        .map_err(|e| AppError::ToolFailure {
            detail: format!("yt-dlp metadata failed: {}", e),
        })?;

    // Check generation hasn't been superseded
    {
        let g = LOAD_GENERATION.lock().unwrap();
        if *g != generation {
            return Err(AppError::Cancelled); // Silently drop stale response
        }
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        tracing::warn!("yt-dlp metadata error: {}", stderr);
        return Err(AppError::from_ytdlp_stderr(&stderr));
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|_| AppError::VideoUnavailable)?;

    let id = json["id"]
        .as_str()
        .ok_or(AppError::VideoUnavailable)?
        .to_string();

    let title = json["title"]
        .as_str()
        .unwrap_or("Untitled")
        .to_string();

    let duration_seconds = json["duration"]
        .as_f64()
        .ok_or(AppError::VideoUnavailable)?;

    let thumbnail_url = json["thumbnail"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(VideoInfo {
        id,
        url,
        title,
        duration_seconds,
        thumbnail_url,
    })
}

/// Begin an export. Runs in a background task; emits progress events.
/// Returns the final ExportResult on success.
#[tauri::command]
pub async fn export_clips(
    app: AppHandle,
    video: VideoInfo,
    clips: Vec<Clip>,
    options: ExportOptions,
) -> Result<ExportResult, AppError> {
    // Reset cancellation from any previous export
    crate::cancel::reset_cancel();

    tracing::info!(
        "Starting export: {} clips for '{}' ({})",
        clips.len(),
        video.title,
        video.id
    );

    export::run_export(&app, &video, &clips, &options).await
}

/// Cancel the currently running export.
#[tauri::command]
pub async fn cancel_export() {
    tracing::info!("Cancel requested");
    crate::cancel::cancel_all().await;
}

/// Return the default output directory path as a string for the frontend.
#[tauri::command]
pub fn get_default_output_dir() -> String {
    paths::default_output_directory()
        .to_string_lossy()
        .to_string()
}

/// Open a directory in the system file manager.
#[tauri::command]
pub async fn open_directory(app: AppHandle, path: String) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path, None::<String>)
        .map_err(|e| AppError::Unknown(e.to_string()))
}

/// Perform a health check on bundled sidecars. Non-blocking — called at startup.
/// Emits a "sidecar-status" event to the frontend.
#[tauri::command]
pub async fn check_sidecars(app: AppHandle) -> Result<serde_json::Value, AppError> {
    let ytdlp = binaries::check_version(&app, "yt-dlp", "--version").await;
    let ffmpeg = binaries::check_version(&app, "ffmpeg", "-version").await;
    let ffprobe = binaries::check_version(&app, "ffprobe", "-version").await;

    let status = serde_json::json!({
        "yt_dlp": ytdlp.as_deref().unwrap_or("error").to_string(),
        "ffmpeg": ffmpeg.as_deref().unwrap_or("error").to_string(),
        "ffprobe": ffprobe.as_deref().unwrap_or("error").to_string(),
        "ok": ytdlp.is_ok() && ffmpeg.is_ok() && ffprobe.is_ok(),
    });

    let _ = app.emit("sidecar-status", &status);
    Ok(status)
}

/// Validate that a URL is a recognisable YouTube URL.
fn is_youtube_url(url: &str) -> bool {
    let url = url.trim();
    url.contains("youtube.com/watch")
        || url.contains("youtu.be/")
        || url.contains("youtube.com/shorts/")
        || url.contains("youtube.com/live/")
}
