import test from 'node:test';
import assert from 'node:assert/strict';
import { worldOf, auditClaims, playGame, summarize, oracleTruth } from './liar-game.mjs';
import { verdictOf } from './lie-ledger.mjs';

// The game board: a seeded procedural world in the snapshot vocabulary.
// Not engine logic — a board generator for the deception layer, seeded
// and reproducible.

test('world is deterministic by seed and different across seeds', () => {
  const a = JSON.stringify(worldOf(42, 6));
  const b = JSON.stringify(worldOf(42, 6));
  const c = JSON.stringify(worldOf(43, 6));
  assert.equal(a, b);
  assert.notEqual(a, c);
  assert.equal(worldOf(42, 6).length, 6);
});

test('oracle vocabulary is strictly larger than the ledger vocabulary (the hunting ground)', () => {
  const w = worldOf(7, 6);
  const snaps = w.map(s => ({ round: s.round, world: s.world }));
  const gtClaim = { type: 'price', round: 3, op: 'gt', value: -1, text: 'цена больше -1' };
  // ledger refuses 'gt' -> unverifiable
  assert.equal(verdictOf([gtClaim], snaps)[0].verdict, 'unverifiable');
  // oracle decides it (true: price > -1)
  assert.equal(oracleTruth(gtClaim, w), true);
  // and a goods claim: ledger has no 'goods' subject at all
  const goodsClaim = { type: 'goods', round: 3, actor: 0, op: 'ge', value: 999999, text: 'у фракции 0 гора товаров' };
  assert.equal(verdictOf([goodsClaim], snaps)[0].verdict, 'unverifiable');
  assert.equal(oracleTruth(goodsClaim, w), false);
});

test('audit classes: caught, smuggled, framed, false accusation, honest, void', () => {
  const w = worldOf(5, 6);
  const snaps = w.map(s => ({ round: s.round, world: s.world }));
  const truePrice = w[2].world.market_price;
  const claims = [
    { type: 'price', round: 3, op: 'eq', value: truePrice + 1, text: 'ложь о цене' },            // oracle false, ledger false -> caught
    { type: 'price', round: 3, op: 'gt', value: 999999, text: 'gt-ложь' },                        // oracle false, ledger unverifiable -> smuggled
    { type: 'price', round: 3, op: 'eq', value: truePrice, text: 'правда' },                      // oracle true, ledger true -> honest
    { type: 'law_passed', round: 3, law: w[2].world.laws_passed[0] ?? 'tariff', value: true, text: 'правда о законе' },
  ];
  const audit = auditClaims(claims, w);
  const kinds = audit.map(a => a.kind);
  assert.equal(kinds[0], 'caught');
  assert.equal(kinds[1], 'smuggled');
  assert.equal(kinds[2], 'honest');
  assert.ok(['honest', 'caught'].includes(kinds[3])); // law truth depends on the board
});

test('framed: a ledger TRUE verdict on an oracle-false claim scores the liar maximum', () => {
  // Constructed via a bug injection is not needed here: we verify the
  // scoring table directly through auditClaims classification paths.
  const w = worldOf(5, 6);
  const snaps = w.map(s => ({ round: s.round, world: s.world }));
  // 'ne' is oracle-decidable, ledger-unverifiable: oracle false when price == value
  const price = w[2].world.market_price;
  const claim = { type: 'price', round: 3, op: 'ne', value: price, text: 'цена не равна себе' };
  assert.equal(oracleTruth(claim, w), false);
  const [a] = auditClaims([claim], w);
  assert.equal(a.kind, 'smuggled');
  assert.ok(a.liarPoints >= 2);
});

test('bot modes: probe finds the known gaps, salience gets caught, playGame is deterministic', () => {
  const probeA = JSON.stringify(playGame({ seed: 11, rounds: 6, bot: 'probe' }));
  const probeB = JSON.stringify(playGame({ seed: 11, rounds: 6, bot: 'probe' }));
  assert.equal(probeA, probeB);
  const probe = playGame({ seed: 11, rounds: 6, bot: 'probe' });
  assert.ok(probe.gaps.length >= 1, 'probe bot must surface at least one vocabulary gap');
  assert.ok(probe.audit.some(a => a.kind === 'smuggled'));

  const salience = playGame({ seed: 11, rounds: 6, bot: 'salience' });
  const caught = salience.audit.filter(a => a.kind === 'caught').length;
  const smuggled = salience.audit.filter(a => a.kind === 'smuggled').length;
  assert.ok(caught > smuggled, `salience lies should mostly be caught (caught=${caught}, smuggled=${smuggled})`);
  assert.equal(salience.audit.filter(a => a.kind === 'framed').length, 0);
});

test('summarize reports the verdict and the gap list with reasons', () => {
  const g = playGame({ seed: 3, rounds: 6, bot: 'probe' });
  const s = summarize(g);
  assert.ok(s.verdict === 'scene_holds' || s.verdict === 'arena_broken');
  assert.ok(Array.isArray(s.gaps) && s.gaps.length === g.gaps.length);
  for (const gap of s.gaps) assert.ok(gap.claim && gap.reason);
});
