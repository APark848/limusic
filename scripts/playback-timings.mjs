// Usage: node scripts/playback-timings.mjs path/to/limusic.log
// Reads only numeric timings/stages. Does not copy URLs, credentials, paths or track metadata.
import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export function summarize(log) {
  const requests = new Map(), generations = new Map(), samples = new Map();
  const number = (line, key) => {
    const value = line.match(new RegExp(`\\b${key}=(?:Some\\()?([0-9.]+)`));
    return value ? Number(value[1]) : undefined;
  };
  const add = (key, value) => {
    if (!Number.isFinite(value) || value < 0) return;
    if (!samples.has(key)) samples.set(key, []);
    samples.get(key).push(value);
  };
  for (const raw of log.split('\n')) {
    const line = raw.replace(/\x1b\[[0-9;]*m/g, '');
    const id = number(line, 'request_id'), gen = number(line, 'generation');
    const time = Date.parse(line.match(/\d{4}-\d\d-\d\dT[\d:.]+Z/)?.[0] ?? '');
    if (id && line.includes('playback command received')) {
      const source = line.match(/source="?([a-z_-]+)/)?.[1] ?? 'unknown';
      requests.set(id, { source, time, ipc: number(line, 'ipc_ms'), stages: new Set() });
    }
    if (id && gen !== undefined && line.includes('playback startup entered')) {
      const request = requests.get(id);
      if (request) generations.set(gen, { request, offset: time - request.time + request.ipc });
    }
    if (line.includes('iOS AVPlayer stage')) {
      const stage = line.match(/stage="([a-z-]+)"/)?.[1];
      if (stage === 'resume-first-progress') add('resume/first-progress', number(line, 'elapsed_ms'));
      const trace = generations.get(gen);
      if (!trace || !['load-received', 'item-ready-to-play', 'first-playback-progress'].includes(stage)) continue;
      if (trace.request.stages.has(stage)) continue;
      trace.request.stages.add(stage);
      add(`${trace.request.source}/tap-to-${stage}`, trace.offset + number(line, 'startup_ms'));
    }
    if (id && line.includes('stream cache lookup completed')) {
      const request = requests.get(id);
      if (request) request.warm = /\bhit=true\b/.test(line);
    }
    if (id && line.includes('playback resolve completed') && /foreground=true/.test(line)
        && !/cancelled=true/.test(line)) {
      const request = requests.get(id);
      const kind = /reused=true/.test(line) ? 'inflight-reused'
        : request?.warm === undefined ? 'unknown' : request.warm ? 'cache-hit' : 'cache-miss';
      if (request) add(`resolver/${kind}`, number(line, 'elapsed_ms'));
    }
  }
  return [...samples].sort(([a], [b]) => a.localeCompare(b)).map(([metric, values]) => {
    values.sort((a, b) => a - b);
    const quantile = p => Math.round(values[Math.max(0, Math.ceil(values.length * p) - 1)] * 100) / 100;
    return { metric, n: values.length, p50_ms: quantile(0.5),
      p95_ms: values.length >= 20 ? quantile(0.95) : null,
      min_ms: values[0], max_ms: values.at(-1) };
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (!process.argv[2]) throw new Error('Provide a diagnostics log path');
  console.log(JSON.stringify(summarize(readFileSync(process.argv[2], 'utf8')), null, 2));
}
