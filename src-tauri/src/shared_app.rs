//! Platform-neutral application state and player-event wiring.

use std::sync::Arc;

use innertube::{Clients, InnerTube, Locale, Session};
use tauri::{Emitter, Manager};

use crate::cipher::{CipherDeobfuscator, PlayerConfigStore};
use crate::db::Db;
use crate::orchestrator::Orchestrator;
use crate::platform::player::PlayerEvent;
use crate::potoken::PoTokenGenerator;
use crate::state::AppState;

pub(crate) struct CoreRuntime {
    pub(crate) state: Arc<AppState>,
    pub(crate) db: Arc<Db>,
}

pub(crate) fn build(
    app: &tauri::App,
    data_dir: &std::path::Path,
    db: Arc<Db>,
    proxy: Option<&str>,
) -> anyhow::Result<CoreRuntime> {
    let handle = app.handle().clone();
    let cache_dir = data_dir.join("audio-cache");
    std::fs::create_dir_all(&cache_dir).ok();

    let cookie = db.get_setting("session_cookie").filter(|s| !s.is_empty());
    let data_sync_id = crate::state::persisted_data_sync_id(&db);
    let visitor_data = db.get_setting("visitor_data").filter(|s| !s.is_empty());
    let needs_visitor_bootstrap = visitor_data.is_none();
    let visitor_for_prewarm = visitor_data.clone();
    let session = Session { locale: Locale::default(), visitor_data, data_sync_id, cookie };
    let it = InnerTube::new(session, proxy)?;
    if let Some(hl) = db.get_setting("locale") {
        it.set_locale(&hl);
    }
    it.set_hide_videos(db.get_setting("hide_videos").as_deref() == Some("true"));
    it.set_blocked(crate::blocked::block_list(&db));
    let clients = Clients::bundled();

    let mut player = crate::platform::player::create(&handle, &cache_dir)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    if let Err(error) = player.set_http_proxy(proxy) {
        tracing::warn!(%error, "player refused the proxy setting");
    }
    let volume = crate::state::saved_volume(&db);
    // Preserve the desktop startup contract: a backend refusing a persisted volume must not keep
    // the application from opening. The saved value is clamped, so this is diagnostic only.
    if let Err(error) = player.set_volume(volume) {
        tracing::warn!(%error, "player refused the saved volume");
    }
    player.set_crossfade(crate::state::saved_crossfade(&db));
    let events = player
        .take_events()
        .ok_or_else(|| anyhow::anyhow!("the player's event channel was already taken"))?;

    let config = Arc::new(PlayerConfigStore::new(data_dir));
    let cipher = Arc::new(CipherDeobfuscator::new(handle.clone(), data_dir, config));
    let potoken = Arc::new(PoTokenGenerator::new(db.clone()));
    let orchestrator =
        Arc::new(Orchestrator::new(it.clone(), clients.clone(), cipher.clone(), potoken.clone()));
    let media = crate::media::spawn(handle.clone());
    if let Some(media) = &media {
        media.set_volume(volume);
    }
    let discord = crate::discord::spawn(
        db.get_setting("discord_rpc").as_deref() == Some("true"),
        crate::discord::RpcConfig::parse(db.get_setting("discord_rpc_config").as_deref()),
    );
    let lastfm = crate::lastfm::spawn(
        handle.clone(),
        db.get_setting("lastfm_session_key").filter(|s| !s.is_empty()),
        db.get_setting("listenbrainz_token").filter(|s| !s.is_empty()),
        crate::lastfm::ScrobbleConfig::load(&db),
    );
    let lt_url = db.get_setting("lt_server_url").unwrap_or_default();
    let (lt, mut lt_sync_rx) = crate::listentogether::LtSession::new(handle.clone(), lt_url);
    let state = Arc::new(AppState::new(
        it,
        clients,
        player,
        db.clone(),
        handle.clone(),
        orchestrator,
        lt,
        cache_dir,
        media,
        discord,
        lastfm,
    ));
    app.manage(state.clone());

    {
        let state = state.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(command) = lt_sync_rx.recv().await {
                state.apply_sync(command).await;
            }
        });
    }
    {
        let state = state.clone();
        tauri::async_runtime::spawn(async move { state.restore_queue().await });
    }
    if needs_visitor_bootstrap {
        let state = state.clone();
        let potoken = potoken.clone();
        tauri::async_runtime::spawn(async move {
            match state.it.fetch_visitor_data().await {
                Ok(visitor) => {
                    state.it.set_visitor_data(Some(visitor.clone()));
                    state.db.set_setting("visitor_data", &visitor);
                    potoken.prewarm(&visitor).await;
                }
                Err(error) => tracing::warn!(%error, "visitorData bootstrap failed"),
            }
        });
    }
    // Desktop keeps authenticated sessions alive and can rebuild them through its login webview.
    // Mobile deliberately does not inherit that browser integration as part of playback wiring.
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        let state = state.clone();
        let app_handle = handle.clone();
        let rejected = state.it.session_rejected();
        let rotated = state.it.cookie_changed();
        tauri::async_runtime::spawn(async move {
            if state.it.is_logged_in() {
                if let Some(client) = state.clients.get(innertube::METADATA_CLIENT) {
                    let _ = innertube::without_healing(state.it.account_menu(client)).await;
                }
            }
            let mut keepalive = tokio::time::interval(std::time::Duration::from_secs(30 * 60));
            keepalive.tick().await;
            loop {
                tokio::select! {
                    _ = rejected.notified() => {
                        let _heal = state.it.begin_heal();
                        innertube::without_healing(crate::session::refresh_session(
                            app_handle.clone(),
                            state.clone(),
                        )).await;
                    }
                    _ = rotated.notified() => state.persist_rotated_cookie(),
                    _ = keepalive.tick() => state.keep_session_alive().await,
                }
            }
        });
    }
    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
    {
        let prewarm_cipher = cipher.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            prewarm_cipher.prewarm().await;
        });
        if let Some(visitor) = visitor_for_prewarm {
            let potoken = potoken.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
                potoken.prewarm(&visitor).await;
            });
        }
        let potoken = potoken.clone();
        let cipher = cipher.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                potoken.teardown_if_idle(std::time::Duration::from_secs(600)).await;
                cipher.teardown_if_idle(std::time::Duration::from_secs(600)).await;
            }
        });
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let _ = visitor_for_prewarm;

    spawn_event_pump(state.clone(), handle, events);
    Ok(CoreRuntime { state, db })
}

pub(crate) struct PositionThrottle {
    last_emit: std::time::Instant,
    last_pos: f64,
}

impl PositionThrottle {
    pub(crate) fn new() -> Self {
        Self {
            last_emit: std::time::Instant::now() - std::time::Duration::from_secs(1),
            last_pos: f64::NAN,
        }
    }

    pub(crate) fn should_emit(&mut self, pos: f64, now: std::time::Instant) -> bool {
        let elapsed = now.duration_since(self.last_emit);
        let jumped =
            self.last_pos.is_nan() || (pos - self.last_pos).abs() > elapsed.as_secs_f64() + 0.75;
        if jumped || elapsed >= std::time::Duration::from_millis(250) {
            self.last_emit = now;
            self.last_pos = pos;
            true
        } else {
            false
        }
    }
}

pub(crate) fn spawn_event_pump(
    state: Arc<AppState>,
    app: tauri::AppHandle,
    mut events: tokio::sync::mpsc::UnboundedReceiver<PlayerEvent>,
) {
    tauri::async_runtime::spawn(async move {
        let mut throttle = PositionThrottle::new();
        while let Some(event) = events.recv().await {
            #[cfg(all(target_os = "ios", feature = "ios-player-self-test"))]
            crate::ios_player_self_test::observe_event(&event);
            match event {
                PlayerEvent::Position(position) => {
                    if throttle.should_emit(position, std::time::Instant::now()) {
                        let _ = app.emit("position", serde_json::json!({ "position": position }));
                    }
                    state.on_position(position).await;
                }
                PlayerEvent::Duration(duration) => {
                    let _ = app.emit("duration", serde_json::json!({ "duration": duration }));
                    state.on_duration(duration).await;
                }
                PlayerEvent::Playing(playing) => {
                    let _ = app.emit("playback-state", if playing { "playing" } else { "paused" });
                    if !playing {
                        state.flush_position();
                        let _ = app.emit(
                            "position",
                            serde_json::json!({ "position": state.current_position() }),
                        );
                    }
                    state.media_set_playing(playing);
                    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
                    crate::tray::set_playing(&app, playing);
                }
                PlayerEvent::Paused(paused) => state.lt_on_play_state(!paused).await,
                PlayerEvent::TrackEnded => state.on_track_ended().await,
                PlayerEvent::TrackFailed(message) => {
                    let client = state.current_stream_client().await;
                    tracing::warn!(error = %message, client = ?client, "track failed");
                    if !state.on_track_failed().await {
                        let message =
                            client.map_or(message.clone(), |c| format!("{message} ({c})"));
                        let _ =
                            app.emit("playback-error", serde_json::json!({ "message": message }));
                    }
                }
                PlayerEvent::AudioDeviceLost => {
                    tracing::warn!("audio device unavailable, holding the queue where it is");
                    state.on_audio_device_lost().await;
                    let _ = app.emit(
                        "playback-error",
                        serde_json::json!({
                            "message": "Your audio device is unavailable. Press play once it's back."
                        }),
                    );
                }
                PlayerEvent::LookaheadFailed(message) => {
                    tracing::warn!(error = %message, "lookahead preload failed");
                    state.on_lookahead_failed().await;
                }
                PlayerEvent::VideoFailed(audio) => state.on_video_failed(&audio).await,
                PlayerEvent::Error(message) => {
                    tracing::error!(error = %message, "player error");
                    let _ = app.emit("playback-error", serde_json::json!({ "message": message }));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::PositionThrottle;
    use std::time::{Duration, Instant};

    #[test]
    fn position_throttle_passes_first_tick_cadence_and_jumps() {
        let mut throttle = PositionThrottle::new();
        let start = Instant::now();
        assert!(throttle.should_emit(0.0, start));
        assert!(!throttle.should_emit(0.1, start + Duration::from_millis(100)));
        assert!(throttle.should_emit(0.25, start + Duration::from_millis(250)));
        assert!(throttle.should_emit(30.0, start + Duration::from_millis(300)));
    }
}
