import test from 'node:test';
import assert from 'node:assert/strict';
import { isDeepStrictEqual } from 'node:util';
import { chooseAction as uniform, UNIFORM_MODEL } from './uniform-legal-provider.mjs';
import { chooseAction as firstSlot, FIRST_SLOT_MODEL } from './first-slot-provider.mjs';

const entry = (action, params) => ({ action, params });
const opponents = rows => rows.map(r => [...r]);

test('uniform picks only menu entries and is deterministic per prompt', () => {
  const menu = [entry('pass', {}), entry('produce', {}), entry('bribe', { to: 4, amount: 5000000 })];
  const prompt = { phase: 'action', stage: 'primary', allowed_actions: menu, selected_opponents: opponents([[4, 9000000, 0, 0, false]]) };
  const seen = new Set();
  for (let i = 0; i < 40; i++) {
    const answer = uniform(prompt, () => (i % 3) / 3);
    assert.ok(menu.some(m => isDeepStrictEqual(m, answer)));
    seen.add(JSON.stringify(answer));
  }
  assert.ok(seen.size > 1, 'uniform must vary with the RNG draw');
});

test('first-slot bribes selected_opponents[0] when legal', () => {
  const menu = [entry('produce', {}), entry('bribe', { to: 9, amount: 5000000 }), entry('bribe', { to: 2, amount: 5000000 })];
  const prompt = { phase: 'action', stage: 'primary', allowed_actions: menu,
    selected_opponents: opponents([[2, 100000, 0, 0, false], [9, 99000000, 0, 0, false], [5, 5000000, 0, 0, false]]) };
  assert.deepEqual(firstSlot(prompt), entry('bribe', { to: 2, amount: 5000000 }));
});

test('first-slot ignores a richer later peer: position beats wealth by design', () => {
  const menu = [entry('bribe', { to: 2, amount: 5000000 }), entry('bribe', { to: 9, amount: 5000000 })];
  const prompt = { phase: 'action', stage: 'primary', allowed_actions: menu,
    selected_opponents: opponents([[2, 100000, 0, 0, false], [9, 99000000, 0, 0, false]]) };
  assert.deepEqual(firstSlot(prompt), entry('bribe', { to: 2, amount: 5000000 }));
});

test('first-slot falls back to produce when the first peer has no legal offer', () => {
  const menu = [entry('produce', {}), entry('bribe', { to: 9, amount: 5000000 })];
  const prompt = { phase: 'action', stage: 'primary', allowed_actions: menu,
    selected_opponents: opponents([[2, 100000, 0, 0, false], [9, 99000000, 0, 0, false]]) };
  assert.deepEqual(firstSlot(prompt), entry('produce', {}));
});

test('both providers sell max in market and abstain in law like the L4 arm', () => {
  const market = { phase: 'market', stage: 'primary',
    allowed_actions: [entry('pass', {}), entry('sell', { units: 1 }), entry('sell', { units: 3 })], selected_opponents: opponents([]) };
  assert.deepEqual(firstSlot(market), entry('sell', { units: 3 }));
  const law = { phase: 'law', stage: 'primary',
    allowed_actions: [entry('vote_yes', {}), entry('vote_abstain', {})], selected_opponents: opponents([]) };
  assert.deepEqual(firstSlot(law), entry('vote_abstain', {}));
  const post = { phase: 'law', stage: 'post_vote',
    allowed_actions: [entry('veto', {}), entry('pass', {})], selected_opponents: opponents([]) };
  assert.deepEqual(firstSlot(post), entry('pass', {}));
});

test('http layer of both new providers answers OpenAI-shaped replies', async () => {
  const { createServer } = await import('node:http');
  const providers = await Promise.all([
    (await import('./uniform-legal-provider.mjs')).serve(), (await import('./first-slot-provider.mjs')).serve()]);
  const menu = [entry('pass', {}), entry('produce', {})];
  const messages = [{ role: 'system', content: 's' },
    { role: 'user', content: JSON.stringify({ phase: 'action', stage: 'primary', allowed_actions: menu, selected_opponents: [] }) }];
  try {
    for (const { port } of providers) {
      const response = await fetch(`http://127.0.0.1:${port}/v1/chat/completions`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ model: 'x', messages }) });
      assert.equal(response.status, 200);
      const body = await response.json();
      const action = JSON.parse(body.choices[0].message.content);
      assert.ok(menu.some(m => isDeepStrictEqual(m, action)));
      assert.equal(body.usage.prompt_tokens + body.usage.completion_tokens, body.usage.total_tokens);
    }
  } finally { for (const { server } of providers) server.close(); }
});
