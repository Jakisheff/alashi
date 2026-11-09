#!/usr/bin/env node
// Standalone read-only audit. Never calls a provider or runWorld; worker use is CPU JSONL replay only.
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { resolve, join, dirname } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { isDeepStrictEqual as equal } from 'node:util';
import { auditWorld } from './alashi-world-evidence-audit.mjs';

export const PRICE_CLAUSE = 'The supplied price curve is current; earlier trades can change your actual quote, so the opening price is not guaranteed for you. Future laws are unknown.';
export const canonical = x => JSON.stringify(x, (_k,v) => v && typeof v === 'object' && !Array.isArray(v)
  ? Object.fromEntries(Object.keys(v).sort().map(k => [k,v[k]])) : v);
const sha = x => createHash('sha256').update(x).digest('hex');
const sum = xs => xs.reduce((a,b)=>a+b,0);
const hex = x => typeof x === 'string' && /^[a-f0-9]{64}$/.test(x);
const uint = x => Number.isSafeInteger(x) && x >= 0;
const modelLabel = x => typeof x === 'string' && /^[a-zA-Z0-9][a-zA-Z0-9._+-]{0,127}$/.test(x) ? x : x == null ? 'unreported' : 'redacted';
const id = (step,actor) => `${String(step).padStart(4,'0')}-${String(actor).padStart(3,'0')}`;
const phaseKey = s => `${s.round}:${s.phase}:${s.stage}`;
const location = x => ({round:x.round,phase:x.phase,stage:x.stage,actor:x.actor,execution_position:x.position});
const compareOrder = (a,b) => a.order-b.order || a.position-b.position || a.actor-b.actor;
const phaseOrder = s => (s.round-1)*4 + (s.phase==='market'?0:s.phase==='action'?1:s.stage==='post_vote'?3:2);
const require = (condition,code) => { if (!condition) throw Error(code); };
const SERIAL_DRIVER_SHA256='fb2f7ec36fb062d5ba5855f97b728e82cb23e526c5c3a1721318c50de0a7f34e';
export function serializedBoundsVerified(configs,driverHash) {
  return configs.length===2&&configs.every(c=>c.parallelism===1)&&driverHash===SERIAL_DRIVER_SHA256;
}
function read(path) {
  const stat=lstatSync(path); require(stat.isFile()&&!stat.isSymbolicLink()&&stat.size<=25_000_000,'unsafe_json_input');
  return JSON.parse(readFileSync(path,'utf8'));
}
function regularHash(path) {
  const stat=lstatSync(path);require(stat.isFile()&&!stat.isSymbolicLink()&&stat.size<500_000_000,'unsafe_runtime_input');
  return sha(readFileSync(path));
}
function manifest(directory) {
  const rows=[];
  function visit(folder,prefix='') {
    const stat=lstatSync(folder);require(stat.isDirectory()&&!stat.isSymbolicLink(),'unsafe_journal_directory');
    for(const name of readdirSync(folder).sort()) {
      const path=join(folder,name),st=lstatSync(path),relative=prefix+name;
      require(!st.isSymbolicLink(),'journal_symlink_not_allowed');
      if(st.isDirectory())visit(path,relative+'/');
      else {require(st.isFile()&&st.size<=25_000_000,'unsafe_journal_file');rows.push([relative,sha(readFileSync(path))]);}
    }
  }
  visit(directory);return {files:rows.length,sha256:sha(canonical(rows))};
}
function loadRun(directory) {
  const dir=resolve(directory);require(!existsSync(join(dir,'.world.lock')),'active_run_stop_before_audit');
  const frozen=read(join(dir,'frozen.json'));
  const snapshots=new Map(readdirSync(join(dir,'snapshots')).filter(n=>/^\d{4}\.json$/.test(n)).sort().map(n=>[Number(n.slice(0,4)),read(join(dir,'snapshots',n))]));
  const phases=readdirSync(join(dir,'phases')).filter(n=>/^\d{4}\.json$/.test(n)).sort().map(n=>read(join(dir,'phases',n)));
  const calls=new Map(readdirSync(join(dir,'calls')).filter(n=>/^\d{4}-\d{3}\.json$/.test(n)).sort().map(n=>[n.slice(0,-5),read(join(dir,'calls',n))]));
  return {dir,frozen,snapshots,phases,calls,summary:read(join(dir,'summary.json')),final:read(join(dir,'final-snapshot.json'))};
}

// Independent expected delta; this does not merely trust that both calls use the same driver function.
export function policyScopeIssues({baseline,candidate,snapshot,agent,focal=0}) {
  const issues=[],check=(ok,code)=>{if(!ok)issues.push(code);};
  try {
    check(baseline.length===2&&candidate.length===2&&baseline[0].role==='system'&&candidate[0].role==='system'
      &&baseline[1].role==='user'&&candidate[1].role==='user','prompt_message_shape');
    const b=JSON.parse(baseline[1].content),c=JSON.parse(candidate[1].content);
    check(!JSON.stringify(baseline).includes('market_context_v1')&&!JSON.stringify(candidate).includes('market_context_v1')
      &&!Object.hasOwn(b,'comparison')&&!Object.hasOwn(c,'comparison')&&!Object.hasOwn(b,'arm')&&!Object.hasOwn(c,'arm'),'experiment_label_leak');
    check(!Object.hasOwn(b.world,'market_unit_prices_by_sold_count')&&!Object.hasOwn(b.world,'market_price_curve_note')
      &&!Object.hasOwn(b.competition,'execution_position')&&!baseline[0].content.includes(PRICE_CLAUSE),'baseline_context_leak');
    const treated=agent.actor===focal&&snapshot.phase==='market';
    if(!treated)check(equal(baseline,candidate),'untreated_prompt_difference');
    else {
      check(equal(c.world.market_unit_prices_by_sold_count,snapshot.world.market_unit_prices_by_sold_count),'candidate_curve_not_snapshot');
      check(c.world.market_price_curve_note===snapshot.world.market_price_curve_note,'candidate_note_not_snapshot');
      check(c.competition.execution_position===agent.competition.execution_position,'candidate_position_not_snapshot');
      check(candidate[0].content.split(PRICE_CLAUSE).length===2,'candidate_price_clause_count');
      check(candidate[0].content.replace(' '+PRICE_CLAUSE,'')===baseline[0].content,'unexpected_system_intervention');
      delete c.world.market_unit_prices_by_sold_count;delete c.world.market_price_curve_note;delete c.competition.execution_position;
      check(JSON.stringify(c)===baseline[1].content,'unexpected_user_intervention');
    }
  } catch { issues.push('unreadable_prompt_scope'); }
  return issues;
}

export function soleWinnerIssues(snapshot,init) {
  if(snapshot.phase!=='finished')return [];
  const r=snapshot.result,issues=[],check=(ok,code)=>{if(!ok)issues.push(code);};
  if(!r||!Array.isArray(r.payouts)||!Array.isArray(r.order)||!Array.isArray(r.leaderboard))return ['missing_wta_settlement'];
  check(snapshot.world.payout_policy==='winner_takes_all'&&r.payout_policy==='winner_takes_all','settlement_not_wta');
  check(equal(snapshot.world.payout_shares,[100]),'settlement_shares_not_one_hundred');
  check(r.bank===init.agents*init.entry_fee&&r.rake===Math.floor(r.bank*snapshot.world.rake_bps/10000),'settlement_bank_or_rake');
  check(r.payouts.length===init.agents&&r.payouts.every(uint),'settlement_payout_vector');
  check(r.payouts.filter(x=>x>0).length===1,'not_exactly_one_positive_payout');
  check(r.winner_actor===r.order[0]&&r.payouts[r.order[0]]===r.bank-r.rake,'winner_not_rank_one_or_wrong_prize');
  check(sum(r.payouts)+r.rake===r.bank,'payout_plus_rake_not_bank');
  check(r.leaderboard.every((row,i)=>row.actor===r.order[i]&&row.rank===i+1&&row.payout===r.payouts[row.actor]
    &&(i===0||r.leaderboard[i-1].cash>=row.cash)),'cash_rank_or_payout_inconsistent');
  return issues;
}

function normalizedAction(call) {
  return call?.status==='completed'&&call.result?.action ? call.result.action : null;
}
function normalizedDecision(row) {
  const r=row.receipt;
  return r ? {action:r.action,params:r.params,accepted:r.accepted,skipped:r.skipped} : null;
}
function publicAction(action) {
  if(!action)return null;
  const allowed=new Set(['pass','sell','buy','sell_credit','produce','donkey','shuttle','bribe','roof','bid','vote_yes','vote_no','vote_abstain','veto']);
  if(!allowed.has(action.action))return {action:'unrecognized',params:null};
  const params={};for(const [k,v] of Object.entries(action.params??{}))if(['units','to','amount'].includes(k)&&uint(v))params[k]=v;
  return {action:action.action,params};
}

export function compareRecords(baseline,candidate,focal=0) {
  const bmap=new Map(baseline.map(r=>[`${phaseKey(r)}:${r.actor}`,r])),pairs=[];
  for(const c of candidate) {const b=bmap.get(`${phaseKey(c)}:${c.actor}`);if(b)pairs.push({b,c});}
  pairs.sort((x,y)=>compareOrder(x.b,y.b));
  const same=pairs.filter(p=>equal(p.b.call.messages,p.c.call.messages));
  const responseDrift=same.filter(p=>!equal(normalizedAction(p.b.call),normalizedAction(p.c.call)));
  const decisionDrift=same.filter(p=>!equal(normalizedDecision(p.b),normalizedDecision(p.c)));
  const appliedResponseDrift=responseDrift.filter(p=>p.b.receipt?.accepted||p.c.receipt?.accepted);
  const focalPairs=pairs.filter(p=>p.b.actor===focal&&p.b.phase==='market');
  const focalChanges=focalPairs.filter(p=>!equal(normalizedAction(p.b.call),normalizedAction(p.c.call)));
  const acceptedFocalChanges=focalChanges.filter(p=>p.b.receipt?.accepted&&p.c.receipt?.accepted);
  const first=focalChanges[0],firstDrift=responseDrift[0];
  const describe=p=>p?{...location(p.b),baseline:publicAction(normalizedAction(p.b.call)),candidate:publicAction(normalizedAction(p.c.call)),
    baseline_accepted:p.b.receipt?.accepted??null,candidate_accepted:p.c.receipt?.accepted??null}:null;
  return {matched_calls:pairs.length,identical_input_calls:same.length,identical_input_action_differences:responseDrift.length,
    identical_input_receipt_status_or_action_differences:decisionDrift.length,
    receipt_difference_basis:'normalized receipt action, params, accepted and skipped; this count includes differences where neither action was accepted',
    first_identical_input_action_difference:describe(firstDrift),first_identical_input_receipt_status_or_action_difference:describe(decisionDrift[0]),
    first_identical_input_action_difference_with_an_accepted_action:describe(appliedResponseDrift[0]),
    focal_market_action_differences:focalChanges.length,accepted_focal_market_action_differences:acceptedFocalChanges.length,
    first_focal_market_action_difference:describe(first),first_accepted_focal_market_action_difference:describe(acceptedFocalChanges[0]),
    identical_input_action_difference_before_first_focal_market_difference:firstDrift?(!first||compareOrder(firstDrift.b,first.b)<0):false,
    identical_input_action_difference_before_first_accepted_focal_market_difference:firstDrift?(!acceptedFocalChanges[0]||compareOrder(firstDrift.b,acceptedFocalChanges[0].b)<0):false,
    identical_input_accepted_action_difference_before_first_focal_market_difference:appliedResponseDrift[0]?(!first||compareOrder(appliedResponseDrift[0].b,first.b)<0):false,
    matching_basis:'round, phase, stage, actor; identical input means exact messages with an identical frozen model profile',
    temporal_order_basis:'canonical phase order, then actor execution position within the phase; not inter-arm response arrival order',
    focal_markets:focalPairs.map(p=>({...location(p.b),same_messages:equal(p.b.call.messages,p.c.call.messages),
      baseline:publicAction(normalizedAction(p.b.call)),candidate:publicAction(normalizedAction(p.c.call)),
      baseline_accepted:p.b.receipt?.accepted??null,candidate_accepted:p.c.receipt?.accepted??null}))};
}

function intervalsOf(calls) {
  return [...calls.values()].flatMap(c=>{
    const start=Date.parse(c.started_at),duration=c.result?.elapsed_ms;
    return Number.isFinite(start)&&uint(duration)&&duration>1?[{start,end:start+duration-1}]:[];
  });
}
export function peakOverlap(intervals) {
  const events=intervals.flatMap(x=>[[x.start,1],[x.end,-1]]).sort((a,b)=>a[0]-b[0]||a[1]-b[1]);
  let active=0,peak=0;for(const [,d] of events){active+=d;peak=Math.max(peak,active);}return peak;
}
export function timingPair(a,b) {
  const left=intervalsOf(a.calls),right=intervalsOf(b.calls),all=[...left,...right];
  const window=xs=>xs.length?[Math.min(...xs.map(x=>x.start)),Math.max(...xs.map(x=>x.end))]:null;
  const aw=window(left),bw=window(right),both=window(all);
  const anomalies=[];
  for(const [arm,run] of [['baseline',a],['candidate',b]]) {
    const calls=[...run.calls.values()].sort((x,y)=>Date.parse(x.started_at)-Date.parse(y.started_at));
    for(let i=1;i<calls.length;i++) {
      const previous=calls[i-1],next=calls[i],delta=Date.parse(next.started_at)-Date.parse(previous.started_at),elapsed=previous.result?.elapsed_ms;
      if(uint(elapsed)&&Number.isFinite(delta)&&elapsed-1>delta)anomalies.push({arm,
        previous:{step:previous.step,actor:previous.actor,started_at:previous.started_at},next:{step:next.step,actor:next.actor,started_at:next.started_at},
        previous_monotonic_elapsed_ms:elapsed,wall_clock_start_delta_ms:delta,raw_duration_minus_start_delta_ms:elapsed-delta,estimator_overlap_ms:elapsed-1-delta});
    }
  }
  return {baseline_reconstructed_interval_peak:peakOverlap(left),candidate_reconstructed_interval_peak:peakOverlap(right),pair_reconstructed_interval_peak:peakOverlap(all),
    measured_calls:all.length,unmeasured_calls:a.calls.size+b.calls.size-all.length,
    arm_start_difference_ms:aw&&bw?bw[0]-aw[0]:null,pair_measured_window_ms:both?both[1]-both[0]:null,
    arm_window_overlap_ms:aw&&bw?Math.max(0,Math.min(aw[1],bw[1])-Math.max(aw[0],bw[0])):null,
    actual_network_request_peak:null,adjacent_same_arm_hybrid_overlaps:anomalies,timing_quality_warning:anomalies.length>0,
    basis:'Raw hybrid estimate: wall-clock started_at plus monotonic performance.now elapsed_ms minus 1 ms. These clocks and start instants differ; this is not an independently measured network overlap.',
    limitation:'started_at is recorded before journal fsync, while provider elapsed time starts after it. Wall-clock adjustments are possible but are not proven by this journal. Raw intervals are retained without clamping or a larger tolerance.'};
}

function compareWorlds(a,b) {
  const amap=new Map([...a.snapshots.values(),a.final].map(s=>[phaseKey(s),s]));
  const bmap=new Map([...b.snapshots.values(),b.final].map(s=>[phaseKey(s),s]));
  const pairs=[...bmap.values()].flatMap(c=>amap.has(phaseKey(c))?[{b:amap.get(phaseKey(c)),c}]:[]).sort((x,y)=>x.b.step-y.b.step);
  const state=pairs.find(p=>p.b.state_hash!==p.c.state_hash);
  const keys=['public_state','market_price','sold_this_round','active_tax_bps','active_subsidy_goods','active_price_shift','active_boom','president','law_card','veto_pending','current_yes_weight','current_no_weight','laws_passed','last_law_passed'];
  const projection=s=>Object.fromEntries(keys.filter(k=>Object.hasOwn(s.world,k)).map(k=>[k,s.world[k]]));
  const economic=pairs.find(p=>!equal(projection(p.b),projection(p.c)));
  const describe=p=>p?{round:p.b.round,phase:p.b.phase,stage:p.b.stage,baseline_step:p.b.step,candidate_step:p.c.step}:null;
  return {matched_snapshots_including_final:pairs.length,first_prephase_or_final_state_hash_difference:describe(state),
    first_public_economic_projection_difference:describe(economic),economic_projection_fields:keys,
    final_state_hashes_equal:a.final.state_hash===b.final.state_hash,
    note:'Snapshots are prephase observations; an earlier batch can contain differing actions whose consequences first appear at the next snapshot. Hash differences include canonical hidden state; public projection is not a full state reconstruction.'};
}

function focalOutcome(run,records,focal) {
  if(run.final.phase!=='finished')return null;
  const row=run.final.result.leaderboard.find(r=>r.actor===focal),leader=run.final.result.leaderboard[0];
  const own=records.filter(x=>x.actor===focal),markets=own.filter(x=>x.phase==='market');
  return {actor:focal,rank:row.rank,cash:row.cash,payout:row.payout,winner:row.actor===run.final.result.winner_actor,
    gap_to_winner:leader.cash-row.cash,terminal_goods:row.goods,invalid_or_skipped_decisions:own.filter(x=>x.receipt&&!x.receipt.accepted).length,
    sale_receipts_total:sum(markets.filter(x=>x.receipt?.accepted&&x.receipt.action==='sell').map(x=>x.receipt.canonical?.detail?.revenue??0)),
    markets:markets.map(x=>({round:x.round,execution_position:x.position,cash_before:x.agent.observation.cash,goods_before:x.agent.observation.goods,
      action:publicAction(normalizedAction(x.call)),accepted:x.receipt?.accepted??null,skipped:x.receipt?.skipped??null,
      cash_after:x.receipt?.cash_after??null,goods_after:x.receipt?.goods_after??null,
      sale_revenue:x.receipt?.accepted&&x.receipt.action==='sell'?x.receipt.canonical?.detail?.revenue??null:null}))};
}

function reconstruct(run,driver,check) {
  const memory=Array.from({length:run.frozen.config.init.agents},()=>[]),records=[],stats={reconstructed_calls:0,matched_calls:0,scope_cases:0,treated_scope_cases:0,
    added_prompt_bytes_on_same_recorded_inputs:0,actual_treated_scope_prompt_tokens:0,actual_treated_scope_completion_tokens:0,actual_treated_scope_unknown_usage_calls:0};
  const phaseMap=new Map(run.phases.map(p=>[p.request.expected_step,p]));
  for(const s of [...run.snapshots.values()].sort((a,b)=>a.step-b.step)) {
    driver.validateSnapshot(s,100,'winner_takes_all');
    const phase=phaseMap.get(s.step),receipts=new Map((phase?.response?.receipts??[]).map(r=>[r.actor,r]));
    for(const [index,actor] of s.required_actors.entries()) {
      const agent=s.agents.find(x=>x.actor===actor),own=memory[actor];
      const base=driver.messagesForAgent(s,agent,own,100,{id:'market_context_v1',arm:'baseline',focal_actor:0});
      const candidate=driver.messagesForAgent(s,agent,own,100,{id:'market_context_v1',arm:'candidate',focal_actor:0});
      const issues=policyScopeIssues({baseline:base,candidate,snapshot:s,agent});
      for(const issue of issues)check(false,issue,{arm:run.frozen.config.comparison.arm,step:s.step,actor});
      check(issues.length===0,'treatment_scope_failed',{arm:run.frozen.config.comparison.arm,step:s.step,actor});
      stats.scope_cases++;if(actor===0&&s.phase==='market'){
        stats.treated_scope_cases++;
        stats.added_prompt_bytes_on_same_recorded_inputs+=Buffer.byteLength(JSON.stringify(candidate))-Buffer.byteLength(JSON.stringify(base));
      }
      const call=run.calls.get(id(s.step,actor));if(!call)continue;
      const expected=run.frozen.config.comparison.arm==='baseline'?base:candidate;
      stats.reconstructed_calls++;const matched=equal(expected,call.messages);if(matched)stats.matched_calls++;
      check(matched,'recorded_prompt_not_reconstructed',{arm:run.frozen.config.comparison.arm,step:s.step,actor});
      check(call.request_sha256===sha(canonical({profile:run.frozen.config.profile,messages:expected})),'reconstructed_request_hash_mismatch',{arm:run.frozen.config.comparison.arm,step:s.step,actor});
      check(call.reserved_tokens===Buffer.byteLength(JSON.stringify(expected))+2304,'call_token_reservation_mismatch',{arm:run.frozen.config.comparison.arm,step:s.step,actor});
      if(call.result?.http_status===200)check(call.result.requested_model===run.frozen.config.profile.model,'call_requested_model_mismatch',{arm:run.frozen.config.comparison.arm,step:s.step,actor});
      if(actor===0&&s.phase==='market'){
        if(call.result?.usage){stats.actual_treated_scope_prompt_tokens+=call.result.usage.prompt_tokens;stats.actual_treated_scope_completion_tokens+=call.result.usage.completion_tokens;}
        else stats.actual_treated_scope_unknown_usage_calls++;
      }
      records.push({round:s.round,phase:s.phase,stage:s.stage,actor,position:index+1,order:phaseOrder(s),call,receipt:receipts.get(actor),agent});
    }
    for(const r of receipts.values())memory[r.actor].push({round:s.round,phase:s.phase,stage:s.stage,...r});
  }
  check(stats.reconstructed_calls===run.calls.size,'unreconstructed_call_records',{arm:run.frozen.config.comparison.arm});
  return {records,stats};
}

async function replay(run,driver,workerPath,check) {
  const worker=driver.createWorker(workerPath);let batches=0,matches=0;
  try {
    const initial=await worker.request(run.frozen.config.init);
    check(equal(initial,run.snapshots.get(0)),'canonical_replay_initial_mismatch',{arm:run.frozen.config.comparison.arm});
    for(const phase of run.phases) {
      if(!phase.response)break;
      const answer=await worker.request(phase.request);batches++;
      if(equal(answer,phase.response))matches++;
      check(equal(answer,phase.response),'canonical_replay_response_mismatch',{arm:run.frozen.config.comparison.arm,step:phase.request.expected_step});
    }
  } finally {worker.close();}
  return {performed:true,batches,full_response_matches:matches,model_calls:0,network_calls:0,method:'frozen Rust JSONL worker init and saved apply requests; no driver runWorld/provider invocation'};
}

function frozenChecks(a,b,driverPath,workerPath,driver,check) {
  for(const [arm,f] of [['baseline',a],['candidate',b]]) {
    const c=f.config;driver.validateConfig(c);
    check(hex(f.salt)&&f.digest===sha(canonical({config:c,salt:f.salt})),'frozen_digest_invalid',{arm});
    check(c.driver_sha256===regularHash(driverPath)&&c.worker_sha256===regularHash(workerPath),'runtime_hash_mismatch',{arm});
    check(equal(c.comparison,{id:'market_context_v1',arm,focal_actor:0}),'comparison_contract_mismatch',{arm});
    check(equal(c.init,{op:'init',experimental:true,game_id:20261110,seed:20261110,agents:100,epoch:0,entry_fee:10_000_000,payout_policy:'winner_takes_all'}),'init_protocol_mismatch',{arm});
    check(c.profile.source==='provider_reported'&&c.profile.model==='qwen3.8-flash-next-fp4-alesha'&&c.profile.temperature===0&&c.profile.max_tokens===256
      &&equal(c.profile.chat_template_kwargs,{enable_thinking:false}),'provider_protocol_mismatch',{arm});
    check(c.parallelism===1&&c.max_calls===2000&&c.max_tokens===4_000_000&&c.call_timeout_ms===60000&&c.max_seconds===7200&&c.max_consecutive_provider_failures===5,'budget_protocol_mismatch',{arm});
  }
  const normalized=f=>{const c=structuredClone(f.config);delete c.comparison.arm;return c;};
  check(equal(normalized(a),normalized(b)),'control_configs_differ_beyond_arm');
}

function protocolEvidence(dirs,frozen,protocol,check) {
  const parent=dirname(dirs[0]);
  if(parent!==dirname(dirs[1])||!existsSync(join(parent,'preflight.json'))){
    check(false,'recorded_preflight_missing');return {recorded:false};
  }
  const p=read(join(parent,'preflight.json')),source=typeof p.source_commit==='string'&&/^[a-f0-9]{40}$/.test(p.source_commit)?p.source_commit:null;
  check(source!==null,'preflight_source_commit_invalid');
  check(equal(p.frozen_digests,{baseline:frozen[0].digest,candidate:frozen[1].digest}),'preflight_digests_mismatch');
  check(p.worker_sha256===frozen[0].config.worker_sha256&&p.driver_sha256===frozen[0].config.driver_sha256,'preflight_runtime_hash_mismatch');
  check(!!protocol&&p.protocol_sha256===regularHash(protocol),'preflight_protocol_hash_mismatch');
  check(existsSync(join(parent,'protocol.md'))&&regularHash(join(parent,'protocol.md'))===p.protocol_sha256,'preserved_protocol_copy_mismatch');
  check(p.model_calls===0&&p.configurations_differ_only_in_arm===true&&p.initial_snapshots_identical===true&&p.matched_nonfocal_prompts===99,'preflight_scope_not_verified');
  const launch=existsSync(join(parent,'launch.json'))?read(join(parent,'launch.json')):null;
  if(launch)check(launch.source_commit===source&&launch.parallelism_per_arm===1&&launch.global_concurrency_cap===2&&equal(launch.arms,['baseline','candidate']),'launch_contract_mismatch');
  const timestamp=x=>Number.isFinite(Date.parse(x))?new Date(x).toISOString():null;
  return {recorded:true,source_commit:source,preflight_sha256:regularHash(join(parent,'preflight.json')),protocol_sha256:hex(p.protocol_sha256)?p.protocol_sha256:null,
    preflight_created_at:timestamp(p.created_at),initial_state_hash:hex(p.initial_state_hash)?p.initial_state_hash:null,launch_recorded:!!launch,
    launch_started_at:timestamp(launch?.started_at),basis:'locally recorded preflight and preserved protocol copy; Git commit publication timing is verified separately'};
}

export async function auditPair({baseline,candidate,driver:driverPath,worker:workerPath,protocol,doReplay=false,preflight=false}) {
  const issues=[];let checks=0;
  const check=(ok,code,where={})=>{checks++;if(!ok)issues.push({code,...where});return !!ok;};
  const dirs=[resolve(baseline),resolve(candidate)];require(dirs[0]!==dirs[1],'pair_requires_separate_journals');
  for(const dir of dirs)require(!existsSync(join(dir,'.world.lock')),'active_run_stop_before_audit');
  const before=dirs.map(manifest),frozen=dirs.map(d=>read(join(d,'frozen.json')));
  const workerHash=regularHash(workerPath),driverHash=regularHash(driverPath);
  require(frozen.every(f=>f.config.driver_sha256===driverHash&&f.config.worker_sha256===workerHash),'runtime_hash_not_frozen');
  const driver=await import(pathToFileURL(resolve(driverPath)).href);
  frozenChecks(frozen[0],frozen[1],driverPath,workerPath,driver,check);
  const boundsVerified=serializedBoundsVerified(frozen.map(f=>f.config),driverHash);
  check(boundsVerified,'serialized_client_request_bound_unverified');
  const provenance=protocolEvidence(dirs,frozen,protocol,check);
  const baseReport={schema:'alashi-market-context-pair-audit-v1',generated_at:new Date().toISOString(),mode:preflight?'cpu_preflight':'completed_journal_audit',
    auditor_sha256:regularHash(fileURLToPath(import.meta.url)),generic_auditor_sha256:regularHash(fileURLToPath(new URL('./alashi-world-evidence-audit.mjs',import.meta.url))),
    protocol_sha256:protocol?regularHash(protocol):null,worker_sha256:workerHash,driver_sha256:driverHash,
    experimental_unit:'one predeclared paired scenario; 100 interacting actors and repeated decisions are not independent evaluation samples',
    provenance,focal_actor:0,seed:20261110,game_id:20261110,payout_policy:'winner_takes_all',unit:'micro_peso',peso:1_000_000,
    ceilings:{per_arm_calls:2000,per_arm_tokens:4_000_000,per_arm_seconds:7200,pair_calls:4000,pair_tokens:8_000_000,per_arm_concurrency:1,pair_concurrency:2},
    concurrency_control:{serialized_driver_and_config_verified:boundsVerified,per_arm_client_request_bound:1,pair_client_request_bound:2,
      basis:'Hash-bound reviewed driver creates one lane per arm and awaits complete(), including response-body reading, before the next request. This bounds these two runner processes; it is not a network traffic measurement.'},
    audit_interpretation_revision:{previous_auditor_sha256:'7f80b737587e2adb799ec5595d8b8a29a0aa7ec21987e49a25585f6a6f98bd03',
      initial_report_sha256:'77832a6d30002759904d691a542f7da080c553443a6ee545626b6f0a41fe8753',
      reason:'The initial audit treated hybrid wall-clock/monotonic intervals as proof of request concurrency. The correction preserves their raw peaks and anomalies as a timing-quality limitation, verifies the sequential code/config bound separately, and leaves actual network peak unmeasured. Outcome criteria, runtime and journals are unchanged.'}};
  let body;
  if(preflight) {
    const initial=[];
    for(const f of frozen) {const w=driver.createWorker(workerPath);try{initial.push(await w.request(f.config.init));}finally{w.close();}}
    check(equal(initial[0],initial[1]),'initial_worlds_differ');
    check(initial.every(s=>s.state_hash===provenance.initial_state_hash),'recorded_preflight_initial_hash_mismatch');
    let cases=0;
    for(const s of initial) {
      driver.validateSnapshot(s,100,'winner_takes_all');
      for(const agent of s.agents) {
        const b=driver.messagesForAgent(s,agent,[],100,{id:'market_context_v1',arm:'baseline',focal_actor:0});
        const c=driver.messagesForAgent(s,agent,[],100,{id:'market_context_v1',arm:'candidate',focal_actor:0});
        const errors=policyScopeIssues({baseline:b,candidate:c,snapshot:s,agent});
        for(const e of errors)check(false,e,{actor:agent.actor});check(errors.length===0,'initial_scope_failed',{actor:agent.actor});cases++;
      }
    }
    body={preflight:{initial_snapshots_equal:equal(initial[0],initial[1]),initial_state_hash:initial[0].state_hash,scope_cases:cases,model_calls:0,
      limitation:'CPU initialization and initial prompt scope only; actual journal evidence and completed canonical replay remain pending'}};
  } else {
    const a=loadRun(dirs[0]),b=loadRun(dirs[1]);
    const firstCallStart=Math.min(...[...a.calls.values(),...b.calls.values()].map(c=>Date.parse(c.started_at)));
    check(Number.isFinite(Date.parse(provenance.preflight_created_at))&&Date.parse(provenance.preflight_created_at)<=firstCallStart,'calls_precede_recorded_preflight');
    check(provenance.launch_recorded&&Number.isFinite(Date.parse(provenance.launch_started_at))&&Date.parse(provenance.launch_started_at)<=firstCallStart,'calls_precede_recorded_launch');
    const generic=[auditWorld({run:dirs[0],worker:workerPath,driver:driverPath}),auditWorld({run:dirs[1],worker:workerPath,driver:driverPath})];
    for(const [i,g] of generic.entries())check(g.evidence.journal_consistent,'generic_journal_audit_failed',{arm:i?'candidate':'baseline',issue_count:g.evidence.issues_count});
    check(equal(a.snapshots.get(0),b.snapshots.get(0)),'initial_snapshots_differ');
    check([a,b].every(run=>run.snapshots.get(0)?.state_hash===provenance.initial_state_hash),'recorded_preflight_initial_hash_mismatch');
    const reconstruction=[reconstruct(a,driver,check),reconstruct(b,driver,check)];
    for(const r of [a,b]) {
      const arm=r.frozen.config.comparison.arm;
      check(equal(r.summary.comparison,r.frozen.config.comparison),'summary_comparison_mismatch',{arm});
      for(const issue of soleWinnerIssues(r.final,r.frozen.config.init))check(false,issue,{arm});
    }
    const drift=compareRecords(reconstruction[0].records,reconstruction[1].records);
    const focal=[focalOutcome(a,reconstruction[0].records,0),focalOutcome(b,reconstruction[1].records,0)];
    const completed=focal.every(Boolean),cashDelta=completed?focal[1].cash-focal[0].cash:null,rankDelta=completed?focal[1].rank-focal[0].rank:null;
    const timings=timingPair(a,b);
    const replays=doReplay?[await replay(a,driver,workerPath,check),await replay(b,driver,workerPath,check)]:[{performed:false},{performed:false}];
    const arms=[a,b].map((run,i)=>{
      const g=generic[i],r=run.final.result;
      return {arm:i?'candidate':'baseline',frozen_digest:run.frozen.digest,journal_manifest:before[i],generic_checks:g.evidence.checks,
        generic_issues:g.evidence.issues,journal_consistent:g.evidence.journal_consistent,game_completed:g.experiment.game_completed,
        driver_status:g.experiment.driver_status,final_state_hash:run.final.state_hash,prompt_reconstruction:reconstruction[i].stats,canonical_replay:replays[i],
        provider:g.provider,participation:g.participation,actions:g.actions,timing:g.timing,spending:g.spending,focal:focal[i],
        settlement:r?{bank:r.bank,rake:r.rake,total_payout:sum(r.payouts),positive_payouts:r.payouts.filter(x=>x>0).length,winner_actor:r.winner_actor,
          winner_cash:r.leaderboard[0].cash,winner_payout:r.payouts[r.winner_actor],top_ten:r.leaderboard.slice(0,10).map(x=>({actor:x.actor,rank:x.rank,cash:x.cash,payout:x.payout}))}:null};
    });
    const directional=completed&&cashDelta>0&&rankDelta<=0&&drift.accepted_focal_market_action_differences>0;
    const modelIds=[a,b].map(run=>{
      const labels=[...run.calls.values()].sort((x,y)=>Date.parse(x.started_at)-Date.parse(y.started_at)).map(c=>modelLabel(c.result?.returned_model));
      return {distinct_returned_identifiers:[...new Set(labels)],identifier_transitions:labels.slice(1).filter((v,i)=>v!==labels[i]).length};
    });
    body={completed_pair:completed,arms,scope:{checked_against:'both policies evaluated independently on every same recorded snapshot and actor-only receipt history',
      permitted_difference:'only candidate actor 0 Market: current 16-entry price curve, curve note, execution_position, and the exact explanatory system clause',
      arm_labels_sent_to_model:false,policies_fixed_opponent_actions_recomputed:true},provider_variability:{...drift,model_identifiers:{baseline:modelIds[0],candidate:modelIds[1],
        same_identifier_sets:equal([...modelIds[0].distinct_returned_identifiers].sort(),[...modelIds[1].distinct_returned_identifiers].sort()),
        same_names_do_not_attest_same_weights:true}},state_divergence:compareWorlds(a,b),timing:timings,
      resource_delta:{candidate_minus_baseline_prompt_tokens:generic[1].provider.known_prompt_tokens-generic[0].provider.known_prompt_tokens,
        candidate_minus_baseline_completion_tokens:generic[1].provider.known_completion_tokens-generic[0].provider.known_completion_tokens,
        candidate_minus_baseline_total_tokens:generic[1].provider.known_tokens-generic[0].provider.known_tokens,
        pair_known_tokens:generic[0].provider.known_tokens+generic[1].provider.known_tokens,
        pair_unknown_usage_calls:generic[0].provider.unknown_usage_calls+generic[1].provider.unknown_usage_calls,
        provider_cost_usd:null,note:'Total resource delta includes downstream world/response changes; the extra treatment input length is only one component. Peer-reported usage; USD price unknown.'},
      outcome:{primary:'candidate minus baseline focal final cash',cash_delta:cashDelta,rank_delta_candidate_minus_baseline:rankDelta,
        observed_win_indicator_delta:completed?Number(focal[1].winner)-Number(focal[0].winner):null,payout_delta:completed?focal[1].payout-focal[0].payout:null,
        directional_pilot_criterion_met:directional,
        criterion:'completed pair, focal cash higher, focal rank no worse, and at least one different focal Market action accepted in both games',
        label:!completed?'incomplete_pair':directional?'directionally_promising_single_scenario':'no_positive_directional_pilot_signal',
        cause_isolated_by_pair:drift.identical_input_action_differences>0?false:null,
        earlier_provider_action_variability:drift.identical_input_action_difference_before_first_focal_market_difference,
        earlier_applied_provider_action_variability:drift.identical_input_accepted_action_difference_before_first_focal_market_difference,
        interpretation:drift.identical_input_accepted_action_difference_before_first_focal_market_difference
          ?'Identical-input provider action differences, with an accepted action in at least one arm, precede any focal Market action difference. The cause of final cash/rank differences is not isolated.'
          :'One descriptive paired scenario. Even absent detected earlier drift, it cannot establish win probability, repeatability or a population causal effect.'}};
  }
  for(const [i,dir] of dirs.entries())check(equal(before[i],manifest(dir)),'journal_changed_during_audit',{arm:i?'candidate':'baseline'});
  check(workerHash===regularHash(workerPath)&&driverHash===regularHash(driverPath),'runtime_changed_during_audit');
  if(body.outcome){
    body.outcome.eligible_for_pilot_interpretation=issues.length===0&&body.completed_pair&&doReplay;
    if(issues.length)body.outcome.label='invalid_pair_evidence';
    else if(!doReplay)body.outcome.label='pending_canonical_replay';
  }
  return {...baseReport,...body,evidence:{checks,issues_count:issues.length,issues:issues.slice(0,200),issues_truncated:issues.length>200,
    valid:issues.length===0,journals_unchanged:!issues.some(i=>i.code==='journal_changed_during_audit'),additional_model_calls:0,
    timing_quality_warnings:body.timing?.timing_quality_warning?[{code:'hybrid_clock_overlap_not_network_concurrency',observed_adjacent_overlap_count:body.timing.adjacent_same_arm_hybrid_overlaps.length}]:[]},
    limitations:['Only one paired scenario, one fixed focal seat and one environment seed; no holdout, seat rotation, win-rate estimate or meaningful effect confidence interval.',
      'All 100 players are fresh model-controlled identities in each arm. Distinct states do not imply independent models or independent statistical samples.',
      'Alias, returned model names and token usage are peer-reported; matching names and temperature zero do not attest identical weights or deterministic inference.',
      'The intervention is a whole information block including extra tokens, not a test that separately identifies price-curve versus queue-position value.',
      'A higher simulated cash score is a diagnostic. Only rank one receives the fixed prize; increasing the prize relative to previous four-winner games is not evidence of intelligence.',
      'This allowlisted report excludes local paths, endpoint URLs, peer identifiers, raw prompts, response IDs, content and error text. Private journals retain that metadata.']};
}

async function main(args) {
  const options={};let doReplay=false,preflight=false;
  for(let i=0;i<args.length;i++) {
    if(args[i]==='--replay'){doReplay=true;continue;}if(args[i]==='--preflight'){preflight=true;continue;}
    require(['--baseline','--candidate','--driver','--worker','--protocol','--out'].includes(args[i])&&args[i+1]&&!options[args[i]],'invalid_audit_arguments');
    options[args[i]]=args[++i];
  }
  require(['--baseline','--candidate','--driver','--worker','--out'].every(k=>options[k]),'audit_requires_baseline_candidate_driver_worker_out');
  const out=resolve(options['--out']);
  for(const k of ['--baseline','--candidate'])require(out!==resolve(options[k])&&!out.startsWith(resolve(options[k])+'/'),'output_must_be_outside_journals');
  if(existsSync(out))require(lstatSync(out).isFile()&&!lstatSync(out).isSymbolicLink(),'unsafe_output');
  const report=await auditPair({baseline:options['--baseline'],candidate:options['--candidate'],driver:options['--driver'],worker:options['--worker'],protocol:options['--protocol'],doReplay,preflight});
  writeFileSync(out,JSON.stringify(report,null,2)+'\n',{mode:0o600});
  console.log(JSON.stringify({valid:report.evidence.valid,checks:report.evidence.checks,issues:report.evidence.issues_count,mode:report.mode,
    completed_pair:report.completed_pair??null,outcome:report.outcome??null,additional_model_calls:0}));
  if(!report.evidence.valid)process.exitCode=2;
}
if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href) {
  try {await main(process.argv.slice(2));}catch(error){console.error(/^[a-z0-9_]+$/.test(error.message)?error.message:'pair_audit_invalid_or_unreadable_input');process.exitCode=1;}
}
