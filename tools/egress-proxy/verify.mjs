#!/usr/bin/env node
// verify.mjs: outside-judge check for an egress-proxy receipt journal.
// Verifies every receipt signature against the proxy pubkey, refuses
// journals whose lines are not receipts, and totals attested usage.
//   node verify.mjs <journal.jsonl> [--pubkey <64-hex> | seed env ALASHI_EGRESS_SEED]
// Exit 0 = every line verifies. Exit 1 = tampered, malformed or unsigned.

import { readFileSync } from 'node:fs';
import { verifyReceipt, keyPairFromSeed, REQUIRED_RECEIPT_FIELDS } from './attest.mjs';

const args = process.argv.slice(2);
const journal = args[0];
if (!journal) {
  console.error('usage: node verify.mjs <journal.jsonl> [--pubkey <64-hex>]');
  process.exit(2);
}
const pubArg = args.includes('--pubkey') ? args[args.indexOf('--pubkey') + 1] : null;
let pubRaw;
if (pubArg) {
  pubRaw = Buffer.from(pubArg, 'hex');
  if (pubRaw.length !== 32) { console.error('bad --pubkey: expected 64 hex chars'); process.exit(2); }
} else if (process.env.ALASHI_EGRESS_SEED) {
  pubRaw = keyPairFromSeed(process.env.ALASHI_EGRESS_SEED).pubRaw;
} else {
  console.error('no pubkey: pass --pubkey or set ALASHI_EGRESS_SEED');
  process.exit(2);
}

const lines = readFileSync(journal, 'utf8').split('\n').filter(l => l.trim());
let bad = 0, totalTokens = 0, models = {};
for (const line of lines) {
  const r = verifyReceipt(line, pubRaw);
  if (!r.ok) {
    console.error(`FAIL ${r.receipt_id ?? '?'}: ${r.reason}`);
    bad += 1;
    continue;
  }
  for (const f of REQUIRED_RECEIPT_FIELDS) {
    if (!(f in r.receipt)) { console.error(`FAIL ${r.receipt_id}: missing ${f}`); bad += 1; }
  }
  const u = r.receipt.usage;
  if (u) {
    totalTokens += (u.output ?? 0) + (u.input_fresh ?? 0) + (u.input_cache_read ?? 0) + (u.input_cache_write ?? 0);
    models[r.receipt.model_id] = (models[r.receipt.model_id] ?? 0) + 1;
  }
}
console.log(JSON.stringify({ receipts: lines.length, verified: lines.length - bad, failed: bad, attested_tokens: totalTokens, models }, null, 2));
process.exit(bad ? 1 : 0);
