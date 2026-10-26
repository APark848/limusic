//! Platform entry points for Limusic.
//!
//! Desktop and iOS share the application state, playback commands, and player-event wiring while
//! selecting their native player and platform integrations at the entry-point boundary.

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
mod appicon;
mod audioproxy;
mod blocked;
mod cipher;
mod commands;
mod db;
mod diagnostics;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
mod discord;
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
use platform::mobile_services::discord;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
mod hotkeys;
mod http;
mod import;
#[cfg(target_os = "linux")]
mod inhibit;
mod lastfm;
mod listenbrainz;
mod listentogether;
mod local;
mod lyrics;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux", target_os = "ios"))]
mod media;
#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux",
    target_os = "ios"
)))]
use platform::mobile_services::media;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
mod mini;
#[cfg(target_os = "linux")]
mod nativevideo;
#[cfg(windows)]
#[path = "nativevideo_windows.rs"]
mod nativevideo;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
mod notify;
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
use platform::mobile_services::notify;
mod orchestrator;
mod platform;
mod potoken;
mod romanize;
mod session;
mod shared_app;
mod spotify;
mod state;
#[cfg(target_os = "windows")]
mod taskbar;
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
mod tray;
mod videoproxy;
mod webview;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
include!("desktop_app.rs");

#[cfg(all(target_os = "ios", feature = "ios-player-self-test"))]
mod ios_player_self_test;
#[cfg(target_os = "ios")]
mod mobile_app;

#[cfg(target_os = "ios")]
pub use mobile_app::run;
