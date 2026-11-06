import { test } from 'node:test';
import assert from 'node:assert/strict';
import { summarize } from './playback-timings.mjs';

test('correlates interleaved requests and does not count old progress twice', () => {
  const rows = summarize(`
2026-11-05T00:00:00.000Z play_index{request_id=1}: playback command received source="queue" ipc_ms=5
2026-11-05T00:00:00.010Z play_index{request_id=1}: playback startup entered generation=7
2026-11-05T00:00:00.020Z play{request_id=2}: playback command received source="song" ipc_ms=2
2026-11-05T00:00:00.100Z iOS AVPlayer stage stage="load-received" generation=7 startup_ms=90
2026-11-05T00:00:00.200Z iOS AVPlayer stage stage="first-playback-progress" generation=7 startup_ms=190
2026-11-05T00:00:00.250Z iOS AVPlayer stage stage="first-playback-progress" generation=7 startup_ms=240
2026-11-05T00:00:00.300Z iOS AVPlayer stage stage="first-playback-progress" generation=99 startup_ms=1
`);
  assert.equal(rows.length, 4);
  assert.equal(rows.find(r => r.metric.endsWith('load-received')).p50_ms, 105);
  assert.equal(rows.find(r => r.metric.endsWith('first-playback-progress')).p50_ms, 205);
  assert.ok(rows.every(r => r.n === 1 && r.p95_ms === null));
});

test('reports nearest-rank p95 only with sufficient samples', () => {
  const log = Array.from({length: 20}, (_, i) => `iOS AVPlayer stage stage="resume-first-progress" elapsed_ms=${i + 1}`).join('\n');
  assert.deepEqual(summarize(log)[0], { metric: 'resume/first-progress', n: 20, p50_ms: 10, p95_ms: 19, min_ms: 1, max_ms: 20 });
  assert.deepEqual(summarize('no timing data'), []);
});

test('accepts the unquoted tracing display format used by the native bridge', () => {
  const rows = summarize('iOS AVPlayer stage stage=resume-first-progress elapsed_ms=123.25');
  assert.equal(rows[0].p50_ms, 123.25);
});

test('distinguishes an existing radio row from ordinary queue selection', () => {
  const rows = summarize(`
2026-11-05T00:00:00.000Z play_index{request_id=3}: playback command received source=queue ipc_ms=2
2026-11-05T00:00:00.001Z play_index{request_id=3}: manual queue selection generation=9 radio=true
2026-11-05T00:00:00.010Z play_index{request_id=3}: playback startup entered generation=9
2026-11-05T00:00:00.100Z iOS AVPlayer stage stage=load-received generation=Some(9) startup_ms=90
`);
  const load = rows.find(r => r.metric === 'radio-queue/tap-to-load-received');
  assert.equal(load.p50_ms, 102);
});

test('separates distant radio preparation from native playback stages', () => {
  const rows = summarize(`
2026-11-05T00:00:00.000Z play_index{request_id=1}: playback command received source=queue ipc_ms=2
2026-11-05T00:00:00.003Z play_index{request_id=1}: manual queue selection generation=4 radio=true index=20 distance=20 elapsed_ms=3
2026-11-05T00:00:00.004Z play_index{request_id=1}: playback startup entered generation=4
2026-11-05T00:00:00.010Z play_index{request_id=1}: stream resolution started
2026-11-05T00:00:00.020Z play_index{request_id=1}: stream prerequisites completed elapsed_ms=10
2026-11-05T00:00:00.030Z play_index{request_id=1}: stream client attempt completed client=WEB_REMIX elapsed_ms=10
2026-11-05T00:00:00.040Z play_index{request_id=1}: stream validation completed client=WEB_REMIX elapsed_ms=10
2026-11-05T00:00:00.050Z iOS AVPlayer stage stage=asset-creation generation=4 elapsed_ms=1 startup_ms=46
2026-11-05T00:00:00.080Z iOS AVPlayer stage stage=time-control-playing generation=4 startup_ms=76
`);
  assert.equal(rows.find(r => r.metric.endsWith('/command-to-resolution')).p50_ms, 10);
  assert.equal(rows.find(r => r.metric.endsWith('/queue-admission')).p50_ms, 3);
  assert.equal(rows.find(r => r.metric.endsWith('/tap-to-time-control-playing')).p50_ms, 82);
  assert.ok(rows.some(r => r.metric === 'radio-queue/distant/validation-WEB_REMIX'));
  assert.ok(rows.every(r => r.p95_ms === null));
});
