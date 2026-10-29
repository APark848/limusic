//! iOS implementation of the existing application-facing player surface.
//!
//! Only the transport features AVPlayer owns are implemented here. mpv-specific lookahead,
//! filters, proxy configuration, and video attachment fail explicitly so a future caller cannot
//! mistake an ignored command for a working feature.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;

pub(crate) use ios_player::PlayerEvent;

pub(crate) type Error = ios_player::Error;

pub(crate) fn create(
    app: &tauri::AppHandle,
    _cache_dir: &std::path::Path,
) -> Result<Player, Error> {
    Player::new(app)
}

pub(crate) struct Player {
    native: ios_player::IosPlayer<tauri::Wry>,
    events: Option<tokio::sync::mpsc::UnboundedReceiver<PlayerEvent>>,
    current_path: Mutex<Option<String>>,
    volume: AtomicI64,
}

impl Player {
    pub(crate) fn new(app: &tauri::AppHandle) -> Result<Self, Error> {
        let native = ios_player::IosPlayer::from_manager(app)?;
        let events = native.take_events();
        Ok(Self { native, events, current_path: Mutex::new(None), volume: AtomicI64::new(100) })
    }

    pub(crate) fn take_events(
        &mut self,
    ) -> Option<tokio::sync::mpsc::UnboundedReceiver<PlayerEvent>> {
        self.events.take()
    }

    pub(crate) fn current_path(&self) -> Option<String> {
        self.current_path.lock().ok().and_then(|path| path.clone())
    }

    pub(crate) fn is_idle(&self) -> bool {
        self.native.is_idle()
    }

    pub(crate) fn set_crossfade(&self, _secs: Option<f64>) {
        tracing::warn!("crossfade is not supported by the iOS playback MVP");
    }

    pub(crate) fn set_http_proxy(&self, proxy: Option<&str>) -> Result<(), Error> {
        if proxy.is_some() {
            Err(Error::Unsupported("HTTP proxy configuration"))
        } else {
            Ok(())
        }
    }

    pub(crate) fn set_volume(&self, volume: i64) -> Result<(), Error> {
        self.native.set_volume(volume)?;
        self.volume.store(volume, Ordering::Relaxed);
        Ok(())
    }

    /// Preserve startup state without invoking the mobile plugin from Tauri's setup callback.
    pub(crate) fn remember_volume(&self, volume: i64) {
        self.volume.store(volume, Ordering::Relaxed);
    }

    pub(crate) fn volume(&self) -> i64 {
        self.volume.load(Ordering::Relaxed)
    }

    pub(crate) fn set_gain(&self, gain: Option<f64>) -> Result<(), Error> {
        if gain.is_some_and(|gain| gain.abs() > f64::EPSILON) {
            Err(Error::Unsupported("loudness gain"))
        } else {
            Ok(())
        }
    }

    pub(crate) fn set_speed(&self, speed: f64) -> Result<(), Error> {
        if (speed - 1.0).abs() <= f64::EPSILON {
            Ok(())
        } else {
            Err(Error::Unsupported("playback speed"))
        }
    }

    pub(crate) fn set_pitch(&self, semitones: i32) -> Result<(), Error> {
        if semitones == 0 {
            Ok(())
        } else {
            Err(Error::Unsupported("pitch shifting"))
        }
    }

    pub(crate) fn set_video_for(&self, _audio: &str, _video: &str) {
        tracing::warn!("music-video attachment is not supported by the iOS playback MVP");
    }

    pub(crate) fn clear_playlist(&self) -> Result<(), Error> {
        Err(Error::Unsupported("gapless lookahead playlist"))
    }

    pub(crate) fn stop(&self) -> Result<(), Error> {
        self.native.stop()?;
        if let Ok(mut path) = self.current_path.lock() {
            *path = None;
        }
        Ok(())
    }

    pub(crate) fn play(&self) -> Result<(), Error> {
        tracing::info!("iOS Player::play entered");
        let result = self.native.play();
        if let Err(error) = &result {
            tracing::warn!(%error, "iOS Player::play failed");
        }
        result
    }

    pub(crate) fn pause(&self) -> Result<(), Error> {
        tracing::info!("iOS Player::pause entered");
        self.native.pause()
    }

    pub(crate) fn toggle(&self) -> Result<(), Error> {
        if self.native.is_playing() {
            self.pause()
        } else {
            self.play()
        }
    }

    pub(crate) fn set_loop_file(&self, on: bool) -> Result<(), Error> {
        self.native.set_loop(on)
    }

    pub(crate) fn seek(&self, position: f64) -> Result<(), Error> {
        tracing::info!(position, "iOS Player::seek entered");
        self.native.seek(position)
    }

    pub(crate) fn enqueue(&self, _url: &str) -> Result<(), Error> {
        Err(Error::Unsupported("gapless lookahead enqueue"))
    }

    pub(crate) fn load(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        gain: Option<f64>,
        start: Option<f64>,
        expected_duration: Option<f64>,
    ) -> Result<(), Error> {
        tracing::info!(
            header_count = headers.len(),
            has_gain = gain.is_some(),
            has_start = start.is_some(),
            "iOS Player::load entered"
        );
        if gain.is_some_and(|gain| gain.abs() > f64::EPSILON) {
            return Err(Error::Unsupported("loudness gain"));
        }
        self.native.load(url, headers, start, expected_duration)?;
        self.native.set_volume(self.volume())?;
        if let Ok(mut path) = self.current_path.lock() {
            *path = Some(url.to_owned());
        }
        tracing::info!("iOS Player::load completed");
        Ok(())
    }
}
