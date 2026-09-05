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
test('six full rounds, no duplicate laws, settle once', () => {
  const g=fresh(), cards=[];
  for(let round=1;round<=6;round++) {
    assert.equal(g.round,round);
    phase(g,g.players.map(p=>p.goods?{type:'sell',units:p.goods}:pass)); E.next(g);
    phase(g,[produce,produce]); E.next(g); cards.push(g.law);
    phase(g,[abstain,abstain]); E.next(g);
  }
  assert.equal(new Set(cards).size,6);
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
const roster = n => Array.from({length:n},(_,i)=>({name:'Фракция '+i,color:'#74b3a0'}));
function forceLaw(g,card,votes=g.players.map(()=>yes)) {
  g.phase='law'; g.stage='gate'; g.turn=0; g.choices=g.players.map(()=>null);
  g.president=[...g.players].sort((a,b)=>b.influence-a.influence||a.index-b.index)[0].index;
  g.law=card; phase(g,votes);
}
test('2-6 players accepted, invalid counts and case-insensitive duplicates rejected',()=>{
  for(let n=2;n<=6;n++) { const g=E.create(roster(n)); assert.equal(g.bank,n*10*M); assert.equal(g.choices.length,n); }
  for(const value of [null,[],roster(1),roster(7)]) assert.throws(()=>E.create(value));
  assert.throws(()=>E.create([{name:' Север '},{name:'сЕвЕр'}]));
});
test('three factions: 30M bank, 95 denominator, floor and first-place remainder',()=>{
  const g=E.create(roster(3)),r=E.settlement(g.players,g.bank);
  assert.equal(r.rake,1500000); assert.equal(r.denominator,95);
  assert.deepEqual(Array.from(r.rows,p=>p.payout),[15000000,9000000,4500000]);
  const odd=E.settlement(g.players,30000003);
  assert.equal(odd.rows[1].payout,Number(BigInt(odd.net)*30n/95n));
  assert.equal(odd.rows[2].payout,Number(BigInt(odd.net)*15n/95n));
  assert.equal(odd.rows[0].payout,odd.net-odd.rows[1].payout-odd.rows[2].payout);
});
test('six factions: 60M bank, 5% rake, zero payout after fourth place',()=>{
  const g=E.create(roster(6)),r=E.settlement(g.players,g.bank);
  assert.equal(r.rake,3*M); assert.equal(r.denominator,100);
  assert.deepEqual(Array.from(r.rows,p=>p.payout),[28500000,17100000,8550000,2850000,0,0]);
  assert.equal(E.settlement(g.players,60000019).rake,3000000);
});
test('buy two at counter five pays slots five and four, no sales tax',()=>{
  const g=fresh();g.sold=5;g.tax=20;g.players[0].cash=20*M;
  phase(g,[{type:'buy',units:2},pass]);
  assert.equal(g.players[0].cash,7*M); assert.equal(g.players[0].goods,2);
  assert.equal(g.sold,3);assert.equal(g.taxVault,0);assert.equal(g.marketBalance,13*M);
});
test('buy clamps counter at zero and rejects insufficient or malformed amounts',()=>{
  const g=fresh();g.players[0].cash=24*M;
  assert.ok(E.validate(g,{type:'buy',units:3},0));
  for(const units of [0,-1,1.5,Infinity,Number.MAX_SAFE_INTEGER]) assert.ok(E.validate(g,{type:'buy',units},0));
  phase(g,[{type:'buy',units:2},pass]);assert.equal(g.sold,0);assert.equal(g.players[0].cash,0);
});
test('buy is cancelled atomically if earlier buyer makes it unaffordable',()=>{
  const g=fresh();g.sold=5;g.players[0].cash=13*M;g.players[1].cash=13*M;
  phase(g,[{type:'buy',units:2},{type:'buy',units:2}]);
  assert.equal(g.players[0].goods,2);assert.equal(g.players[1].goods,0);
  assert.equal(g.players[1].cash,13*M);assert.equal(g.sold,3);
  assert.match(g.revealed[1].text,/отменена/);
});
test('donkey costs exactly 1M and rejects insufficient cash',()=>{
  const g=fresh();phase(g,[pass,pass]);E.next(g);
  g.players[0].cash=M-1;assert.ok(E.validate(g,{type:'donkey'},0));
  g.players[0].cash=M;phase(g,[{type:'donkey'},pass]);
  assert.equal(g.players[0].cash,0);assert.equal(g.players[0].goods,1);assert.equal(g.marketBalance,M);
});
test('accepted poor law preserves tax; new tax replaces old rate',()=>{
  const g=fresh();forceLaw(g,'tax10');forceLaw(g,'poor');assert.equal(g.tax,10);
  forceLaw(g,'tax20');assert.equal(g.tax,20);forceLaw(g,'boom');assert.equal(g.tax,20);
  forceLaw(g,'tax10');assert.equal(g.tax,10);
});
test('production subsidy adds one until next accepted law, rejection preserves it',()=>{
  const g=fresh();forceLaw(g,'production');E.next(g);
  phase(g,[pass,pass]);E.next(g);phase(g,[produce,produce]);assert.equal(g.players[0].goods,3);
  forceLaw(g,'status',[no,no]);assert.equal(g.productionBonus,1);
  forceLaw(g,'boom');assert.equal(g.productionBonus,0);E.next(g);
  phase(g,[pass,pass]);E.next(g);phase(g,[produce,produce]);assert.equal(g.players[0].goods,5);
});
test('each accepted card overwrites production and pending price effects',()=>{
  for(const card of Object.keys(E.LAWS)) {
    const g=fresh();g.productionBonus=1;g.nextBoom=-2;g.tax=10;forceLaw(g,card);
    assert.equal(g.productionBonus,card==='production'?1:0);
    assert.equal(g.nextBoom,card==='boom'?2:card==='embargo'?-2:0);
    assert.equal(g.tax,card==='tax20'?20:10);
  }
});
test('embargo then boom replaces -2 with +2, never accumulates',()=>{
  const g=fresh();forceLaw(g,'embargo');forceLaw(g,'boom');E.next(g);
  assert.equal(g.boom,2);assert.equal(E.price(g),14*M);
});
test('embargo lasts one round and floors all prices at 1M',()=>{
  const g=fresh();forceLaw(g,'embargo');E.next(g);
  assert.equal(E.price(g),10*M);assert.equal(E.price(g,8),M);assert.equal(E.price(g,30),M);
  forceLaw(g,'poor',[no,no]);E.next(g);assert.equal(g.boom,0);assert.equal(E.price(g),12*M);
});
test('rich and poor use cash with entry-order tie break',()=>{
  const g=E.create(roster(3));g.players[1].cash=g.players[2].cash=5*M;
  forceLaw(g,'rich');assert.equal(g.players[1].influence,2);assert.equal(g.players[2].influence,1);
  forceLaw(g,'poor');assert.equal(g.players[0].influence,2);
});
test('three-player president uses post-bribe influence, ties and veto are correct',()=>{
  const g=E.create(roster(3),()=>.99);phase(g,[pass,pass,pass]);E.next(g);
  g.players[2].cash=10*M;phase(g,[produce,produce,{type:'bribe',amount:10*M,to:0}]);E.next(g);
  assert.equal(g.president,2);assert.ok(E.validate(g,{type:'yes',veto:true},0));
  phase(g,[yes,yes,{type:'yes',veto:true}]);assert.equal(g.verdict.passed,false);
  forceLaw(g,'tax10',[yes,no,abstain]);assert.equal(g.verdict.passed,false);
  g.players[1].influence=3;forceLaw(g,'tax10',[no,yes,abstain]);
  assert.equal(g.president,1);assert.equal(g.verdict.yes,3);assert.equal(g.verdict.passed,true);
});
test('eight-card deck is exhausted before reshuffling',()=>{
  const g=fresh(),cards=[];assert.equal(Object.keys(E.LAWS).length,8);
  for(let i=0;i<16;i++) {
    // Exercise actual draw path beyond a normal six-round game without settling.
    g.phase='action';g.stage='done';E.next(g);cards.push(g.law);
  }
  assert.equal(new Set(cards.slice(0,8)).size,8);assert.equal(new Set(cards.slice(8)).size,8);
});
test('all 2-6 player games conserve money including market counterparty, settle once',()=>{
  let seed=8271;const rng=()=>((seed=(seed*1664525+1013904223)>>>0)/4294967296);
  for(let n=2;n<=6;n++)for(let trial=0;trial<10;trial++) {
    const g=E.create(roster(n),rng),initial=g.bank;
    const conserved=()=>assert.equal(g.players.reduce((a,p)=>a+p.cash,0)+g.bank+g.taxVault+g.marketBalance,initial);
    while(g.stage!=='final') {
      for(let i=0;i<n;i++) {
        const p=g.players[i];let choices;
        if(g.phase==='market')choices=[pass,{type:'sell',units:p.goods},{type:'buy',units:2}];
        else if(g.phase==='action')choices=[produce,{type:'donkey'},{type:'bribe',to:(i+1)%n,amount:5*M},pass];
        else choices=[yes,no,abstain,{type:'yes',veto:true}];
        choices=choices.filter(c=>!E.validate(g,c,i));E.open(g);E.submit(g,choices[Math.floor(rng()*choices.length)]);conserved();
      }
      E.beginReveal(g);while(g.stage==='revealing'){E.step(g);conserved();}E.next(g);
    }
    assert.equal(g.result.rake+g.result.rows.reduce((a,p)=>a+p.payout,0),g.bank);
    assert.throws(()=>E.next(g));
  }
});
