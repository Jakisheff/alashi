// Liar vs Ledger (Лжец против Реестра) — the credulous-generals game,
// playable offline. The board is a seeded procedural world in the
// snapshot vocabulary (a board generator, NOT a second implementation
// of alashi rules). The oracle knows the full board; the ledger
// (lie-ledger.mjs) is deliberately the shipped, narrower checker.
// The hunting ground is the vocabulary difference: claims the oracle
// can decide but the ledger refuses (smuggled) are coverage gaps —
// each one is reported and becomes the next ledger upgrade.
//
// Modes:
//   node liar-game.mjs --seed 42 --rounds 6 --bot probe|salience|random
//   node liar-game.mjs --seed 42 --duel        (human liar, JSON claims on stdin)
//
// Scoring (per audited claim):
//   caught   oracle false, ledger false        -> ledger +1
//   smuggled oracle false, ledger unverifiable -> liar +3 (gap)
//   framed   oracle false, ledger true         -> liar +10 (proof bug)
//   false_accusation oracle true, ledger false -> liar +5 (dangerous bug)
//   honest   oracle true, ledger true          -> no points
//   void     claim about nothing decidable     -> no points

import { verdictOf } from './lie-ledger.mjs';
import { createInterface } from 'node:readline';

function rng(seed) { // mulberry32
  let a = seed >>> 0;
  return () => {
    a |= 0; a = (a + 0x6D2B79F5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const LAW_DECK = ['tariff', 'amnesty', 'subsidy', 'customs', 'license_reform', 'export_ban'];

// Seeded board in the exact snapshot vocabulary the ledger consumes.
export function worldOf(seed, rounds = 6, factions = 3) {
  const r = rng(seed);
  let price = 10 + Math.floor(r() * 10);
  const rows = Array.from({ length: factions }, (_, actor) => ({ actor, cash: 100 + Math.floor(r() * 200), goods: Math.floor(r() * 10), influence: 1 + Math.floor(r() * 5), is_president: false }));
  return Array.from({ length: rounds }, (_, i) => {
    const round = i + 1;
    price = Math.max(1, price + Math.floor(r() * 7) - 3);
    for (const row of rows) {
      row.cash += Math.floor(r() * 50);
      row.goods = Math.max(0, row.goods + (r() < 0.5 ? 1 : -1));
    }
    const laws = r() < 0.5 ? [] : [LAW_DECK[Math.floor(r() * LAW_DECK.length)]];
    return { round, world: {
      laws_passed: laws,
      last_law_passed: laws[0] ?? null,
      veto_pending: r() < 0.3,
      market_price: price,
      public_state: rows.map(({ actor, cash, goods, influence }) => ({ actor, cash, goods, influence, is_president: false })),
    } };
  });
}

// The oracle: decidable content with a STRICTLY LARGER vocabulary than
// the ledger (ops gt/lt/ne; subjects goods/influence). This difference
// IS the game's hunting ground and the ledger's upgrade backlog.
export function oracleTruth(claim, world) {
  const snap = world.find(s => s.round === claim.round);
  if (!snap) return null; // void
  const w = snap.world;
  switch (claim.type) {
    case 'law_passed': {
      const passed = (w.laws_passed ?? []).includes(claim.law);
      return typeof claim.value === 'boolean' ? passed === claim.value : null;
    }
    case 'veto_pending':
      return typeof claim.value === 'boolean' ? w.veto_pending === claim.value : null;
    case 'price': {
      if (typeof claim.value !== 'number') return null;
      return cmp(w.market_price, claim.op, claim.value);
    }
    case 'cash':
    case 'goods':
    case 'influence': {
      const row = (w.public_state ?? []).find(x => x.actor === claim.actor);
      if (!row || typeof claim.value !== 'number') return null;
      return cmp(row[claim.type], claim.op, claim.value);
    }
    default: return null;
  }
}

function cmp(a, op, b) {
  switch (op) {
    case 'eq': return a === b; case 'ne': return a !== b;
    case 'ge': return a >= b;  case 'le': return a <= b;
    case 'gt': return a > b;   case 'lt': return a < b;
    default: return null;
  }
}

// Audit: oracle truth + shipped-ledger verdict -> game classification.
export function auditClaims(claims, world) {
  const snaps = world.map(s => ({ round: s.round, world: s.world }));
  const verdicts = verdictOf(claims, snaps);
  return claims.map((claim, i) => {
    const truth = oracleTruth(claim, world);
    const verdict = verdicts[i].verdict;
    let kind = 'void', liarPoints = 0, ledgerPoints = 0, reason = '';
    if (truth === false) {
      if (verdict === 'false') { kind = 'caught'; ledgerPoints = 1; reason = 'ложь поймана со ссылкой на состояние'; }
      else if (verdict === 'unverifiable') { kind = 'smuggled'; liarPoints = 3; reason = 'ложь проскользнула в непроверяемую зону: словарь реестра уже, чем оракул'; }
      else { kind = 'framed'; liarPoints = 10; reason = 'реестр признал ложь истиной — баг доказательства'; }
    } else if (truth === true) {
      if (verdict === 'false') { kind = 'false_accusation'; liarPoints = 5; reason = 'реестр назвал правду ложью — опасный баг протокола обвинения'; }
      else if (verdict === 'true') { kind = 'honest'; reason = 'правдивое клейм согласовано'; }
      else { kind = 'wasted'; reason = 'правда ушла в непроверяемую зону — реестр слеп к своему словарю'; }
    } else {
      kind = 'void'; reason = 'клейм ни о чём решаемом';
    }
    return { claim, truth, verdict, kind, liarPoints, ledgerPoints, reason, evidence: verdicts[i].evidence };
  });
}

// Bot liar policies. probe systematically hunts the vocabulary gap
// (ops and subjects beyond the ledger); salience emits loud eq-lies
// (mostly caught); random mixes.
function botClaims(world, mode, r) {
  const snap = world[0]; // claims target the first recorded round
  const w = snap.world;
  const claims = [];
  if (mode === 'probe') {
    claims.push({ type: 'price', round: snap.round, op: 'gt', value: w.market_price + 1000, text: `цена больше ${w.market_price + 1000}` });
    claims.push({ type: 'goods', round: snap.round, actor: 0, op: 'ge', value: 999999, text: 'у фракции 0 гора товаров' });
    claims.push({ type: 'influence', round: snap.round, actor: 1, op: 'eq', value: -1, text: 'влияние фракции 1 равно -1' });
    claims.push({ type: 'price', round: snap.round, op: 'ne', value: w.market_price, text: 'цена не равна себе' });
  } else if (mode === 'salience') {
    claims.push({ type: 'law_passed', round: snap.round, law: w.laws_passed[0] ?? 'tariff', value: false, text: 'закон точно не принят!' });
    claims.push({ type: 'price', round: snap.round, op: 'eq', value: w.market_price + 3, text: `цена взлетит до ${w.market_price + 3}!` });
    claims.push({ type: 'cash', round: snap.round, actor: 2, op: 'eq', value: 10 ** 9, text: 'у фракции 2 миллиард!' });
  } else { // random
    const pick = arr => arr[Math.floor(r() * arr.length)];
    claims.push({ type: pick(['price', 'law_passed', 'cash']), round: snap.round,
      op: pick(['eq', 'ge', 'le', 'gt', 'ne']), value: Math.floor(r() * 1000), text: 'случайный вброс' });
  }
  return claims;
}

// A full game: R rounds, the liar strikes each round, the ledger audits.
export function playGame({ seed = 1, rounds = 6, bot = 'probe', claimsPerRound = 3 } = {}) {
  const world = worldOf(seed, rounds);
  const r = rng(seed * 7919 + 13);
  const audit = [];
  const log = [];
  for (let i = 0; i < world.length; i++) {
    const snap = world[i];
    const sub = world.slice(i); // bot lies against the current board
    const roundWorld = sub.map(s => s); // claims target this round
    const claims = botClaims(roundWorld, bot, r).slice(0, claimsPerRound)
      .map(c => ({ ...c, round: snap.round }));
    const audited = auditClaims(claims, world);
    audit.push(...audited);
    log.push({ round: snap.round, claims: claims.length,
      caught: audited.filter(a => a.kind === 'caught').length,
      smuggled: audited.filter(a => a.kind === 'smuggled').length,
      framed: audited.filter(a => a.kind === 'framed').length });
  }
  const liar = audit.reduce((s, a) => s + a.liarPoints, 0);
  const ledger = audit.reduce((s, a) => s + a.ledgerPoints, 0);
  const gaps = audit.filter(a => a.kind === 'smuggled' || a.kind === 'framed' || a.kind === 'false_accusation' || a.kind === 'wasted')
    .map(a => ({ claim: a.claim, verdict: a.verdict, reason: a.reason }));
  return { seed, rounds, bot, world, audit, log, liar, ledger, gaps };
}

export function summarize(game) {
  const verdict = game.liar >= 10 ? 'arena_broken' : 'scene_holds';
  return { verdict, liar: game.liar, ledger: game.ledger,
    claims: game.audit.length,
    caught: game.audit.filter(a => a.kind === 'caught').length,
    smuggled: game.audit.filter(a => a.kind === 'smuggled').length,
    framed: game.audit.filter(a => a.kind === 'framed').length,
    gaps: game.gaps };
}

// --- CLI ---
function printGame(game) {
  const s = summarize(game);
  console.log(`\n=== Лжец против Реестра · seed=${game.seed} · ${game.rounds} раундов · bot=${game.bot} ===`);
  for (const row of game.log) {
    console.log(`раунд ${row.round}: клеймов ${row.claims}, поймано ${row.caught}, протащено ${row.smuggled}, фальшивых доказательств ${row.framed}`);
  }
  console.log(`\nЛжец: ${s.liar} очков · Реестр: ${s.ledger} очков`);
  console.log(`Вердикт: ${s.verdict === 'scene_holds' ? 'СЦЕНА ДЕРЖИТ' : 'АРЕНУ СЛОМАЛИ'}`);
  if (s.gaps.length) {
    console.log(`\nДыры реестра (${s.gaps.length}) — каждая = задача на расширение словаря:`);
    for (const g of s.gaps) console.log(`  · ${JSON.stringify(g.claim)} → ${g.verdict}: ${g.reason}`);
  }
}

async function duel(seed, rounds) {
  const world = worldOf(seed, rounds);
  const rl = createInterface({ input: process.stdin });
  const claims = [];
  console.log(`Дуэль. Сид ${seed}. Отправляй клеймы построчно (JSON), например:`);
  console.log(`  {"type":"price","round":3,"op":"gt","value":999}` );
  console.log(`Пустая строка — закончить и получить приговор.`);
  for await (const line of rl) {
    if (!line.trim()) break;
    try { claims.push(JSON.parse(line)); }
    catch { console.log('не JSON, пропускаю'); }
  }
  const audit = auditClaims(claims, world);
  const liar = audit.reduce((s, a) => s + a.liarPoints, 0);
  const ledger = audit.reduce((s, a) => s + a.ledgerPoints, 0);
  for (const a of audit) console.log(`${a.kind.padEnd(18)} ${a.claim.text ?? JSON.stringify(a.claim)} — ${a.reason}`);
  console.log(`\nЛжец: ${liar} · Реестр: ${ledger} · ${liar >= 10 ? 'АРЕНУ СЛОМАЛИ' : 'СЦЕНА ДЕРЖИТ'}`);
}

if (process.argv[1] && import.meta.url === new URL(`file://${process.argv[1]}`).href) {
  const args = process.argv.slice(2);
  const flag = n => { const i = args.indexOf(n); return i >= 0 ? args[i + 1] : undefined; };
  const seed = Number(flag('--seed') ?? 1);
  const rounds = Number(flag('--rounds') ?? 6);
  if (args.includes('--duel')) { await duel(seed, rounds); }
  else { printGame(playGame({ seed, rounds, bot: flag('--bot') ?? 'probe' })); }
}
