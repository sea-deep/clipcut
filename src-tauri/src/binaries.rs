use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

use crate::error::AppError;

/// Resolve the path to a bundled sidecar binary.
/// Falls back to system PATH during development if not bundled.
///
/// Tauri names sidecars with the target triple suffix at runtime:
/// e.g. `binaries/yt-dlp-x86_64-unknown-linux-gnu`
/// The plugin handles this automatically when using `sidecar()`.
pub fn sidecar_command(
    app: &AppHandle,
    name: &str,
) -> Result<tauri_plugin_shell::process::Command, AppError> {
    app.shell()
        .sidecar(name)
        .map_err(|e| AppError::ToolFailure {
            detail: format!("Could not locate sidecar '{}': {}", name, e),
        })
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
        // Take just the first line
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
