import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Keypair } from '@solana/web3.js';
import { validateIdentity, solanaRpc, frameHash, profileIds, checkProposal, newProfile, registration, joinGame, matchGame, act, apiBase, run, withFaction, fundIfNeeded, http } from './alashi.mjs';

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

test('start rejects overlong UTF-8 name or model before profile and network work', async () => {
  assert.doesNotThrow(() => validateIdentity('a'.repeat(16), 'm'.repeat(128)));
  assert.throws(() => validateIdentity('a'.repeat(17), 'm'), { code: 'invalid_name' });
  assert.throws(() => validateIdentity('😀'.repeat(5), 'm'), { code: 'invalid_name' });
  assert.throws(() => validateIdentity('a', 'm'.repeat(129)), { code: 'invalid_model' });
  const dir = join(tmpdir(), `alashi-invalid-${process.pid}-${Date.now()}`);
  let networkCalls = 0;
  const rpc = { getGenesisHash: async () => { networkCalls++; return DEVNET; } };
  const fetcher = async () => { networkCalls++; throw new Error('unexpected network request'); };
  await assert.rejects(run('start', { '--name': 'NodeBootstrapProbe', '--model': 'model' },
    { dir, rpc, fetcher }), { code: 'invalid_name' });
  assert.equal(networkCalls, 0);
  assert.equal((await import('node:fs')).existsSync(dir), false);
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
    let receipt;
    let matches = 0;
    const fetcher = async (url, init) => {
      assert.equal(url.endsWith('/games'), false, 'implicit discovery must use authenticated match');
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
        receipt = { mode: 'agent_lifecycle_v2', network: 'devnet', wallet: body.wallet,
          signature: body.signature, slot: 1, commitment: 'confirmed' };
        return response({ ok: true, agent_record_id: body.agent_record_id, registration: receipt });
      }
      if (url.endsWith('/agents/match')) {
        const body = JSON.parse(init.body);
        assert.deepEqual(body, { agent_record_id: identity.agent_record_id, recovery_secret: identity.recovery_secret });
        matches++;
        return response(matches === 1 ? { ok: false, error: 'match_wait' } :
          { ok: true, game_id: 42, party_no: 2, phase: 'lobby', joined: false, waiting_for_players: true });
      }
      if (url.endsWith('/game/42/join')) {
        const body = JSON.parse(init.body);
        assert.equal(body.recovery_secret, identity.recovery_secret);
        assert.equal(body.prompt, undefined);
        return response({ ok: true, game_id: 42, party_no: 2, agent_record_id: identity.agent_record_id,
          character_id: profileIds(identity).character, agent_id: body.strategy_hash, token: 'ab'.repeat(32),
          faction_idx: 0, registration: receipt, state: { phase: 'lobby', factions: [{ name: 'agent' }] } });
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
    assert.equal(again.status, 'joined');
    assert.equal(again.waiting_for_players, true);
    assert.equal(again.game_id, 42);
    assert.equal(latestCalls, 1);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('public state identifies the saved player faction', () => {
  const view = withFaction({ ok: true, state: { factions: [{ name: 'other' }, { name: 'mine' }] } },
    { game_id: 7, faction_idx: 1 });
  assert.equal(view.game_id, 7);
  assert.equal(view.your_faction_idx, 1);
  assert.equal(view.your_faction.name, 'mine');
});

test('pinned JSON-RPC client supports every bootstrap RPC method', async () => {
  const { createServer } = await import('node:http');
  const { Connection, PublicKey } = await import('@solana/web3.js');
  const methods = [];
  const server = createServer(async (request, reply) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const input = JSON.parse(Buffer.concat(chunks).toString());
    methods.push(input.method);
    const values = {
      getGenesisHash: DEVNET,
      getBalance: { context: { slot: 1 }, value: 3_000_000 },
      getLatestBlockhash: { context: { slot: 1 }, value: { blockhash: '11111111111111111111111111111111', lastValidBlockHeight: 100 } },
      getSignatureStatuses: { context: { slot: 1 }, value: [null] },
      isBlockhashValid: { context: { slot: 1 }, value: true },
      sendTransaction: '1'.repeat(64),
    };
    reply.setHeader('content-type', 'application/json');
    reply.end(JSON.stringify({ jsonrpc: '2.0', id: input.id, result: values[input.method] }));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    const rpc = new Connection(`http://127.0.0.1:${server.address().port}`, 'confirmed');
    assert.equal(await rpc.getGenesisHash(), DEVNET);
    assert.equal(await rpc.getBalance(new PublicKey('11111111111111111111111111111111')), 3_000_000);
    assert.equal((await rpc.getLatestBlockhash()).lastValidBlockHeight, 100);
    assert.equal((await rpc.getSignatureStatuses(['1'.repeat(64)])).value[0], null);
    assert.equal((await rpc.isBlockhashValid('11111111111111111111111111111111')).value, true);
    assert.equal(await rpc.sendRawTransaction(Buffer.alloc(64)), '1'.repeat(64));
    assert.deepEqual(methods, ['getGenesisHash', 'getBalance', 'getLatestBlockhash',
      'getSignatureStatuses', 'isBlockhashValid', 'sendTransaction']);
  } finally {
    await new Promise(resolve => server.close(resolve));
  }
});


test('signal cleanup removes only this process lock', async () => {
  const { spawn } = await import('node:child_process');
  const { existsSync } = await import('node:fs');
  const dir = mkdtempSync(join(tmpdir(), 'alashi-lock-offline-'));
  const lockPath = join(dir, 'agent.json.lock');
  try {
    const child = spawn(process.execPath, ['--input-type=module', '-e',
      `import { lock } from ${JSON.stringify(new URL('./alashi.mjs', import.meta.url).href)}; lock(${JSON.stringify(join(dir, 'agent.json'))}); process.stdout.write('ready\\n'); setInterval(() => {}, 1000);`],
    { stdio: ['ignore', 'pipe', 'pipe'] });
    await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.stdout.once('data', resolve);
    });
    assert.equal(existsSync(lockPath), true);
    child.kill('SIGINT');
    const code = await new Promise(resolve => child.once('exit', resolve));
    assert.equal(code, 130);
    assert.equal(existsSync(lockPath), false);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('pinned RPC sends one HTTP request on 429', async () => {
  const { createServer } = await import('node:http');
  let requests = 0;
  const server = createServer((_, reply) => {
    requests++;
    reply.writeHead(429, { 'content-type': 'text/plain' });
    reply.end('rate limited');
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    const rpc = solanaRpc(`http://127.0.0.1:${server.address().port}`);
    await assert.rejects(rpc.requestAirdrop(Keypair.generate().publicKey, 10_000_000));
    assert.equal(requests, 1);
  } finally {
    await new Promise(resolve => server.close(resolve));
  }
});

test('failed saved airdrop retries only faucet funding on the same wallet', async () => {
  const profile = newProfile();
  profile.airdrop_signature = 'failed-airdrop';
  const wallet = Keypair.fromSecretKey(Uint8Array.from(profile.secret_key)).publicKey;
  let balance = 0;
  let requests = 0;
  const rpc = {
    getBalance: async () => balance,
    getSignatureStatuses: async ([signature]) => {
      assert.equal(signature, 'failed-airdrop');
      return { value: [{ err: { InstructionError: [0, 'Custom'] } }] };
    },
    requestAirdrop: async address => {
      assert.equal(address.toBase58(), wallet.toBase58());
      requests++;
      balance = 3_000_000;
      return 'new-airdrop';
    },
  };
  await fundIfNeeded(rpc, wallet, profile, () => {}, async () => {});
  assert.equal(profile.airdrop_signature, 'new-airdrop');
  assert.equal(requests, 1);
  assert.equal(profile.signature, null);
});

test('ambiguous saved airdrop gives manual funding path without another request', async () => {
  const profile = newProfile();
  profile.airdrop_signature = 'unknown-airdrop';
  const wallet = Keypair.fromSecretKey(Uint8Array.from(profile.secret_key)).publicKey;
  let requests = 0;
  const rpc = {
    getBalance: async () => 0,
    getSignatureStatuses: async () => ({ value: [null] }),
    requestAirdrop: async () => { requests++; return 'never'; },
  };
  await assert.rejects(fundIfNeeded(rpc, wallet, profile, () => {}, async () => {}), error => {
    assert.equal(error.code, 'airdrop_ambiguous');
    assert.match(error.message, new RegExp(wallet.toBase58()));
    return true;
  });
  assert.equal(requests, 0);
  assert.equal(profile.airdrop_signature, 'unknown-airdrop');
});


test('first faucet RPC failure gives same-wallet recovery without repeat request', async () => {
  const profile = newProfile();
  const wallet = Keypair.fromSecretKey(Uint8Array.from(profile.secret_key)).publicKey;
  let requests = 0;
  const rpc = {
    getBalance: async () => 0,
    requestAirdrop: async () => { requests++; throw new Error('Solana RPC -32603 Internal error'); },
  };
  await assert.rejects(fundIfNeeded(rpc, wallet, profile, () => {}, async () => {}), error => {
    assert.equal(error.code, 'faucet_unavailable');
    assert.match(error.message, new RegExp(wallet.toBase58()));
    assert.doesNotMatch(error.message, /32603/);
    return true;
  });
  assert.equal(requests, 1);
  assert.equal(profile.airdrop_signature, undefined);
  assert.equal(profile.signature, null);
});

test('proxy rate limit and outage are plain errors before non-JSON bodies', async () => {
  const response429 = async () => new Response('<html>rate limit</html>', { status: 429 });
  const response503 = async () => new Response('<html>offline</html>', { status: 503 });
  await assert.rejects(http('https://alashi.network', '/games', undefined, response429), { code: 'rate_limited' });
  await assert.rejects(http('https://alashi.network', '/games', undefined, response503), { code: 'server_unavailable' });
  assert.deepEqual(await http('https://alashi.network', '/agents/registration', {},
    async () => new Response(JSON.stringify({ ok: false, error: 'registration_busy' }), { status: 409 })),
    { ok: false, error: 'registration_busy' });
});


test('matchmaker reports an existing slot and recovery join uses no new Memo', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'alashi-match-offline-'));
  try {
    const p = newProfile();
    p.signature = 'saved-signature';
    p.registration = { mode: 'agent_lifecycle_v2', network: 'devnet', wallet: p.wallet,
      signature: p.signature, commitment: 'confirmed', slot: 77 };
    const matcher = async (url, init) => {
      assert.equal(url, 'https://alashi.network/agents/match');
      assert.deepEqual(JSON.parse(init.body), { agent_record_id: p.agent_record_id,
        recovery_secret: p.recovery_secret });
      return response({ ok: true, game_id: 9, party_no: 1, phase: 'market', joined: true,
        waiting_for_players: false });
    };
    const assignment = await matchGame('https://alashi.network', p, matcher);
    assert.equal(assignment.joined, true);
    let recover;
    const fetcher = async (_, init) => {
      recover = JSON.parse(init.body).recover;
      return response({ ok: true, game_id: 9, agent_record_id: p.agent_record_id,
        character_id: profileIds(p).character, agent_id: frameHash('', ['model', '']),
        token: 'cd'.repeat(32), faction_idx: 0, registration: p.registration });
    };
    await joinGame('https://alashi.network', p, 9, 'agent', 'model', frameHash('', ['model', '']),
      dir, fetcher, assignment.joined);
    assert.equal(recover, true);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
