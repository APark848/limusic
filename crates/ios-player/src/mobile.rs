use std::ffi::{c_char, CStr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::plugin::{PluginApi, PluginHandle};
use tauri::{Manager, Runtime};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

tauri::ios_plugin_binding!(init_plugin_limusic_player);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("iOS player bridge: {0}")]
    Plugin(#[from] tauri::plugin::mobile::PluginInvokeError),
    #[error("{0} is not supported by the iOS player")]
    Unsupported(&'static str),
    #[error("the required HTTP header '{0}' is not supported by the iOS player")]
    UnsupportedHeader(String),
    #[error("invalid playback value: {0}")]
    Invalid(&'static str),
    #[error("the iOS player plugin is not registered")]
    NotRegistered,
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerEvent {
    Scoped { epoch: u64, event: Box<PlayerEvent> },
    Position(f64),
    Duration(f64),
    Playing(bool),
    Buffering,
    Paused(bool),
    TrackEnded,
    TrackFailed(String),
    AudioDeviceLost,
    LookaheadFailed(String),
    VideoFailed(String),
    Error(String),
}

static EVENT_TX: OnceLock<Mutex<Option<UnboundedSender<PlayerEvent>>>> = OnceLock::new();
static IDLE: AtomicBool = AtomicBool::new(true);
static PLAYING: AtomicBool = AtomicBool::new(false);
static WANTS_PLAYBACK: AtomicBool = AtomicBool::new(false);
static REPORTED_PROGRESS: AtomicBool = AtomicBool::new(false);
static TRACE: Mutex<Option<(u64, std::time::Instant)>> = Mutex::new(None);
static EVENT_EPOCH: AtomicU64 = AtomicU64::new(0);

pub fn is_current_event(epoch: u64) -> bool {
    EVENT_EPOCH.load(Ordering::SeqCst) == epoch
}

fn event_sink() -> &'static Mutex<Option<UnboundedSender<PlayerEvent>>> {
    EVENT_TX.get_or_init(|| Mutex::new(None))
}

fn send_event(event: PlayerEvent) {
    if let Ok(guard) = event_sink().lock() {
        if let Some(tx) = guard.as_ref() {
            let _ = tx.send(event);
        }
    }
}

/// Event callback called by the Swift AVPlayer observer.
///
/// # Safety
///
/// `message`, when non-null, must point to a NUL-terminated UTF-8 string that remains valid for
/// the duration of this call. The Swift bridge satisfies that contract with `withCString`.
#[no_mangle]
pub unsafe extern "C" fn limusic_ios_player_emit(
    kind: u32,
    value: f64,
    message: *const c_char,
    epoch: u64,
) {
    if !is_current_event(epoch) {
        return;
    }
    let message = if message.is_null() {
        String::new()
    } else {
        CStr::from_ptr(message).to_string_lossy().into_owned()
    };

    if kind == 7 {
        let trace = *TRACE.lock().unwrap();
        tracing::info!(stage = %message, elapsed_ms = value,
            generation = trace.map(|t| t.0),
            startup_ms = trace.map(|t| t.1.elapsed().as_millis() as u64), "iOS AVPlayer stage");
        return;
    }

    let event = match kind {
        0 => {
            if value > 0.0 && !REPORTED_PROGRESS.swap(true, Ordering::SeqCst) {
                tracing::info!(position = value, "iOS AVPlayer reported first playback progress");
            }
            PlayerEvent::Position(value)
        }
        1 => {
            tracing::info!(duration = value, "iOS AVPlayer item ready with duration");
            PlayerEvent::Duration(value)
        }
        2 => {
            let playing = value != 0.0;
            tracing::info!(playing, "iOS AVPlayer time-control state changed");
            PLAYING.store(playing, Ordering::SeqCst);
            PlayerEvent::Playing(playing)
        }
        3 => {
            WANTS_PLAYBACK.store(value == 0.0, Ordering::SeqCst);
            PlayerEvent::Paused(value != 0.0)
        }
        8 => {
            PLAYING.store(false, Ordering::SeqCst);
            PlayerEvent::Buffering
        }
        4 => {
            PLAYING.store(false, Ordering::SeqCst);
            IDLE.store(true, Ordering::SeqCst);
            PlayerEvent::TrackEnded
        }
        5 => {
            tracing::warn!(category = "item-failed", error = %message, "iOS AVPlayer track failed");
            PLAYING.store(false, Ordering::SeqCst);
            IDLE.store(true, Ordering::SeqCst);
            PlayerEvent::TrackFailed(message)
        }
        6 => {
            tracing::error!(category = "player", error = %message, "iOS AVPlayer error");
            PlayerEvent::Error(message)
        }
        _ => PlayerEvent::Error(format!("unknown iOS player event {kind}")),
    };
    send_event(PlayerEvent::Scoped { epoch, event: Box::new(event) });
}

pub struct IosPlayer<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Clone for IosPlayer<R> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

pub(crate) fn init<R: Runtime, C: serde::de::DeserializeOwned>(
    api: PluginApi<R, C>,
) -> Result<IosPlayer<R>> {
    let handle = api.register_ios_plugin(init_plugin_limusic_player)?;
    Ok(IosPlayer(handle))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LoadArgs<'a> {
    epoch: u64,
    url: &'a str,
    headers: &'a std::collections::HashMap<String, String>,
    start: Option<f64>,
    expected_duration: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareArgs<'a> {
    url: &'a str,
    headers: &'a std::collections::HashMap<String, String>,
    expected_duration: Option<f64>,
}

#[derive(Serialize)]
struct SeekArgs {
    position: f64,
}

#[derive(Serialize)]
struct VolumeArgs {
    volume: i64,
}

#[derive(Serialize)]
struct LoopArgs {
    enabled: bool,
}

impl<R: Runtime> IosPlayer<R> {
    #[cfg(debug_assertions)]
    pub fn snapshot(&self) -> Result<serde_json::Value> {
        self.0.run_mobile_plugin("snapshot", ()).map_err(Into::into)
    }
    pub fn invalidate_events(&self) {
        EVENT_EPOCH.fetch_add(1, Ordering::SeqCst);
    }
    pub fn begin_trace(&self, generation: u64, started: std::time::Instant) {
        *TRACE.lock().unwrap() = Some((generation, started));
        tracing::info!(generation, "native playback trace assigned");
    }
    pub fn from_manager(manager: &impl Manager<R>) -> Result<Self> {
        manager.try_state::<Self>().map(|state| state.inner().clone()).ok_or(Error::NotRegistered)
    }

    pub fn take_events(&self) -> Option<UnboundedReceiver<PlayerEvent>> {
        let (tx, rx) = unbounded_channel();
        let Ok(mut guard) = event_sink().lock() else {
            return None;
        };
        if guard.is_some() {
            return None;
        }
        *guard = Some(tx);
        Some(rx)
    }

    pub fn load(
        &self,
        url: &str,
        headers: &std::collections::HashMap<String, String>,
        start: Option<f64>,
        expected_duration: Option<f64>,
    ) -> Result<()> {
        if url.is_empty() {
            return Err(Error::Invalid("stream URL is empty"));
        }
        if start.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(Error::Invalid("start position must be finite and non-negative"));
        }
        if expected_duration.is_some_and(|value| !value.is_finite() || value <= 0.0) {
            return Err(Error::Invalid("expected duration must be finite and positive"));
        }
        if let Some(name) = headers.keys().find(|name| !crate::supported_stream_header(name)) {
            tracing::warn!(header = %name, "iOS player rejected unsupported stream header");
            return Err(Error::UnsupportedHeader(name.clone()));
        }
        let mut header_names: Vec<&str> = headers.keys().map(String::as_str).collect();
        header_names.sort_unstable();
        tracing::info!(headers = ?header_names, has_start = start.is_some(), "calling Swift player load");
        let epoch = EVENT_EPOCH.fetch_add(1, Ordering::SeqCst) + 1;
        self.0.run_mobile_plugin::<()>(
            "load",
            LoadArgs { epoch, url, headers, start, expected_duration },
        )?;
        tracing::info!("Swift player load returned successfully");
        IDLE.store(false, Ordering::SeqCst);
        PLAYING.store(false, Ordering::SeqCst);
        REPORTED_PROGRESS.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Ask AVFoundation to resolve the immediate successor's asset metadata while the current
    /// track is still playing. This deliberately does not create a second player or queue.
    pub fn prepare(
        &self,
        url: &str,
        headers: &std::collections::HashMap<String, String>,
        expected_duration: Option<f64>,
    ) -> Result<()> {
        if url.is_empty() {
            return Err(Error::Invalid("stream URL is empty"));
        }
        if expected_duration.is_some_and(|value| !value.is_finite() || value <= 0.0) {
            return Err(Error::Invalid("expected duration must be finite and positive"));
        }
        if let Some(name) = headers.keys().find(|name| !crate::supported_stream_header(name)) {
            tracing::warn!(header = %name, "iOS player rejected unsupported prepared-stream header");
            return Err(Error::UnsupportedHeader(name.clone()));
        }
        tracing::info!(header_count = headers.len(), "calling Swift player prepare");
        self.0
            .run_mobile_plugin::<()>("prepare", PrepareArgs { url, headers, expected_duration })?;
        tracing::info!("Swift player prepare returned successfully");
        Ok(())
    }

    pub fn play(&self) -> Result<()> {
        tracing::info!("calling Swift player play");
        self.0.run_mobile_plugin::<()>("play", ())?;
        WANTS_PLAYBACK.store(true, Ordering::SeqCst);
        tracing::info!("Swift player play returned successfully; awaiting AVPlayer state callback");
        Ok(())
    }

    pub fn pause(&self) -> Result<()> {
        self.0.run_mobile_plugin::<()>("pause", ())?;
        WANTS_PLAYBACK.store(false, Ordering::SeqCst);
        PLAYING.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        self.0.run_mobile_plugin::<()>("stop", ())?;
        WANTS_PLAYBACK.store(false, Ordering::SeqCst);
        PLAYING.store(false, Ordering::SeqCst);
        IDLE.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn seek(&self, position: f64) -> Result<()> {
        if !position.is_finite() || position < 0.0 {
            return Err(Error::Invalid("seek position must be finite and non-negative"));
        }
        self.0.run_mobile_plugin::<()>("seek", SeekArgs { position }).map_err(Into::into)
    }

    pub fn set_volume(&self, volume: i64) -> Result<()> {
        if !(0..=100).contains(&volume) {
            return Err(Error::Invalid("volume must be between 0 and 100"));
        }
        self.0.run_mobile_plugin::<()>("setVolume", VolumeArgs { volume }).map_err(Into::into)
    }

    pub fn set_loop(&self, enabled: bool) -> Result<()> {
        self.0.run_mobile_plugin::<()>("setLoop", LoopArgs { enabled }).map_err(Into::into)
    }

    pub fn is_idle(&self) -> bool {
        IDLE.load(Ordering::SeqCst)
    }

    pub fn is_playing(&self) -> bool {
        PLAYING.load(Ordering::SeqCst)
    }

    pub fn wants_playback(&self) -> bool {
        WANTS_PLAYBACK.load(Ordering::SeqCst)
    }
}
