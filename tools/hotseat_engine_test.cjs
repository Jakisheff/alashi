#!/usr/bin/env node
// Run with: node tools/hotseat_engine_test.cjs
// Tests the exact inline engine shipped in the standalone HTML, no packages needed.
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const { test } = require('node:test');
const html = readFileSync(resolve(__dirname, '../app/alashi-hotseat.html'), 'utf8');
const script = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const context = {};
vm.runInNewContext(script.split('/* UI */')[0], context);
const E = context.AlashiEngine, M = E.M;
const players = [{ name: 'Север', color: '#74b3a0' }, { name: 'Юг', color: '#d6a074' }];
const fresh = (rng = () => .99) => E.create(players, rng);
function phase(g, choices) {
  for (const choice of choices) { E.open(g); E.submit(g, choice); }
  E.beginReveal(g);
  while (g.stage === 'revealing') E.step(g);
}
const pass = { type: 'pass' }, produce = { type: 'produce' };
const yes = { type: 'yes' }, no = { type: 'no' }, abstain = { type: 'abstain' };
function law(g, card) {
  phase(g, [pass, pass]); E.next(g);
  phase(g, [pass, pass]); E.next(g);
  g.law = card;
}
test('entry fees go to bank, factions start empty with one influence', () => {
  const g = fresh();
  assert.equal(g.bank, 20*M);
  assert.ok(g.players.every(p => p.cash === 0 && p.goods === 0 && p.influence === 1));
  assert.throws(() => E.create([players[0]]));
  assert.throws(() => E.create([players[0], players[0]]));
});
test('all sixteen prices, exhausted market floor, and sequential lot pricing', () => {
  const g = fresh();
  assert.deepEqual(Array.from({length:18}, (_,i) => E.price(g,i)/M), [12,10,9,8,7,6,5,4,3,3,2,2,2,1,1,1,1,1]);
  g.players.forEach(p => p.goods = 2);
  phase(g, [{type:'sell',units:2},{type:'sell',units:2}]);
  assert.equal(g.players[0].cash,22*M); assert.equal(g.players[1].cash,17*M);
  assert.equal(g.sold,4); assert.equal(E.price(g),7*M);
});
test('random queue changes who receives first high prices', () => {
  const g = fresh(() => 0);
  g.players.forEach(p => p.goods = 2);
  phase(g, [{type:'sell',units:2},{type:'sell',units:2}]);
  assert.equal(g.players[1].cash,22*M); assert.equal(g.players[0].cash,17*M);
});
test('sealed choices do not change balances; duplicate or premature operations fail', () => {
  const g = fresh(); g.players[0].goods=2;
  assert.throws(() => E.submit(g,pass)); E.open(g);
  assert.throws(() => E.submit(g,{type:'sell',units:3}));
  assert.throws(() => E.submit(g,{type:'sell',units:1.5}));
  E.submit(g,{type:'sell',units:2});
  assert.equal(g.players[0].cash,0); assert.equal(g.players[0].goods,2);
  assert.throws(() => E.submit(g,pass)); assert.throws(() => E.beginReveal(g));
});
test('9M bribe grants one influence, 10M grants two; transfer conserves money', () => {
  for (const [amount, gained] of [[9*M,1],[10*M,2]]) {
    const g=fresh(); phase(g,[pass,pass]); E.next(g); g.players[0].cash=20*M;
    phase(g,[{type:'bribe',to:1,amount},produce]);
    assert.equal(g.players[0].cash,20*M-amount); assert.equal(g.players[1].cash,amount);
    assert.equal(g.players[0].influence,1+gained); assert.equal(g.players[1].goods,2);
    E.next(g); assert.equal(g.president,0);
  }
});
test('bribe cannot target self or spend unreceived funds', () => {
  const g=fresh(); phase(g,[pass,pass]); E.next(g);
  g.players[0].cash=10*M;
  assert.ok(E.validate(g,{type:'bribe',to:0,amount:5*M},0));
  assert.ok(E.validate(g,{type:'bribe',to:0,amount:5*M},1));
});
test('president elected after actions, ties follow entry order', () => {
  const g=fresh(); law(g,'poor'); assert.equal(g.president,0);
  const h=fresh(); h.players[1].influence=3; law(h,'poor'); assert.equal(h.president,1);
});
test('votes use influence, not number of voters', () => {
  const g=fresh(); g.players[1].influence=3; law(g,'tax10');
  phase(g,[no,yes]); assert.equal(g.verdict.yes,3); assert.equal(g.verdict.no,1);
  assert.equal(g.verdict.passed,true); assert.equal(g.tax,10);
});
test('ties and two abstentions reject law; abstention contributes no weight', () => {
  for (const votes of [[yes,no],[abstain,abstain]]) {
    const g=fresh(); law(g,'tax10'); phase(g,votes);
    assert.equal(g.verdict.passed,false); assert.equal(g.tax,0);
  }
  const g=fresh(); g.players[1].influence=10; law(g,'tax10'); phase(g,[yes,abstain]);
  assert.equal(g.verdict.yes,1); assert.equal(g.verdict.no,0); assert.equal(g.verdict.passed,true);
});
test('only president can veto; veto blocks unanimous approval', () => {
  const g=fresh(); law(g,'boom');
  assert.ok(E.validate(g,{type:'yes',veto:true},1));
  phase(g,[{type:'yes',veto:true},yes]);
  assert.equal(g.verdict.veto,true); assert.equal(g.verdict.passed,false); assert.equal(g.nextBoom,0);
});
test('boom affects only next market; sold counter resets', () => {
  const g=fresh(); law(g,'boom'); g.sold=9; phase(g,[yes,yes]); E.next(g);
  assert.equal(g.sold,0); assert.equal(E.price(g),14*M);
  law(g,'tax10'); phase(g,[no,no]); E.next(g);
  assert.equal(g.boom,0); assert.equal(E.price(g),12*M);
});
test('tax persists and is held outside entry bank', () => {
  const g=fresh(); law(g,'tax10'); phase(g,[yes,yes]); E.next(g);
  g.players[0].goods=2; phase(g,[{type:'sell',units:2},pass]);
  assert.equal(g.players[0].cash,19.8*M); assert.equal(g.taxVault,2.2*M); assert.equal(g.bank,20*M);
  E.next(g); phase(g,[pass,pass]); E.next(g); g.law='poor'; phase(g,[no,no]); E.next(g);
  assert.equal(g.tax,10);
});
test('poor subsidy ranks cash, breaks ties by entry, and applies once', () => {
  const g=fresh(); law(g,'poor'); g.players[0].cash=2*M;
  phase(g,[yes,yes]); assert.equal(g.players[1].influence,2);
  assert.throws(() => E.step(g)); assert.equal(g.players[1].influence,2);
  const h=fresh(); law(h,'poor'); phase(h,[yes,yes]); assert.equal(h.players[0].influence,2);
});
test('two-place split normalizes 50/30 and deducts rake from bank', () => {
  const g=fresh(), r=E.settlement(g.players,g.bank);
  assert.equal(r.rake,M); assert.equal(r.net,19*M);
  assert.equal(r.rows[0].payout,11875000); assert.equal(r.rows[1].payout,7125000);
  assert.equal(r.rounding,0); assert.equal(r.winner.index,0);
});
test('integer division remainder goes to first place, cash tie ignores influence', () => {
  const g=fresh(); g.players[1].influence=99;
  const r=E.settlement(g.players,20000003);
  assert.equal(r.rake,1000000); assert.equal(r.rounding,1);
  assert.equal(r.rows[0].payout,11875002); assert.equal(r.rows[1].payout,7125001);
  assert.equal(r.rows[0].index,0);
  assert.equal(r.rake+r.rows.reduce((n,p)=>n+p.payout,0),r.bank);
});
test('cash decides rank and final winner, not influence', () => {
  const g=fresh(); g.players[1].cash=5*M; g.players[0].influence=100;
  const r=E.settlement(g.players,g.bank);
  assert.equal(r.rows[0].index,1); assert.equal(r.winner.index,1);
  assert.equal(r.winner.total,16875000);
});
test('six full rounds, three-card cycles without duplicates, settle once', () => {
  const g=fresh(), cards=[];
  for(let round=1;round<=6;round++) {
    assert.equal(g.round,round);
    phase(g,g.players.map(p=>p.goods?{type:'sell',units:p.goods}:pass)); E.next(g);
    phase(g,[produce,produce]); E.next(g); cards.push(g.law);
    phase(g,[abstain,abstain]); E.next(g);
  }
  assert.equal(new Set(cards.slice(0,3)).size,3); assert.equal(new Set(cards.slice(3)).size,3);
  assert.equal(g.stage,'final'); assert.equal(g.players.reduce((n,p)=>n+p.cash,0),195*M);
  assert.equal(g.result.rows.reduce((n,p)=>n+p.total,0),214*M);
  assert.throws(()=>E.next(g)); assert.equal(g.result.net,19*M);
  const restarted=E.create(g.players);
  assert.equal(restarted.players[0].cash,0); assert.equal(restarted.round,1);
});
test('decimal input preserves individual pesos without float errors', () => {
  assert.equal(E.parseMillions('9,000001'),9000001);
  assert.equal(E.parseMillions('0.000001'),1);
  for(const text of ['-1','Infinity','1e6','1.0000001','abc','9007199254740993']) assert.equal(E.parseMillions(text),null);
});
test('standalone page has no script imports, persistent storage, or game networking', () => {
  assert.doesNotMatch(html,/<script[^>]+src=/i);
  assert.doesNotMatch(script,/\b(localStorage|sessionStorage|fetch|XMLHttpRequest|WebSocket|indexedDB)\b/);
});
