use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::ShellExt;

use crate::error::AppError;

/// Returns the directory where sidecars are located on disk.
pub fn sidecar_directory(app: &AppHandle) -> Option<PathBuf> {
    // 1. Next to the running executable (macOS .app/Contents/MacOS, Windows install dir, Linux AppImage/bin)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if parent.join("ffmpeg").exists()
                || parent.join("ffmpeg.exe").exists()
                || parent.join("yt-dlp").exists()
                || parent.join("yt-dlp.exe").exists()
            {
                return Some(parent.to_path_buf());
            }
        }
    }

    // 2. Tauri resource directory
    if let Ok(res_dir) = app.path().resource_dir() {
        if res_dir.join("ffmpeg").exists() || res_dir.join("ffmpeg.exe").exists() {
            return Some(res_dir);
        }
        let res_bin = res_dir.join("binaries");
        if res_bin.join("ffmpeg").exists() || res_bin.join("ffmpeg.exe").exists() {
            return Some(res_bin);
        }
    }

    // 3. Workspace development directories
    for dev_path in ["src-tauri/binaries", "binaries", "../src-tauri/binaries"] {
        let p = Path::new(dev_path);
        if p.is_dir() {
            if let Ok(canon) = p.canonicalize() {
                return Some(canon);
            }
        }
    }

    // 4. Fallback to exe parent directory
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return Some(parent.to_path_buf());
        }
    }

    None
}

/// Returns the location of the ffmpeg binary or containing directory.
pub fn ffmpeg_location(app: &AppHandle) -> Option<PathBuf> {
    if let Some(dir) = sidecar_directory(app) {
        if dir.join("ffmpeg").exists() || dir.join("ffmpeg.exe").exists() {
            return Some(dir);
        }
        // In development, binaries may have a target triple suffix
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("ffmpeg")
                    && !name.ends_with(".part")
                    && !name.ends_with(".ytdl")
                {
                    return Some(dir);
                }
            }
        }
    }

    // Fallback to system locations
    for sys_dir in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        let p = Path::new(sys_dir);
        if p.join("ffmpeg").exists() || p.join("ffmpeg.exe").exists() {
            return Some(p.to_path_buf());
        }
    }

    None
}

/// Detect available JavaScript runtime for yt-dlp EJS challenges.
/// Searches for node, deno, or bun.
pub fn js_runtime_arg() -> Option<String> {
    for candidate in [
        "/opt/homebrew/bin/node",
        "/usr/local/bin/node",
        "/usr/bin/node",
    ] {
        if Path::new(candidate).exists() {
            return Some(format!("node:{}", candidate));
        }
    }

    for candidate in [
        "/opt/homebrew/bin/deno",
        "/usr/local/bin/deno",
        "/usr/bin/deno",
    ] {
        if Path::new(candidate).exists() {
            return Some(format!("deno:{}", candidate));
        }
    }

    if std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok()
    {
        return Some("node".to_string());
    }
    if std::process::Command::new("deno")
        .arg("--version")
        .output()
        .is_ok()
    {
        return Some("deno".to_string());
    }
    if std::process::Command::new("bun")
        .arg("--version")
        .output()
        .is_ok()
    {
        return Some("bun".to_string());
    }

    None
}

/// Resolve the path to a bundled sidecar binary.
/// Sets up PATH to ensure sibling sidecars (such as ffmpeg from yt-dlp) are discoverable.
pub fn sidecar_command(
    app: &AppHandle,
    name: &str,
) -> Result<tauri_plugin_shell::process::Command, AppError> {
    let mut cmd = app
        .shell()
        .sidecar(name)
        .unwrap_or_else(|_| app.shell().command(name));

    let mut path_components = Vec::new();
    if let Some(dir) = sidecar_directory(app) {
        path_components.push(dir.to_string_lossy().to_string());
    }
    if let Ok(current_path) = std::env::var("PATH") {
        path_components.push(current_path);
    }
    #[cfg(unix)]
    {
        path_components.push("/opt/homebrew/bin".to_string());
        path_components.push("/usr/local/bin".to_string());
        path_components.push("/usr/bin".to_string());
        path_components.push("/bin".to_string());
    }

    let separator = if cfg!(windows) { ";" } else { ":" };
    let new_path = path_components.join(separator);
    cmd = cmd.env("PATH", new_path);

    Ok(cmd)
}

/// Quick health check — runs `<binary> --version` and returns the version string.
/// Non-blocking; called from setup() in a background task.
pub async fn check_version(app: &AppHandle, name: &str, flag: &str) -> Result<String, AppError> {
    let output = sidecar_command(app, name)?
        .args([flag])
        .output()
        .await
        .map_err(|e| AppError::ToolFailure {
            detail: format!("{} version check failed: {}", name, e),
        })?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().next().unwrap_or("unknown").to_string())
    } else {
        Err(AppError::ToolFailure {
            detail: format!(
                "{} exited non-zero: {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            ),
        })
    }
}
