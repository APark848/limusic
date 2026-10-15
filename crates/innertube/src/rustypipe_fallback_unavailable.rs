//! Explicit unavailable rustypipe fallback for mobile targets.
//!
//! The direct InnerTube clients remain available. Rustypipe embeds QuickJS and has no supported
//! iOS simulator bindings, so the orchestrator treats this optional last-resort resolver as not
//! having reached YouTube.

/// Kept API-compatible with the desktop fallback so shared orchestration code still compiles.
#[derive(Debug, Clone)]
pub struct StreamCandidate {
    pub url: String,
    pub itag: u32,
    pub mime: String,
    pub size: u64,
    pub bitrate: u32,
    pub expires_in_seconds: u32,
    pub loudness_db: Option<f32>,
    pub title: Option<String>,
    pub duration_secs: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum FallbackError {
    #[error("rustypipe fallback is not implemented on this platform")]
    Unsupported,
}

impl FallbackError {
    /// No network request was made, so this cannot count as YouTube answering about the track.
    pub fn answered(&self) -> bool {
        false
    }
}

pub async fn resolve(
    _video_id: &str,
    _prefer_high: bool,
) -> Result<StreamCandidate, FallbackError> {
    Err(FallbackError::Unsupported)
}
