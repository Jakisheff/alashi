#!/usr/bin/env node
// Composite loopback provider for the ES experiment: one world, one driver
// endpoint. Requests for the focal actor are proxied to MODEL_URL (the
// python inference server with the current perturbed weights); all other
// actors are answered by a local deterministic scripted policy.
// The driver's frozen profile still points at this single loopback URL,
// so its security invariant (observations never leave the machine) holds.
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { chooseAction as uniformChoose } from './uniform-legal-provider.mjs';

const rngOf = message => {
  let state = BigInt('0x' + createHash('sha256').update(message).digest('hex').slice(0, 16));
  return () => {
    state = (state * 6364136223846793005n + 1442695040888963407n) & 0xffffffffffffffffn;
    return Number(state >> 11n) / 2 ** 53;
  };
};

const MODEL_URL = process.env.ES_MODEL_URL ?? 'http://127.0.0.1:7779/v1/chat/completions';
const FOCAL_ACTOR = Number(process.env.ES_FOCAL_ACTOR ?? 0);
const PORT = Number(process.argv[2] ?? 7780);
const MODEL_NAME = process.env.ES_MODEL_NAME ?? 'qwen2.5-0.5b-es';

const observationOf = body => {
  const content = body?.messages?.[1]?.content;
  try { return typeof content === 'string' ? JSON.parse(content) : null; } catch { return null; }
};

const server = createServer(async (request, response) => {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  const body = Buffer.concat(chunks).toString('utf8');
  let data; try { data = JSON.parse(body); } catch { response.writeHead(400); response.end('{}'); return; }
  const observation = observationOf(data);
  const actor = observation?.player ?? observation?.observation?.actor;
  if (observation === null || actor === undefined) {
    response.writeHead(500, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify({ error: 'unparsable_observation' }));
    return;
  }
  if (actor === FOCAL_ACTOR) {
    try {
      const upstream = await fetch(MODEL_URL, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ ...data, model: MODEL_NAME }),
        signal: AbortSignal.timeout(60000),
      });
      const payload = await upstream.text();
      response.writeHead(upstream.status, { 'Content-Type': 'application/json' });
      response.end(payload);
    } catch (error) {
      response.writeHead(500, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify({ error: 'model_upstream_unreachable' }));
    }
    return;
  }
  const action = uniformChoose(observation, rngOf(body));
  const content = JSON.stringify(action);
  response.writeHead(200, { 'Content-Type': 'application/json' });
  response.end(JSON.stringify({
    model: 'mixed-uniform', choices: [{ message: { role: 'assistant', content }, finish_reason: 'stop' }],
    usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
  }));
});

server.listen(PORT, '127.0.0.1', () => console.error(`mixed provider on :${PORT}, focal=${FOCAL_ACTOR} -> ${MODEL_URL}`));
