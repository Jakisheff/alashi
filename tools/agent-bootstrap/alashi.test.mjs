import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Keypair } from '@solana/web3.js';
import { frameHash, profileIds, checkProposal, newProfile, registration, joinGame, chooseGame, act, apiBase, run } from './alashi.mjs';

const DEVNET = 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG';
const MEMO = 'MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr';
const response = value => new Response(JSON.stringify(value), { status: 200, headers: { 'content-type': 'application/json' } });

test('v2 golden hashes and proposal validation', () => {
  const p = { wallet: 'wallet', recovery_secret: 'ab'.repeat(32), agent_record_id: 'cd'.repeat(32) };
  const { owner, character } = profileIds(p);
  assert.equal(owner, 'd8d041d59e9d55c61790d37a8e2bc3f17b9c8f4d350062a090ea8b5d64a086fa');
  assert.equal(character, '6d92bd091fb2d69e295fe5bba10caa3628abf2cac55bc80f7c74a018c4465c71');
  assert.equal(frameHash('', ['glm-5.3-flash', 'test']), 'bcf7a4c486fd2c390bdc97df49b4cd019aeb8db20a72fa65a05361f5210c2240');
  const memo = `alashi:agent-lifecycle:v2:devnet:alashi.network:${owner}:${p.agent_record_id}:${character}:${'ef'.repeat(32)}`;
  assert.equal(checkProposal(p, { ok: true, mode: 'agent_lifecycle_v2', network: 'devnet', wallet: p.wallet,
    agent_record_id: p.agent_record_id, owner_id: owner, character_id: character, memo_program_id: MEMO, memo }), memo);
  assert.throws(() => checkProposal(p, { ok: true, memo }), { code: 'invalid_proposal' });
  assert.throws(() => apiBase('https://evil.example'), { code: 'invalid_url' });
});

test('one durable Memo, unknown outcome resumes same signature without signing again', async () => {
  const profile = newProfile();
  const snapshots = [];
  const save = () => snapshots.push(structuredClone(profile));
  const requests = [];
  let confirmed = false;
  let sends = 0;
  let blockhashCalls = 0;
  const blockhash = Keypair.generate().publicKey.toBase58();
  const rpc = {
    getGenesisHash: async () => DEVNET,
    getBalance: async () => 3_000_000,
    getLatestBlockhash: async () => { blockhashCalls++; return { blockhash, lastValidBlockHeight: 123456 }; },
    getSignatureStatuses: async () => ({ value: [{ err: null, confirmationStatus: 'confirmed' }].filter(() => confirmed).concat(confirmed ? [] : [null]) }),
    isBlockhashValid: async () => ({ value: true }),
    sendRawTransaction: async bytes => {
      sends++;
      assert.equal(snapshots.at(-1).signature, profile.signature);
      assert.equal(snapshots.at(-1).signed_tx, Buffer.from(bytes).toString('base64'));
      if (sends === 1) throw new Error('lost response');
      confirmed = true;
      return profile.signature;
    },
    confirmTransaction: async () => ({ value: { err: null } }),
  };
  const fetcher = async (url, init) => {
    const body = JSON.parse(init.body);
    requests.push({ url, body });
    assert.equal(init.redirect, 'manual');
    assert.equal(JSON.stringify(body).includes('secret_key'), false);
    if (url.endsWith('/agents/registration')) {
      const { owner, character } = profileIds(profile);
      return response({ ok: true, mode: 'agent_lifecycle_v2', network: 'devnet', wallet: profile.wallet,
        agent_record_id: profile.agent_record_id, owner_id: owner, character_id: character,
        memo_program_id: MEMO, memo: `alashi:agent-lifecycle:v2:devnet:alashi.network:${owner}:${profile.agent_record_id}:${character}:${'ef'.repeat(32)}`,
        registration: null });
    }
    return response({ ok: true, agent_record_id: profile.agent_record_id, registration: {
      mode: 'agent_lifecycle_v2', network: 'devnet', wallet: profile.wallet, signature: profile.signature,
      slot: 77, commitment: 'confirmed' } });
  };
  await assert.rejects(registration('https://alashi.network', profile, save, rpc, fetcher), { code: 'registration_unknown' });
  const signature = profile.signature;
  assert.ok(signature && profile.signed_tx);
  assert.equal(blockhashCalls, 1);
  confirmed = true;
  const receipt = await registration('https://alashi.network', profile, save, rpc, fetcher);
  assert.equal(receipt.signature, signature);
  assert.equal(blockhashCalls, 1);
  assert.equal(sends, 1);
  assert.deepEqual(requests.map(r => new URL(r.url).pathname), ['/agents/registration', '/agents/confirm']);
});

test('one profile joins two games, no prompt upload, pending op replay is exact', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'alashi-node-offline-'));
  try {
    const p = newProfile();
    p.signature = 'saved-signature';
    p.registration = { mode: 'agent_lifecycle_v2', network: 'devnet', wallet: p.wallet,
      signature: p.signature, commitment: 'confirmed', slot: 77 };
    const strategy = frameHash('', ['model', 'private prompt']);
    const seen = [];
    const fetcher = async (url, init) => {
      const body = JSON.parse(init.body);
      seen.push({ url, body });
      if (url.endsWith('/join')) {
        const game = Number(url.split('/')[4]);
        return response({ ok: true, game_id: game, agent_record_id: p.agent_record_id,
          character_id: profileIds(p).character, agent_id: strategy, token: 'ab'.repeat(32),
          faction_idx: 0, registration: p.registration });
      }
      return response(seen.filter(x => x.url.endsWith('/act')).length === 1
        ? { ok: false, error: 'bad_token', op_id: 1, op_consumed: false }
        : { ok: true, op_id: 1, op_consumed: true });
    };
    for (const game of [41, 42]) await joinGame('https://alashi.network', p, game, 'name', 'model', strategy, dir, fetcher);
    assert.equal(seen.filter(x => x.url.endsWith('/join')).length, 2);
    assert.equal(JSON.stringify(seen).includes('private prompt'), false);
    const path = join(dir, 'game-41.json');
    let session = JSON.parse(readFileSync(path));
    const first = await act('https://alashi.network', session, path, '{"action":"produce"}', fetcher);
    assert.equal(first.op_consumed, false);
    session = JSON.parse(readFileSync(path));
    assert.equal(session.pending_act.op_id, 1);
    await assert.rejects(act('https://alashi.network', session, path, '{"action":"sell"}', fetcher), { code: 'pending_action' });
    const second = await act('https://alashi.network', session, path, null, fetcher);
    assert.equal(second.op_consumed, true);
    assert.deepEqual(seen.filter(x => x.url.endsWith('/act')).map(x => x.body),
      [seen.filter(x => x.url.endsWith('/act'))[0].body, seen.filter(x => x.url.endsWith('/act'))[0].body]);
    assert.equal(JSON.parse(readFileSync(path)).next_op_id, 2);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('no open lobby returns waiting rather than joining', async () => {
  const fetcher = async () => response({ ok: true, games: [{ game_id: 1, phase: 'market', factions: 2 }] });
  assert.equal(await chooseGame('https://alashi.network', fetcher), null);
});

test('start persists one private wallet and returns identity link while no game is open', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'alashi-start-offline-'));
  try {
    let confirmed = false;
    let latestCalls = 0;
    const rpc = {
      getGenesisHash: async () => DEVNET,
      getBalance: async () => 3_000_000,
      getLatestBlockhash: async () => { latestCalls++; return { blockhash: Keypair.generate().publicKey.toBase58(), lastValidBlockHeight: 100 }; },
      getSignatureStatuses: async () => ({ value: [confirmed ? { err: null, confirmationStatus: 'confirmed' } : null] }),
      isBlockhashValid: async () => ({ value: true }),
      sendRawTransaction: async () => { confirmed = true; return 'mock-signature'; },
    };
    let identity;
    const fetcher = async (url, init) => {
      if (url.endsWith('/agents/registration')) {
        const body = JSON.parse(init.body);
        identity = body;
        const { owner, character } = profileIds(body);
        return response({ ok: true, mode: 'agent_lifecycle_v2', network: 'devnet', wallet: body.wallet,
          agent_record_id: body.agent_record_id, owner_id: owner, character_id: character,
          memo_program_id: MEMO, memo: `alashi:agent-lifecycle:v2:devnet:alashi.network:${owner}:${body.agent_record_id}:${character}:${'ef'.repeat(32)}`,
          registration: null });
      }
      if (url.endsWith('/agents/confirm')) {
        const body = JSON.parse(init.body);
        return response({ ok: true, agent_record_id: body.agent_record_id, registration: {
          mode: 'agent_lifecycle_v2', network: 'devnet', wallet: body.wallet,
          signature: body.signature, slot: 1, commitment: 'confirmed' } });
      }
      return response({ ok: true, games: [] });
    };
    const options = { '--name': 'agent', '--model': 'test-model' };
    const result = await run('start', options, { dir, rpc, fetcher });
    assert.equal(result.status, 'waiting_for_game');
    assert.equal(result.watch_url, `https://alashi.network/?agent=${result.agent_record_id}`);
    assert.equal(JSON.stringify(result).includes(identity.recovery_secret), false);
    assert.equal(JSON.stringify(result).includes('secret_key'), false);
    const stat = (await import('node:fs')).statSync(join(dir, 'agent.json'));
    assert.equal(stat.mode & 0o077, 0);
    const again = await run('start', options, { dir, rpc, fetcher });
    assert.equal(again.agent_record_id, result.agent_record_id);
    assert.equal(latestCalls, 1);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
