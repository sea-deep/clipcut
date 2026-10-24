pub mod binaries;
pub mod cancel;
pub mod clip;
pub mod commands;
pub mod cookies;
pub mod download;
pub mod error;
pub mod export;
pub mod ffmpeg;
pub mod paths;
pub mod progress;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::default()
                .target(tauri_plugin_log::Target::new(
                    tauri_plugin_log::TargetKind::Stdout,
                ))
                .level(log::LevelFilter::Info)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::load_video,
            commands::export_clips,
            commands::cancel_export,
            commands::get_default_output_dir,
            commands::open_directory,
            commands::check_sidecars,
            commands::get_auth_state,
            commands::set_cookie_source,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            // Non-blocking sidecar health check on launch
            tauri::async_runtime::spawn(async move {
                let _ = commands::check_sidecars(handle).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
