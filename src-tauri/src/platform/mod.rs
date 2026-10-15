//! Compile-time platform classification for application services.
//!
//! Keep this deliberately small: the desktop implementation remains the current application,
//! while future mobile work can select native services without treating every non-Linux target as
//! a desktop target.

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub(crate) mod desktop;

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub(crate) mod player {
    pub(crate) use ::player::{Player, PlayerEvent};
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub(crate) mod mobile_services;

#[cfg(target_os = "ios")]
#[path = "ios_player.rs"]
pub(crate) mod player;

#[cfg(not(any(
    target_os = "windows",
    target_os = "macos",
    target_os = "linux",
    target_os = "ios"
)))]
pub(crate) mod player {
    use std::collections::HashMap;

    #[derive(Debug, thiserror::Error)]
    #[error("playback is not implemented on this platform")]
    pub(crate) struct Error;

    pub(crate) struct Player;

    pub(crate) enum PlayerEvent {
        Position(f64),
        Duration(f64),
        Playing(bool),
        Paused(bool),
        TrackEnded,
        TrackFailed(String),
        AudioDeviceLost,
        LookaheadFailed(String),
        VideoFailed(String),
        Error(String),
    }

    impl Player {
        pub(crate) fn new(_cache_dir: &str) -> Result<Self, Error> {
            Err(Error)
        }
        pub(crate) fn take_events(
            &mut self,
        ) -> Option<tokio::sync::mpsc::UnboundedReceiver<PlayerEvent>> {
            None
        }
        pub(crate) fn current_path(&self) -> Option<String> {
            None
        }
        pub(crate) fn is_idle(&self) -> bool {
            true
        }
        pub(crate) fn set_crossfade(&self, _secs: Option<f64>) {}
        pub(crate) fn set_http_proxy(&self, _proxy: Option<&str>) -> Result<(), Error> {
            Err(Error)
        }
        pub(crate) fn set_volume(&self, _volume: i64) -> Result<(), Error> {
            Err(Error)
        }
        pub(crate) fn set_gain(&self, _gain: Option<f64>) -> Result<(), Error> {
            Err(Error)
        }
        pub(crate) fn set_video_for(&self, _audio: &str, _video: &str) {}
        pub(crate) fn clear_playlist(&self) -> Result<(), Error> {
            Err(Error)
        }
        pub(crate) fn stop(&self) -> Result<(), Error> {
            Err(Error)
        }
        pub(crate) fn play(&self) -> Result<(), Error> {
            Err(Error)
        }
        pub(crate) fn pause(&self) -> Result<(), Error> {
            Err(Error)
        }
        pub(crate) fn toggle(&self) -> Result<(), Error> {
            Err(Error)
        }
        pub(crate) fn set_loop_file(&self, _on: bool) -> Result<(), Error> {
            Err(Error)
        }
        pub(crate) fn seek(&self, _position: f64) -> Result<(), Error> {
            Err(Error)
        }
        pub(crate) fn enqueue(&self, _url: &str) -> Result<(), Error> {
            Err(Error)
        }
        pub(crate) fn load(
            &self,
            _url: &str,
            _headers: &HashMap<String, String>,
            _gain: Option<f64>,
            _start: Option<f64>,
        ) -> Result<(), Error> {
            Err(Error)
        }
    }
}
