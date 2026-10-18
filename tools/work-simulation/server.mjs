import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { runExperiment } from './experiment.mjs';

export function workServer(experiment = runExperiment) {
  let busy = false;
  return createServer(async (req, res) => {
    res.setHeader('Content-Type', 'application/json'); res.setHeader('Cache-Control', 'no-store');
    const reply = (status, value) => { res.writeHead(status); res.end(JSON.stringify(value)); };
    if (req.url !== '/run') return reply(404, { error: 'not_found' });
    if (req.method !== 'POST') return reply(405, { error: 'post_required' });
    if (!/^http:\/\/(127\.0\.0\.1|localhost):[0-9]+$/.test(req.headers.origin ?? '')) return reply(403, { error: 'local_origin_required' });
    if (busy) return reply(409, { error: 'test_already_running' });
    busy = true;
    try {
      let size = 0; for await (const part of req) { size += part.length; if (size > 64) return reply(413, { error: 'body_not_allowed' }); }
      if (size) return reply(400, { error: 'body_not_allowed' });
      reply(200, await experiment());
    } catch { if (!res.headersSent) reply(500, { error: 'test_run_failed' }); }
    finally { busy = false; }
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const port = Number(process.env.ALASHI_WORK_TEST_PORT ?? 18715);
  if (!Number.isInteger(port) || port < 1024 || port > 65535) throw new Error('invalid_local_port');
  workServer().listen(port, '127.0.0.1', () => console.log(`TEST-only work simulation: http://127.0.0.1:${port}`));
}
