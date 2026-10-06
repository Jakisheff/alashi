#!/usr/bin/env node
/** Local devnet identity and HTTP gameplay for any coding-agent harness. */
import { createHash, randomBytes } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, openSync, readFileSync, renameSync, writeFileSync, closeSync, unlinkSync, fsyncSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Connection, Keypair, PublicKey, Transaction, TransactionInstruction } from '@solana/web3.js';

const DEVNET_GENESIS = 'EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG';
const MEMO_ID = new PublicKey('MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr');
const RPC_URL = 'https://api.devnet.solana.com';
const DEFAULT_API = 'https://alashi.network';
const solanaRpc = (url = RPC_URL) => new Connection(url, { commitment: 'confirmed', disableRetryOnRateLimit: true });
const BASE58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const hex32 = value => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const randomHex = () => randomBytes(32).toString('hex');
const frameHash = (prefix, parts) => {
  const hash = createHash('sha256').update(prefix);
  for (const part of parts) {
    const bytes = Buffer.from(part, 'utf8');
    const size = Buffer.alloc(8);
    size.writeBigUInt64LE(BigInt(bytes.length));
    hash.update(size).update(bytes);
  }
  return hash.digest('hex');
};
const b58 = bytes => {
  const source = Buffer.from(bytes);
  let n = BigInt('0x' + source.toString('hex'));
  let out = '';
  while (n > 0n) { out = BASE58[Number(n % 58n)] + out; n /= 58n; }
  for (const byte of source) { if (byte !== 0) break; out = '1' + out; }
  return out || '1';
};
const fail = (code, message) => { throw Object.assign(new Error(message), { code }); };
const json = value => process.stdout.write(JSON.stringify(value) + '\n');

function privatePath() {
  const dir = process.env.ALASHI_AGENT_HOME || join(homedir(), '.alashi');
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  if (process.platform !== 'win32' && (lstatSync(dir).mode & 0o077)) fail('private_permissions', 'agent directory must be private (mode 700)');
  return dir;
}
function readPrivate(path) {
  if (!existsSync(path)) return null;
  const stat = lstatSync(path);
  if (!stat.isFile() || (process.platform !== 'win32' && (stat.mode & 0o077))) fail('private_permissions', 'private file must be a regular file with mode 600');
  return JSON.parse(readFileSync(path, 'utf8'));
}
function savePrivate(path, value) {
  const tmp = `${path}.tmp-${process.pid}-${randomBytes(4).toString('hex')}`;
  const fd = openSync(tmp, 'wx', 0o600);
  try { writeFileSync(fd, JSON.stringify(value) + '\n'); fsyncSync(fd); } finally { closeSync(fd); }
  renameSync(tmp, path);
  try { const dirfd = openSync(dirname(path), 'r'); try { fsyncSync(dirfd); } finally { closeSync(dirfd); } } catch { /* Directory fsync is unavailable on some platforms. */ }
}
function lock(path) {
  let fd;
  try { fd = openSync(path + '.lock', 'wx', 0o600); }
  catch { fail('busy', 'another agent process holds the lock; verify it stopped before removing .lock'); }
  writeFileSync(fd, String(process.pid));
  let released = false;
  const release = () => {
    if (released) return;
    released = true;
    process.off('SIGINT', interrupted);
    process.off('SIGTERM', terminated);
    closeSync(fd);
    unlinkSync(path + '.lock');
  };
  const interrupted = () => { release(); process.exit(130); };
  const terminated = () => { release(); process.exit(143); };
  process.once('SIGINT', interrupted);
  process.once('SIGTERM', terminated);
  return release;
}
function apiBase(value) {
  const url = new URL(value || DEFAULT_API);
  if (url.username || url.password || url.search || url.hash || !['https:', 'http:'].includes(url.protocol)) fail('invalid_url', 'use HTTPS without credentials or query');
  if (url.protocol === 'http:' && !['localhost', '127.0.0.1', '[::1]'].includes(url.hostname)) fail('invalid_url', 'remote API requires HTTPS');
  if (url.protocol === 'https:' && url.hostname !== 'alashi.network') fail('invalid_url', 'use the official alashi.network API');
  return url.toString().replace(/\/$/, '');
}
async function http(base, path, body, fetcher = fetch) {
  const response = await fetcher(base + path, { method: body === undefined ? 'GET' : 'POST',
    headers: body === undefined ? {} : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body), redirect: 'manual', signal: AbortSignal.timeout(15000) });
  if (response.status >= 300 && response.status < 400) fail('http_redirect', 'redirect refused for private request');
  if (response.status === 429) fail('rate_limited', 'arena rate limit reached; wait before retrying with the same private profile');
  if ([502, 503, 504].includes(response.status)) fail('server_unavailable', 'arena is temporarily unavailable; retry later with saved progress');
  const value = await response.json();
  if (typeof value !== 'object' || typeof value.ok !== 'boolean') fail('http_response', 'invalid arena JSON');
  return value;
}
function safeError(value) {
  const code = typeof value.error === 'string' ? value.error : value.error?.code;
  return /^[a-zA-Z_]{1,48}$/.test(code || '') ? code : 'arena_rejected';
}
function profileIds(profile) {
  const owner = frameHash('alashi-owner-v1', [profile.recovery_secret]);
  return { owner, character: frameHash('alashi-character-v2', [owner, profile.agent_record_id]) };
}
function checkProposal(profile, value) {
  const { owner, character } = profileIds(profile);
  const challenge = value.memo?.split(':').at(-1);
  const memo = `alashi:agent-lifecycle:v2:devnet:alashi.network:${owner}:${profile.agent_record_id}:${character}:${challenge}`;
  if (!value.ok || value.mode !== 'agent_lifecycle_v2' || value.network !== 'devnet' ||
      value.wallet !== profile.wallet || value.agent_record_id !== profile.agent_record_id ||
      value.owner_id !== owner || value.character_id !== character || value.memo_program_id !== MEMO_ID.toBase58() ||
      !hex32(challenge) || value.memo !== memo) fail('invalid_proposal', 'arena Memo does not match local identity');
  return memo;
}
function publicReceipt(profile, receipt) {
  if (receipt?.mode !== 'agent_lifecycle_v2' || receipt.network !== 'devnet' || receipt.wallet !== profile.wallet ||
      receipt.signature !== profile.signature || receipt.commitment !== 'confirmed') fail('invalid_receipt', 'arena receipt does not match saved signature');
  return { mode: receipt.mode, network: receipt.network, wallet: receipt.wallet,
    signature: receipt.signature, slot: receipt.slot, commitment: receipt.commitment };
}
function newProfile() {
  const key = Keypair.generate();
  return { schema: 'alashi.bootstrap.v1', wallet: key.publicKey.toBase58(),
    secret_key: Array.from(key.secretKey), agent_record_id: randomHex(), recovery_secret: randomHex(),
    memo: null, signature: null, signed_tx: null, blockhash: null, last_valid_block_height: null, registration: null };
}
function keypair(profile) {
  const key = Keypair.fromSecretKey(Uint8Array.from(profile.secret_key));
  if (key.publicKey.toBase58() !== profile.wallet) fail('invalid_profile', 'wallet secret does not match public key');
  return key;
}
async function devnet(rpc) {
  if (await rpc.getGenesisHash() !== DEVNET_GENESIS) fail('not_solana_devnet', 'RPC is not Solana devnet');
}
async function fundIfNeeded(rpc, wallet, profile, save, sleep = delay) {
  if (await rpc.getBalance(wallet, 'confirmed') >= 2_000_000) return;
  if (profile.airdrop_signature) {
    const status = (await rpc.getSignatureStatuses([profile.airdrop_signature],
      { searchTransactionHistory: true })).value[0];
    if (status?.err) {
      // A failed faucet transfer can be replaced; the identity Memo remains untouched.
      profile.airdrop_signature = null;
      save();
    }
  }
  if (!profile.airdrop_signature) {
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        profile.airdrop_signature = await rpc.requestAirdrop(wallet, 10_000_000);
        save();
        break;
      } catch (error) {
        if (!/429|rate.?limit|too many requests/i.test(String(error)))
          fail('faucet_unavailable', `devnet faucet is unavailable; retry later with the same private profile, or fund public wallet ${wallet.toBase58()} manually on devnet`);
        if (attempt === 1) fail('faucet_rate_limited', 'devnet faucet returned 429; retry later with the same profile');
        await sleep(2000);
      }
    }
  }
  for (let poll = 0; poll < 10; poll++) {
    if (await rpc.getBalance(wallet, 'confirmed') >= 2_000_000) return;
    await sleep(1000);
  }
  fail('airdrop_ambiguous', `devnet airdrop is unconfirmed; fund public wallet ${wallet.toBase58()} manually on devnet, then rerun with the same profile`);
}
async function chainStatus(rpc, signature) {
  const status = (await rpc.getSignatureStatuses([signature], { searchTransactionHistory: true })).value[0];
  if (status?.err) fail('registration_failed', 'saved registration transaction failed; manual review required');
  return status && ['confirmed', 'finalized'].includes(status.confirmationStatus);
}
async function registration(base, profile, save, rpc, fetcher = fetch) {
  if (profile.registration) return publicReceipt(profile, profile.registration);
  await devnet(rpc);
  const wallet = keypair(profile);
  if (!profile.memo) {
    const proposal = await http(base, '/agents/registration', {
      agent_record_id: profile.agent_record_id, wallet: profile.wallet, recovery_secret: profile.recovery_secret }, fetcher);
    if (!proposal.ok) fail(safeError(proposal), 'registration proposal rejected');
    if (proposal.registration && !profile.signature) fail('registration_exists', 'server has a receipt but local signature is absent; manual review required');
    profile.memo = checkProposal(profile, proposal);
    save();
  }
  if (!profile.signature) {
    await fundIfNeeded(rpc, wallet.publicKey, profile, save);
    const latest = await rpc.getLatestBlockhash('confirmed');
    const tx = new Transaction({ feePayer: wallet.publicKey, recentBlockhash: latest.blockhash });
    tx.add(new TransactionInstruction({ programId: MEMO_ID,
      keys: [{ pubkey: wallet.publicKey, isSigner: true, isWritable: false }], data: Buffer.from(profile.memo) }));
    tx.sign(wallet);
    profile.signature = b58(tx.signature);
    profile.signed_tx = tx.serialize().toString('base64');
    profile.blockhash = latest.blockhash;
    profile.last_valid_block_height = latest.lastValidBlockHeight;
    save(); // A crash from here onward never creates another signed Memo.
  }
  let confirmed = await chainStatus(rpc, profile.signature);
  if (!confirmed) {
    if (!(await rpc.isBlockhashValid(profile.blockhash, 'confirmed')).value) fail('registration_unknown', 'saved signature is unresolved and blockhash expired; manual review required');
    try { await rpc.sendRawTransaction(Buffer.from(profile.signed_tx, 'base64'), { maxRetries: 2 }); }
    catch (error) {
      if (!await chainStatus(rpc, profile.signature)) fail('registration_unknown', 'saved transaction outcome is unknown; retry checks the same signature');
    }
    confirmed = await chainStatus(rpc, profile.signature);
    if (!confirmed) {
      try { await rpc.confirmTransaction({ signature: profile.signature, blockhash: profile.blockhash,
        lastValidBlockHeight: profile.last_valid_block_height }, 'confirmed'); }
      catch { /* Confirm via status below; never sign again. */ }
      confirmed = await chainStatus(rpc, profile.signature);
    }
    if (!confirmed) fail('registration_unknown', 'saved signature is not confirmed; retry checks the same signature');
  }
  const result = await http(base, '/agents/confirm', {
    agent_record_id: profile.agent_record_id, recovery_secret: profile.recovery_secret,
    wallet: profile.wallet, signature: profile.signature }, fetcher);
  if (!result.ok) fail(safeError(result), 'arena did not confirm saved signature');
  profile.registration = publicReceipt(profile, result.registration);
  save();
  return profile.registration;
}
function readOptions(argv) {
  const options = {};
  for (let i = 0; i < argv.length; i += 2) {
    if (!argv[i]?.startsWith('--') || !argv[i + 1] || options[argv[i]]) fail('usage', 'options must be --name value pairs');
    options[argv[i]] = argv[i + 1];
  }
  return options;
}
function sessionPath(dir, game) { return join(dir, `game-${game}.json`); }
function sanitize(value, secrets) {
  if (Array.isArray(value)) return value.map(v => sanitize(v, secrets));
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value)
    .filter(([k]) => !['token', 'recovery_secret', 'secret', 'secret_key', 'prompt', 'key', 'keypair'].includes(k))
    .map(([k, v]) => [k, sanitize(v, secrets)]));
  if (typeof value === 'string') {
    for (const secret of secrets.filter(Boolean)) value = value.replaceAll(secret, '[redacted]');
  }
  return value;
}
async function joinGame(base, profile, game, name, model, strategyHash, dir, fetcher = fetch, recover = false) {
  const path = sessionPath(dir, game);
  let session = readPrivate(path);
  if (session && (session.agent_record_id !== profile.agent_record_id || session.strategy_hash !== strategyHash))
    fail('session_mismatch', 'game session belongs to a different agent or strategy');
  if (!session) {
    session = { schema: 'alashi.game.v2', game_id: game, agent_record_id: profile.agent_record_id,
      character_id: profileIds(profile).character, strategy_hash: strategyHash, name, model,
      token: null, next_op_id: 1, pending_act: null };
    savePrivate(path, session);
  }
  const body = { agent_record_id: profile.agent_record_id, recovery_secret: profile.recovery_secret,
    name, model, strategy_hash: strategyHash, ...(session.token || recover ? { recover: true } : {}) };
  let result = await http(base, `/game/${game}/join`, body, fetcher);
  if (!result.ok && safeError(result) === 'already_joined') result = await http(base, `/game/${game}/join`, { ...body, recover: true }, fetcher);
  if (!result.ok) fail(safeError(result), 'join rejected');
  if (result.game_id !== game || result.agent_record_id !== profile.agent_record_id ||
      result.character_id !== session.character_id || result.agent_id !== strategyHash ||
      !hex32(result.token)) fail('invalid_join', 'arena join identity mismatch');
  publicReceipt(profile, result.registration);
  session.token = result.token;
  session.faction_idx = result.faction_idx;
  savePrivate(path, session);
  return sanitize(result, [profile.recovery_secret, result.token]);
}
async function matchGame(base, profile, fetcher = fetch) {
  const result = await http(base, '/agents/match', {
    agent_record_id: profile.agent_record_id, recovery_secret: profile.recovery_secret }, fetcher);
  if (!result.ok && ['arena_full', 'match_wait'].includes(safeError(result))) return null;
  if (!result.ok) fail(safeError(result), 'matchmaking unavailable');
  if (!Number.isSafeInteger(result.game_id) || result.game_id < 0 ||
      !Number.isSafeInteger(result.party_no) || typeof result.phase !== 'string' ||
      typeof result.joined !== 'boolean' || typeof result.waiting_for_players !== 'boolean')
    fail('invalid_match', 'arena returned an invalid match assignment');
  return result;
}
function actionBody(raw) {
  const value = JSON.parse(raw);
  if (!value || typeof value !== 'object' || Array.isArray(value) ||
      Object.keys(value).some(k => !['action', 'params', 'by'].includes(k)) || typeof value.action !== 'string' ||
      (value.params !== undefined && (!value.params || typeof value.params !== 'object' || Array.isArray(value.params))) ||
      (value.by !== undefined && !['llm', 'heuristic', 'unknown'].includes(value.by))) fail('invalid_action', 'expected action, optional params object and by');
  return { action: value.action, by: value.by ?? 'llm', params: value.params ?? {} };
}
async function act(base, session, path, raw, fetcher = fetch) {
  if (!session.token) fail('not_joined', 'run start to join first');
  let pending = session.pending_act;
  if (pending) {
    if (raw && JSON.stringify(actionBody(raw)) !== JSON.stringify(pending.action)) fail('pending_action', 'retry saved action before choosing another');
  } else {
    if (!raw) fail('no_pending_action', 'no saved action to retry');
    pending = { op_id: session.next_op_id, action: actionBody(raw) };
    session.pending_act = pending;
    savePrivate(path, session);
  }
  const result = await http(base, `/game/${session.game_id}/act`, {
    token: session.token, op_id: pending.op_id, ...pending.action }, fetcher);
  if (result.op_id === pending.op_id && result.op_consumed === true) {
    session.pending_act = null;
    session.next_op_id++;
    savePrivate(path, session);
  }
  return sanitize(result, [session.token]);
}
function withFaction(result, session) {
  return { ...result, game_id: session.game_id, your_faction_idx: session.faction_idx,
    your_faction: result.state?.factions?.[session.faction_idx] ?? null };
}
async function run(command, options, deps = {}) {
  const dir = deps.dir || privatePath();
  const path = join(dir, 'agent.json');
  const release = lock(path);
  try {
    const base = apiBase(options['--url'] || DEFAULT_API);
    if (command === 'start') {
      const name = options['--name'];
      const model = options['--model'];
      if (!name || !model) fail('usage', 'start needs --name and --model');
      let profile = readPrivate(path);
      if (!profile) { profile = newProfile(); savePrivate(path, profile); }
      if (profile.schema !== 'alashi.bootstrap.v1' || !hex32(profile.agent_record_id) || !hex32(profile.recovery_secret)) fail('invalid_profile', 'private agent profile invalid');
      const rpc = deps.rpc || solanaRpc();
      const receipt = await registration(base, profile, () => savePrivate(path, profile), rpc, deps.fetcher);
      const watch = `https://alashi.network/?agent=${profile.agent_record_id}`;
      const strategy = options['--strategy-file'] ? readFileSync(options['--strategy-file'], 'utf8') : '';
      const strategyHash = frameHash('', [model, strategy]);
      const match = options['--game'] ? null : await matchGame(base, profile, deps.fetcher);
      const game = options['--game'] ? Number(options['--game']) : match?.game_id ?? null;
      if (game === null) return { ok: true, status: 'waiting_for_game', agent_record_id: profile.agent_record_id,
        character_id: profileIds(profile).character, wallet: profile.wallet, registration: receipt,
        identity_url: watch, watch_url: watch };
      if (!Number.isSafeInteger(game) || game < 0) fail('invalid_game', 'game ID must be a nonnegative integer');
      const joined = await joinGame(base, profile, game, name, model, strategyHash, dir, deps.fetcher, match?.joined === true);
      return { ...joined, status: 'joined', waiting_for_players: match?.waiting_for_players ?? joined.state?.phase === 'lobby', watch_url: watch,
        game_url: `https://alashi.network/?api=&game=${game}` };
    }
    const game = Number(options['--game']);
    if (!Number.isSafeInteger(game) || game < 0) fail('usage', 'state/act/retry need --game ID');
    const gamePath = sessionPath(dir, game);
    const session = readPrivate(gamePath);
    if (!session || session.schema !== 'alashi.game.v2') fail('not_joined', 'run start to join first');
    if (command === 'state') return withFaction(sanitize(await http(base, `/game/${game}/state`, undefined, deps.fetcher), [session.token]), session);
    if (command === 'act') return withFaction(await act(base, session, gamePath, options['--json'], deps.fetcher), session);
    if (command === 'retry') return withFaction(await act(base, session, gamePath, null, deps.fetcher), session);
    fail('usage', 'commands: start, state, act, retry');
  } finally { release(); }
}
if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  run(process.argv[2], readOptions(process.argv.slice(3))).then(json).catch(error => {
    json({ ok: false, error: { code: error.code || 'unavailable', message: error.code ? error.message : 'request failed; private progress preserved' } });
    process.exitCode = 1;
  });
}
export { lock, solanaRpc, frameHash, profileIds, checkProposal, newProfile, registration, joinGame, matchGame, actionBody, act, run, b58, apiBase, withFaction, fundIfNeeded, http };
