// alashi egress-proxy attestation library (Diff 2, Zero-Trust RFC v2).
// Inference receipts: every provider call that leaves through the proxy is
// reduced to a signed record, so "model usage" stops being owner-submitted
// prose inside experiment journals and becomes an attestable trail.
// No external dependencies: node:crypto only.

import { createHash, createPrivateKey, createPublicKey, sign as edSign, verify as edVerify } from 'node:crypto';
// Deterministic Ed25519 keypair from a 32-byte seed (hex). PKCS8 DER prefix
// for ed25519 + seed is the standard encoding, so the same seed always
// produces the same identity for receipt signing and verification.
const ED25519_PKCS8_PREFIX = Buffer.from('302e020100300506032b657004220420', 'hex');

export function keyPairFromSeed(seedHex) {
  const seed = Buffer.from(seedHex, 'hex');
  if (seed.length !== 32) throw new Error('seed must be 32 bytes hex');
  const priv = createPrivateKey({ key: Buffer.concat([ED25519_PKCS8_PREFIX, seed]), format: 'der', type: 'pkcs8' });
  const pub = createPublicKey(priv);
  const spki = pub.export({ type: 'spki', format: 'der' });
  const pubRaw = spki.subarray(spki.length - 32); // last 32 bytes for ed25519
  return { priv, pub, pubRaw };
}

// Wrap a raw 32-byte ed25519 public key back into a KeyObject for verify():
// the 12-byte SPKI prefix is the standard RFC 8410 header.
const ED25519_SPKI_PREFIX = Buffer.from('302a300506032b6570032100', 'hex');
export function pubKeyFromRaw(pubRaw) {
  if (pubRaw.length !== 32) throw new Error('pubkey must be 32 bytes');
  return createPublicKey({ key: Buffer.concat([ED25519_SPKI_PREFIX, pubRaw]), format: 'der', type: 'spki' });
}

export function sha256Hex(data) {
  return createHash('sha256').update(data).digest('hex');
}

// OpenAI-compatible and Anthropic-style usage fields collapse into the
// receipt shape. Values the provider did not report stay null, never 0:
// a missing number must not be laundered into an attested zero.
export function normalizeUsage(usage) {
  if (!usage || typeof usage !== 'object') return null;
  // Falsification (anonymous audit): numbers must be finite NON-NEGATIVE
  // integers within a sane bound; a provider reply of -500 would deflate
  // the budget and 1e300 would brick it forever, both under a valid
  // signature.
  const num = v => (typeof v === 'number' && Number.isSafeInteger(v) && v >= 0 && v <= 1e12 ? v : null);
  const output = num(usage.completion_tokens);
  const prompt = num(usage.prompt_tokens);
  const cached = num(usage.prompt_tokens_details?.cached_tokens) ?? num(usage.cache_read_input_tokens);
  const cacheWrite = num(usage.cache_creation_input_tokens) ?? num(usage.prompt_tokens_details?.cache_write_input_tokens);
  // Fresh input is whatever the provider charged beyond cache hits; a plain
  // prompt_tokens with no cache breakdown is fully fresh input.
  let inputFresh = null;
  if (prompt !== null) {
    inputFresh = prompt - (cached ?? 0) - (cacheWrite ?? 0);
    if (inputFresh < 0) inputFresh = null;
  }
  return { input_fresh: inputFresh, input_cache_read: cached, input_cache_write: cacheWrite, output };
}

// Hash of the immutable context prefix the provider is expected to cache:
// model id + first system message. Dialogue turns and world events live in
// the append-only tail and must not change this hash (cache-busting DoS).
export function promptCachePrefixSha256(model, body) {
  const messages = Array.isArray(body?.messages) ? body.messages : [];
  const system = messages.find(m => m && m.role === 'system');
  const content = typeof system?.content === 'string' ? system.content : '';
  return sha256Hex(`${model}\u0000${content}`);
}

// Canonical, order-stable receipt encoding: the signature covers exactly
// these bytes, so any field shuffle breaks verification.
export function canonicalReceipt(r) {
  return JSON.stringify({
    receipt_id: r.receipt_id,
    ts_ms: r.ts_ms,
    model_id: r.model_id,
    endpoint_host: r.endpoint_host,
    request_sha256: r.request_sha256,
    response_sha256: r.response_sha256,
    prompt_cache_prefix_sha256: r.prompt_cache_prefix_sha256,
    usage: r.usage,
    latency_ms: r.latency_ms,
    http_status: r.http_status,
  });
}

export function signReceipt(receipt, priv) {
  return edSign(null, Buffer.from(canonicalReceipt(receipt), 'utf8'), priv).toString('base64');
}

export function verifyReceipt(line, pubRaw) {
  let parsed;
  try { parsed = JSON.parse(line); } catch { return { ok: false, reason: 'bad_json' }; }
  const { proxy_sig, ...receipt } = parsed;
  if (typeof proxy_sig !== 'string') return { ok: false, reason: 'missing_signature', receipt_id: receipt.receipt_id };
  const pub = Buffer.isBuffer(pubRaw) ? pubKeyFromRaw(pubRaw) : pubRaw;
  const ok = edVerify(null, Buffer.from(canonicalReceipt(receipt), 'utf8'), pub, Buffer.from(proxy_sig, 'base64'));
  return { ok, reason: ok ? null : 'signature_mismatch', receipt_id: receipt.receipt_id ?? null, receipt: ok ? receipt : null };
}

export const REQUIRED_RECEIPT_FIELDS = [
  'receipt_id', 'ts_ms', 'model_id', 'endpoint_host', 'request_sha256',
  'response_sha256', 'prompt_cache_prefix_sha256', 'usage', 'latency_ms', 'http_status',
];

// Receipt journal reader: transparently supports the legacy JSONL file and
// the current sqlite store (receipts.jsonl.db).
import { readFileSync } from 'node:fs';
import { DatabaseSync } from 'node:sqlite';
export function readJournalLines(journalPath) {
  try {
    return readFileSync(journalPath, 'utf8').split('\n').filter(l => l.trim());
  } catch (e) {
    if (e.code !== 'ENOENT') throw e;
  }
  const db = new DatabaseSync(journalPath + '.db', { readOnly: true });
  try {
    return db.prepare('SELECT receipt_json FROM receipts ORDER BY rowid').all()
      .map(r => r.receipt_json);
  } finally {
    db.close();
  }
}
