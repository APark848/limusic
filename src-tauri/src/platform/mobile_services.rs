//! Explicit unavailable implementations for desktop integrations on mobile.
//!
//! These are deliberately inert. They preserve the shared state graph until native iOS audio and
//! media integrations are introduced, without pretending that those integrations exist.

pub(crate) mod media {
    use tauri::AppHandle;

    pub(crate) struct MediaHandle;

    impl MediaHandle {
        pub(crate) fn set_metadata(
            &self,
            _title: &str,
            _artists: &str,
            _album: Option<&str>,
            _cover: Option<&str>,
        ) {
        }
        pub(crate) fn set_album(&self, _album: &str) {}
        pub(crate) fn set_duration(&self, _secs: f64) {}
        pub(crate) fn set_playback(&self, _playing: bool, _position: f64) {}
        pub(crate) fn set_volume(&self, _volume: i64) {}
    }

    pub(crate) fn spawn(_app: AppHandle) -> Option<MediaHandle> {
        None
    }
    pub(crate) fn cover_url(path: &str) -> String {
        path.to_owned()
    }
}

pub(crate) mod discord {
    use innertube::SongItem;

    #[derive(Default)]
    pub(crate) struct RpcConfig;

    impl RpcConfig {
        pub(crate) fn parse(_json: Option<&str>) -> Self {
            Self
        }
    }

    pub(crate) struct DiscordHandle;

    impl DiscordHandle {
        pub(crate) fn set_track(&self, _item: &SongItem) {}
        pub(crate) fn set_album(&self, _video_id: &str, _album: &str) {}
        pub(crate) fn set_duration(&self, _secs: f64) {}
        pub(crate) fn set_position(&self, _pos: f64) {}
        pub(crate) fn set_playing(&self, _playing: bool) {}
        pub(crate) fn set_enabled(&self, _on: bool) {}
        pub(crate) fn set_config(&self, _config: RpcConfig) {}
    }

    pub(crate) fn spawn(_enabled: bool, _config: RpcConfig) -> Option<DiscordHandle> {
        None
    }
}

pub(crate) mod notify {
    pub(crate) fn track_changed(
        _app: &tauri::AppHandle,
        _title: &str,
        _artists: &str,
        _even_focused: bool,
    ) {
    }
}
