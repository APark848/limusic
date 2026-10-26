//! Debug-only, headless AVPlayer functional smoke test used by the iOS CI runner.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::platform::player::PlayerEvent;
use crate::state::AppState;

const RESULT_FILE: &str = "ios-player-self-test.json";
const REMOTE_RESULT_FILE: &str = "ios-remote-stream-self-test.json";
const PUBLIC_VIDEO_ID: &str = "xl9cFAOKg_Y";
const FIXTURE_SECONDS: f64 = 8.0;
const SEEK_SECONDS: f64 = 4.0;
const START_DELAY: Duration = Duration::from_secs(2);

static EVENT_TAP: OnceLock<Mutex<Option<UnboundedSender<PlayerEvent>>>> = OnceLock::new();

pub(crate) struct Prepared {
    events: UnboundedReceiver<PlayerEvent>,
}

pub(crate) fn prepare(data_dir: &Path) -> std::io::Result<Prepared> {
    let (tx, rx) = unbounded_channel();
    if let Ok(mut tap) = EVENT_TAP.get_or_init(|| Mutex::new(None)).lock() {
        *tap = Some(tx);
    }
    write_progress(&data_dir.join(RESULT_FILE), "prepared")?;
    Ok(Prepared { events: rx })
}

pub(crate) fn observe_event(event: &PlayerEvent) {
    if let Some(tx) =
        EVENT_TAP.get().and_then(|tap| tap.lock().ok()).and_then(|tap| tap.as_ref().cloned())
    {
        let _ = tx.send(event.clone());
    }
}

pub(crate) fn spawn(state: Arc<AppState>, data_dir: PathBuf, prepared: Prepared) {
    std::thread::Builder::new()
        .name("ios-player-self-test".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let report = serde_json::json!({
                        "status": "failed",
                        "error": format!("could not create self-test runtime: {error}"),
                    });
                    let _ = std::fs::write(data_dir.join(RESULT_FILE), report.to_string());
                    return;
                }
            };
            runtime.block_on(run_spawned(state, data_dir, prepared));
        })
        .expect("could not start iOS player self-test thread");
}

async fn run_spawned(state: Arc<AppState>, data_dir: PathBuf, prepared: Prepared) {
    let result_path = data_dir.join(RESULT_FILE);
    let _ = write_progress(&result_path, "starting");
    tokio::time::sleep(START_DELAY).await;
    let mut events = prepared.events;
    let result = run(state.clone(), &data_dir, &result_path, &mut events).await;
    let local_passed = result.is_ok();
    let report = match result {
        Ok(observed) => serde_json::json!({
            "status": "passed",
            "observed": observed,
        }),
        Err(error) => serde_json::json!({
            "status": "failed",
            "error": format!("{error:#}"),
        }),
    };
    if let Err(error) = std::fs::write(&result_path, report.to_string()) {
        tracing::error!(%error, "could not write iOS player self-test result");
    }
    if local_passed {
        run_remote(state, &data_dir, &mut events).await;
    }
}

async fn run(
    state: Arc<AppState>,
    data_dir: &Path,
    result_path: &Path,
    events: &mut UnboundedReceiver<PlayerEvent>,
) -> anyhow::Result<Vec<&'static str>> {
    let fixture = data_dir.join("ios-player-self-test.wav");
    write_wav_fixture(&fixture)?;
    let fixture_url = tauri::Url::from_file_path(&fixture)
        .map_err(|_| anyhow!("could not convert the fixture path to a file URL"))?
        .to_string();
    let mut observed = vec!["constructed"];

    write_progress(result_path, "loading")?;
    state.player.load(&fixture_url, &HashMap::new(), None, None).context("load failed")?;
    // Deliberately do not wait for AVPlayerItem.readyToPlay. This is the production song-tap
    // sequence and protects the pending-play intent in the Swift bridge.
    write_progress(result_path, "playing-before-ready")?;
    state.player.play().context("play before ready failed")?;
    let duration = wait_for_ready_and_playing(events).await?;
    if (duration - FIXTURE_SECONDS).abs() > 0.25 {
        bail!("unexpected fixture duration {duration:.3}s");
    }
    observed.push("duration");

    let first = wait_for(events, "positive position", |event| match event {
        PlayerEvent::Position(value) if *value > 0.0 => Some(*value),
        _ => None,
    })
    .await?;
    let advanced = wait_for(events, "advancing position", |event| match event {
        PlayerEvent::Position(value) if *value > first + 0.2 => Some(*value),
        _ => None,
    })
    .await?;
    observed.extend(["playing", "position-advanced"]);

    write_progress(result_path, "pausing")?;
    state.player.pause().context("pause failed")?;
    wait_for(events, "not playing after pause", |event| match event {
        PlayerEvent::Playing(false) => Some(()),
        _ => None,
    })
    .await?;
    wait_for(events, "paused", |event| match event {
        PlayerEvent::Paused(true) => Some(()),
        _ => None,
    })
    .await?;
    let mut paused_positions = vec![advanced];
    while let Ok(event) = events.try_recv() {
        reject_failure(&event)?;
        if let PlayerEvent::Position(value) = event {
            paused_positions.push(value);
        }
    }
    tokio::time::sleep(Duration::from_millis(800)).await;
    while let Ok(event) = events.try_recv() {
        reject_failure(&event)?;
        if let PlayerEvent::Position(value) = event {
            paused_positions.push(value);
        }
    }
    let paused_min = paused_positions.iter().copied().fold(f64::INFINITY, f64::min);
    let paused_max = paused_positions.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if paused_max - paused_min > 0.35 {
        bail!("position advanced while paused: {paused_min:.3}s to {paused_max:.3}s");
    }
    observed.extend(["paused", "position-stable"]);

    write_progress(result_path, "seeking")?;
    state.player.seek(SEEK_SECONDS).context("seek failed")?;
    wait_for(events, "seek position", |event| match event {
        PlayerEvent::Position(value) if (*value - SEEK_SECONDS).abs() <= 0.5 => Some(*value),
        _ => None,
    })
    .await?;
    observed.push("seeked");

    write_progress(result_path, "resuming")?;
    state.player.play().context("resume failed")?;
    wait_for(events, "resumed", |event| match event {
        PlayerEvent::Playing(true) => Some(()),
        _ => None,
    })
    .await?;
    wait_for(events, "position after resume", |event| match event {
        PlayerEvent::Position(value) if *value > SEEK_SECONDS + 0.2 => Some(*value),
        _ => None,
    })
    .await?;
    observed.push("resumed");

    write_progress(result_path, "stopping")?;
    state.player.stop().context("stop failed")?;
    wait_for(events, "stopped position", |event| match event {
        PlayerEvent::Position(value) if value.abs() <= f64::EPSILON => Some(()),
        _ => None,
    })
    .await?;
    wait_for(events, "stopped state", |event| match event {
        PlayerEvent::Playing(false) => Some(()),
        _ => None,
    })
    .await?;
    observed.push("stopped");
    Ok(observed)
}

async fn run_remote(
    state: Arc<AppState>,
    data_dir: &Path,
    events: &mut UnboundedReceiver<PlayerEvent>,
) {
    let result_path = data_dir.join(REMOTE_RESULT_FILE);
    let _ = write_progress(&result_path, "resolving-public-track");
    while events.try_recv().is_ok() {}

    let data = match tokio::time::timeout(
        Duration::from_secs(45),
        state.resolve(PUBLIC_VIDEO_ID, false, 240),
    )
    .await
    {
        Ok(Ok(data)) => data,
        Ok(Err(error)) => {
            write_remote_failure(
                &result_path,
                "external-service",
                "resolver",
                &format!("The anonymous production resolver failed: {error}"),
            );
            return;
        }
        Err(_) => {
            write_remote_failure(
                &result_path,
                "external-service",
                "resolver-timeout",
                "The anonymous production resolver did not finish within 45 seconds",
            );
            return;
        }
    };

    if !matches!(
        tauri::Url::parse(&data.stream_url).ok().map(|url| url.scheme().to_owned()).as_deref(),
        Some("http" | "https")
    ) {
        write_remote_failure(
            &result_path,
            "application",
            "resolved-contract",
            "The production resolver returned a non-HTTP stream URL",
        );
        return;
    }
    if let Some(header) = data
        .headers
        .keys()
        .find(|name| !matches!(name.to_ascii_lowercase().as_str(), "user-agent" | "cookie"))
    {
        write_remote_failure(
            &result_path,
            "application",
            "resolved-contract",
            &format!("The resolver requires unsupported header {header}"),
        );
        return;
    }

    let header_names: Vec<String> =
        data.headers.keys().map(|name| name.to_ascii_lowercase()).collect();
    if !header_names.iter().any(|name| name == "user-agent") {
        write_remote_failure(
            &result_path,
            "application",
            "resolved-contract",
            "The anonymous production resolver returned no User-Agent header",
        );
        return;
    }
    let client = data.stream_client.clone();
    let itag = data.itag;
    let _ = write_progress(&result_path, "loading-remote-stream");
    if let Err(error) = state.player.load(&data.stream_url, &data.headers, None, None) {
        write_remote_failure(
            &result_path,
            "application",
            "player-load",
            &format!("The iOS player rejected the resolved request: {error}"),
        );
        return;
    }
    if let Err(error) = state.player.play() {
        write_remote_failure(
            &result_path,
            "application",
            "player-play",
            &format!("The iOS player rejected play before readiness: {error}"),
        );
        return;
    }

    let playback = tokio::time::timeout(Duration::from_secs(25), async {
        wait_for_ready_and_playing(events).await?;
        let first = wait_for(events, "remote positive position", |event| match event {
            PlayerEvent::Position(value) if *value > 0.0 => Some(*value),
            _ => None,
        })
        .await?;
        wait_for(events, "remote advancing position", |event| match event {
            PlayerEvent::Position(value) if *value > first + 0.2 => Some(()),
            _ => None,
        })
        .await
    })
    .await;

    let _ = state.player.stop();
    match playback {
        Ok(Ok(())) => {
            let report = serde_json::json!({
                "status": "passed",
                "observed": ["production-resolved", "headers-forwarded", "duration", "playing", "position-advanced"],
                "streamClient": client,
                "itag": itag,
                "headerNames": header_names,
            });
            let _ = std::fs::write(result_path, report.to_string());
        }
        Ok(Err(_)) | Err(_) => write_remote_failure(
            &result_path,
            "external-service",
            "remote-playback",
            "AVPlayer did not begin advancing the resolved remote stream within the timeout",
        ),
    }
}

fn write_remote_failure(path: &Path, category: &str, stage: &str, message: &str) {
    let report = serde_json::json!({
        "status": "failed",
        "category": category,
        "stage": stage,
        "error": message,
    });
    if let Err(error) = std::fs::write(path, report.to_string()) {
        tracing::error!(%error, "could not write iOS remote-stream self-test result");
    }
}

fn write_progress(path: &Path, step: &'static str) -> std::io::Result<()> {
    std::fs::write(path, serde_json::json!({ "status": "running", "step": step }).to_string())
}

async fn wait_for_ready_and_playing(
    events: &mut UnboundedReceiver<PlayerEvent>,
) -> anyhow::Result<f64> {
    tokio::time::timeout(Duration::from_secs(8), async {
        let mut duration = None;
        let mut playing = false;
        loop {
            let event = events.recv().await.ok_or_else(|| anyhow!("event channel closed"))?;
            reject_failure(&event)?;
            match event {
                PlayerEvent::Duration(value) if value.is_finite() && value > 0.0 => {
                    duration = Some(value);
                }
                PlayerEvent::Playing(true) => playing = true,
                _ => {}
            }
            if let (Some(duration), true) = (duration, playing) {
                return Ok(duration);
            }
        }
    })
    .await
    .map_err(|_| anyhow!("timed out waiting for ready autoplay"))?
}

async fn wait_for<T>(
    events: &mut UnboundedReceiver<PlayerEvent>,
    label: &'static str,
    mut select: impl FnMut(&PlayerEvent) -> Option<T>,
) -> anyhow::Result<T> {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let event = events.recv().await.ok_or_else(|| anyhow!("event channel closed"))?;
            reject_failure(&event)?;
            if let Some(value) = select(&event) {
                return Ok(value);
            }
        }
    })
    .await
    .map_err(|_| anyhow!("timed out waiting for {label}"))?
}

fn reject_failure(event: &PlayerEvent) -> anyhow::Result<()> {
    match event {
        PlayerEvent::TrackFailed(message) | PlayerEvent::Error(message) => {
            bail!("AVPlayer failure: {message}")
        }
        _ => Ok(()),
    }
}

fn write_wav_fixture(path: &Path) -> anyhow::Result<()> {
    const SAMPLE_RATE: u32 = 8_000;
    const CHANNELS: u16 = 1;
    const BITS_PER_SAMPLE: u16 = 16;
    let sample_count = (f64::from(SAMPLE_RATE) * FIXTURE_SECONDS) as u32;
    let data_size = sample_count * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE / 8);
    let mut wav = Vec::with_capacity(44 + data_size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&CHANNELS.to_le_bytes());
    wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for index in 0..sample_count {
        let phase = f64::from(index) * 440.0 * std::f64::consts::TAU / f64::from(SAMPLE_RATE);
        let sample = (phase.sin() * 4_000.0) as i16;
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, wav).context("could not write deterministic WAV fixture")
}

#[cfg(test)]
mod tests {
    use super::write_wav_fixture;

    #[test]
    fn fixture_has_a_valid_pcm_wav_header() {
        let path = std::env::temp_dir().join("limusic-ios-player-test-fixture.wav");
        write_wav_fixture(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 8_000);
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 128_000);
    }
}
