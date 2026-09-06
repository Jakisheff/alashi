#!/usr/bin/env node
const {readFileSync}=require('node:fs');
const {resolve}=require('node:path');
const vm=require('node:vm');
const assert=require('node:assert/strict');
const {test}=require('node:test');
const html=readFileSync(resolve(__dirname,'../app/arena.html'),'utf8'),script=html.match(/<script>([\s\S]*?)<\/script>/)[1],context={};
vm.runInNewContext(script.split('/* VIEW */')[0],context);const V=context.BoardView;
test('viewer does not implement an economy or submit game actions',()=>{assert.doesNotMatch(script,/method\s*:\s*['"]POST|localStorage|sessionStorage|AlashiEngine/);assert.doesNotMatch(html,/<script[^>]+src=/)});
test('integer peso formatting preserves exact payouts',()=>{assert.equal(V.cash(11875000),'11,875M');assert.equal(V.cash(1),'0,000001M');assert.equal(V.cash(0),'0M')});
test('event reconciliation avoids repeat animation and preserves duplicate occurrences',()=>{const event={ts:1,round:2,phase:'law',actor:0,action:'veto',by:'llm',ok:true};assert.equal(V.reconcile([event],[event]).length,0);assert.equal(V.reconcile([event],[event,event]).length,1);assert.equal(V.reconcile([],[event,event]).length,2);assert.equal(V.reconcile([event],[{...event,ok:false}]).length,1)});
test('server payout mapping follows faction indices, never recomputes settlement',()=>{const r=V.resultRows({agents:[{name:'A'},{name:'B'},{name:'C'}],ranks:[2,0,1],final_cash:[20,10,40],payouts:[9,4,15]});assert.equal(r[0].name,'C');assert.equal(r[0].total,55);assert.equal(r[1].name,'A');assert.equal(r[1].payout,9)});
test('untrusted names are escaped for HTML',()=>{assert.equal(V.esc('<img src="x">'),'&lt;img src=&quot;x&quot;&gt;')});
test('six unique faction locations and all classic laws are present',()=>{assert.equal(new Set(V.sites.map(p=>p.x+','+p.y)).size,6);assert.equal(new Set(V.colors).size,6);for(const law of ['status_quo','tax_10','tax_20','subsidy_produce','subsidy_poor','subsidy_rich','embargo','boom'])assert.ok(V.lawNames[law])});
