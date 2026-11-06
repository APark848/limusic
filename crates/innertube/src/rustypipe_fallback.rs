//! The D3 safety-net extractor: rustypipe whole-videoId resolution. context/12 §rustypipe.
//!
//! Phase-0 audit (spikes/REPORT.md §3): rustypipe is all-or-nothing per videoId — it runs its
//! own `/player` + cipher + PoToken internally. We cannot hand it a `signatureCipher` from our
//! own response. So it slots in at the videoId level only: "our direct clients all failed →
//! ask rustypipe to resolve the whole id." It must be able to carry the queue SOLO.

use rustypipe::client::RustyPipe;
use rustypipe::error::{Error as RpError, ExtractionError, UnavailabilityReason};
use rustypipe::model::AudioStream;

/// A resolved stream from the fallback. Mirrors what the orchestrator needs.
#[derive(Debug, Clone)]
pub struct StreamCandidate {
    pub url: String,
    pub itag: u32,
    pub mime: String,
    /// Full file size in bytes, for the bounded `&range=` URL googlevideo requires.
    pub size: u64,
    pub bitrate: u32,
    pub expires_in_seconds: u32,
    /// rustypipe's loudness (inverse ReplayGain — see AudioStream docs). Feeds context/14 gain.
    pub loudness_db: Option<f32>,
    pub title: Option<String>,
    pub duration_secs: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum FallbackError {
    #[error("age-restricted (rustypipe)")]
    AgeRestricted,
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("no audio stream in rustypipe result")]
    NoAudio,
    #[error("rustypipe: {0}")]
    RustyPipe(String),
}

impl FallbackError {
    /// Did YouTube answer? A refusal is a verdict on the track; only `RustyPipe` can be silence.
    ///
    /// The orchestrator needs the two apart. With every InnerTube client skipped or erroring, the
    /// last thing that spoke to YouTube is rustypipe, and reading its "unavailable" as an outage
    /// makes the queue treat a dead video as systemic: it holds the track in place instead of
    /// skipping it and fails on it forever. Issue #292.
    pub fn answered(&self) -> bool {
        match self {
            // `map_err` builds these two from `ExtractionError::Unavailable` only, which is
            // YouTube's own playability verdict, and `NoAudio` from a player response that
            // parsed. Transport, parse and cipher failures all land in `RustyPipe`.
            Self::AgeRestricted | Self::Unavailable(_) | Self::NoAudio => true,
            Self::RustyPipe(_) => false,
        }
    }
}

/// Resolve a videoId to its best audio stream via rustypipe. `prefer_high`: pick the
/// highest-bitrate opus/mp4a stream (matches our HIGH preference); else lowest ≤128k.
pub async fn resolve(video_id: &str, prefer_high: bool) -> Result<StreamCandidate, FallbackError> {
    resolve_inner(video_id, prefer_high, false).await
}

pub async fn resolve_ios(
    video_id: &str,
    prefer_high: bool,
) -> Result<StreamCandidate, FallbackError> {
    resolve_inner(video_id, prefer_high, true).await
}

async fn resolve_inner(
    video_id: &str,
    prefer_high: bool,
    ios: bool,
) -> Result<StreamCandidate, FallbackError> {
    // Keep rustypipe's cache out of the app CWD (it defaults to ./rustypipe_cache.json +
    // ./rustypipe_reports/, and an installed app's CWD may not be writable).
    let storage = std::env::temp_dir().join("limusic-rustypipe");
    std::fs::create_dir_all(&storage).ok();
    let player = RustyPipe::builder()
        .storage_dir(storage)
        .build()
        .map_err(|e| FallbackError::RustyPipe(e.to_string()))?
        .query()
        .player(video_id)
        .await
        .map_err(map_err)?;

    let best = pick_audio(&player.audio_streams, prefer_high, ios).ok_or(FallbackError::NoAudio)?;
    Ok(StreamCandidate {
        url: best.url.clone(),
        itag: best.itag,
        mime: best.mime.clone(),
        size: best.size,
        bitrate: best.bitrate,
        expires_in_seconds: player.expires_in_seconds,
        loudness_db: best.loudness_db,
        title: player.details.name.clone(),
        duration_secs: Some(player.details.duration),
    })
}

fn pick_audio(streams: &[AudioStream], prefer_high: bool, ios: bool) -> Option<&AudioStream> {
    fn codec_score(mime: &str) -> u8 {
        if mime.contains("opus") {
            2
        } else if mime.contains("mp4a") {
            1
        } else {
            0
        }
    }
    // A dubbed video lists every language at the same bitrates; keep to the original track
    // (`track` is None when there is only one). Mirrors `Format::is_original`.
    let compatible: Vec<_> =
        streams.iter().filter(|s| !ios || crate::ios_audio_mime_supported(&s.mime)).collect();
    let original: Vec<&AudioStream> = compatible
        .iter()
        .copied()
        .filter(|s| s.track.as_ref().is_none_or(|t| t.is_default))
        .collect();
    let streams = if original.is_empty() { compatible } else { original };
    if prefer_high {
        streams.into_iter().max_by(|a, b| {
            codec_score(&a.mime).cmp(&codec_score(&b.mime)).then(a.bitrate.cmp(&b.bitrate))
        })
    } else {
        let capped: Vec<&AudioStream> =
            streams.iter().copied().filter(|s| s.bitrate <= 128_000).collect();
        if capped.is_empty() {
            streams.into_iter().min_by_key(|s| s.bitrate)
        } else {
            capped.into_iter().max_by_key(|s| s.bitrate)
        }
    }
}

fn map_err(e: RpError) -> FallbackError {
    match e {
        RpError::Extraction(ExtractionError::Unavailable { reason, msg }) => match reason {
            UnavailabilityReason::AgeRestricted => FallbackError::AgeRestricted,
            _ => FallbackError::Unavailable(msg),
        },
        other => FallbackError::RustyPipe(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn ios_last_resort_keeps_aac_and_rejects_opus_only() {
        let stream = |itag, mime, format, codec| {
            serde_json::from_value::<rustypipe::model::AudioStream>(serde_json::json!({
            "url": "https://example.test/audio", "itag": itag, "bitrate": 128000,
            "average_bitrate": 128000, "size": 1024, "mime": mime, "format": format, "codec": codec, "drm_systems": []
        })).unwrap()
        };
        let audio = vec![
            stream(251, "audio/webm; codecs=\"opus\"", "webm", "opus"),
            stream(140, "audio/mp4; codecs=\"mp4a.40.2\"", "m4a", "mp4a"),
        ];
        assert_eq!(super::pick_audio(&audio, true, false).unwrap().itag, 251);
        assert_eq!(super::pick_audio(&audio, true, true).unwrap().itag, 140);
        assert!(super::pick_audio(&audio[..1], true, true).is_none());
    }
    use super::FallbackError;

    #[test]
    fn only_a_verdict_counts_as_an_answer() {
        assert!(FallbackError::AgeRestricted.answered());
        assert!(FallbackError::Unavailable("gone".into()).answered());
        assert!(FallbackError::NoAudio.answered());
        assert!(
            !FallbackError::RustyPipe("connection refused".into()).answered(),
            "a transport failure must still read as an outage"
        );
    }
}
