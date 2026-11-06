# Podcast playback and distant radio selection

Base: PR #14, `c137fa9d933de9965d949795a85f6a79e3d8696a`. This branch is stacked on
`codex/ios-manual-playback-continuity`; PRs #13 and #14 are not modified or merged.

## Evidence and fixes

The user identified The Diary of a CEO, without a specific episode or device diagnostic log.
The following defects are reproducible in code/fixtures; they are not presented as a confirmed
causal trace from that physical phone:

* Podcast duration parsing treated `2 hr` as `2:00` and `45:30` as `45:30:00`. The iOS format
  duration filter then rejected valid audio against those bad hints. Clock fields and English
  unit fields are now parsed separately. Unknown localized units remain unknown rather than
  inventing a duration. Podcast player metadata outranks the rounded browse hint; music retains
  its existing queue-duration precedence. Podcast episode type is not classified as a music video.
* The main resolver selected AAC on iOS, but the rustypipe safety net still preferred Opus/WebM.
  Both paths now use the native-compatible MIME policy. The fallback filters before ranking, so
  AAC remains available even when Opus has higher bitrate. Unsupported audio fails explicitly.
* External podcast enclosures could be skipped when `expiresInSeconds` was absent, or rejected
  when an origin supported GET but returned 405/501 to HEAD. Missing expiry is now allowed only
  for external origins, with zero reusable cache lifetime. Method rejection permits one bounded
  range GET; 401/403 do not. No response audio is persisted, and signed YouTube expiry rules remain.
* A terminal native failure could leave the loading state visible, and retries could end by
  advancing the queue as though the episode had finished. iOS now reports paused/error after
  bounded recovery, retaining the selected failed entry. Failure cleanup rechecks generation
  after awaits so it cannot stop a newer selection. Native diagnostics include at most three
  underlying error domains/codes, never localized error payloads or media URLs.

The public metadata probe uses `iDsYrU2o2R8`, a Diary of a CEO sample, not necessarily the user's
episode. On November 5, 2026, the anonymous WEB_REMIX response was UNPLAYABLE with no formats;
VISIONOS returned OK, duration 5,618 seconds, eight formats and compatible AAC itag 140.
This was a metadata request, not a verified media download or physical-device playback.
The opt-in `podcast_probe` integration test reproduces this privacy-safe inspection.

## Radio path

QueueList already submits a direct index, and `QueueState::seek_to` assigns that pointer without
walking preceding rows. Hydration, persistence and successor preparation are not prerequisites
to selecting row 20. Nearby successor selection can consume prepared work; distant rows ordinarily
take the uncached resolver path.

The HIGH-quality second pass previously noticed HIGH Opus or malformed-duration audio, deferred
the valid MEDIUM AAC candidate, and tried more clients before returning it. On iOS it now checks
whether the preferred native-compatible, duration-valid choice is HIGH. Desktop ranking remains
unchanged. No additional resolver fan-out or multi-track media prefetch is introduced.

Manual queue requests also carry the clicked video identity. If a reorder/removal changed that
slot before command admission, the app requests another selection instead of playing the wrong
row. Existing queue-epoch, generation, resolver ownership, session checks and one-successor
preparation remain. Tests exercise positions 1/20, intervening mutation, cancellation and a new
queue epoch while resolution is pending.

## Measurements and validation

Correlated diagnostics cover tap-to-command, queue admission, command-to-resolution, cipher/token
prerequisites, individual client attempts, URL/cipher preparation, validation, native asset
creation, readiness, time-control playing and first advancing progress. Radio summaries split
near versus distant jumps; cache-hit/miss/reused resolution remains separate. Progress is sampled
at 250 ms and is not a microphone measurement. No signed URL, cookie, token or signature is added
to these diagnostics.

Use `node scripts/playback-timings.mjs limusic.log` on separate logs for equivalent cold/warm
network conditions. Collect at least 20 samples per condition for p95. No physical near-versus-
distant before/after timings are available in this environment. The eliminated second-pass
attempts are established by fixtures, not advertised as a measured millisecond improvement.

Apple CI generates its own two-hour silent AAC/M4A fixture using `afconvert`; no third-party audio
is downloaded. The simulator checks long duration, a one-hour seek, same-item pause/resume, an
invalid MP3 container producing failure, and normal music after that failure. PR #14's controlled
buffer-loss, no-data, reconnection and local playback tests remain. The remote smoke leg uses the
public podcast sample separately from deterministic tests. External refusals are reported honestly.

Exact check results, commit, Actions runs and unsigned ARM64 IPA are recorded in the stacked PR.
Physical checks remain necessary for the user's exact episode, real near/deep radio timing,
background/foreground, lock-screen/Bluetooth/headphone controls, loss/recovery of connectivity,
shuffle/repeat, and long seeks across uncached ranges. The built bundle's background-audio mode is
checked in CI, but simulator success does not prove physical background behavior.

There is still one AVPlayer and one prepared successor. New metadata and timing fields are small;
resource savings from avoiding extra clients have not been measured with Instruments. No
unauthorized YouTube downloads or persistent offline audio cache are added.
