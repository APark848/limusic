//! Native iOS playback bridge for Limusic.
//!
//! Rust sends one-shot commands through Tauri's supported mobile plugin API. AVFoundation sends
//! long-lived playback events back through the iOS FFI hook documented by Tauri, because native
//! plugin command responses are intentionally one-shot.

use tauri::{plugin::Builder, plugin::TauriPlugin, Runtime};

#[cfg(any(target_os = "ios", test))]
fn supported_stream_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("user-agent") || name.eq_ignore_ascii_case("cookie")
}

#[cfg(target_os = "ios")]
mod mobile;

#[cfg(target_os = "ios")]
pub use mobile::{Error, IosPlayer, PlayerEvent, Result};

/// Register the Swift package with Tauri. The generated Xcode project discovers it from this
/// crate's `ios/Package.swift`; generated Apple files never need to be edited or committed.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("limusic-ios-player")
        .setup(|_app, api| {
            #[cfg(target_os = "ios")]
            {
                let player = mobile::init(api)?;
                _app.manage(player);
            }
            #[cfg(not(target_os = "ios"))]
            let _ = api;
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::supported_stream_header;

    #[test]
    fn accepts_the_headers_limusic_streams_use_case_insensitively() {
        assert!(supported_stream_header("User-Agent"));
        assert!(supported_stream_header("user-agent"));
        assert!(supported_stream_header("Cookie"));
        assert!(supported_stream_header("COOKIE"));
    }

    #[test]
    fn rejects_headers_avurlasset_cannot_preserve() {
        assert!(!supported_stream_header("Authorization"));
        assert!(!supported_stream_header("Referer"));
    }
}
