use std::sync::Mutex;

use once_cell::sync::Lazy;
use tauri::{AppHandle, Emitter};

use crate::{
    binaries,
    clip::{Clip, ExportOptions, ExportResult, VideoInfo},
    error::AppError,
    export, paths,
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

    let ytdlp_cache = paths::cache_directory().join("ytdlp");
    let cache_dir_str = ytdlp_cache.to_string_lossy().to_string();

    let mut args = vec![
        "--cache-dir".to_string(),
        cache_dir_str,
        "-4".to_string(),
        "--no-warnings".to_string(),
        "--dump-json".to_string(),
        "--no-download".to_string(),
        "--no-playlist".to_string(),
    ];
    if let Some(ffmpeg_loc) = binaries::ffmpeg_location(&app) {
        args.push("--ffmpeg-location".to_string());
        args.push(ffmpeg_loc.to_string_lossy().to_string());
    }
    if let Some(js_runtime) = binaries::js_runtime_arg() {
        args.push("--js-runtimes".to_string());
        args.push(js_runtime);
    }
    args.push(url.clone());

    let output = binaries::sidecar_command(&app, "yt-dlp")?
        .args(&args)
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

    // Cache info.json so subsequent clipping operations skip YouTube handshake & JS challenges
    let video_cache = paths::video_cache_directory(&id);
    if paths::ensure_dir(&video_cache).is_ok() {
        let info_json_path = video_cache.join("info.json");
        let _ = std::fs::write(&info_json_path, &output.stdout);
    }

    let title = json["title"].as_str().unwrap_or("Untitled").to_string();

    let is_live = json["is_live"].as_bool().unwrap_or(false)
        || json["live_status"]
            .as_str()
            .map(|s| s == "is_live")
            .unwrap_or(false);
    if is_live {
        return Err(AppError::ToolFailure {
            detail: "Live streams cannot be clipped until broadcasting has ended and the archive is processed.".into(),
        });
    }

    let duration_seconds = json["duration"]
        .as_f64()
        .or_else(|| json["duration"].as_i64().map(|d| d as f64))
        .filter(|&d| d > 0.0)
        .ok_or(AppError::VideoUnavailable)?;

    let thumbnail_url = json["thumbnail"].as_str().unwrap_or("").to_string();

    // Pre-warm the cache for videos <= 10 minutes (600s) in the background.
    // While the user reviews metadata and enters timestamps (typically 15-45s),
    // source.mp4 downloads in the background. When they click Export, cutting from
    // the local source file is instantaneous (~0.1s).
    if duration_seconds <= 600.0 {
        let app_handle = app.clone();
        let url_clone = url.clone();
        let id_clone = id.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::download::download_full(
                &app_handle,
                &url_clone,
                &id_clone,
                &crate::clip::QualityOption::Highest,
            )
            .await;
        });
    }

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

/// Open a directory or URL in the default system handler.
#[tauri::command]
pub async fn open_directory(app: AppHandle, path: String) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    if path.starts_with("http://") || path.starts_with("https://") {
        app.opener()
            .open_url(path, None::<String>)
            .map_err(|e| AppError::Unknown(e.to_string()))
    } else {
        app.opener()
            .open_path(path, None::<String>)
            .map_err(|e| AppError::Unknown(e.to_string()))
    }
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
        || url.contains("youtube.com/embed/")
        || url.contains("youtube.com/v/")
        || url.contains("youtube-nocookie.com/embed/")
}
