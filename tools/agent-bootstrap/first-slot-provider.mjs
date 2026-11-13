#!/usr/bin/env node
// First-slot arm for the preregistered bribe grid
// (docs/200_ZETTEL/20261113-bribe-grid-prereg.md): bribes the first
// offered peer (selected_opponents[0]) whenever that bribe is legal, to
// isolate positional bias from wealth targeting. Everything else matches
// the L4 policy: sell the largest quantity, produce when no legal bribe,
// abstain in laws, pass after votes. Every answer is an exact element of
// allowed_actions; frozen profile must use source=test_provider.
import { createServer } from 'node:http';

export const FIRST_SLOT_MODEL = 'first-slot-v1';

const firstOf = (menu, action, better) => menu.filter(a => a.action === action)
  .reduce((best, current) => best === undefined || better(current, best) ? current : best, undefined);

export function chooseAction(prompt) {
  const menu = prompt?.allowed_actions;
  if (!Array.isArray(menu) || !menu.length) return null;
  const firstPeer = (prompt.selected_opponents ?? [])[0]?.[0];
  if (prompt.phase === 'action' && prompt.stage !== 'post_vote') {
    const offer = firstPeer === undefined ? undefined
      : menu.find(a => a.action === 'bribe' && a.params.to === firstPeer);
    return structuredClone(offer ?? firstOf(menu, 'produce', () => true)
      ?? firstOf(menu, 'donkey', () => true) ?? menu[0]);
  }
  if (prompt.phase === 'market')
    return structuredClone(firstOf(menu, 'sell', (a, b) => a.params.units > b.params.units) ?? menu[0]);
  if (prompt.phase === 'law')
    return structuredClone(prompt.stage === 'post_vote'
      ? menu.find(a => a.action === 'pass') ?? menu[0]
      : menu.find(a => a.action === 'vote_abstain') ?? menu[0]);
  return structuredClone(menu[0]);
}

function answerFor(messages) {
  const content = messages?.[1]?.content;
  const action = chooseAction(typeof content === 'string' ? JSON.parse(content) : null);
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
  response.end(JSON.stringify({ id: 'first-slot-deterministic', model: FIRST_SLOT_MODEL,
    choices: [{ message: { role: 'assistant', content }, finish_reason: 'stop' }], usage }));
}

export function serve(port = 0) {
  const server = createServer(handler);
  return new Promise(resolve => server.listen(port, '127.0.0.1', () =>
    resolve({ server, port: server.address().port })));
}

if (process.argv[1] && process.argv[1].endsWith('first-slot-provider.mjs'))
  serve(Number(process.argv[2] ?? 7783)).then(({ port }) =>
    process.stdout.write(JSON.stringify({ model: FIRST_SLOT_MODEL, listening: `http://127.0.0.1:${port}/v1/chat/completions` }) + '\n'));
