#!/usr/bin/env node
// L4 adversary for the red-team ladder (docs/200_ZETTEL/20261113-red-team-ladder.md):
// a legal-only concentration attacker. It answers from the supplied
// allowed_actions menu only, so an out-of-menu decision is impossible by
// construction. Strategy: bribe the richest visible peer whenever a legal
// bribe exists; otherwise produce goods, sell everything to fund later
// bribes, abstain in laws and pass after votes. Served as a loopback
// OpenAI-compatible provider for world-run.mjs; every call is
// deterministic and offline. Usage counts are synthetic byte lengths, not
// model tokens: the frozen profile must use source=test_provider.
import { createServer } from 'node:http';
import { isDeepStrictEqual } from 'node:util';

export const L4_MODEL = 'l4-legal-concentration-v1';

const cashOf = (selected, actor) => {
  const row = selected.find(candidate => candidate[0] === actor);
  return row === undefined ? -1 : row[1];
};

// The richest visible peer that also has a legal bribe offer in the menu.
// Ties break by smaller actor, keeping the policy deterministic.
export function bribeTargetOf(prompt) {
  const offers = prompt.allowed_actions.filter(a => a.action === 'bribe');
  if (!offers.length) return null;
  let best = null;
  for (const offer of offers) {
    const cash = cashOf(prompt.selected_opponents ?? [], offer.params.to);
    if (best === null || cash > best.cash || cash === best.cash && offer.params.to < best.actor)
      best = { actor: offer.params.to, cash };
  }
  return best.actor;
}

const firstOf = (menu, action, better) => menu.filter(a => a.action === action)
  .reduce((best, current) => best === undefined || better(current, best) ? current : best, undefined);

export function chooseAction(prompt) {
  const menu = prompt.allowed_actions;
  if (!Array.isArray(menu) || !menu.length) return null;
  if (prompt.phase === 'action' && prompt.stage !== 'post_vote') {
    const target = bribeTargetOf(prompt);
    if (target !== null)
      return structuredClone(menu.find(a => a.action === 'bribe' && a.params.to === target));
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
  const payload = { id: 'l4-deterministic', model: L4_MODEL,
    choices: [{ message: { role: 'assistant', content }, finish_reason: 'stop' }], usage };
  response.writeHead(content === null ? 500 : 200, { 'Content-Type': 'application/json' });
  response.end(JSON.stringify(payload));
}

export function serve(port = 0) {
  const server = createServer(handler);
  return new Promise(resolve => server.listen(port, '127.0.0.1', () =>
    resolve({ server, port: server.address().port })));
}

if (process.argv[1] && process.argv[1].endsWith('l4-provider.mjs'))
  serve(Number(process.argv[2] ?? 7780)).then(({ server, port }) =>
    process.stdout.write(JSON.stringify({ model: L4_MODEL, listening: `http://127.0.0.1:${port}/v1/chat/completions` }) + '\n'));
