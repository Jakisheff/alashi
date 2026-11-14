#!/usr/bin/env node
// L4-blind arm for the preregistered wealth-visibility ablation
// (docs/200_ZETTEL/20261113-blind-ablation-prereg.md): identical target
// logic to l4-provider (richest visible legal target, ties by smaller
// actor), except that when the visible cash carries no signal (all
// offered targets show the same cash, as under the blind driver), it
// falls back to a uniform pick among the offered targets, deterministic
// per prompt hash. Everything else matches the L4 policy. Every answer
// is an exact element of allowed_actions; source=test_provider.
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';

export const L4_BLIND_MODEL = 'l4-blind-v1';

function rng(message) {
  let state = BigInt('0x' + createHash('sha256').update(message).digest('hex').slice(0, 16));
  return () => {
    state = (state * 6364136223846793005n + 1442695040888963407n) & 0xffffffffffffffffn;
    return Number(state >> 11n) / 2 ** 53;
  };
}

const cashOf = (selected, actor) => {
  const row = (selected ?? []).find(candidate => candidate[0] === actor);
  return row === undefined ? -1 : row[1];
};

const firstOf = (menu, action, better) => menu.filter(a => a.action === action)
  .reduce((best, current) => best === undefined || better(current, best) ? current : best, undefined);

export function chooseAction(prompt, random = () => 0) {
  const menu = prompt?.allowed_actions;
  if (!Array.isArray(menu) || !menu.length) return null;
  if (prompt.phase === 'action' && prompt.stage !== 'post_vote') {
    const offers = menu.filter(a => a.action === 'bribe');
    if (offers.length) {
      const cashes = offers.map(o => cashOf(prompt.selected_opponents, o.params.to));
      const min = Math.min(...cashes), max = Math.max(...cashes);
      let pick;
      if (max > min) {
        pick = offers[0];
        for (const offer of offers) {
          const cash = cashOf(prompt.selected_opponents, offer.params.to);
          if (cash > cashOf(prompt.selected_opponents, pick.params.to)
            || cash === cashOf(prompt.selected_opponents, pick.params.to) && offer.params.to < pick.params.to)
            pick = offer;
        }
      } else pick = offers[Math.floor(random() * offers.length) % offers.length];
      return structuredClone(pick);
    }
    return structuredClone(firstOf(menu, 'produce', () => true)
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
  response.end(JSON.stringify({ id: 'l4-blind-deterministic', model: L4_BLIND_MODEL,
    choices: [{ message: { role: 'assistant', content }, finish_reason: 'stop' }], usage }));
}

export function serve(port = 0) {
  const server = createServer(handler);
  return new Promise(resolve => server.listen(port, '127.0.0.1', () =>
    resolve({ server, port: server.address().port })));
}

if (process.argv[1] && process.argv[1].endsWith('l4-blind-provider.mjs'))
  serve(Number(process.argv[2] ?? 7784)).then(({ port }) =>
    process.stdout.write(JSON.stringify({ model: L4_BLIND_MODEL, listening: `http://127.0.0.1:${port}/v1/chat/completions` }) + '\n'));
