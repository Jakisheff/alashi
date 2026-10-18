/** Isolated acceptance driver. Synthetic wallets and loopback providers only. */
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { readFileSync, writeFileSync, existsSync, renameSync } from 'node:fs';
import { join } from 'node:path';
import { ownerApi, readPrivate, modelProvider, runCycle, httpTrade } from './strategy-run.mjs';
const [base,root]=process.argv.slice(2);
assert.match(base,/^http:\/\/127\.0\.0\.1:\d+$/);
const profiles=Array.from({length:4},(_,i)=>readPrivate(join(root,`owner-${i}.json`)));
const apis=await Promise.all(profiles.map(p=>ownerApi(base,p)));
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const request=async(path,body)=>{const res=await fetch(base+path,{method:body?'POST':'GET',headers:{'Content-Type':'application/json'},...(body?{body:JSON.stringify(body)}:{})});return res.json();};
async function advance(game){
 const file=join(root,'advance.json');writeFileSync(file+'.tmp',JSON.stringify({game}));renameSync(file+'.tmp',file);
 const until=Date.now()+10000;while(existsSync(file)){assert.ok(Date.now()<until,'phase driver timeout');await sleep(10);}
}
async function newGame(owners,label){
 const created=await request('/game/new',{entry_fee:1,phase_duration:300,lobby_duration:300,grace_s:0,label});assert.equal(created.ok,true);
 const game=created.game_id;
 for(const i of owners){
  const p=profiles[i],v=await request(`/game/${game}/join`,{agent_record_id:p.agent_record_id,recovery_secret:p.recovery_secret,name:`Trader${i}`,model:'TEST-only',strategy_hash:'a'.repeat(64)});assert.equal(v.ok,true,JSON.stringify(v));
  writeFileSync(join(root,`game-${game}-${i}.json`),JSON.stringify({schema:'alashi.game.v2',game_id:game,agent_record_id:p.agent_record_id,token:v.token,next_op_id:v.next_op_id,pending_act:null}),{mode:0o600});
 }
 await advance(game);await advance(game); // Lobby -> Market -> Action, ordinary rules.
 return game;
}
const model=(id,depends_on,instruction)=>({id,kind:'model',depends_on,instruction,tool:null});
const original=(await apis[1].post('/strategies',{parent:null,title:'Trading with two model checks',change:'',published:true,plan:[model('market',[],'Read market and legal actions.'),model('risk',[],'Check inventory and cash.'),model('deliver',['market','risk'],'Produce in Action. Sell two goods in Market.')]})).version;
const derivative=(await apis[2].post('/strategies',{parent:original.id,title:'Reuse the structured game state',change:'Replace the two model checks with the local trade_state tool. Keep the same final decision contract.',published:false,plan:[{id:'state',kind:'tool',depends_on:[],instruction:'Read the canonical shared state.',tool:'trade_state'},model('deliver',['state'],'Produce in Action. Sell two goods in Market.')]})).version;
let blocked=false,totalCalls=0;const rounds=[],games=[];
async function duel(game,owners,versions,phase,key,{dropResponse=false}={}){
 const challenge=(await apis[owners[0]].post('/strategy-challenges',{match_key:key,round_seconds:180,title:`${phase} with equal initial resources`,audience:'Agent owners',task:phase==='action'?'Produce goods using the current game rules.':'Sell two goods. Compare the actual receipt and model tokens.',facts:['One alashi is 1000000 game units.','Market prices depend on transaction order.'],required_terms:[],cta:'',max_words:0,budget:20000,trading:{mode:'http',game:String(game)}})).challenge;
 if(!blocked){await assert.rejects(apis[2].post('/strategy-runs',{challenge_id:challenge.id,version:derivative.id,client_id:'blocked-draft'}),/publish_derivative_first/);blocked=true;await apis[2].post(`/strategies/${derivative.id}/publish`,{});}
 const runs=await Promise.all(owners.map((i,k)=>apis[i].post('/strategy-runs',{challenge_id:challenge.id,version:versions[k],client_id:`${key}-owner-${i}`})));
 const waiting=[],seen=new Set(),counts=new Map(),intervals=[];
 const server=createServer(async(req,res)=>{
  let raw='';for await(const part of req)raw+=part;
  const input=JSON.parse(raw),message=JSON.parse(input.messages[1].content),owner=message.owner;
  assert.ok(owners.some(i=>profiles[i].wallet===owner));assert.equal(message.brief.trading.game,String(game));
  seen.add(owner);counts.set(owner,(counts.get(owner)??0)+1);totalCalls++;
  const started=Date.now();
  const answer=()=>{const report=message.task==='deliver'?{action:phase==='action'?'produce':'sell',params:phase==='action'?{}:{units:2}}:{legal_action:phase==='action'?'produce':'sell',source:'SYNTHETIC_PROVIDER'};
   res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({id:`TEST-${key}-${owner}-${message.task}`,choices:[{message:{content:JSON.stringify(report)}}],usage:{prompt_tokens:100,completion_tokens:20,total_tokens:120}}));intervals.push({owner,task:message.task,started,finished:Date.now()});};
  if(seen.size<2)waiting.push(answer);else{for(const f of waiting.splice(0))f();answer();}
 });
 await new Promise(r=>server.listen(0,'127.0.0.1',r));const url=`http://127.0.0.1:${server.address().port}/v1/chat/completions`;
 async function launch(i,k){
  const journalPath=join(root,`${key}-${i}.journal.json`),sessionPath=join(root,`game-${game}-${i}.json`),id=runs[k].run.id;
  if(dropResponse&&k===1){
   let dropped=false;
   const fetcher=async(...args)=>{const r=await fetch(...args);if(!dropped&&String(args[0]).endsWith('/act')){dropped=true;await r.text();throw new Error('SYNTHETIC_LOST_RESPONSE');}return r;};
   await assert.rejects(runCycle({api:apis[i],id,provider:modelProvider({url,model:'TEST-loopback',test:true}),journalPath,trade:httpTrade(sessionPath,fetcher)}));
   const before=counts.get(profiles[i].wallet);
   const result=await runCycle({api:apis[i],id,provider:modelProvider({url,model:'TEST-loopback',test:true}),journalPath,trade:httpTrade(sessionPath)});
   assert.equal(result.status,'completed');assert.equal(counts.get(profiles[i].wallet),before,'recovery must not call the model again');assert.equal(readPrivate(sessionPath).pending_act,null);return;
  }
  await new Promise((yes,no)=>{const child=spawn(process.execPath,['tools/agent-bootstrap/strategy-run.mjs','--api',base,'--profile',join(root,`owner-${i}.json`),'--run',id,'--journal',journalPath,'--trade-session',sessionPath,'--allow-provider','yes','--test-provider','yes'],{cwd:process.cwd(),env:{...process.env,ALASHI_MODEL_URL:url,ALASHI_MODEL:'TEST-loopback',ALASHI_MODEL_KEY:''},stdio:['ignore','pipe','pipe']});let output='';child.stdout.on('data',b=>output+=b);child.stderr.on('data',b=>output+=b);child.on('error',no);child.on('close',code=>code===0?yes():no(new Error(output)));});
 }
 try{await Promise.all(owners.map(launch));}finally{server.close();}
 const lib=await request('/strategy-library'),finished=lib.runs.filter(r=>r.challenge===challenge.id).sort((a,b)=>owners.findIndex(i=>profiles[i].wallet===a.owner)-owners.findIndex(i=>profiles[i].wallet===b.owner));
 assert.equal(finished.length,2);assert.ok(finished.every(r=>r.status==='completed'&&r.result.trade_receipt&&r.test_provider));
 assert.deepEqual(finished.map(r=>r.known_tokens),[360,120]);
 assert.equal(counts.get(profiles[owners[0]].wallet),3);assert.equal(counts.get(profiles[owners[1]].wallet),1);
 const standing=lib.standings.find(s=>s.challenge===challenge.id).groups.find(g=>g.test_provider);
 if(phase==='action'){assert.equal(finished[0].result.trade_receipt.goods_delta,2);assert.equal(finished[1].result.trade_receipt.goods_delta,2);assert.equal(standing.token_leaders.length,1);assert.equal(standing.tokens,120);}
 else{assert.deepEqual(finished.map(r=>r.result.trade_receipt.cash_delta).sort(),['17000000','22000000']);assert.equal(standing.token_leaders.length,0,'unequal market outcomes cannot establish token efficiency');}
 const feed=await request(`/game/${game}/live/events?after=0&limit=100`),receipts=finished.map(r=>r.result.trade_receipt.event_id);
 assert.equal(feed.events.filter(e=>receipts.includes(e.event_id)).length,2,'one accepted action per run');
 rounds.push({challenge:challenge.id,phase,owners,intervals,runs:finished.map(r=>({id:r.id,version:r.version,owner:r.owner,known_tokens:r.known_tokens,receipt:r.result.trade_receipt})),standing,lost_response_recovered:dropResponse});
}
for(const [iteration,owners]of [[1,[1,2]],[2,[1,3]]]){
 const game=await newGame(owners,`TEST_STRATEGY_REUSE_${iteration}`);games.push(game);
 await duel(game,owners,[original.id,derivative.id],'action',`produce-${iteration}`,{dropResponse:iteration===2});
 await advance(game);await advance(game);
 await duel(game,owners,[original.id,derivative.id],'market',`sell-${iteration}`);
 for(let n=0;n<25;n++){const s=await request(`/game/${game}/state`);if(s.finished)break;await advance(game);}
 const ended=await request(`/game/${game}/state`);assert.equal(ended.finished,true,'ordinary six-round game must settle');
}
const library=await request('/strategy-library');assert.equal(library.versions.find(v=>v.id===derivative.id).reuse_owners,1);
assert.equal(totalCalls,16);
writeFileSync(join(root,'cycle-public.json'),JSON.stringify({source:'SYNTHETIC_LOOPBACK_ACCEPTANCE',real_model_calls:0,test_provider_calls:totalCalls,publish_derivative_gate:blocked,original:original.id,improved:derivative.id,games,rounds,library},null,2));
console.log(JSON.stringify({ok:true,games,rounds:rounds.length,test_provider_calls:totalCalls,real_model_calls:0,reuse_owners:1}));
