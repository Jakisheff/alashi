import test from 'node:test';
import assert from 'node:assert/strict';
import { verdictOf, liarClaims, snapshotFixture } from './lie-ledger.mjs';

// Protocol fixture: synthetic 6-round world snapshots in the exact field
// vocabulary world-run snapshots carry (laws_passed, veto_pending,
// market_price, public_state rows). Not a second implementation of rules.

test('negative control: truthful claims all verify as true', () => {
  const snaps = snapshotFixture();
  const claims = liarClaims(snaps, { mode: 'truth' });
  assert.ok(claims.length >= 12, `expected >=12 claims, got ${claims.length}`);
  for (const v of verdictOf(claims, snaps)) {
    assert.equal(v.verdict, 'true', `truthful claim flagged: ${JSON.stringify(v.claim)}`);
    assert.ok(v.evidence, 'true verdict must cite evidence');
  }
});

test('positive control: lying claims all caught as false (100%)', () => {
  const snaps = snapshotFixture();
  const claims = liarClaims(snaps, { mode: 'lie' });
  assert.ok(claims.length >= 12, `expected >=12 claims, got ${claims.length}`);
  const verdicts = verdictOf(claims, snaps);
  const caught = verdicts.filter(v => v.verdict === 'false');
  assert.equal(caught.length, verdicts.length, 'every lie must be caught');
  for (const v of caught) assert.ok(v.evidence, 'false verdict must cite evidence');
});

test('salient lies are lies too: salience mode fully caught', () => {
  const snaps = snapshotFixture();
  const claims = liarClaims(snaps, { mode: 'salience' });
  const verdicts = verdictOf(claims, snaps);
  assert.ok(claims.length >= 6);
  for (const v of verdicts) assert.equal(v.verdict, 'false', `salient lie missed: ${JSON.stringify(v)}`);
});

test('unverifiable: claims about missing rounds or unknown subjects are not called lies', () => {
  const snaps = snapshotFixture();
  const claims = [
    { type: 'law_passed', round: 99, law: 'tariff', value: true, text: 'закон прошёл в раунде 99' },
    { type: 'aura', round: 2, value: true, text: 'непонятно о чём' },
    { type: 'cash', round: 2, actor: 42, op: 'ge', value: 100, text: 'у фракции 42 много денег' },
  ];
  assert.deepEqual(verdictOf(claims, snaps).map(v => v.verdict), ['unverifiable', 'unverifiable', 'unverifiable']);
});

test('round semantics: a claim about a round beyond the history is unverifiable, about a recorded round is decidable', () => {
  const snaps = snapshotFixture(); // rounds 1..6 recorded
  const future = verdictOf([{ type: 'law_passed', round: 7, law: 'tariff', value: true, text: 'x' }], snaps);
  assert.equal(future[0].verdict, 'unverifiable'); // no snapshot for round 7
  const past = verdictOf([{ type: 'law_passed', round: 3, law: 'tariff', value: true, text: 'x' }], snaps);
  assert.equal(past[0].verdict, 'true'); // tariff is in round-3 end snapshot
  const pastLie = verdictOf([{ type: 'law_passed', round: 3, law: 'tariff', value: false, text: 'x' }], snaps);
  assert.equal(pastLie[0].verdict, 'false'); // negation of a recorded fact is a caught lie
});

test('verdicts are pure: same input, same output, no mutation of snapshots', () => {
  const snaps = snapshotFixture();
  const claims = liarClaims(snaps, { mode: 'lie' });
  const before = JSON.stringify(snaps);
  const a = JSON.stringify(verdictOf(claims, snaps));
  const b = JSON.stringify(verdictOf(claims, snaps));
  assert.equal(a, b);
  assert.equal(JSON.stringify(snaps), before);
});
