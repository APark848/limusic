//! Optional anonymous metadata probe. Never prints or downloads media URLs, tokens or cookies.
#![cfg(feature = "integration-tests")]

#[tokio::test]
#[ignore = "external service; not a deterministic playback test"]
async fn diary_of_a_ceo_metadata_probe() {
    use innertube::{find_ios_format, AudioQuality, Clients, InnerTube, Session};
    use std::time::Duration;
    let it = InnerTube::new(Session::default(), None).unwrap();
    let visitor = tokio::time::timeout(Duration::from_secs(20), it.fetch_visitor_data())
        .await
        .ok()
        .and_then(Result::ok);
    let it = InnerTube::new(Session { visitor_data: visitor, ..Session::default() }, None).unwrap();
    let clients = Clients::bundled();
    // Public Diary of a CEO episode; not assumed to be the user's exact failing episode.
    for key in ["WEB_REMIX", "VISIONOS"] {
        let result = tokio::time::timeout(
            Duration::from_secs(25),
            it.player(clients.get(key).unwrap(), "iDsYrU2o2R8", None, None, None),
        )
        .await;
        match result {
            Ok(Ok(response)) => {
                let data = response.streaming_data.as_ref();
                let duration =
                    response.video_details.as_ref().and_then(|v| v.length_seconds.as_deref());
                let duration_ms =
                    duration.and_then(|s| s.parse::<u64>().ok()).and_then(|s| s.checked_mul(1000));
                let selected =
                    data.and_then(|d| find_ios_format(d, AudioQuality::High, duration_ms));
                eprintln!("client={key} status={} duration_secs={duration:?} compatible_itag={:?} formats={} expiry_present={}",
                    response.playability_status.status, selected.map(|f| f.itag),
                    data.map_or(0, |d| d.adaptive_formats.len() + d.formats.as_ref().map_or(0, Vec::len)),
                    data.is_some_and(|d| d.expires_in_seconds.is_some()));
            }
            Ok(Err(_)) => eprintln!("client={key} result=request-failed"),
            Err(_) => eprintln!("client={key} result=timeout"),
        }
    }
}
