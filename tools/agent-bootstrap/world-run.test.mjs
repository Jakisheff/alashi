import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, existsSync, readdirSync, chmodSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { initWorld, runWorld, bitllmProvider, validateSnapshot, messagesForAgent } from './world-run.mjs';

const read = p => JSON.parse(readFileSync(p,'utf8'));
const save = (p,v) => writeFileSync(p,JSON.stringify(v),{mode:0o600});
const digest = value => createHash('sha256').update(JSON.stringify(value)).digest('hex');
const usage = {prompt_tokens:8,completion_tokens:2,total_tokens:10};
const wait = ms => new Promise(resolve=>setTimeout(resolve,ms));
const pick = messages => {
  const {allowed_actions:menu,phase,stage}=JSON.parse(messages[1].content);
  const desired=stage==='post_vote'?'pass':phase==='market'?'sell':phase==='action'?'produce':menu.some(x=>x.action==='vote_yes')?'vote_yes':'vote';
  const choices=menu.filter(x=>x.action===desired);
  return structuredClone(choices.at(-1)??menu[0]);
};
const response = messages => ({action:pick(messages),usage:{...usage},error:null,http_status:200,elapsed_ms:1,returned_model:'test-fixture',peer:'test-peer',finish_reason:'stop'});

// Protocol fixture, deliberately not a second implementation of ALASHI rules.
function fakeWorker({badActor=false,changedReplay=false,failAfterApply=false}={}) {
  let count=0,step=0,history=[],init;
  function snapshot() {
    const finished=step===24,phase=finished?'finished':['market','action','law','law'][step%4],stage=finished?'finished':step%4===3?'post_vote':'primary';
    const required=finished?[]:stage==='post_vote'?[0]:Array.from({length:count},(_,i)=>i);
    const menu=stage==='post_vote'?[{action:'pass',params:{}}]:phase==='market'?[{action:'pass',params:{}},{action:'sell',params:{units:1}}]:phase==='action'?[{action:'produce',params:{}}]:[{action:'vote',params:{choice:'yes'}}];
    const public_state=Array.from({length:count},(_,actor)=>({actor,name:`agent-${actor}`,cash:100+actor,goods:2,influence:1,alive:true,is_president:actor===0}));
    return {ok:true,type:'snapshot',step,game_id:init.game_id,round:finished?6:Math.floor(step/4)+1,phase,stage,state_hash:digest({init,step,history,changedReplay}),required_actors:required,
      agents:required.map(actor=>({actor:badActor?count:actor,name:`agent-${actor}`,observation:{cash:100+actor,goods:2,influence:1},allowed_actions:menu})),world:{public_state,rounds:6},
      ...(finished?{result:{participants:count,ranks:public_state.map(p=>p.actor),bank:count*100,payouts:[count*100]}}:{})};
  }
  return {async request(request) {
    if(request.op==='init'){init=request;count=request.agents;return snapshot();}
    if(request.op==='snapshot')return snapshot();
    assert.equal(request.op,'apply');const before=snapshot();
    assert.equal(request.expected_step,step);assert.equal(request.expected_state_hash,before.state_hash);
    assert.deepEqual([...request.decisions.map(d=>d.actor)].sort((a,b)=>a-b),before.required_actors);
    const receipts=request.decisions.map(d=>({actor:d.actor,accepted:d.action!==null,skipped:d.action===null,error:d.error??null,action:d.action,params:d.params}));
    history.push(request.decisions);step++;
    if(failAfterApply&&step===1)throw new Error('worker_reply_lost');
    return {ok:true,type:'applied',step:step-1,receipts,snapshot:snapshot()};
  },close(){}};
}
function fixture(t,options={}) {
  const root=mkdtempSync(join(tmpdir(),'alashi-world-test-'));t.after(()=>rmSync(root,{recursive:true,force:true}));
  const dir=join(root,'world'),binary=options.worker??process.execPath;
  initWorld(dir,{worker:binary,source:'test_provider',agents:options.agents??3,parallelism:options.parallelism??2,maxCalls:options.maxCalls??2000,maxTokens:options.maxTokens??2_000_000,maxConsecutiveFailures:options.maxConsecutiveFailures??5});
  const frozen=read(join(dir,'frozen.json')),calls=[];
  const provider={profile:frozen.config.profile,complete:async messages=>{calls.push(messages);return response(messages);}};
  return {root,dir,frozen,calls,provider,start:extra=>runWorld({dir,allowProvider:true,provider,workerFactory:()=>fakeWorker(),...extra})};
}

test('100 distinct players complete six protocol rounds with bounded concurrency and resume makes zero calls',async t=>{
  const f=fixture(t,{agents:100});let active=0,peak=0;
  f.provider.complete=async messages=>{f.calls.push(messages);active++;peak=Math.max(peak,active);await wait(1);active--;return response(messages);};
  const result=await f.start();
  assert.equal(result.status,'completed');assert.equal(result.game_completed,true);assert.equal(result.agents,100);
  assert.equal(result.calls,1806);assert.equal(result.known_tokens,18060);assert.equal(result.agents_with_accepted_model_action,100);assert.equal(result.skipped_decisions,0);
  assert.equal(peak,2);assert.equal(new Set(f.calls.map(m=>JSON.parse(m[1].content).identity)).size,100);
  assert.ok(f.calls.every(m=>JSON.parse(m[1].content).selected_opponents.length<=7));
  assert.ok(f.calls.some(m=>JSON.parse(m[1].content).own_recent_receipts.length>0));
  assert.deepEqual(await f.start(),result);assert.equal(f.calls.length,1806);
  assert.equal(existsSync(join(f.dir,'.world.lock')),false);
});

test('missing usage remains unknown and invalid JSON has an explicit skipped decision',async t=>{
  const f=fixture(t);let n=0;
  f.provider.complete=async messages=>{f.calls.push(messages);return ++n===1?{usage:null,action:null,error:'invalid_action_json',http_status:200}:response(messages);};
  const result=await f.start();
  assert.equal(result.status,'completed_with_errors');assert.equal(result.game_completed,true);
  assert.equal(result.skipped_decisions,1);assert.equal(result.unknown_usage_calls,1);assert.ok(result.reserved_or_unknown_tokens>0);
  assert.equal(result.known_tokens,(result.calls-1)*10);
  const phase=read(join(f.dir,'phases','0000.json'));
  assert.deepEqual(phase.request.decisions[0],{actor:0,action:null,params:{},error:'invalid_action_json'});
});

test('a crash-marked inflight request is not retried, then resumes the common world',async t=>{
  const f=fixture(t,{parallelism:1});let interrupted=false;
  await assert.rejects(f.start({onProgress:e=>{if(e.event==='decision_saved'&&!interrupted){interrupted=true;throw new Error('test_crash');}}}),/test_crash/);
  assert.equal(f.calls.length,1);assert.equal(existsSync(join(f.dir,'.world.lock')),false);
  const path=join(f.dir,'calls','0000-000.json'),call=read(path);call.status='inflight';delete call.result;save(path,call);
  const result=await f.start();
  assert.equal(result.status,'completed_with_errors');assert.equal(result.unknown_usage_calls,1);assert.equal(result.skipped_decisions,1);
  assert.equal(f.calls.length,result.calls); // original request plus each remaining call exactly once
  assert.equal(read(path).status,'unknown');
});

test('prepared phase is replayed after a lost worker reply without repeating model work',async t=>{
  const f=fixture(t);await assert.rejects(f.start({workerFactory:()=>fakeWorker({failAfterApply:true})}),/worker_reply_lost/);
  assert.equal(f.calls.length,3);assert.equal(read(join(f.dir,'phases','0000.json')).response,null);
  const result=await f.start();assert.equal(result.status,'completed');assert.equal(f.calls.length,60);
  assert.ok(read(join(f.dir,'phases','0000.json')).response);
});

test('concurrent reservation stops at a global call budget without applying an incomplete phase',async t=>{
  const f=fixture(t,{agents:10,maxCalls:1});const result=await f.start();
  assert.equal(result.status,'stopped');assert.equal(result.reason,'call_budget');assert.equal(result.calls,1);assert.equal(f.calls.length,1);
  assert.equal(result.game_completed,false);assert.equal(readdirSync(join(f.dir,'phases')).length,0);
  assert.deepEqual(await f.start(),result);assert.equal(f.calls.length,1);
});

test('token reservation prevents any provider request before insufficient budget',async t=>{
  const f=fixture(t,{maxTokens:1});const result=await f.start();
  assert.equal(result.reason,'token_budget');assert.equal(f.calls.length,0);assert.equal(result.calls,0);
});

test('quota response stops new traffic, preserves in-flight result, and leaves world unapplied',async t=>{
  const f=fixture(t,{agents:20});let n=0;
  f.provider.complete=async messages=>{f.calls.push(messages);const call=++n;await wait(call===1?1:10);return call===1?{action:null,usage:null,error:'provider_http_429',http_status:429}:response(messages);};
  const result=await f.start();
  assert.equal(result.status,'stopped');assert.equal(result.reason,'provider_access_or_rate_limit');assert.equal(result.calls,2);
  assert.equal(result.unknown_usage_calls,1);assert.equal(readdirSync(join(f.dir,'phases')).length,0);
});

test('sustained malformed outputs stop at the frozen failure threshold',async t=>{
  const f=fixture(t,{agents:20,parallelism:1,maxConsecutiveFailures:3});
  f.provider.complete=async messages=>{f.calls.push(messages);return {action:null,usage:{...usage},error:'invalid_action_json',http_status:200};};
  const result=await f.start();assert.equal(result.reason,'provider_failure_threshold');assert.equal(result.calls,3);
  assert.equal(result.game_completed,false);assert.equal(result.known_tokens,30);
});

test('wrong actor set and changed provider are rejected before any model request',async t=>{
  const f=fixture(t);
  await assert.rejects(f.start({workerFactory:()=>fakeWorker({badActor:true})}),/invalid_world_actor_set/);
  await assert.rejects(f.start({provider:{...f.provider,profile:{...f.provider.profile,model:'other'}}}),/world_provider_profile_mismatch/);
  assert.equal(f.calls.length,0);assert.equal(existsSync(join(f.dir,'.world.lock')),false);
});

test('frozen config and worker binary changes cannot silently resume',async t=>{
  const root=mkdtempSync(join(tmpdir(),'alashi-world-binary-'));t.after(()=>rmSync(root,{recursive:true,force:true}));
  const binary=join(root,'worker');writeFileSync(binary,'original');
  const f=fixture(t,{worker:binary});writeFileSync(binary,'modified');
  await assert.rejects(f.start(),/world_runtime_changed/);assert.equal(f.calls.length,0);
  const other=fixture(t),p=join(other.dir,'frozen.json'),v=read(p);v.config.parallelism=3;save(p,v);
  await assert.rejects(other.start(),/world_frozen_config_changed/);assert.equal(other.calls.length,0);
});

test('invalid journal validation does not leave a lock and changed replay is rejected',async t=>{
  const f=fixture(t);await f.start();
  await assert.rejects(f.start({workerFactory:()=>fakeWorker({changedReplay:true})}),/world_replay_scope_mismatch/);
  assert.equal(existsSync(join(f.dir,'.world.lock')),false);
  const p=join(f.dir,'calls','0000-000.json'),c=read(p);c.digest='bad';save(p,c);
  await assert.rejects(f.start(),/invalid_world_call_journal/);assert.equal(existsSync(join(f.dir,'.world.lock')),false);
});

test('provider sends explicit bounded thinking settings and preserves provenance without reasoning text',async()=>{
  const profile={url:'http://127.0.0.1:7778/v1/chat/completions',model:'Qwen-test',source:'test_provider',temperature:0,max_tokens:256,chat_template_kwargs:{enable_thinking:false}};
  let sent;
  const provider=bitllmProvider(profile,{fetcher:async(url,options)=>{sent={url,options,body:JSON.parse(options.body)};return new Response(JSON.stringify({id:'response-1',model:'declared-upstream',choices:[{message:{content:'{"action":"pass","params":{}}',reasoning_content:'DO_NOT_STORE_REASONING'},finish_reason:'stop'}],usage:{...usage}}),{headers:{'x-bitllm-peer':'peer-1'}});}});
  const result=await provider.complete([{role:'user',content:'public test'}]);
  assert.deepEqual(sent.body.chat_template_kwargs,{enable_thinking:false});assert.equal(sent.body.max_tokens,256);assert.equal(sent.options.redirect,'error');
  assert.equal(result.peer,'peer-1');assert.equal(result.returned_model,'declared-upstream');assert.equal(result.usage.total_tokens,10);
  assert.ok(!JSON.stringify(result).includes('DO_NOT_STORE_REASONING'));assert.deepEqual(result.action,{action:'pass',params:{}});
});

test('actual Rust worker: 100 players finish six rounds, then resume issues zero model calls',{skip:!process.env.WORLD_WORKER,timeout:120000},async t=>{
  const f=fixture(t,{worker:resolve(process.env.WORLD_WORKER),agents:100});
  const result=await f.start({workerFactory:undefined});
  assert.equal(result.game_completed,true);assert.equal(result.round,6);assert.equal(result.agents,100);
  assert.equal(result.agents_with_model_response,100);assert.equal(result.agents_with_accepted_model_action,100);
  assert.ok(result.calls>=1800&&result.calls<=1806);assert.equal(result.unknown_usage_calls,0);assert.equal(result.skipped_decisions,0);
  const count=f.calls.length;const resumed=await f.start({workerFactory:undefined});assert.deepEqual(resumed,result);assert.equal(f.calls.length,count);
});
