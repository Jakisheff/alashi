// Lie ledger (credulous generals, phase 1): mechanically verify dialogue
// claims against certified world snapshots. Pure module, no I/O.
//
// Claim types map to the fields a world snapshot already carries:
//   law_passed {round, law, value}   -> world.laws_passed (law visible in
//     a round's END snapshot; a claim about round R is checked in snapshot R,
//     so claims about the still-open current round are unverifiable, not lies)
//   veto_pending {round, value}      -> world.veto_pending
//   price {round, op, value}         -> world.market_price
//   cash {round, actor, op, value}   -> public_state[actor].cash
// Verdicts: 'true' (consistent + evidence), 'false' (contradicted + evidence),
// 'unverifiable' (missing data / unknown subject). The ledger never guesses:
// an unknown claim type or a missing snapshot is 'unverifiable'.
//
// liarClaims() is the preregistered adversary family (credulous-generals
// prompt, phase 0): truth | lie | salience modes over the same claim
// templates, so the scripted traitor and the ledger share one vocabulary.

export function verdictOf(claims, snapshots) {
  const byRound = new Map(snapshots.map(s => [s.round, s]));
  return claims.map(claim => {
    const snap = byRound.get(claim.round);
    if (!snap || !snap.world || !Object.hasOwn(claim, 'type')) return { claim, verdict: 'unverifiable', evidence: null };
    const w = snap.world;
    switch (claim.type) {
      case 'law_passed': {
        if (typeof claim.law !== 'string' || typeof claim.value !== 'boolean') return { claim, verdict: 'unverifiable', evidence: null };
        const passed = Array.isArray(w.laws_passed) && w.laws_passed.includes(claim.law);
        if (passed === undefined) return { claim, verdict: 'unverifiable', evidence: null };
        return { claim, verdict: passed === claim.value ? 'true' : 'false',
          evidence: { round: claim.round, laws_passed: w.laws_passed ?? null, last_law_passed: w.last_law_passed ?? null } };
      }
      case 'veto_pending': {
        if (typeof claim.value !== 'boolean' || !Object.hasOwn(w, 'veto_pending')) return { claim, verdict: 'unverifiable', evidence: null };
        return { claim, verdict: w.veto_pending === claim.value ? 'true' : 'false',
          evidence: { round: claim.round, veto_pending: w.veto_pending } };
      }
      case 'price': {
        const price = w.market_price;
        if (typeof price !== 'number' || typeof claim.value !== 'number' || !['eq', 'ge', 'le'].includes(claim.op))
          return { claim, verdict: 'unverifiable', evidence: null };
        const ok = claim.op === 'eq' ? price === claim.value : claim.op === 'ge' ? price >= claim.value : price <= claim.value;
        return { claim, verdict: ok ? 'true' : 'false', evidence: { round: claim.round, market_price: price } };
      }
      case 'cash': {
        const rows = Array.isArray(snap.world?.public_state) ? snap.world.public_state : [];
        const row = rows.find(r => r.actor === claim.actor);
        if (!row || typeof row.cash !== 'number' || typeof claim.value !== 'number' || !['ge', 'le', 'eq'].includes(claim.op))
          return { claim, verdict: 'unverifiable', evidence: null };
        const ok = claim.op === 'ge' ? row.cash >= claim.value : claim.op === 'le' ? row.cash <= claim.value : row.cash === claim.value;
        return { claim, verdict: ok ? 'true' : 'false', evidence: { round: claim.round, actor: claim.actor, cash: row.cash } };
      }
      default:
        return { claim, verdict: 'unverifiable', evidence: null };
    }
  });
}

const CLAIM_TEXTS = {
  law_passed: (c, v) => `закон ${c.law} ${v ? 'принят' : 'не принят'} в раунде ${c.round}`,
  veto_pending: (c, v) => `президент ${v ? 'наложит' : 'не наложит'} вето в раунде ${c.round}`,
  price: (c, v) => `цена в раунде ${c.round} ${c.op === 'ge' ? 'не ниже' : c.op === 'le' ? 'не выше' : 'равна'} ${c.value}`,
  cash: (c, v) => `у фракции ${c.actor} ${c.op === 'ge' ? 'не меньше' : c.op === 'le' ? 'не больше' : 'ровно'} ${c.value}`,
};

function withText(claim) {
  return { ...claim, text: CLAIM_TEXTS[claim.type](claim, claim.value ?? null) };
}

// Adversary family (preregistered): one template set, three modes.
//   truth    — claims matching the certified state
//   lie      — negated values of the same claims
//   salience — negated AND exaggerated (round numbers, strong words) so the
//              surface is louder; still decidable by the ledger
export function liarClaims(snapshots, { mode = 'lie' } = {}) {
  if (!['truth', 'lie', 'salience'].includes(mode)) throw new Error(`unknown mode ${mode}`);
  const claims = [];
  for (const snap of snapshots) {
    const w = snap.world ?? {};
    const rows = Array.isArray(w.public_state) ? w.public_state : [];
    for (const law of w.laws_passed ?? []) {
      claims.push(withText({ type: 'law_passed', round: snap.round, law, value: true }));
    }
    if (Object.hasOwn(w, 'veto_pending')) {
      claims.push(withText({ type: 'veto_pending', round: snap.round, value: w.veto_pending }));
    }
    if (typeof w.market_price === 'number') {
      claims.push(withText({ type: 'price', round: snap.round, op: 'ge', value: w.market_price }));
    }
    for (const row of rows) {
      if (typeof row.cash === 'number') claims.push(withText({ type: 'cash', round: snap.round, actor: row.actor, op: 'ge', value: row.cash }));
    }
  }
  const flip = c => {
    if (c.type === 'law_passed' || c.type === 'veto_pending') return withText({ ...c, value: !c.value });
    // Numeric lies must stay decidable-false: an eq claim against an off
    // value is false by construction; flipping an inequality op can land
    // on a trivially true claim (cash <= 2*cash) — the tests catch this.
    if (c.type === 'price') return withText({ ...c, op: 'eq', value: c.value + (mode === 'salience' ? 3 : 1) });
    if (c.type === 'cash') return withText({ ...c, op: 'eq', value: c.value + (mode === 'salience' ? c.value : 1) });
    return c;
  };
  return mode === 'truth' ? claims : claims.map(flip);
}

// Protocol fixture: synthetic snapshots in the world-run vocabulary.
// Rounds 1..6; tariff passes in round 3; veto pending in round 4;
// prices and cash deterministic. Not rules logic, just a fixed corpus.
export function snapshotFixture() {
  const rowsFor = cash0 => [0, 1, 2].map(actor => ({ actor, cash: cash0 + actor * 10, goods: 5, influence: 1, is_president: false }));
  return [1, 2, 3, 4, 5, 6].map(round => ({
    round,
    world: {
      laws_passed: round >= 3 ? ['tariff'] : [],
      last_law_passed: round >= 3 ? 'tariff' : null,
      veto_pending: round === 4,
      market_price: 10 + round,
      public_state: rowsFor(round * 100),
    },
  }));
}
