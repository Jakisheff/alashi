import test from 'node:test';
import assert from 'node:assert/strict';
import { isDeepStrictEqual } from 'node:util';
import { chooseAction, L4_BLIND_MODEL } from './l4-blind-provider.mjs';

const entry = (action, params) => ({ action, params });
const opponents = rows => rows.map(r => [...r]);

test('with a cash signal the blind arm targets the richest like L4', () => {
  const menu = [entry('produce', {}), entry('bribe', { to: 7, amount: 5000000 }), entry('bribe', { to: 12, amount: 5000000 })];
  const prompt = { phase: 'action', stage: 'primary', allowed_actions: menu,
    selected_opponents: opponents([[7, 3000000, 0, 1, false], [12, 40000000, 0, 0, false]]) };
  assert.deepEqual(chooseAction(prompt), entry('bribe', { to: 12, amount: 5000000 }));
});

test('without a cash signal it falls back to a uniform pick among offers', () => {
  const menu = [entry('bribe', { to: 7, amount: 5000000 }), entry('bribe', { to: 12, amount: 5000000 })];
  const prompt = { phase: 'action', stage: 'primary', allowed_actions: menu,
    selected_opponents: opponents([[7, 0, 0, 0, false], [12, 0, 0, 0, false]]) };
  const seen = new Set();
  for (let i = 0; i < 10; i++) {
    const answer = chooseAction(prompt, () => i / 10);
    assert.ok(menu.some(m => isDeepStrictEqual(m, answer)), 'answer must be a menu bribe');
    seen.add(answer.params.to);
  }
  assert.equal(seen.size, 2, 'both targets must be reachable when cash is uninformative');
});

test('non-bribe phases match the L4 policy', () => {
  const market = { phase: 'market', stage: 'primary',
    allowed_actions: [entry('pass', {}), entry('sell', { units: 2 })], selected_opponents: opponents([]) };
  assert.deepEqual(chooseAction(market), entry('sell', { units: 2 }));
  const law = { phase: 'law', stage: 'primary',
    allowed_actions: [entry('vote_yes', {}), entry('vote_abstain', {})], selected_opponents: opponents([]) };
  assert.deepEqual(chooseAction(law), entry('vote_abstain', {}));
});

test('blind driver zeroes the cash channel in selected_opponents', async () => {
  const { createServer } = await import('node:http');
  const { serve } = await import('./l4-blind-provider.mjs');
  const { server, port } = await serve();
  try {
    const menu = [entry('bribe', { to: 5, amount: 5000000 }), entry('bribe', { to: 9, amount: 5000000 })];
    const messages = [{ role: 'system', content: 's' }, { role: 'user', content: JSON.stringify({
      phase: 'action', stage: 'primary', allowed_actions: menu,
      selected_opponents: opponents([[5, 0, 0, 0, false], [9, 0, 0, 0, false]]) }) }];
    const response = await fetch(`http://127.0.0.1:${port}/v1/chat/completions`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ model: L4_BLIND_MODEL, messages }) });
    const body = await response.json();
    const action = JSON.parse(body.choices[0].message.content);
    assert.ok(menu.some(m => isDeepStrictEqual(m, action)));
  } finally { server.close(); }
});
