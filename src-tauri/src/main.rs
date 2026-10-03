// ClipCut — Tauri application entry point

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    clipcut_lib::run();
}
