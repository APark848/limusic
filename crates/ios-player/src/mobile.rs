use std::ffi::{c_char, CStr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::plugin::{PluginApi, PluginHandle};
use tauri::{Manager, Runtime};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

tauri::ios_plugin_binding!(init_plugin_limusic_player);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("iOS player bridge: {0}")]
    Plugin(#[from] tauri::plugin::PluginInvokeError),
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

static EVENT_TX: OnceLock<Mutex<Option<UnboundedSender<PlayerEvent>>>> = OnceLock::new();
static IDLE: AtomicBool = AtomicBool::new(true);
static PLAYING: AtomicBool = AtomicBool::new(false);

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
pub unsafe extern "C" fn limusic_ios_player_emit(kind: u32, value: f64, message: *const c_char) {
    let message = if message.is_null() {
        String::new()
    } else {
        CStr::from_ptr(message).to_string_lossy().into_owned()
    };

    let event = match kind {
        0 => PlayerEvent::Position(value),
        1 => PlayerEvent::Duration(value),
        2 => {
            let playing = value != 0.0;
            PLAYING.store(playing, Ordering::SeqCst);
            PlayerEvent::Playing(playing)
        }
        3 => PlayerEvent::Paused(value != 0.0),
        4 => {
            PLAYING.store(false, Ordering::SeqCst);
            IDLE.store(true, Ordering::SeqCst);
            PlayerEvent::TrackEnded
        }
        5 => {
            PLAYING.store(false, Ordering::SeqCst);
            IDLE.store(true, Ordering::SeqCst);
            PlayerEvent::TrackFailed(message)
        }
        6 => PlayerEvent::Error(message),
        _ => PlayerEvent::Error(format!("unknown iOS player event {kind}")),
    };
    send_event(event);
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
    url: &'a str,
    headers: &'a std::collections::HashMap<String, String>,
    start: Option<f64>,
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
    ) -> Result<()> {
        if url.is_empty() {
            return Err(Error::Invalid("stream URL is empty"));
        }
        if start.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(Error::Invalid("start position must be finite and non-negative"));
        }
        if let Some(name) = headers.keys().find(|name| !crate::supported_stream_header(name)) {
            return Err(Error::UnsupportedHeader(name.clone()));
        }
        self.0.run_mobile_plugin("load", LoadArgs { url, headers, start })?;
        IDLE.store(false, Ordering::SeqCst);
        PLAYING.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn play(&self) -> Result<()> {
        self.0.run_mobile_plugin("play", ())?;
        PLAYING.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn pause(&self) -> Result<()> {
        self.0.run_mobile_plugin("pause", ())?;
        PLAYING.store(false, Ordering::SeqCst);
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        self.0.run_mobile_plugin("stop", ())?;
        PLAYING.store(false, Ordering::SeqCst);
        IDLE.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn seek(&self, position: f64) -> Result<()> {
        if !position.is_finite() || position < 0.0 {
            return Err(Error::Invalid("seek position must be finite and non-negative"));
        }
        self.0.run_mobile_plugin("seek", SeekArgs { position }).map_err(Into::into)
    }

    pub fn set_volume(&self, volume: i64) -> Result<()> {
        if !(0..=100).contains(&volume) {
            return Err(Error::Invalid("volume must be between 0 and 100"));
        }
        self.0.run_mobile_plugin("setVolume", VolumeArgs { volume }).map_err(Into::into)
    }

    pub fn set_loop(&self, enabled: bool) -> Result<()> {
        self.0.run_mobile_plugin("setLoop", LoopArgs { enabled }).map_err(Into::into)
    }

    pub fn is_idle(&self) -> bool {
        IDLE.load(Ordering::SeqCst)
    }

    pub fn is_playing(&self) -> bool {
        PLAYING.load(Ordering::SeqCst)
    }
}
