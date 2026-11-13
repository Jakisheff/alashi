import test from 'node:test';
import assert from 'node:assert/strict';
import { isDeepStrictEqual } from 'node:util';
import { chooseAction, bribeTargetOf, L4_MODEL } from './l4-provider.mjs';

const entry = (action, params) => ({ action, params });
const opponents = rows => rows.map(([actor, cash, goods, influence, is_president]) =>
  [actor, cash, goods, influence, is_president]);

test('action phase bribes the richest visible legal target', () => {
  const menu = [entry('pass', {}), entry('produce', {}),
    entry('bribe', { to: 7, amount: 5000000 }), entry('bribe', { to: 12, amount: 5000000 })];
  const prompt = { phase: 'action', stage: 'primary', allowed_actions: menu,
    selected_opponents: opponents([[7, 3000000, 0, 1, false], [12, 40000000, 0, 0, false], [90, 99000000, 0, 0, false]]) };
  assert.deepEqual(chooseAction(prompt), entry('bribe', { to: 12, amount: 5000000 }));
  assert.equal(bribeTargetOf(prompt), 12);
});

test('cash ties broken by smaller actor so the policy is deterministic', () => {
  const menu = [entry('bribe', { to: 12, amount: 5000000 }), entry('bribe', { to: 7, amount: 5000000 })];
  const prompt = { phase: 'action', stage: 'primary', allowed_actions: menu,
    selected_opponents: opponents([[7, 5000000, 0, 0, false], [12, 5000000, 0, 0, false]]) };
  assert.deepEqual(chooseAction(prompt), entry('bribe', { to: 7, amount: 5000000 }));
});

test('target visible only through legal offers, not through visibility alone', () => {
  const menu = [entry('produce', {}), entry('bribe', { to: 7, amount: 5000000 })];
  const prompt = { phase: 'action', stage: 'primary', allowed_actions: menu,
    selected_opponents: opponents([[7, 1000000, 0, 0, false], [12, 90000000, 0, 0, false]]) };
  assert.deepEqual(chooseAction(prompt), entry('bribe', { to: 7, amount: 5000000 }));
});

test('no affordable bribe falls back to produce, then donkey, then first entry', () => {
  const produce = { phase: 'action', stage: 'primary',
    allowed_actions: [entry('pass', {}), entry('produce', {})], selected_opponents: opponents([]) };
  assert.deepEqual(chooseAction(produce), entry('produce', {}));
  const donkey = { ...produce, allowed_actions: [entry('pass', {}), entry('donkey', {})] };
  assert.deepEqual(chooseAction(donkey), entry('donkey', {}));
  const only = { ...produce, allowed_actions: [entry('pass', {})] };
  assert.deepEqual(chooseAction(only), entry('pass', {}));
});

test('market phase sells the largest available quantity to fund bribes', () => {
  const prompt = { phase: 'market', stage: 'primary',
    allowed_actions: [entry('pass', {}), entry('sell', { units: 1 }), entry('sell', { units: 4 }), entry('buy', { units: 1 })],
    selected_opponents: opponents([]) };
  assert.deepEqual(chooseAction(prompt), entry('sell', { units: 4 }));
});

test('law primary abstains and post_vote passes', () => {
  const primary = { phase: 'law', stage: 'primary',
    allowed_actions: [entry('vote_yes', {}), entry('vote_no', {}), entry('vote_abstain', {})], selected_opponents: opponents([]) };
  assert.deepEqual(chooseAction(primary), entry('vote_abstain', {}));
  const post = { phase: 'law', stage: 'post_vote',
    allowed_actions: [entry('veto', {}), entry('pass', {})], selected_opponents: opponents([]) };
  assert.deepEqual(chooseAction(post), entry('pass', {}));
});

test('every phase answer is an exact element of the supplied menu', () => {
  const prompts = [
    { phase: 'action', stage: 'primary', allowed_actions: [entry('pass', {}), entry('bribe', { to: 3, amount: 5000000 })],
      selected_opponents: opponents([[3, 8000000, 0, 0, false]]) },
    { phase: 'market', stage: 'primary', allowed_actions: [entry('pass', {})], selected_opponents: opponents([]) },
    { phase: 'law', stage: 'primary', allowed_actions: [entry('vote_yes', {})], selected_opponents: opponents([]) },
    { phase: 'law', stage: 'post_vote', allowed_actions: [entry('pass', {})], selected_opponents: opponents([]) }];
  for (const prompt of prompts) {
    const answer = chooseAction(prompt);
    assert.ok(prompt.allowed_actions.some(a => isDeepStrictEqual(a, answer)),
      `answer for ${prompt.phase} must come from the menu`);
  }
});

test('http handler answers OpenAI-shaped deterministic replies', async () => {
  const { createServer } = await import('node:http');
  const { handler } = await import('./l4-provider.mjs');
  const server = createServer(handler);
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const menu = [entry('pass', {}), entry('bribe', { to: 5, amount: 5000000 })];
  const messages = [{ role: 'system', content: 'sys' },
    { role: 'user', content: JSON.stringify({ phase: 'action', stage: 'primary', allowed_actions: menu,
      selected_opponents: opponents([[5, 6000000, 0, 0, false]]) }) }];
  try {
    const response = await fetch(`http://127.0.0.1:${server.address().port}/v1/chat/completions`, {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ model: L4_MODEL, messages }) });
    assert.equal(response.status, 200);
    const body = await response.json();
    assert.equal(body.model, L4_MODEL);
    const action = JSON.parse(body.choices[0].message.content);
    assert.deepEqual(action, entry('bribe', { to: 5, amount: 5000000 }));
    assert.equal(body.usage.prompt_tokens + body.usage.completion_tokens, body.usage.total_tokens);
  } finally { server.close(); }
});
