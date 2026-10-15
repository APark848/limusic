//! Platform entry points for Limusic.
//!
//! The complete existing application remains the desktop implementation. Mobile gets a minimal
//! Tauri shell until its shared services and native audio backend are extracted in later work.

mod platform;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
include!("desktop_app.rs");

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
#[tauri::mobile_entry_point]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Limusic mobile shell");
}
