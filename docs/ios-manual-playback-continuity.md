# Manual playback and pause/resume continuity

Base: PR #13, `a2cb24a746072b8bd2125f3159d5147a8c246269`. This change is stacked on
`ios-playback-smoothness`; it does not merge or replace that PR.

## Findings and changes

The code inspection identifies avoidable work, not a measured dominant cause on a physical phone:

* `start_current` waited for resolution and native load before announcing the selected song.
  iOS now announces a separate loading intent first. The spinner represents waiting, and native
  playback callbacks remain the authority for actual playback. Pause during resolution cancels it.
* Lookahead and foreground selection could independently resolve the same song. A single bounded,
  cancellable resolver slot promotes matching work and aborts unrelated work when a selection arrives.
  Its key includes queue epoch, song, upload status, duration, and a private session/configuration
  fingerprint. It retains no completed results; existing expiring URL data and one successor remain.
* Older asynchronous work was checked before, but not always after, queue-lock acquisition. Loads
  now recheck identity under the queue lock and issue iOS load/play without an intervening await.
  Radio discovery has a generation reservation so a slow radio response cannot replace a newer queue.
* Queued native callbacks could outlive their item. Native item epochs are checked both at the FFI
  boundary and after event-pump dequeue. Old failures, positions and durations cannot affect a new item.
* A prepared asset still caused an unnecessary fresh AVURLAsset construction. Construction is now
  conditional. Prepared URLs must outlive the selected track plus a 60-second seek margin.
* Native waiting only produced a diagnostic. It now produces explicit buffering, without changing
  intentional pause into buffering or treating buffering as audible playback. Toggle follows intent.
* Radio hydration, rating enrichment, and automatic continuation were already after native startup.
  They are not claimed as newly removed audio-path blockers. Starting a new artist/album radio still
  requires its initial metadata/page fetch; selecting an existing radio row uses normal queue playback.
* Cold cipher discovery, sequential fallback clients, URL validation and network latency remain.
  iOS's PoToken provider is unavailable in the base implementation. Client ordering, authentication,
  header validation and token/URL restrictions are unchanged. No speculative resolver fan-out was added.

## Resume and source boundaries

An intentional pause keeps the same AVPlayerItem. Play resumes that item without resolving a URL or
resetting position. If at least 250 ms at the paused position is already buffered, an explicit resume
uses `playImmediately(atRate:)`; ordinary startup and subsequent stall management retain Apple's
defaults. No global buffer-size reduction is made. A ready item experiencing a recoverable transport
error is retained. Reconnection retries play only when playback intent remains active. A terminally
failed item cannot supply playable bytes; paused failures save the position for the next explicit Play.

This is **best-effort transient-buffer continuity**, not a downloaded library. AVFoundation can discard
bytes, a seek can require a new HTTP request, and a failed item may need a fresh URL when online.
Offline playback is not guaranteed beyond available bytes. Background execution can delay callbacks;
physical device testing remains necessary.

The current online source is YouTube. No evidence of permission for persistent audiovisual downloads
was found, and this change adds no such download/cache. URL/metadata caching is not an audio file.
YouTube's [developer policies](https://developers.google.com/youtube/terms/developer-policies) restrict
storing audiovisual copies without prior written approval. User uploads on YouTube are not treated
as permission to bypass its service restrictions. Native buffer management is left to AVFoundation.

The repository's authorized offline source is already-owned local files on desktop. That path already
plays full files directly without network access. The iOS command surface does not expose local-file
import, folder access or a licensed-download provider, and release Swift accepts only HTTP(S). Adding
a new Files import UI and entitlement/security-scope lifecycle is separate work; this patch does not
advertise a mobile offline library or silently enable arbitrary file URLs.

For a future explicitly authorized mobile importer, the bounded cache design is: app-owned directory,
128 MiB total / 32 MiB per file; source opt-in before copying; random `.partial` names; stream-counted
size limit; validate full container, byte length and SHA-256 before atomic rename; transactionally index
only committed files; remove abandoned partials on launch; verify length/hash before local playback;
LRU eviction excluding the active/pinned item; fail admission if no safe eviction fits. Original
user-owned files are never deleted. No URL fetched from the YouTube resolver may enter this cache.
There is no new persistent audio cache to test or claim as shipped in this PR.

## Measurements and validation

Numeric request IDs link frontend tap, command receipt (`ipc_ms`), queue/startup, resolution, native
load, readiness and first advancing playback position. Generation links native asynchronous logs to
the request span. Logs contain no new titles, URLs, credentials or session fingerprints. The first
position advance is a playback proxy, not a microphone measurement of audible output; its sampling
interval is 250 ms. Resume has its own first-progress duration. Seek callbacks alone do not count.

Run `node scripts/playback-timings.mjs limusic.log` on a diagnostics log. It emits numeric summaries
by source/stage and resolver cache/reuse category, never raw log contents. p95 is omitted below 20
samples. Collect at least 20 samples per condition on the same phone/network/build configuration:

| Condition | Baseline | Feature branch |
| --- | --- | --- |
| Unprepared queue and existing radio row | Not collected on physical hardware | Pending physical run |
| Prepared successor selection | Not collected on physical hardware | Pending physical run |
| Cold / warm resolver, degraded working network | Not collected on physical hardware | Pending physical run |
| Buffered offline resume / exhausted buffer / reconnect | Not collected on physical hardware | Pending physical run |

No numerical startup improvement, battery saving, or p50/p95 phone result is claimed. Expected resource
effects are fewer duplicate/cancelled requests, one shared pending result plus one prepared successor,
one low-frequency network-path monitor, and no new media files. There is still one AVPlayer. Measure
energy/network use with Instruments on device; static analysis cannot establish those quantities.

Deterministic tests cover shared resolve promotion, cancellation under rapid selection, background
priority, queue/session invalidation, noncached failure/retry and prepared URL lifetime. Existing queue,
radio, repeat-one, preparation ownership and delayed retry tests remain. The local-WAV simulator smoke
test also checks same-item resume and pause before readiness; this is not airplane-mode or physical
iPhone verification. CI and final validation results are recorded in the feature PR.

Physical scenarios still required: rapid queue/radio/list selection; pause then airplane mode with
and without sufficient buffered ranges; network loss while playing; reconnect while paused vs waiting;
seek inside/outside available ranges; background/foreground; lock-screen, headphone and Bluetooth
controls; long mixes, repeat/shuffle and automatic next-track transitions. Verify timestamps and queue
history, not just the spinner.

Apple references: [playImmediately(atRate:)](https://developer.apple.com/documentation/avfoundation/avplayer/playimmediately(atrate:)),
[timeControlStatus](https://developer.apple.com/documentation/avfoundation/avplayer/timecontrolstatus-swift.property).
