import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, existsSync, readdirSync, chmodSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { initWorld, runWorld, bitllmProvider, validateConfig, validateSnapshot, messagesForAgent } from './world-run.mjs';

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
function winnerResult(rows) {
  const order=[...rows].reverse().map(r=>r.actor),bank=rows.length*100,rake=rows.length*5;
  const payouts=rows.map(r=>r.actor===order[0]?bank-rake:0);
  return {payout_policy:'winner_takes_all',winner_actor:order[0],settlement_mode:'simulated',order,bank,rake,payouts,
    leaderboard:order.map((actor,i)=>({rank:i+1,actor,cash:rows[actor].cash,payout:payouts[actor]}))};
}

// Protocol fixture, deliberately not a second implementation of ALASHI rules.
function fakeWorker({badActor=false,changedReplay=false,failAfterApply=false}={}) {
  let count=0,step=0,history=[],init;
  function snapshot() {
    const finished=step===24,phase=finished?'finished':['market','action','law','law'][step%4],stage=finished?'finished':step%4===3?'post_vote':'primary';
    const required=finished?[]:stage==='post_vote'?[0]:Array.from({length:count},(_,i)=>i);
    const menu=stage==='post_vote'?[{action:'pass',params:{}}]:phase==='market'?[{action:'pass',params:{}},{action:'sell',params:{units:1}}]:phase==='action'?[{action:'produce',params:{}}]:[{action:'vote',params:{choice:'yes'}}];
    const public_state=Array.from({length:count},(_,actor)=>({actor,name:`agent-${actor}`,cash:100+actor,goods:2,influence:1,alive:true,is_president:actor===0}));
    const payout_policy=init.payout_policy??'canonical_top4',winnerMode=payout_policy==='winner_takes_all',round=finished?6:Math.floor(step/4)+1;
    return {ok:true,type:'snapshot',step,game_id:init.game_id,round,phase,stage,state_hash:digest({init,step,history,changedReplay}),required_actors:required,
      agents:required.map(actor=>({actor:badActor?count:actor,name:`agent-${actor}`,observation:{cash:100+actor,goods:2,influence:1},
        interaction_peers:Array.from({length:Math.min(3,count-1)},(_,i)=>(actor+i+1)%count),allowed_actions:menu,
        ...(winnerMode?{competition:{rank:count-actor,score:100+actor,leader_score:99+count,gap_to_leader:count-actor-1,execution_position:required.indexOf(actor)+1}}:{})})),
      world:{public_state,rounds:6,interaction_policy:'balanced_cyclic_peers_v1',payout_policy,
        ...(winnerMode?{epoch:0,payout_shares:[100],remaining_market_phases_including_current:finished?0:6-round+(phase==='market'?1:0),action_rules:{produce_cash_cost:0},
          sold_this_round:0,market_price:1600,market_unit_prices_by_sold_count:Array.from({length:16},(_,i)=>1600-i*100),market_price_curve_note:'Current fixture curve; last entry applies to higher sold counts, not a future forecast.'}:{})},
      ...(finished?{result:winnerMode?winnerResult(public_state):{payout_policy,participants:count,ranks:public_state.map(p=>p.actor),bank:count*100,payouts:[count*100]}}:{})};
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
  initWorld(dir,{worker:binary,source:'test_provider',agents:options.agents??3,epoch:options.epoch,payoutPolicy:options.payoutPolicy,comparison:options.comparison,parallelism:options.parallelism??2,maxCalls:options.maxCalls??2000,maxTokens:options.maxTokens??2_000_000,maxConsecutiveFailures:options.maxConsecutiveFailures??5});
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
  assert.ok(f.calls.every(m=>JSON.parse(m[1].content).selected_opponents.length===3));
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

test('visible opponents exactly follow ordered interaction peers independently of wealth, influence and row order',async()=>{
  const snapshot=await fakeWorker().request({op:'init',game_id:1,agents:8});
  const agent=snapshot.agents[0];agent.interaction_peers=[4,2,7];
  agent.allowed_actions=[{action:'produce',params:{}},...agent.interaction_peers.map(to=>({action:'bribe',params:{to,amount:5000000}}))];
  validateSnapshot(snapshot,8);
  const prompt=()=>JSON.parse(messagesForAgent(snapshot,agent,[],8)[1].content);
  const before=prompt();
  assert.deepEqual(before.selected_opponents.map(row=>row[0]),[4,2,7]);
  assert.deepEqual(before.allowed_actions.filter(a=>a.action==='bribe').map(a=>a.params.to),[4,2,7]);
  assert.equal(before.world.interaction_policy,'balanced_cyclic_peers_v1');
  for (const row of snapshot.world.public_state) {row.cash=(8-row.actor)*100000000;row.influence=100-row.actor;}
  snapshot.world.public_state.reverse();validateSnapshot(snapshot,8);
  const after=prompt();
  assert.deepEqual(after.selected_opponents.map(row=>row[0]),[4,2,7]);
  assert.deepEqual(after.selected_opponents.map(row=>row[1]),[400000000,600000000,100000000]);
  assert.deepEqual(after.allowed_actions,before.allowed_actions);
});

test('malformed interaction peers, public references and unseen bribe targets are rejected',async()=>{
  const original=await fakeWorker().request({op:'init',game_id:1,agents:8});
  const mutations=[
    s=>{delete s.agents[0].interaction_peers;},
    s=>{s.agents[0].interaction_peers=[1,2];},
    s=>{s.agents[0].interaction_peers=[1,2,3,4];},
    s=>{s.agents[0].interaction_peers=[0,2,3];},
    s=>{s.agents[0].interaction_peers=[1,1,3];},
    s=>{s.agents[0].interaction_peers=[-1,2,3];},
    s=>{s.agents[0].interaction_peers=[8,2,3];},
    s=>{s.agents[0].interaction_peers=['1',2,3];},
    s=>{s.agents[0].interaction_peers=[1.5,2,3];},
    s=>{s.world.public_state[1].alive=false;},
    s=>{s.world.public_state[0].alive=false;},
    s=>{s.world.public_state.pop();},
    s=>{s.world.public_state[1].actor=0;},
    s=>{s.world.public_state[1].actor=8;},
    s=>{delete s.world.public_state[1].alive;},
    s=>{s.world.interaction_policy='wealth_ranked';},
    s=>{s.agents[0].allowed_actions.push({action:'bribe',params:{to:7,amount:5000000}});},
    s=>{s.agents[0].allowed_actions.push({action:'bribe',params:{to:'1',amount:5000000}});}
  ];
  for (const mutate of mutations) {
    const snapshot=structuredClone(original);mutate(snapshot);
    assert.throws(()=>validateSnapshot(snapshot,8),/invalid_world_(interaction_peers|public_rows)/);
  }
});

test('invalid peers stop before any model request and small worlds show every other living actor',async t=>{
  const f=fixture(t);
  await assert.rejects(f.start({workerFactory:()=>{
    const worker=fakeWorker();return {...worker,async request(input){const result=await worker.request(input);result.agents[0].interaction_peers=[0,1];return result;}};
  }}),/invalid_world_interaction_peers/);
  assert.equal(f.calls.length,0);assert.equal(existsSync(join(f.dir,'.world.lock')),false);
  for (const count of [2,3,4]) {
    const snapshot=await fakeWorker().request({op:'init',game_id:1,agents:count});validateSnapshot(snapshot,count);
    for (const agent of snapshot.agents) {
      const prompt=JSON.parse(messagesForAgent(snapshot,agent,[],count)[1].content);
      assert.equal(prompt.selected_opponents.length,count-1);
      assert.deepEqual(new Set(prompt.selected_opponents.map(row=>row[0])),new Set(Array.from({length:count},(_,i)=>i).filter(i=>i!==agent.actor)));
    }
  }
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

test('actual Rust worker: 100 players finish six rounds, then resume issues zero model calls',{skip:!process.env.WORLD_WORKER,timeout:300000},async t=>{
  const f=fixture(t,{worker:resolve(process.env.WORLD_WORKER),agents:100});
  let menusWithBribes=0;
  f.provider.complete=async messages=>{
    const prompt=JSON.parse(messages[1].content),seen=prompt.selected_opponents.map(row=>row[0]);
    assert.equal(prompt.world.interaction_policy,'balanced_cyclic_peers_v1');assert.equal(seen.length,3);
    assert.equal(new Set(seen).size,3);assert.ok(!seen.includes(prompt.player));
    const bribes=prompt.allowed_actions.filter(a=>a.action==='bribe');
    if(bribes.length){menusWithBribes++;assert.deepEqual(bribes.map(a=>a.params.to),seen);}
    f.calls.push(messages);return response(messages);
  };
  const result=await f.start({workerFactory:undefined});
  assert.equal(result.game_completed,true);assert.equal(result.round,6);assert.equal(result.agents,100);
  assert.equal(result.agents_with_model_response,100);assert.equal(result.agents_with_accepted_model_action,100);
  assert.ok(result.calls>=1800&&result.calls<=1806);assert.equal(result.unknown_usage_calls,0);assert.equal(result.skipped_decisions,0);
  assert.ok(menusWithBribes>0);
  const count=f.calls.length;const resumed=await f.start({workerFactory:undefined});assert.deepEqual(resumed,result);assert.equal(f.calls.length,count);
});

test('winner mode is explicit and frozen; unknown policies and epoch 1 fail before a run is created',async t=>{
  const legacy=fixture(t),canonical=fixture(t,{payoutPolicy:'canonical_top4'}),winner=fixture(t,{payoutPolicy:'winner_takes_all'});
  assert.equal(Object.hasOwn(legacy.frozen.config.init,'payout_policy'),false);
  assert.equal(canonical.frozen.config.init.payout_policy,'canonical_top4');
  assert.equal(winner.frozen.config.init.payout_policy,'winner_takes_all');
  for (const [payoutPolicy,epoch] of [['other',0],['',0],[null,0],[false,0],['winner_takes_all',1]]) {
    const dir=join(legacy.root,`invalid-${String(payoutPolicy)}-${epoch}`);
    assert.throws(()=>initWorld(dir,{worker:process.execPath,payoutPolicy,epoch}),/invalid_world_payout_policy/);
    assert.equal(existsSync(dir),false);
  }
  const changed=structuredClone(winner.frozen.config);changed.init.payout_policy='other';
  assert.throws(()=>validateConfig(changed),/invalid_world_payout_policy/);
  const path=join(winner.dir,'frozen.json'),tampered=read(path);tampered.config.init.payout_policy='canonical_top4';save(path,tampered);
  await assert.rejects(winner.start(),/world_frozen_config_changed/);assert.equal(winner.calls.length,0);
  const cliDir=join(legacy.root,'cli-winner');
  execFileSync(process.execPath,[fileURLToPath(new URL('./world-run.mjs',import.meta.url)),'init','--dir',cliDir,'--worker',process.execPath,'--payout-policy','winner_takes_all'],{stdio:'pipe'});
  assert.equal(read(join(cliDir,'frozen.json')).config.init.payout_policy,'winner_takes_all');
});

test('canonical prompt remains byte-identical; winner prompt includes rivalry, rank, free production and horizon',async()=>{
  const legacy=await fakeWorker().request({op:'init',game_id:1,agents:3});
  const legacyMessages=messagesForAgent(legacy,legacy.agents[0],[],3);
  // Captured from the unchanged v2 prompt before introducing this mode.
  assert.equal(digest(legacyMessages),'631db4b7f6172774142daba137c19dc4beb3dedcd96ca65ce664421bb0aa0f78');
  const canonical=await fakeWorker().request({op:'init',game_id:1,agents:3,payout_policy:'canonical_top4'});
  assert.deepEqual(messagesForAgent(canonical,canonical.agents[0],[],3),legacyMessages);
  const winner=await fakeWorker().request({op:'init',game_id:1,agents:3,payout_policy:'winner_takes_all'});
  validateSnapshot(winner,3,'winner_takes_all');
  const messages=messagesForAgent(winner,winner.agents[0],[],3),prompt=JSON.parse(messages[1].content);
  assert.equal(digest(messages),'48b82e91417331fa700fae63e1441807e58e81e1068f945fcec12fdac328a8e7');
  assert.match(messages[0].content,/Compete individually against every other player/);
  assert.match(messages[0].content,/only rank 1 receives the simulated bank after rake; all other players receive zero/);
  assert.match(messages[0].content,/own final cash/);assert.match(messages[0].content,/Goods have no final ranking value/);
  assert.match(messages[0].content,/earlier trades can change your actual quote/);assert.match(messages[0].content,/opening price is not guaranteed for you/);
  assert.match(messages[0].content,/Future laws are unknown/);
  assert.match(messages[0].content,/style only to break ties between equally promising winning choices/);
  assert.match(messages[0].content,/Preserve liquid wealth and avoid unnecessary risk/);
  assert.deepEqual(prompt.competition,{rank:3,score:100,leader_score:102,gap_to_leader:2,execution_position:1});
  assert.equal(prompt.world.payout_policy,'winner_takes_all');assert.deepEqual(prompt.world.payout_shares,[100]);
  assert.equal(prompt.world.action_rules.produce_cash_cost,0);assert.equal(prompt.world.remaining_market_phases_including_current,6);
  assert.deepEqual(prompt.world.market_unit_prices_by_sold_count,winner.world.market_unit_prices_by_sold_count);
  assert.equal(prompt.world.market_price_curve_note,winner.world.market_price_curve_note);
  assert.equal(Object.hasOwn(JSON.parse(legacyMessages[1].content),'competition'),false);
});

test('snapshot mode mismatch and invalid competitive facts stop before any provider call',async t=>{
  const cases=[
    ['winner_takes_all',s=>{s.world.payout_policy='canonical_top4';}],
    [undefined,s=>{s.world.payout_policy='winner_takes_all';}],
    ['winner_takes_all',s=>{delete s.world.payout_policy;}],
    ['winner_takes_all',s=>{delete s.agents[0].competition;}],
    ['winner_takes_all',s=>{s.agents[0].competition.score++;}],
    ['winner_takes_all',s=>{s.agents[0].competition.leader_score++;}],
    ['winner_takes_all',s=>{s.agents[0].competition.gap_to_leader++;}],
    ['winner_takes_all',s=>{s.agents[0].competition.rank=1;}],
    ['winner_takes_all',s=>{s.agents[0].competition.execution_position=2;}],
    ['winner_takes_all',s=>{s.world.remaining_market_phases_including_current=5;}],
    ['winner_takes_all',s=>{s.world.payout_shares=[50,25,15,10];}],
    ['winner_takes_all',s=>{s.world.epoch=1;}],
    ['winner_takes_all',s=>{s.world.action_rules.produce_cash_cost=1;}],
    ['winner_takes_all',s=>{s.world.market_unit_prices_by_sold_count.pop();}],
    ['winner_takes_all',s=>{s.world.market_unit_prices_by_sold_count[0]=-1;}],
    ['winner_takes_all',s=>{s.world.market_price++;}],
    ['winner_takes_all',s=>{s.world.market_price_curve_note=null;}]
  ];
  for (const [payoutPolicy,mutate] of cases) {
    const f=fixture(t,{payoutPolicy});
    await assert.rejects(f.start({workerFactory:()=>{
      const worker=fakeWorker();return {...worker,async request(input){const result=await worker.request(input);mutate(result);return result;}};
    }}),/world_payout_policy_mismatch|invalid_winner_(competition|world|market_curve)/);
    assert.equal(f.calls.length,0);assert.equal(existsSync(join(f.dir,'.world.lock')),false);
  }
});

test('winner validation handles tied scores and rejects tampered settlement and future market counts',async()=>{
  const original=await fakeWorker().request({op:'init',game_id:1,agents:3,payout_policy:'winner_takes_all'});
  const tied=structuredClone(original);
  for (const row of tied.world.public_state) row.cash=100;
  for (const agent of tied.agents) {agent.observation.cash=100;agent.competition={rank:agent.actor+1,score:100,leader_score:100,gap_to_leader:0,execution_position:agent.actor+1};}
  validateSnapshot(tied,3,'winner_takes_all'); // gap zero does not imply rank one
  const laterQuote=structuredClone(original);laterQuote.world.sold_this_round=100;laterQuote.world.market_price=laterQuote.world.market_unit_prices_by_sold_count.at(-1);
  validateSnapshot(laterQuote,3,'winner_takes_all');
  const reordered=structuredClone(original);reordered.required_actors.reverse();
  for (const agent of reordered.agents) agent.competition.execution_position=reordered.required_actors.indexOf(agent.actor)+1;
  validateSnapshot(reordered,3,'winner_takes_all');
  assert.equal(JSON.parse(messagesForAgent(reordered,reordered.agents[0],[],3)[1].content).competition.execution_position,3);
  for (const [round,phase,remaining] of [[1,'action',5],[6,'market',1],[6,'action',0],[6,'law',0]]) {
    const snapshot=structuredClone(original);snapshot.round=round;snapshot.phase=phase;snapshot.world.remaining_market_phases_including_current=remaining;
    validateSnapshot(snapshot,3,'winner_takes_all');
    snapshot.world.remaining_market_phases_including_current++;
    assert.throws(()=>validateSnapshot(snapshot,3,'winner_takes_all'),/invalid_winner_world/);
  }
  const finished={...original,round:6,phase:'finished',stage:'finished',required_actors:[],agents:[],result:winnerResult(original.world.public_state)};
  finished.world={...original.world,remaining_market_phases_including_current:0};
  validateSnapshot(finished,3,'winner_takes_all');
  const mutations=[
    s=>{s.result.payout_policy='canonical_top4';},
    s=>{s.result.winner_actor=0;},
    s=>{s.result.payouts[0]=1;},
    s=>{s.result.payouts[s.result.winner_actor]--;},
    s=>{s.result.payouts[s.result.winner_actor]=0;},
    s=>{s.result.payouts.push(0);},
    s=>{s.result.bank++;},
    s=>{s.result.rake=-1;},
    s=>{s.result.rake=s.result.bank;},
    s=>{s.result.order[0]=s.result.order[1];},
    s=>{s.result.leaderboard[0].actor=0;},
    s=>{s.result.leaderboard[0].rank=2;},
    s=>{s.result.leaderboard[0].cash++;},
    s=>{s.result.leaderboard[1].payout=1;},
    s=>{s.result.settlement_mode='on_chain';},
    s=>{s.world.remaining_market_phases_including_current=1;}
  ];
  for (const mutate of mutations) {
    const snapshot=structuredClone(finished);mutate(snapshot);
    assert.throws(()=>validateSnapshot(snapshot,3,'winner_takes_all'),/world_payout_policy_mismatch|invalid_winner_(settlement|world)/);
  }
});

test('winner protocol completes and resumes; a mode change after apply prevents the next model batch',async t=>{
  const f=fixture(t,{payoutPolicy:'winner_takes_all'});
  const result=await f.start();assert.equal(result.game_completed,true);assert.equal(result.payout_policy,'winner_takes_all');
  assert.equal(result.result.payouts.filter(x=>x>0).length,1);
  assert.equal(result.result.payouts[result.result.winner_actor],result.result.bank-result.result.rake);
  const calls=f.calls.length;assert.deepEqual(await f.start(),result);assert.equal(f.calls.length,calls);
  const changed=fixture(t,{payoutPolicy:'winner_takes_all'});
  await assert.rejects(changed.start({workerFactory:()=>{
    const worker=fakeWorker();return {...worker,async request(input){const result=await worker.request(input);if(input.op==='apply')result.snapshot.world.payout_policy='canonical_top4';return result;}};
  }}),/world_payout_policy_mismatch/);
  assert.equal(changed.calls.length,3);
});

test('actual Rust worker: 100 winner-takes-all players with focal market context settle and resume without model calls',{skip:!process.env.WORLD_WORKER,timeout:300000},async t=>{
  const comparison={id:'market_context_v1',arm:'candidate',focal_actor:0};
  const f=fixture(t,{worker:resolve(process.env.WORLD_WORKER),agents:100,payoutPolicy:'winner_takes_all',comparison});
  const result=await f.start({workerFactory:undefined});
  assert.equal(result.game_completed,true);assert.equal(result.round,6);assert.equal(result.agents,100);
  assert.equal(result.payout_policy,'winner_takes_all');assert.equal(result.agents_with_model_response,100);
  assert.equal(result.agents_with_accepted_model_action,100);assert.equal(result.unknown_usage_calls,0);assert.equal(result.skipped_decisions,0);
  assert.ok(result.calls>=1800&&result.calls<=1806);
  const r=result.result;assert.equal(r.payout_policy,'winner_takes_all');assert.equal(r.order[0],r.winner_actor);
  assert.equal(r.leaderboard[0].actor,r.winner_actor);assert.equal(r.payouts.filter(x=>x>0).length,1);
  assert.equal(r.payouts[r.winner_actor],r.bank-r.rake);
  assert.ok(f.calls.every(messages=>JSON.parse(messages[1].content).world.payout_policy==='winner_takes_all'));
  assert.deepEqual(result.comparison,comparison);
  for (const messages of f.calls) {
    const prompt=JSON.parse(messages[1].content),visible=prompt.player===0&&prompt.phase==='market';
    assert.equal(Object.hasOwn(prompt.world,'market_unit_prices_by_sold_count'),visible);
    assert.equal(Object.hasOwn(prompt.world,'market_price_curve_note'),visible);
    assert.equal(Object.hasOwn(prompt.competition,'execution_position'),visible);
    assert.equal(messages[0].content.includes('The supplied price curve is current'),visible);
    assert.equal(JSON.stringify(messages).includes('market_context_v1'),false);
  }
  const count=f.calls.length;assert.deepEqual(await f.start({workerFactory:undefined}),result);assert.equal(f.calls.length,count);
});

test('comparison freezes exact WTA-only metadata and CLI requires both arm and focal actor',async t=>{
  const comparison={id:'market_context_v1',arm:'baseline',focal_actor:0};
  const f=fixture(t,{payoutPolicy:'winner_takes_all',comparison});
  assert.deepEqual(f.frozen.config.comparison,comparison);
  for (const invalid of [null,{}, {...comparison,id:'other'}, {...comparison,arm:'other'}, {...comparison,focal_actor:-1},
    {...comparison,focal_actor:3}, {...comparison,focal_actor:0.5}, {...comparison,focal_actor:'0'},
    {id:'market_context_v1',arm:'baseline'}, {id:'market_context_v1',focal_actor:0}, {...comparison,extra:true}]) {
    const config=structuredClone(f.frozen.config);config.comparison=invalid;
    assert.throws(()=>validateConfig(config),/invalid_world_comparison/);
  }
  for (const policy of [undefined,'canonical_top4']) {
    const config=structuredClone(f.frozen.config);
    if(policy===undefined)delete config.init.payout_policy;else config.init.payout_policy=policy;
    assert.throws(()=>validateConfig(config),/invalid_world_comparison/);
  }
  const script=fileURLToPath(new URL('./world-run.mjs',import.meta.url));
  const common=['init','--worker',process.execPath,'--payout-policy','winner_takes_all'];
  const cases=[['--market-context-arm','baseline'],['--focal-actor','0'],['--market-context-arm','other','--focal-actor','0']];
  for (const [i,flags] of cases.entries()) {
    const dir=join(f.root,`bad-cli-${i}`);
    assert.throws(()=>execFileSync(process.execPath,[script,...common,'--dir',dir,...flags],{stdio:'pipe'}),/invalid_world_comparison/);
    assert.equal(existsSync(dir),false);
  }
  const dir=join(f.root,'pair-cli');
  execFileSync(process.execPath,[script,...common,'--dir',dir,'--market-context-arm','candidate','--focal-actor','0'],{stdio:'pipe'});
  assert.deepEqual(read(join(dir,'frozen.json')).config.comparison,{...comparison,arm:'candidate'});
  const frozenPath=join(f.dir,'frozen.json'),changed=read(frozenPath);changed.config.comparison.arm='candidate';save(frozenPath,changed);
  await assert.rejects(f.start(),/world_frozen_config_changed/);assert.equal(f.calls.length,0);
});

test('matched-state comparison changes only one information block for the focal market decision',async()=>{
  const snapshot=await fakeWorker().request({op:'init',game_id:20261110,seed:20261110,agents:100,payout_policy:'winner_takes_all'});
  validateSnapshot(snapshot,100,'winner_takes_all');
  const baseline={id:'market_context_v1',arm:'baseline',focal_actor:0},candidate={...baseline,arm:'candidate'};
  const untouched=structuredClone(snapshot),memory=[{round:1,phase:'action',action:'produce',params:{},accepted:true,error:null,cash_after:100,goods_after:2}];
  const priceClause='The supplied price curve is current; earlier trades can change your actual quote, so the opening price is not guaranteed for you. Future laws are unknown.';
  for (const agent of snapshot.agents) {
    const a=messagesForAgent(snapshot,agent,memory,100,baseline),b=messagesForAgent(snapshot,agent,memory,100,candidate);
    const ap=JSON.parse(a[1].content),bp=JSON.parse(b[1].content);
    assert.equal(Object.hasOwn(ap.world,'market_unit_prices_by_sold_count'),false);
    assert.equal(Object.hasOwn(ap.world,'market_price_curve_note'),false);
    assert.equal(Object.hasOwn(ap.competition,'execution_position'),false);
    assert.equal(a[0].content.includes(priceClause),false);
    assert.equal(JSON.stringify(a).includes('market_context_v1'),false);assert.equal(JSON.stringify(b).includes('market_context_v1'),false);
    if(agent.actor!==0){assert.deepEqual(b,a);continue;}
    assert.deepEqual(b,messagesForAgent(snapshot,agent,memory,100)); // prepared full WTA prompt
    assert.deepEqual(bp.world.market_unit_prices_by_sold_count,snapshot.world.market_unit_prices_by_sold_count);
    assert.equal(bp.world.market_price_curve_note,snapshot.world.market_price_curve_note);
    assert.equal(bp.competition.execution_position,agent.competition.execution_position);
    delete bp.world.market_unit_prices_by_sold_count;delete bp.world.market_price_curve_note;delete bp.competition.execution_position;
    assert.equal(JSON.stringify(bp),a[1].content);
    assert.equal(b[0].content.replace(' '+priceClause,''),a[0].content);
  }
  assert.deepEqual(snapshot,untouched); // redaction never mutates canonical evidence
  for (const phase of ['action','law']) for (const stage of ['primary','post_vote']) {
    const state={...snapshot,phase,stage};
    for (const agent of state.agents) {
      const a=messagesForAgent(state,agent,memory,100,baseline),b=messagesForAgent(state,agent,memory,100,candidate);
      assert.deepEqual(a,b);assert.equal(b[0].content.includes(priceClause),false);
      assert.equal(Object.hasOwn(JSON.parse(b[1].content).competition,'execution_position'),false);
    }
  }
  const movedFocal={...candidate,focal_actor:17};
  assert.deepEqual(messagesForAgent(snapshot,snapshot.agents[0],memory,100,movedFocal),messagesForAgent(snapshot,snapshot.agents[0],memory,100,baseline));
  assert.deepEqual(messagesForAgent(snapshot,snapshot.agents[17],memory,100,movedFocal),messagesForAgent(snapshot,snapshot.agents[17],memory,100));
});

test('comparison decisions are reused on resume and an opposite-arm request hash is rejected',async t=>{
  const baseline={id:'market_context_v1',arm:'baseline',focal_actor:0},candidate={...baseline,arm:'candidate'};
  const f=fixture(t,{payoutPolicy:'winner_takes_all',comparison:baseline,parallelism:1,maxCalls:1});
  const result=await f.start();assert.equal(result.reason,'call_budget');assert.deepEqual(result.comparison,baseline);
  assert.equal(f.calls.length,1);assert.deepEqual(await f.start(),result);assert.equal(f.calls.length,1);
  const other=fixture(t,{payoutPolicy:'winner_takes_all',comparison:candidate,parallelism:1,maxCalls:1});
  await other.start();assert.equal(other.calls.length,1);
  const path=join(f.dir,'calls','0000-000.json'),call=read(path),candidateCall=read(join(other.dir,'calls','0000-000.json'));
  assert.notEqual(call.request_sha256,candidateCall.request_sha256);
  call.request_sha256=candidateCall.request_sha256;save(path,call);
  await assert.rejects(f.start(),/world_call_scope_mismatch/);assert.equal(f.calls.length,1);
});
