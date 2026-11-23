// Detector: egress-mode attested usage must stay a structured object
// (falsify catalogue: type-change-usage-string). No external dependencies:
// mocks fetch, signs a receipt with the shared attest.mjs, and asserts the
// driver returns usage FROM the receipt, not a stringified version.
import { keyPairFromSeed, signReceipt } from '../../egress-proxy/attest.mjs';
import { modelProvider } from '../../agent-bootstrap/strategy-run.mjs';
import assert from 'node:assert/strict';

const SEED = 'd'.repeat(64);
const { priv, pubRaw } = keyPairFromSeed(SEED);
const receipt = {
  receipt_id: '1-1', ts_ms: 1, model_id: 'example', endpoint_host: 'h',
  request_sha256: 'a', response_sha256: 'b', prompt_cache_prefix_sha256: 'c',
  usage: { input_fresh: 20, input_cache_read: 80, input_cache_write: null, output: 10 },
  latency_ms: 5, http_status: 200,
};
const signed = { ...receipt, proxy_sig: signReceipt(receipt, priv) };

process.env.ALASHI_EGRESS_URL = 'http://127.0.0.1:7799/';
process.env.ALASHI_EGRESS_KEY_ENV = 'TEST_PROVIDER_KEY';
process.env.ALASHI_EGRESS_PUBKEY = pubRaw.toString('hex');

globalThis.fetch = async () => new Response(
  JSON.stringify({ id: 'x', choices: [{ message: { content: '{"ok":true}' } }], usage: { prompt_tokens: 999, completion_tokens: 999, total_tokens: 1998 } }),
  { headers: { 'x-alashi-receipt': Buffer.from(JSON.stringify(signed)).toString('base64') } });

const provider = modelProvider({ url: 'http://127.0.0.1:7788/v1/chat/completions', model: 'example' });
const result = await provider.complete([]);

assert.equal(typeof result.usage, 'object', `usage must be an object, got ${typeof result.usage}`);
assert.deepEqual(result.usage, { prompt_tokens: 100, completion_tokens: 10, total_tokens: 110, cached_tokens: 80 });
console.log('detector: egress attested usage is a structured object from the receipt');
