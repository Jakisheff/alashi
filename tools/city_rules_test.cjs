#!/usr/bin/env node
// Tests the embedded city rules using only the Node standard library.
const {readFileSync}=require('node:fs');
const {resolve}=require('node:path');
const vm=require('node:vm');
const {test}=require('node:test');
const assert=require('node:assert/strict');
const html=readFileSync(resolve(__dirname,'../app/alashi-city.html'),'utf8');
const source=html.match(/<script id="city-game">([\s\S]*?)<\/script>/)[1];
const ctx={};
vm.runInNewContext(source.slice(0,source.indexOf('globalThis.CityRules=CityRules;')+32),ctx);
const R=ctx.CityRules;
test('market follows every canonical price and floors at 1M',()=>{
 const prices=[12,10,9,8,7,6,5,4,3,3,2,2,2,1,1,1,1,1];
 for(let i=0;i<prices.length;i++)assert.equal(R.sale(1,i).net,prices[i]*1e6);
 assert.equal(R.sale(2,0).net,22e6);assert.equal(R.sale(2,2).net,17e6);
});
test('tax comes from sale proceeds; boom changes each lot',()=>{
 const s=R.sale(2,0,10);assert.equal(s.gross,22e6);assert.equal(s.tax,2.2e6);assert.equal(s.net,19.8e6);
 assert.equal(R.sale(2,0,0,2).net,26e6);assert.equal(R.sale(2,0,10,2).net,23.4e6);
});
test('zero inventory produces no income or tax',()=>{assert.equal(R.sale(0,0,10,2).net,0)});
test('body radius prevents walking through a wall',()=>{
 const walls=[{x:5,z:0,w:2,d:10}];
 const p=R.move(0,0,20,0,.4,walls);assert.equal(p.hit,true);assert.ok(p.x<=3.6);assert.equal(p.z,0);
});
test('high speed cannot tunnel through a thin obstacle',()=>{
 const walls=[{x:0,z:0,w:.6,d:30}];
 const p=R.move(-20,0,70,0,1.25,walls);assert.equal(p.hit,true);assert.ok(p.x<=-1.55);
});
test('player slides along a blocked axis rather than getting stuck',()=>{
 const walls=[{x:5,z:0,w:2,d:20}];
 const p=R.move(3.5,0,2,5,.4,walls);assert.equal(p.hit,true);assert.ok(p.x<=3.6);assert.ok(p.z>4.9);
});
test('world border bounds both forward and reverse movement',()=>{
 const a=R.move(230,230,100,100,1,[]),b=R.move(-230,-230,-100,-100,1,[]);
 assert.equal(a.x,239);assert.equal(a.z,239);assert.equal(b.x,-239);assert.equal(b.z,-239);
});
test('angle wrapping picks the short rotation across north',()=>{
 assert.ok(Math.abs(R.angle(Math.PI*2+.1)-.1)<1e-10);
 assert.ok(Math.abs(R.angle(-Math.PI*2-.1)+.1)<1e-10);
});
test('gameplay does not import scripts, call servers, or persist player data',()=>{
 assert.doesNotMatch(html,/<script[^>]+src=/i);
 assert.doesNotMatch(source,/\b(localStorage|sessionStorage|fetch|XMLHttpRequest|WebSocket|indexedDB)\b/);
 assert.ok(html.includes('CC0'));assert.ok(html.includes('MIT License'));
});
test('embedded character includes in-file buffers and movement animations',()=>{
 const model=JSON.parse(html.match(/<script type="application\/json" id="character-model">([\s\S]*?)<\/script>/)[1]);
 assert.ok(model.buffers.every(b=>b.uri.startsWith('data:')));
 const animations=model.animations.map(a=>a.name);
 for(const name of ['Idle_Neutral','Walk','Run'])assert.ok(animations.includes(name));
});
