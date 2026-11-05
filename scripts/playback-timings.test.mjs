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
  assert.equal(rows.length, 2);
  assert.equal(rows.find(r => r.metric.endsWith('load-received')).p50_ms, 105);
  assert.equal(rows.find(r => r.metric.endsWith('first-playback-progress')).p50_ms, 205);
  assert.ok(rows.every(r => r.n === 1 && r.p95_ms === null));
});

test('reports nearest-rank p95 only with sufficient samples', () => {
  const log = Array.from({length: 20}, (_, i) => `iOS AVPlayer stage stage="resume-first-progress" elapsed_ms=${i + 1}`).join('\n');
  assert.deepEqual(summarize(log)[0], { metric: 'resume/first-progress', n: 20, p50_ms: 10, p95_ms: 19, min_ms: 1, max_ms: 20 });
  assert.deepEqual(summarize('no timing data'), []);
});
