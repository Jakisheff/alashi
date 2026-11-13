#!/usr/bin/env node
// Uniform-legal null arm for the preregistered bribe grid
// (docs/200_ZETTEL/20261113-bribe-grid-prereg.md): one uniformly random
// legal action per decision. Determinism: the RNG seed is the SHA-256 of
// the user message content, so replay of the same prompt reproduces the
// same choice. Served as a loopback OpenAI-compatible provider; the
// frozen profile must use source=test_provider.
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';

export const UNIFORM_MODEL = 'uniform-legal-v1';

function rng(message) {
  let state = BigInt('0x' + createHash('sha256').update(message).digest('hex').slice(0, 16));
  return () => {
    state = (state * 6364136223846793005n + 1442695040888963407n) & 0xffffffffffffffffn;
    return Number(state >> 11n) / 2 ** 53;
  };
}

export function chooseAction(prompt, random) {
  const menu = prompt?.allowed_actions;
  if (!Array.isArray(menu) || !menu.length) return null;
  return menu[Math.floor(random() * menu.length) % menu.length];
}

function answerFor(messages) {
  const content = messages?.[1]?.content;
  if (typeof content !== 'string') return null;
  const action = chooseAction(JSON.parse(content), rng(content));
  return action === null ? null : JSON.stringify(action);
}

export async function handler(request, response) {
  let body = '';
  for await (const chunk of request) body += chunk;
  let data; try { data = JSON.parse(body); } catch { response.writeHead(400); response.end('{}'); return; }
  const content = answerFor(data.messages);
  const completion = Buffer.byteLength(String(content ?? ''), 'utf8');
  const usage = { prompt_tokens: Buffer.byteLength(body, 'utf8'), completion_tokens: completion,
    total_tokens: Buffer.byteLength(body, 'utf8') + completion };
  response.writeHead(content === null ? 500 : 200, { 'Content-Type': 'application/json' });
  response.end(JSON.stringify({ id: 'uniform-deterministic', model: UNIFORM_MODEL,
    choices: [{ message: { role: 'assistant', content }, finish_reason: 'stop' }], usage }));
}

export function serve(port = 0) {
  const server = createServer(handler);
  return new Promise(resolve => server.listen(port, '127.0.0.1', () =>
    resolve({ server, port: server.address().port })));
}

if (process.argv[1] && process.argv[1].endsWith('uniform-legal-provider.mjs'))
  serve(Number(process.argv[2] ?? 7782)).then(({ port }) =>
    process.stdout.write(JSON.stringify({ model: UNIFORM_MODEL, listening: `http://127.0.0.1:${port}/v1/chat/completions` }) + '\n'));
