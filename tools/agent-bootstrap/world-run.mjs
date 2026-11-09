#!/usr/bin/env node
/** Experimental single-world driver. Rust owns every game rule and transition. */
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { spawn } from 'node:child_process';
import { existsSync, lstatSync, mkdirSync, openSync, closeSync, writeFileSync, readFileSync, renameSync, unlinkSync, fsyncSync, readdirSync } from 'node:fs';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { isDeepStrictEqual } from 'node:util';

const SCHEMA = 'alashi-bitllm-world-v1';
const ownPath = fileURLToPath(import.meta.url);
const hash = value => createHash('sha256').update(value).digest('hex');
const canonical = value => JSON.stringify(value, (_key, item) => item && typeof item === 'object' && !Array.isArray(item)
  ? Object.fromEntries(Object.keys(item).sort().map(key => [key, item[key]])) : item);
const fail = code => { throw new Error(code); };
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const integer = (n, min, max) => Number.isSafeInteger(n) && n >= min && n <= max;
const exact = (value, keys) => object(value) && Object.keys(value).length === keys.length && keys.every(k => Object.hasOwn(value, k));
const short = (s, max = 256) => typeof s === 'string' && s.length > 0 && Buffer.byteLength(s) <= max;
const id = (step, actor) => `${String(step).padStart(4, '0')}-${String(actor).padStart(3, '0')}`;

function privateDir(path) {
  const stat = lstatSync(path);
  if (!stat.isDirectory() || stat.isSymbolicLink() || process.platform !== 'win32' && (stat.mode & 0o077)) fail('world_directory_requires_mode_700');
}
function read(path) {
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 25_000_000 || process.platform !== 'win32' && (stat.mode & 0o077)) fail('world_file_requires_private_regular_file');
  return JSON.parse(readFileSync(path, 'utf8'));
}
function save(path, value) {
  if (existsSync(path)) read(path);
  const temporary = `${path}.tmp-${randomUUID()}`, fd = openSync(temporary, 'wx', 0o600);
  try { writeFileSync(fd, JSON.stringify(value, null, 2) + '\n'); fsyncSync(fd); } finally { closeSync(fd); }
  try { renameSync(temporary, path); } catch (error) { unlinkSync(temporary); throw error; }
  const directory = openSync(dirname(path), 'r');
  try { fsyncSync(directory); } finally { closeSync(directory); }
}
function fileHash(path) {
  const stat = lstatSync(path);
  if (!stat.isFile() || stat.isSymbolicLink()) fail('worker_requires_regular_file');
  return hash(readFileSync(path));
}
function profileValid(profile) {
  if (!exact(profile, ['url','model','source','temperature','max_tokens','chat_template_kwargs'])
      || !short(profile.model, 128) || !['provider_reported','test_provider'].includes(profile.source)
      || profile.temperature !== 0 || profile.max_tokens !== 256
      || !isDeepStrictEqual(profile.chat_template_kwargs, {enable_thinking:false})) fail('invalid_world_profile');
  const u = new URL(profile.url);
  if (u.protocol !== 'http:' || !['127.0.0.1','localhost','[::1]'].includes(u.hostname)
      || u.username || u.password || u.search || u.hash || u.pathname !== '/v1/chat/completions') fail('world_requires_loopback_bitllm');
}
export function validateConfig(c) {
  const configKeys=['schema','worker_path','worker_sha256','driver_sha256','init','profile','parallelism','max_calls','max_tokens','call_timeout_ms','max_seconds','max_consecutive_provider_failures'];
  if (object(c)&&Object.hasOwn(c,'comparison')) configKeys.push('comparison');
  if (!exact(c,configKeys)) fail('invalid_world_config');
  if (c.schema !== SCHEMA || !short(c.worker_path, 4096) || !/^[a-f0-9]{64}$/.test(c.worker_sha256) || !/^[a-f0-9]{64}$/.test(c.driver_sha256)) fail('invalid_world_identity');
  const initKeys=['op','experimental','game_id','seed','agents','epoch','entry_fee'];
  if (object(c.init)&&Object.hasOwn(c.init,'payout_policy')) initKeys.push('payout_policy');
  if (!exact(c.init, initKeys) || c.init.op !== 'init' || c.init.experimental !== true
      || !integer(c.init.game_id,1,Number.MAX_SAFE_INTEGER) || !integer(c.init.seed,0,Number.MAX_SAFE_INTEGER)
      || !integer(c.init.agents,2,100) || !integer(c.init.epoch,0,1) || !integer(c.init.entry_fee,0,1_000_000_000)) fail('invalid_world_init');
  if (Object.hasOwn(c.init,'payout_policy')&&!['canonical_top4','winner_takes_all'].includes(c.init.payout_policy)
      || c.init.payout_policy==='winner_takes_all'&&c.init.epoch!==0) fail('invalid_world_payout_policy');
  if (Object.hasOwn(c,'comparison')) validateComparison(c.comparison,c.init.payout_policy,c.init.agents);
  profileValid(c.profile);
  if (!integer(c.parallelism,1,8) || !integer(c.max_calls,1,5000) || !integer(c.max_tokens,1,10_000_000)
      || !integer(c.call_timeout_ms,1000,60000) || !integer(c.max_seconds,1,14400) || !integer(c.max_consecutive_provider_failures,1,100)) fail('invalid_world_budget');
  return c;
}
function validateComparison(comparison,policy,count) {
  if (policy!=='winner_takes_all'||!exact(comparison,['id','arm','focal_actor'])||comparison.id!=='market_context_v1'
      || !['baseline','candidate'].includes(comparison.arm)||!integer(comparison.focal_actor,0,count-1)) fail('invalid_world_comparison');
}
export function initWorld(dir, options) {
  const worker = resolve(options.worker);
  const config = validateConfig({schema:SCHEMA,worker_path:worker,worker_sha256:fileHash(worker),driver_sha256:fileHash(ownPath),
    init:{op:'init',experimental:true,game_id:options.gameId ?? 20261109,seed:options.seed ?? 20261109,agents:options.agents ?? 100,epoch:options.epoch ?? 0,entry_fee:10_000_000,...(options.payoutPolicy===undefined?{}:{payout_policy:options.payoutPolicy})},
    profile:{url:options.url ?? 'http://127.0.0.1:7778/v1/chat/completions',model:options.model ?? 'qwen3.8-flash-next-fp4-alesha',source:options.source ?? 'provider_reported',temperature:0,max_tokens:256,chat_template_kwargs:{enable_thinking:false}},
    parallelism:options.parallelism ?? 2,max_calls:options.maxCalls ?? 2000,max_tokens:options.maxTokens ?? 2_000_000,call_timeout_ms:options.callTimeoutMs ?? 60000,max_seconds:options.maxSeconds ?? 7200,max_consecutive_provider_failures:options.maxConsecutiveFailures ?? 5,
    ...(options.comparison===undefined?{}:{comparison:options.comparison})});
  mkdirSync(dir,{mode:0o700});
  for (const folder of ['calls','phases','snapshots']) mkdirSync(join(dir,folder),{mode:0o700});
  const salt = randomBytes(32).toString('hex'), frozen = {config,salt,digest:hash(canonical({config,salt}))};
  save(join(dir,'frozen.json'),frozen);
  return {directory:dir,digest:frozen.digest,status:'frozen',model_calls:0,next:'run --allow-provider yes explicitly sends game observations to BitLLM peers'};
}
function loadFrozen(dir) {
  privateDir(dir);
  const frozen = read(join(dir,'frozen.json')); validateConfig(frozen.config);
  if (!/^[a-f0-9]{64}$/.test(frozen.salt) || frozen.digest !== hash(canonical({config:frozen.config,salt:frozen.salt}))) fail('world_frozen_config_changed');
  return frozen;
}

export function createWorker(binary, args = []) {
  const child = spawn(binary,args,{stdio:['pipe','pipe','pipe']});
  let pending = null, buffer = '', closed = false, stderr = '';
  child.stderr.on('data',data => { stderr = (stderr + data.toString()).slice(-8192); });
  const reject = error => { if (pending) { clearTimeout(pending.timer); const p=pending; pending=null; p.reject(error); } };
  child.on('error',error => { closed=true; reject(error); });
  child.on('exit',code => { closed=true; reject(new Error(`world_worker_exited_${code}`)); });
  child.stdin.on('error',error => reject(error));
  child.stdout.on('data',data => {
    buffer += data.toString();
    if (Buffer.byteLength(buffer) > 20_000_000) { reject(new Error('world_worker_response_too_large')); child.kill(); return; }
    let end;
    while ((end=buffer.indexOf('\n')) >= 0) {
      const line=buffer.slice(0,end); buffer=buffer.slice(end+1);
      if (!line.trim()) continue;
      if (!pending) { child.kill(); closed=true; return; }
      const p=pending; pending=null; clearTimeout(p.timer);
      try { const value=JSON.parse(line); if (value.ok !== true) p.reject(new Error(`world_worker_refused_${value.error ?? 'unknown'}`)); else p.resolve(value); }
      catch (error) { p.reject(error); }
    }
  });
  return {
    request(value) {
      if (closed) return Promise.reject(new Error('world_worker_closed'));
      if (pending) return Promise.reject(new Error('world_worker_concurrent_request'));
      return new Promise((resolve,rejectRequest) => {
        const timer=setTimeout(()=>{reject(new Error('world_worker_timeout')); child.kill();},60000);
        pending={resolve,reject:rejectRequest,timer}; child.stdin.write(JSON.stringify(value)+'\n');
      });
    },
    close() { child.stdin.end(); child.kill('SIGTERM'); },
    get stderr() { return stderr; }
  };
}

export function validateSnapshot(s, count, expectedPolicy='canonical_top4') {
  if (!object(s) || s.ok !== true || s.type !== 'snapshot' || !integer(s.step,0,1000)
      || !integer(s.round,0,6) || !['market','action','law','finished'].includes(s.phase)
      || !['primary','post_vote','finished'].includes(s.stage) || !/^[a-f0-9]{64}$/.test(s.state_hash)
      || !Array.isArray(s.required_actors) || !Array.isArray(s.agents) || !object(s.world)) fail('invalid_world_snapshot');
  if (s.required_actors.some(a=>!integer(a,0,count-1)) || new Set(s.required_actors).size !== s.required_actors.length
      || s.agents.length !== s.required_actors.length || new Set(s.agents.map(a=>a.actor)).size !== s.agents.length
      || s.agents.some(a=>!s.required_actors.includes(a.actor) || !object(a.observation) || !Array.isArray(a.allowed_actions)
        || !a.allowed_actions.length || a.allowed_actions.length > 64 || a.allowed_actions.some(d=>!exact(d,['action','params']) || !short(d.action,64) || !object(d.params)))) fail('invalid_world_actor_set');
  if (s.phase === 'finished' && (s.required_actors.length || !object(s.result))) fail('invalid_finished_world');
  if (s.phase !== 'finished' && !s.required_actors.length) fail('world_has_no_required_actors');
  const rows=s.world.public_state;
  if (!Array.isArray(rows) || rows.length!==count || new Set(rows.map(r=>r?.actor)).size!==count
      || rows.some(r=>!object(r)||!integer(r.actor,0,count-1)||typeof r.alive!=='boolean')) fail('invalid_world_public_rows');
  const byActor=new Map(rows.map(r=>[r.actor,r])), peerCount=Math.min(3,rows.filter(r=>r.alive).length-1);
  if (s.world.interaction_policy!=='balanced_cyclic_peers_v1'
      || s.agents.some(a=>!byActor.get(a.actor).alive || !Array.isArray(a.interaction_peers)
        || a.interaction_peers.length!==peerCount || new Set(a.interaction_peers).size!==peerCount
        || a.interaction_peers.some(peer=>!integer(peer,0,count-1)||peer===a.actor||!byActor.get(peer)?.alive)
        || a.allowed_actions.some(d=>d.action==='bribe'&&!a.interaction_peers.includes(d.params.to)))) fail('invalid_world_interaction_peers');
  if (!['canonical_top4','winner_takes_all'].includes(expectedPolicy) || s.world.payout_policy!==expectedPolicy
      || s.phase==='finished'&&s.result.payout_policy!==expectedPolicy) fail('world_payout_policy_mismatch');
  if (expectedPolicy==='winner_takes_all') validateWinnerSnapshot(s,rows);
  return s;
}

function validateWinnerSnapshot(s, rows) {
  const count=rows.length,remaining=s.phase==='finished'?0:6-s.round+(s.phase==='market'?1:0);
  if (s.world.epoch!==0 || !isDeepStrictEqual(s.world.payout_shares,[100])
      || s.world.action_rules?.produce_cash_cost!==0
      || !integer(s.world.remaining_market_phases_including_current,0,6)
      || s.world.remaining_market_phases_including_current!==remaining
      || s.phase!=='finished'&&!integer(s.round,1,6)
      || s.phase==='finished'&&s.round!==6
      || rows.some(r=>!integer(r.cash,0,Number.MAX_SAFE_INTEGER))) fail('invalid_winner_world');
  const prices=s.world.market_unit_prices_by_sold_count;
  if (!Array.isArray(prices)||prices.length!==16||prices.some(price=>!integer(price,1,Number.MAX_SAFE_INTEGER))
      || !short(s.world.market_price_curve_note,1024) || !integer(s.world.sold_this_round,0,65535)
      || s.world.market_price!==prices[Math.min(s.world.sold_this_round,prices.length-1)]) fail('invalid_winner_market_curve');
  const leaderScore=Math.max(...rows.map(r=>r.cash)),byActor=new Map(rows.map(r=>[r.actor,r]));
  if (new Set(s.agents.map(a=>a.competition?.rank)).size!==s.agents.length
      || s.agents.some(a=>{
        const c=a.competition,score=byActor.get(a.actor).cash;
        return !exact(c,['rank','score','leader_score','gap_to_leader','execution_position']) || !integer(c.rank,1,count)
          || c.score!==score || c.leader_score!==leaderScore || c.gap_to_leader!==leaderScore-score
          || c.execution_position!==s.required_actors.indexOf(a.actor)+1
          || c.rank<=rows.filter(r=>r.cash>score).length || c.rank>rows.filter(r=>r.cash>=score).length;
      })) fail('invalid_winner_competition');
  if (s.phase!=='finished') return;
  const r=s.result;
  if (!integer(r.bank,1,Number.MAX_SAFE_INTEGER) || !integer(r.rake,0,r.bank-1)
      || !integer(r.winner_actor,0,count-1) || r.settlement_mode!=='simulated'
      || !Array.isArray(r.order) || r.order.length!==count || new Set(r.order).size!==count
      || r.order.some(actor=>!integer(actor,0,count-1)) || r.order[0]!==r.winner_actor
      || !Array.isArray(r.payouts) || r.payouts.length!==count
      || r.payouts.some((p,actor)=>p!==(actor===r.winner_actor?r.bank-r.rake:0))
      || !Array.isArray(r.leaderboard) || r.leaderboard.length!==count
      || r.leaderboard.some((row,i)=>!object(row)||row.actor!==r.order[i]||row.rank!==i+1
        || row.cash!==byActor.get(row.actor).cash||row.payout!==r.payouts[row.actor]
        || i>0&&row.cash>r.leaderboard[i-1].cash)) fail('invalid_winner_settlement');
}

const personas = [
  'Preserve liquid wealth and avoid unnecessary risk.',
  'Build goods for later sales and consider supply competition.',
  'Look for profitable trades while keeping a cash reserve.',
  'Consider political influence when it can improve your final wealth.',
  'Protect your current wealth from unfavorable laws.',
  'Explore a calculated risk when its stated tradeoff is attractive.',
  'Prefer simple productive actions over costly political maneuvers.',
  'Consider rivals and the diminishing prices caused by many sellers.',
  'Adapt to public voting and market outcomes rather than following a fixed policy.',
  'Balance production, cash and influence across the remaining rounds.'
];
export function messagesForAgent(snapshot, agent, memory, count, comparison=undefined) {
  if(comparison!==undefined)validateComparison(comparison,snapshot.world.payout_policy,count);
  const rows = Array.isArray(snapshot.world.public_state) ? snapshot.world.public_state : [];
  // Keep full snapshots on disk; send only game information relevant to a decision.
  const world = Object.fromEntries(['interaction_policy','objective','action_rules','rounds','peso','rake_bps','payout_shares','market_price','sold_this_round','active_tax_bps','active_subsidy_goods','active_price_shift','active_boom','law_card','law_name','veto_pending','president','current_yes_weight','current_no_weight','laws_passed','last_law_passed'].filter(k=>Object.hasOwn(snapshot.world,k)).map(k=>[k,snapshot.world[k]]));
  const winnerMode=snapshot.world.payout_policy==='winner_takes_all';
  const showMarketContext=comparison===undefined||comparison.arm==='candidate'&&agent.actor===comparison.focal_actor&&snapshot.phase==='market';
  if(winnerMode){
    for(const key of ['payout_policy','remaining_market_phases_including_current'])world[key]=snapshot.world[key];
    if(showMarketContext)for(const key of ['market_unit_prices_by_sold_count','market_price_curve_note'])world[key]=snapshot.world[key];
  }
  const byActor=new Map(rows.map(r=>[r.actor,r]));
  const selected=agent.interaction_peers.map(actor=>byActor.get(actor));
  const priceClause='The supplied price curve is current; earlier trades can change your actual quote, so the opening price is not guaranteed for you. Future laws are unknown.';
  const objective=winnerMode?`Compete individually against every other player: only rank 1 receives the simulated bank after rake; all other players receive zero. Maximize your chance of finishing rank 1 through your own final cash. Goods have no final ranking value; consider the remaining market phases.${showMarketContext?' '+priceClause:''} Ties use canonical wallet order. Use the style only to break ties between equally promising winning choices.`:'Maximize your final game wealth;';
  const competition=winnerMode?{...agent.competition}:undefined;
  if(competition&&!showMarketContext)delete competition.execution_position;
  return [{role:'system',content:`You are independent player ${agent.actor} of ${count} in one shared ALASHI political-economy game. ${objective} ${personas[agent.actor%personas.length]} You only know the supplied public observation and your own previous receipts. Select one exact object from allowed_actions and return only {"action":"...","params":{...}}. Do not invent actions, parameters, balances, outcomes or claims. The engine applies all decisions in a fixed order after collection; an action legal now can be rejected after earlier actions. There are six rounds. No commentary or reasoning.`},
    {role:'user',content:JSON.stringify({player:agent.actor,identity:`agent-${String(agent.actor).padStart(3,'0')}`,round:snapshot.round,phase:snapshot.phase,stage:snapshot.stage,observation:agent.observation,world,
      opponent_fields:['actor','cash','goods','influence','is_president'],selected_opponents:selected.map(r=>[r.actor,r.cash,r.goods,r.influence,r.is_president]),
      own_recent_receipts:memory.slice(-3).map(r=>({round:r.round,phase:r.phase,action:r.action,params:r.params,accepted:r.accepted,error:r.error,cash_after:r.cash_after,goods_after:r.goods_after})),allowed_actions:agent.allowed_actions,...(winnerMode?{competition}:{})})}];
}
function usageOf(raw) {
  if (!object(raw) || !['prompt_tokens','completion_tokens','total_tokens'].every(k=>integer(raw[k],0,Number.MAX_SAFE_INTEGER))
      || raw.prompt_tokens+raw.completion_tokens !== raw.total_tokens) return null;
  return {prompt_tokens:raw.prompt_tokens,completion_tokens:raw.completion_tokens,total_tokens:raw.total_tokens};
}
function actionOf(content) {
  if (typeof content !== 'string' || Buffer.byteLength(content)>8192) return null;
  const fenced=content.trim().match(/^```(?:json)?\s*\n([\s\S]*?)\n```$/i);
  let v;try{v=JSON.parse(fenced?fenced[1]:content);}catch{return null;}
  return exact(v,['action','params']) && short(v.action,64) && object(v.params) ? v : null;
}
async function limitedText(response) {
  const limit=1_000_000;
  if(Number(response.headers.get('content-length'))>limit) {if(response.body)void response.body.cancel().catch(()=>{});fail('provider_response_too_large');}
  if(!response.body)return '';
  const reader=response.body.getReader(),parts=[];let size=0;
  try {for(;;){const {done,value}=await reader.read();if(done)return Buffer.concat(parts,size).toString('utf8');size+=value.length;if(size>limit){void reader.cancel().catch(()=>{});fail('provider_response_too_large');}parts.push(value);}}
  finally{reader.releaseLock();}
}
export function bitllmProvider(profile, {key,timeoutMs=60000,fetcher=fetch} = {}) {
  profileValid(profile); const frozen=structuredClone(profile);
  return {profile:frozen,async complete(messages) {
    const begin=performance.now();
    const response=await fetcher(frozen.url,{method:'POST',redirect:'error',signal:AbortSignal.timeout(timeoutMs),headers:{'Content-Type':'application/json',...(key?{Authorization:`Bearer ${key}`}:{})},body:JSON.stringify({model:frozen.model,messages,temperature:0,max_tokens:256,chat_template_kwargs:{enable_thinking:false}})});
    const text=await limitedText(response);let body;try{body=JSON.parse(text);}catch{body={};}
    const choice=body.choices?.[0], content=choice?.message?.content;
    return {http_status:response.status,elapsed_ms:Math.round(performance.now()-begin),response_id:short(body.id,200)?body.id:null,
      requested_model:frozen.model,returned_model:short(body.model,128)?body.model:null,peer:short(response.headers.get('x-bitllm-peer'),256)?response.headers.get('x-bitllm-peer'):null,finish_reason:short(choice?.finish_reason,64)?choice.finish_reason:null,
      usage:usageOf(body.usage),content:typeof content==='string'?content.slice(0,8192):null,
      action:response.ok?actionOf(content):null,error:!response.ok?`provider_http_${response.status}`:actionOf(content)?null:'invalid_action_json'};
  }};
}

function loadCalls(dir,digest) {
  const calls=new Map();
  for(const name of readdirSync(join(dir,'calls')).filter(n=>/^[0-9]{4}-[0-9]{3}\.json$/.test(n)).sort()) {
    const c=read(join(dir,'calls',name));
    if(c.digest!==digest || name!==id(c.step,c.actor)+'.json' || !['inflight','completed','unknown'].includes(c.status)
        || !integer(c.reserved_tokens,1,1_000_000) || !/^[a-f0-9]{64}$/.test(c.request_sha256)) fail('invalid_world_call_journal');
    if(c.status==='completed' && (!object(c.result)||c.result.usage!==null&&!usageOf(c.result.usage))) fail('invalid_world_usage_journal');
    calls.set(id(c.step,c.actor),c);
  }
  return calls;
}
function accounting(calls) {
  let known=0,held=0,unknown=0,overrun=false;
  for(const c of calls.values()) {
    if(c.status==='completed' && c.result.usage) {known+=c.result.usage.total_tokens;if(c.result.usage.total_tokens>c.reserved_tokens)overrun=true;}
    else {held+=c.reserved_tokens;if(c.status!=='inflight')unknown++;}
  }
  return {calls:calls.size,known_tokens:known,reserved_or_unknown_tokens:held,unknown_usage_calls:unknown,budget_overrun:overrun};
}
function decisionFor(call,agent) {
  if(call.status!=='completed') return {actor:agent.actor,action:null,params:{},error:'provider_outcome_unknown'};
  if(call.result.error || !call.result.action) return {actor:agent.actor,action:null,params:{},error:call.result.error??'invalid_action_json'};
  if(!agent.allowed_actions.some(a=>isDeepStrictEqual(a,call.result.action))) return {actor:agent.actor,action:null,params:{},error:'action_not_in_snapshot_menu'};
  return {actor:agent.actor,...call.result.action};
}
function validateApply(response, request, count, expectedPolicy) {
  if(!object(response)||response.ok!==true||response.type!=='applied'||response.step!==request.expected_step
      ||!Array.isArray(response.receipts)||response.receipts.length!==request.decisions.length
      ||new Set(response.receipts.map(r=>r.actor)).size!==response.receipts.length
      ||response.receipts.some(r=>!request.decisions.some(d=>d.actor===r.actor)||typeof r.accepted!=='boolean'||typeof r.skipped!=='boolean'))fail('invalid_world_apply_response');
  validateSnapshot(response.snapshot,count,expectedPolicy);
  if(response.snapshot.step!==request.expected_step+1)fail('invalid_world_step_progress');
  return response;
}
function summaryFor(frozen,calls,snapshot,phases,status,reason=null) {
  const receipts=phases.flatMap(p=>p.response?.receipts??[]), stats=accounting(calls);
  const errors=stats.unknown_usage_calls>0||stats.budget_overrun||receipts.some(r=>!r.accepted||r.skipped);
  return {schema:SCHEMA,digest:frozen.digest,status:status==='completed'&&errors?'completed_with_errors':status,game_completed:snapshot?.phase==='finished',reason,game_id:frozen.config.init.game_id,agents:frozen.config.init.agents,
    profile:frozen.config.profile,step:snapshot?.step??null,round:snapshot?.round??null,phase:snapshot?.phase??null,state_hash:snapshot?.state_hash??null,...(frozen.config.init.payout_policy==='winner_takes_all'?{payout_policy:'winner_takes_all'}:{}),
    ...(frozen.config.comparison?{comparison:frozen.config.comparison}:{}),
    ...stats,accepted_actions:receipts.filter(r=>r.accepted&&!r.skipped).length,rejected_actions:receipts.filter(r=>!r.accepted&&!r.skipped).length,skipped_decisions:receipts.filter(r=>r.skipped).length,
    agents_with_model_response:new Set([...calls.values()].filter(c=>c.status==='completed'&&c.result.action).map(c=>c.actor)).size,
    agents_with_accepted_model_action:new Set(receipts.filter(r=>r.accepted&&!r.skipped).map(r=>r.actor)).size,
    result:snapshot?.result??null,nvidia_spend_usd:0,provider_cost_usd:null,alashi_http_requests:0,blockchain_transactions:0,
    evidence_status:frozen.config.profile.source==='test_provider'?'deterministic_test':'locally_recorded_peer_reported',
    limitations:['Model identity and token usage are peer reported.','Menus are a bounded legal subset; agents see a compact public view and their own recent receipts.','Same model with different identities/personas does not establish diverse independent intelligence.','Invalid or unavailable model decisions are explicit skipped receipts, never scripted replacements.']};
}
export async function runWorld({dir,allowProvider,workerPath,key,provider,workerFactory,onProgress=()=>{},now=()=>Date.now()}) {
  if(allowProvider!==true)fail('world_provider_consent_required');
  const frozen=loadFrozen(dir),c=frozen.config,binary=resolve(workerPath??c.worker_path);
  if(fileHash(binary)!==c.worker_sha256||fileHash(ownPath)!==c.driver_sha256)fail('world_runtime_changed');
  if((provider||workerFactory)&&c.profile.source!=='test_provider')fail('world_test_injection_requires_test_profile');
  const actual=provider??bitllmProvider(c.profile,{key,timeoutMs:c.call_timeout_ms});
  if(!isDeepStrictEqual(actual.profile,c.profile))fail('world_provider_profile_mismatch');
  const lock=join(dir,'.world.lock');let fd;
  try{fd=openSync(lock,'wx',0o600);}catch(error){if(error.code==='EEXIST')fail('world_busy_check_existing_process');throw error;}
  let worker,snapshot;const phases=[],memory=Array.from({length:c.init.agents},()=>[]);
  try {
    try{writeFileSync(fd,JSON.stringify({pid:process.pid}));fsyncSync(fd);}finally{closeSync(fd);}
    const calls=loadCalls(dir,frozen.digest);
    const metaPath=join(dir,'run.json'),meta=existsSync(metaPath)?read(metaPath):{digest:frozen.digest,started_at_ms:now()};
    if(meta.digest!==frozen.digest||!integer(meta.started_at_ms,0,Number.MAX_SAFE_INTEGER))fail('invalid_world_run_metadata');
    if(!existsSync(metaPath))save(metaPath,meta);
    for(const call of calls.values()) if(call.status==='inflight') {call.status='unknown';save(join(dir,'calls',id(call.step,call.actor)+'.json'),call);}
    worker=workerFactory?await workerFactory():createWorker(binary);
    snapshot=validateSnapshot(await worker.request(c.init),c.init.agents,c.init.payout_policy);
    if(snapshot.step!==0)fail('world_init_wrong_step');
    for(const name of readdirSync(join(dir,'phases')).filter(n=>/^[0-9]{4}\.json$/.test(n)).sort()) {
      const phase=read(join(dir,'phases',name));
      if(phase.digest!==frozen.digest||phase.request.expected_step!==snapshot.step||phase.request.expected_state_hash!==snapshot.state_hash)fail('world_replay_scope_mismatch');
      const answer=validateApply(await worker.request(phase.request),phase.request,c.init.agents,c.init.payout_policy);
      if(phase.response&&!isDeepStrictEqual(answer,phase.response))fail('world_replay_changed');
      if(!phase.response){phase.response=answer;save(join(dir,'phases',name),phase);}
      phases.push(phase);for(const r of answer.receipts)memory[r.actor].push({round:snapshot.round,phase:snapshot.phase,stage:snapshot.stage,...r});
      snapshot=answer.snapshot;
    }
    let stop=null,consecutiveFailures=0;
    while(snapshot.phase!=='finished'&&!stop) {
      const snapPath=join(dir,'snapshots',String(snapshot.step).padStart(4,'0')+'.json');
      if(existsSync(snapPath)){if(!isDeepStrictEqual(read(snapPath),snapshot))fail('world_saved_snapshot_changed');}else save(snapPath,snapshot);
      const queue=snapshot.required_actors.map(actor=>snapshot.agents.find(a=>a.actor===actor));let next=0;
      onProgress({event:'phase_started',step:snapshot.step,round:snapshot.round,phase:snapshot.phase,stage:snapshot.stage,actors:queue.length});
      async function lane() {
        try {
        while(next<queue.length&&!stop) {
          const agent=queue[next++],callId=id(snapshot.step,agent.actor),path=join(dir,'calls',callId+'.json');
          const messages=messagesForAgent(snapshot,agent,memory[agent.actor],c.init.agents,c.comparison),requestHash=hash(canonical({profile:c.profile,messages}));
          const old=calls.get(callId);
          if(old){if(old.request_sha256!==requestHash||old.state_hash!==snapshot.state_hash)fail('world_call_scope_mismatch');continue;}
          const stats=accounting(calls),reserve=Buffer.byteLength(JSON.stringify(messages))+2304;
          if(stats.budget_overrun||stats.known_tokens+stats.reserved_or_unknown_tokens+reserve>c.max_tokens){stop='token_budget';break;}
          if(stats.calls>=c.max_calls){stop='call_budget';break;}
          if(now()-meta.started_at_ms>c.max_seconds*1000){stop='wall_time_budget';break;}
          const call={digest:frozen.digest,step:snapshot.step,round:snapshot.round,phase:snapshot.phase,stage:snapshot.stage,actor:agent.actor,state_hash:snapshot.state_hash,request_sha256:requestHash,reserved_tokens:reserve,status:'inflight',started_at:new Date(now()).toISOString(),messages};
          save(path,call);calls.set(callId,call);
          const started=performance.now();
          try {call.result=await actual.complete(messages);if(!object(call.result)||call.result.usage!==null&&!usageOf(call.result.usage))fail('invalid_provider_result');}
          catch(error){call.result={usage:null,action:null,error:short(error.message,160)?error.message:'provider_failed',elapsed_ms:Math.round(performance.now()-started)};}
          if(call.result.action&&!agent.allowed_actions.some(a=>isDeepStrictEqual(a,call.result.action)))call.result.error='action_not_in_snapshot_menu';
          call.status='completed';save(path,call);
          if(call.result.error)consecutiveFailures++;else consecutiveFailures=0;
          if([401,403,429].includes(call.result.http_status))stop='provider_access_or_rate_limit';
          else if(consecutiveFailures>=c.max_consecutive_provider_failures)stop='provider_failure_threshold';
          onProgress({event:'decision_saved',step:snapshot.step,actor:agent.actor,error:call.result.error??null,...accounting(calls)});
        }
        } catch(error) {stop='local_runner_error';throw error;}
      }
      // Await every launched request before closing the journal or applying a phase.
      const lanes=await Promise.allSettled(Array.from({length:Math.min(c.parallelism,queue.length)},()=>lane()));
      const failure=lanes.find(r=>r.status==='rejected');if(failure)throw failure.reason;
      if(stop)break;
      const decisions=queue.map(agent=>decisionFor(calls.get(id(snapshot.step,agent.actor)),agent));
      const request={op:'apply',expected_step:snapshot.step,expected_state_hash:snapshot.state_hash,decisions};
      const phase={digest:frozen.digest,request,response:null},phasePath=join(dir,'phases',String(snapshot.step).padStart(4,'0')+'.json');
      save(phasePath,phase);
      const answer=validateApply(await worker.request(request),request,c.init.agents,c.init.payout_policy);phase.response=answer;save(phasePath,phase);phases.push(phase);
      for(const r of answer.receipts)memory[r.actor].push({round:snapshot.round,phase:snapshot.phase,stage:snapshot.stage,...r});
      snapshot=answer.snapshot;
      const interim=summaryFor(frozen,calls,snapshot,phases,snapshot.phase==='finished'?'completed':'running');save(join(dir,'summary.json'),interim);
      onProgress({event:'phase_completed',...interim});
    }
    const result=summaryFor(frozen,calls,snapshot,phases,snapshot.phase==='finished'?'completed':'stopped',stop);
    save(join(dir,'summary.json'),result);save(join(dir,'final-snapshot.json'),snapshot);return result;
  } finally {if(worker)worker.close();unlinkSync(lock);}
}

function options(args) {
  const result={};for(let i=0;i<args.length;i+=2){const key=args[i];if(!/^--[a-z-]+$/.test(key)||!args[i+1]||Object.hasOwn(result,key.slice(2)))fail('invalid_world_cli_options');result[key.slice(2)]=args[i+1];}return result;
}
export async function main(args=process.argv.slice(2)) {
  const command=args.shift(),opts=options(args),dir=opts.dir&&resolve(opts.dir);
  const allowed={init:['dir','worker','agents','seed','game-id','epoch','payout-policy','market-context-arm','focal-actor','model','url','parallelism','max-calls','max-tokens','max-seconds','max-consecutive-provider-failures','source'],run:['dir','worker','allow-provider'],report:['dir']};
  if(!dir||!allowed[command]||Object.keys(opts).some(k=>!allowed[command].includes(k)))fail('usage_world_run_init_run_report');
  let result;
  if(command==='init') {if(!opts.worker)fail('world_worker_required');const number=k=>opts[k]===undefined?undefined:Number(opts[k]);const comparison=opts['market-context-arm']!==undefined||opts['focal-actor']!==undefined?{id:'market_context_v1',arm:opts['market-context-arm'],focal_actor:number('focal-actor')}:undefined;result=initWorld(dir,{worker:opts.worker,agents:number('agents'),seed:number('seed'),gameId:number('game-id'),epoch:number('epoch'),payoutPolicy:opts['payout-policy'],comparison,model:opts.model,url:opts.url,parallelism:number('parallelism'),maxCalls:number('max-calls'),maxTokens:number('max-tokens'),maxSeconds:number('max-seconds'),maxConsecutiveFailures:number('max-consecutive-provider-failures'),source:opts.source});}
  if(command==='run') result=await runWorld({dir,allowProvider:opts['allow-provider']==='yes',workerPath:opts.worker,key:process.env.ALASHI_MODEL_KEY,onProgress:value=>process.stdout.write(JSON.stringify(value)+'\n')});
  if(command==='report') {loadFrozen(dir);result=read(join(dir,'summary.json'));}
  process.stdout.write(JSON.stringify(result,null,2)+'\n');
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href)main().catch(error=>{console.error(/^[a-z0-9_ -]{1,180}$/i.test(error.message)?error.message:'world_runner_failed');process.exitCode=1;});
