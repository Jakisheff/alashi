#!/usr/bin/env node
// verify.mjs: outside-judge check for an egress-proxy receipt journal.
// Verifies every receipt signature against the proxy pubkey, refuses
// journals whose lines are not receipts, and totals attested usage.
//   node verify.mjs <journal.jsonl> [--pubkey <64-hex> | seed env ALASHI_EGRESS_SEED]
// Exit 0 = every line verifies. Exit 1 = tampered, malformed or unsigned.

import { readJournalLines } from './attest.mjs';
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

const lines = readJournalLines(journal);
// Outside-judge fixes (hy4 falsification): an empty journal proves nothing,
// a replayed line must not count twice, and unknown fields are not covered
// by the signature, so they fail the strict field set.
if (lines.length === 0) {
  console.error('FAIL empty_journal: no receipts to verify');
  process.exit(1);
}
let bad = 0, totalTokens = 0, models = {};
const seenIds = new Set();
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
  const extra = Object.keys(JSON.parse(line)).filter(k => k !== 'proxy_sig' && !REQUIRED_RECEIPT_FIELDS.includes(k));
  if (extra.length) { console.error(`FAIL ${r.receipt_id}: unsigned extra field(s) ${extra.join(',')}`); bad += 1; }
  if (seenIds.has(r.receipt.receipt_id)) { console.error(`FAIL ${r.receipt.receipt_id}: duplicate receipt_id (replayed line)`); bad += 1; }
  seenIds.add(r.receipt.receipt_id);
  const u = r.receipt.usage;
  if (u) {
    totalTokens += (u.output ?? 0) + (u.input_fresh ?? 0) + (u.input_cache_read ?? 0) + (u.input_cache_write ?? 0);
    models[r.receipt.model_id] = (models[r.receipt.model_id] ?? 0) + 1;
  }
}
console.log(JSON.stringify({ receipts: lines.length, verified: lines.length - bad, failed: bad, attested_tokens: totalTokens, models }, null, 2));
process.exit(bad ? 1 : 0);
