//! iOS entry point: the normal Limusic state/command path with the native player backend.

use std::sync::Arc;

use tauri::{Manager, WebviewWindow};

use crate::{commands, db::Db, http, shared_app};

#[tauri::mobile_entry_point]
pub fn run() {
    tauri::Builder::default()
        .plugin(ios_player::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir());
            std::fs::create_dir_all(&data_dir)?;
            crate::diagnostics::init_logging(&data_dir);
            tracing::info!("iOS application logging initialized");
            #[cfg(feature = "ios-player-self-test")]
            let self_test = crate::ios_player_self_test::prepare(&data_dir)?;
            let (db, quarantined) = Db::open_or_quarantine(&data_dir.join("limusic.sqlite"))?;
            if let Some(path) = quarantined {
                tracing::warn!(path = %path.display(), "started with a fresh database");
            }
            let db = Arc::new(db);
            let proxy = db
                .get_setting("proxy")
                .filter(|value| !value.trim().is_empty())
                .filter(|value| reqwest::Proxy::all(value).is_ok());
            http::set_proxy(proxy.as_deref());
            let runtime = shared_app::build(app, &data_dir, db, proxy.as_deref())?;
            #[cfg(feature = "ios-player-self-test")]
            crate::ios_player_self_test::spawn(runtime.state.clone(), data_dir.clone(), self_test);
            Ok(())
        })
        .invoke_handler(only_main(tauri::generate_handler![
            commands::search,
            commands::search_all,
            commands::search_suggestions,
            commands::search_cards,
            commands::search_videos,
            commands::song,
            commands::play,
            commands::play_index,
            commands::switch_version,
            commands::remove_from_queue,
            commands::clear_queued,
            commands::add_to_queue,
            commands::move_in_queue,
            commands::play_next,
            commands::next_track,
            commands::prev_track,
            commands::back_to_previous,
            commands::toggle_shuffle,
            commands::set_repeat,
            commands::toggle_pause,
            commands::seek,
            commands::set_volume,
            commands::set_playback_params,
            commands::get_queue,
            commands::get_playback,
            commands::get_settings,
            commands::set_setting,
            commands::get_stream_clients,
            commands::clear_caches,
            commands::set_app_icon,
            commands::app_icon_path,
            commands::get_account,
            commands::get_account_identities,
            commands::switch_account,
            commands::sign_out,
            commands::get_google_accounts,
            commands::switch_google_account,
            commands::remove_google_account,
            commands::show_main,
            commands::take_launch_args,
            commands::get_home,
            commands::get_home_more,
            commands::get_library,
            commands::get_library_albums,
            commands::get_library_artists,
            commands::get_library_subscriptions,
            commands::get_upload_albums,
            commands::get_history,
            commands::get_playlist,
            commands::get_playlist_more,
            commands::get_playlist_suggestions,
            commands::playlist_index,
            commands::sync_playlist_index,
            commands::play_counts,
            commands::get_album,
            commands::get_artist,
            commands::get_artist_appears_on,
            commands::get_browse_grid,
            commands::get_moods,
            commands::get_mood_art,
            commands::play_playlist,
            commands::start_radio,
            commands::rate,
            commands::set_song_saved,
            commands::set_album_saved,
            commands::add_to_playlist,
            commands::remove_from_playlist,
            commands::remove_many_from_playlist,
            commands::create_playlist,
            commands::edit_playlist_details,
            commands::set_playlist_cover,
            commands::set_playlist_sort,
            commands::delete_playlist,
            commands::subscribe,
            commands::get_blocked_artists,
            commands::block_artist,
            commands::unblock_artist,
            commands::get_lyrics,
            commands::choose_lyrics_source,
            commands::set_lyrics_offset,
            commands::lyrics_providers,
            commands::diagnostics,
            commands::diagnostics_summary,
            commands::save_diagnostics,
            commands::log_ui,
            frontend_ready,
        ]))
        .build(tauri::generate_context!())
        .expect("error while building the Limusic iOS application")
        .run(|_, _| {});
}

/// Written only after Svelte's root layout mounts inside the configured `main` WKWebView. The iOS
/// simulator check reads this marker from the app data container, so a live native process with no
/// webview (or a webview that never reached frontend startup) cannot pass CI.
#[tauri::command]
fn frontend_ready(app: tauri::AppHandle, window: WebviewWindow) -> Result<(), String> {
    tracing::info!(window = window.label(), "iOS frontend ready");
    let marker = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("ios-frontend-ready.json");
    let payload = serde_json::json!({
        "status": "ready",
        "window": window.label(),
    });
    std::fs::write(&marker, payload.to_string()).map_err(|error| error.to_string())
}

fn only_main<R: tauri::Runtime>(
    handler: impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static,
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    move |invoke| {
        if invoke.message.webview_ref().label() == "main" {
            return handler(invoke);
        }
        invoke.resolver.reject("not allowed from this webview");
        true
    }
}
