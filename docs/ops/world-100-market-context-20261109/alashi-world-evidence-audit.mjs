#!/usr/bin/env node
// Read-only audit of a stopped world-run.mjs journal; no provider or worker execution.
// Output is constructed field by field. Never publish raw journals with this report.
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { isDeepStrictEqual as equal } from 'node:util';

const sha = x => createHash('sha256').update(x).digest('hex');
const canonical = value => JSON.stringify(value, (_key, x) => x && typeof x === 'object' && !Array.isArray(x)
  ? Object.fromEntries(Object.keys(x).sort().map(k => [k, x[k]])) : x);
const object = x => x !== null && typeof x === 'object' && !Array.isArray(x);
const uint = x => Number.isSafeInteger(x) && x >= 0;
const hex = x => typeof x === 'string' && /^[a-f0-9]{64}$/.test(x);
const cid = (step, actor) => `${String(step).padStart(4, '0')}-${String(actor).padStart(3, '0')}`;
const unique = xs => new Set(xs).size === xs.length;
const sum = xs => xs.reduce((a, b) => a + b, 0);
const inc = (map, key, amount = 1) => { map[key] = (map[key] ?? 0) + amount; };
const ACTIONS = new Set(['pass','sell','buy','sell_credit','produce','donkey','shuttle','bribe','roof','bid','vote_yes','vote_no','vote_abstain','veto']);
const actionLabel = x => ACTIONS.has(x) ? x : x === null ? 'skipped' : 'unrecognized';
const modelLabel = x => typeof x === 'string' && /^[a-zA-Z0-9][a-zA-Z0-9._+-]{0,127}$/.test(x) ? x : x == null ? 'unreported' : 'redacted';
function errorCategory(x) {
  if (x == null) return 'none';
  if (typeof x !== 'string') return 'unclassified';
  if (/^provider_http_\d{3}$/.test(x)) return x;
  if (['invalid_action_json','action_not_in_snapshot_menu','provider_outcome_unknown','invalid_provider_result','provider_failed','provider_response_too_large'].includes(x)) return x;
  return /timeout|timed out|abort/i.test(x) ? 'timeout_or_abort' : 'other_error';
}
function reasonLabel(x) {
  return ['token_budget','call_budget','wall_time_budget','provider_access_or_rate_limit','provider_failure_threshold','local_runner_error'].includes(x) ? x : x == null ? null : 'other_reason';
}
function usage(raw) {
  return object(raw) && ['prompt_tokens','completion_tokens','total_tokens'].every(k => uint(raw[k]))
    && raw.prompt_tokens + raw.completion_tokens === raw.total_tokens ? raw : null;
}
function parsedAction(text) {
  if (typeof text !== 'string' || Buffer.byteLength(text) > 8192) return null;
  const match = text.trim().match(/^```(?:json)?\s*\n([\s\S]*?)\n```$/i);
  try {
    const x = JSON.parse(match ? match[1] : text);
    return object(x) && equal(Object.keys(x).sort(), ['action','params']) && typeof x.action === 'string' && object(x.params) ? x : null;
  } catch { return null; }
}
function distribution(xs) {
  if (!xs.length) return {count:0,min:null,median:null,p95:null,max:null,mean:null,sum:0};
  const sorted = [...xs].sort((a,b) => a-b), total = sum(xs);
  return {count:xs.length,min:sorted[0],median:sorted[Math.ceil(sorted.length * .5)-1],
    p95:sorted[Math.ceil(sorted.length * .95)-1],max:sorted.at(-1),mean:Number((total/xs.length).toFixed(3)),sum:total};
}

export function auditWorld({run, worker, driver, sourceCommit = null}) {
  const dir = resolve(run), issues = [], manifest = [], documents = new Map();
  let checks = 0;
  const check = (condition, code, where = {}) => { checks++; if (!condition) issues.push({code,...where}); return !!condition; };
  const require = (condition, code) => { if (!condition) throw new Error(code); };
  require(lstatSync(dir).isDirectory() && !lstatSync(dir).isSymbolicLink(), 'run_requires_regular_directory');
  require(!existsSync(join(dir,'.world.lock')), 'active_run_lock_present_stop_driver_before_audit');
  if (sourceCommit !== null) require(/^[a-f0-9]{40}$/.test(sourceCommit), 'invalid_source_commit');
  const read = name => {
    const path = join(dir,name), stat = lstatSync(path);
    require(stat.isFile() && !stat.isSymbolicLink() && stat.size <= 25_000_000, 'unsafe_or_oversized_input_file');
    const bytes = readFileSync(path), value = JSON.parse(bytes);
    manifest.push({name,sha256:sha(bytes)}); documents.set(name,value); return value;
  };
  const names = (folder, pattern) => {
    const stat = lstatSync(join(dir,folder)); require(stat.isDirectory() && !stat.isSymbolicLink(), 'unsafe_input_subdirectory');
    return readdirSync(join(dir,folder)).filter(n => pattern.test(n)).sort();
  };
  const frozen = read('frozen.json'), config = frozen.config;
  require(object(config) && object(config.init) && object(config.profile), 'invalid_frozen_structure');
  const n = config.init.agents;
  require(uint(n) && n >= 2 && n <= 100, 'invalid_agent_count');
  require(hex(frozen.salt) && hex(frozen.digest) && hex(config.worker_sha256) && hex(config.driver_sha256), 'invalid_frozen_hash_fields');
  require(['game_id','seed','epoch','entry_fee'].every(k=>uint(config.init[k]))
    && ['parallelism','max_calls','max_tokens','call_timeout_ms','max_seconds','max_consecutive_provider_failures'].every(k=>uint(config[k])), 'invalid_numeric_config');
  check(config.schema === 'alashi-bitllm-world-v1', 'unsupported_input_schema');
  check(hex(frozen.salt) && hex(frozen.digest) && frozen.digest === sha(canonical({config,salt:frozen.salt})), 'frozen_digest_mismatch');
  check(config.init.experimental === true && config.init.op === 'init', 'explicit_experiment_missing');
  check(config.init.epoch === 0 || config.init.epoch === 1, 'unsupported_epoch');
  check(config.profile.temperature === 0 && config.profile.max_tokens === 256 && equal(config.profile.chat_template_kwargs,{enable_thinking:false}), 'unexpected_model_profile');
  const runtimeChecks = {};
  for (const [kind,path,expected] of [['worker',worker,config.worker_sha256],['driver',driver,config.driver_sha256]]) {
    check(hex(expected), `${kind}_hash_invalid`);
    if (!path) { runtimeChecks[kind] = 'not_requested'; continue; }
    const stat = lstatSync(path); require(stat.isFile() && !stat.isSymbolicLink() && stat.size < 500_000_000,'unsafe_runtime_file');
    runtimeChecks[kind] = sha(readFileSync(path)) === expected ? 'matched' : 'mismatch';
    check(runtimeChecks[kind] === 'matched', `${kind}_file_changed`);
  }
  const summary = read('summary.json'), final = read('final-snapshot.json');
  const runMeta = existsSync(join(dir,'run.json')) ? read('run.json') : null;
  check(summary.digest === frozen.digest, 'summary_digest_mismatch');
  if (runMeta) check(runMeta.digest === frozen.digest && uint(runMeta.started_at_ms), 'run_metadata_mismatch');
  const snapshots = new Map(), calls = new Map();
  for (const name of names('snapshots', /^\d{4}\.json$/)) snapshots.set(Number(name.slice(0,4)), read(`snapshots/${name}`));
  require(snapshots.has(0), 'initial_snapshot_missing');
  const phases = names('phases', /^\d{4}\.json$/).map(name => ({name,...read(`phases/${name}`)}));
  const perAgent = Array.from({length:n}, (_,actor) => ({actor,identity:`agent-${String(actor).padStart(3,'0')}`,
    calls:0,primary_calls:0,post_vote_calls:0,completed_call_records:0,http_2xx_responses:0,parsed_action_responses:0,
    menu_valid_action_responses:0,provider_error_calls:0,unknown_usage_calls:0,inflight_calls:0,known_prompt_tokens:0,
    known_completion_tokens:0,known_total_tokens:0,held_tokens:0,accepted_actions:0,rejected_actions:0,skipped_decisions:0,
    accepted_by_action:{}}));
  const callStatus = {}, httpStatus = {}, models = {}, finishReasons = {}, providerErrors = {}, latency = [], intervals = [];
  const totals = {calls:0,known_prompt_tokens:0,known_completion_tokens:0,known_tokens:0,reserved_or_unknown_tokens:0,unknown_usage_calls:0,inflight_calls:0,budget_overrun:false};

  const validateSnapshot = (s, expectedStep) => {
    if (!check(object(s), 'snapshot_not_object', {step:expectedStep})) return;
    require(uint(s.step) && uint(s.round) && hex(s.state_hash) && ['market','action','law','finished'].includes(s.phase)
      && ['primary','post_vote','finished'].includes(s.stage), 'invalid_snapshot_public_fields');
    require(object(s.world) && ['agents','seed','epoch','peso','rake_bps','logical_time','deadline','laws_passed'].every(k=>uint(s.world[k]))
      && typeof s.world.last_law_passed === 'boolean', 'invalid_world_public_fields');
    check(s.ok === true && s.type === 'snapshot' && s.protocol === 'alashi-world-v1', 'snapshot_protocol_invalid', {step:expectedStep});
    check(s.step === expectedStep && hex(s.state_hash), 'snapshot_identity_invalid', {step:expectedStep});
    check(s.game_id === config.init.game_id && s.world?.agents === n && s.world?.seed === config.init.seed
      && s.world?.epoch === config.init.epoch && s.world?.money === 'simulated', 'snapshot_world_identity_mismatch', {step:expectedStep});
    require(Array.isArray(s.required_actors) && Array.isArray(s.agents) && Array.isArray(s.world?.public_state), 'invalid_snapshot_arrays');
    check(s.world.public_state.length === n && unique(s.world.public_state.map(r=>r.actor)), 'snapshot_public_actor_count', {step:expectedStep});
    for (const row of s.world.public_state) {
      require(uint(row.actor) && row.actor < n && ['cash','hard','goods','influence','vote_weight'].every(k=>uint(row[k])) && typeof row.alive === 'boolean', 'invalid_public_balance');
      check(uint(row.actor) && row.actor < n && ['cash','hard','goods','influence','vote_weight'].every(k=>uint(row[k])) && typeof row.alive === 'boolean', 'snapshot_public_actor_invalid', {step:expectedStep,actor:uint(row.actor)?row.actor:null});
    }
    check(unique(s.required_actors) && s.required_actors.every(a=>uint(a)&&a<n), 'snapshot_required_actors_invalid', {step:expectedStep});
    check(s.agents.length === s.required_actors.length && unique(s.agents.map(a=>a.actor)) && s.agents.every(a=>s.required_actors.includes(a.actor)), 'snapshot_agent_menu_set_invalid', {step:expectedStep});
    if (s.phase === 'finished') check(s.required_actors.length === 0 && s.stage === 'finished' && s.round === 6, 'finished_snapshot_invalid', {step:expectedStep});
    else if (s.stage === 'post_vote') check(s.phase === 'law' && equal(s.required_actors,[s.world.president]), 'post_vote_actor_invalid', {step:expectedStep});
    else check(s.stage === 'primary' && ['market','action','law'].includes(s.phase) && s.required_actors.length === n, 'primary_actor_count_invalid', {step:expectedStep});
    for (const a of s.agents) {
      require(uint(a.actor) && a.actor < n, 'invalid_snapshot_agent_identity');
      check(Array.isArray(a.allowed_actions) && a.allowed_actions.length > 0 && a.allowed_actions.length <= 64
        && a.allowed_actions.every(x=>object(x)&&typeof x.action==='string'&&object(x.params)), 'invalid_action_menu', {step:expectedStep,actor:a.actor});
    }
  };
  for (const [step,s] of snapshots) validateSnapshot(s,step);
  validateSnapshot(final, final.step);
  for (const name of names('calls', /^\d{4}-\d{3}\.json$/)) {
    const call = read(`calls/${name}`), where = {step:call.step,actor:call.actor};
    require(uint(call.actor) && call.actor < n && uint(call.step), 'invalid_call_identity');
    check(name === `${cid(call.step,call.actor)}.json` && call.digest === frozen.digest, 'call_journal_identity_mismatch', where);
    require(['inflight','completed','unknown'].includes(call.status), 'call_status_invalid');
    require(uint(call.reserved_tokens) && call.reserved_tokens > 0, 'invalid_call_reservation');
    check(Array.isArray(call.messages) && call.request_sha256 === sha(canonical({profile:config.profile,messages:call.messages})), 'call_request_hash_mismatch', where);
    const snapshot = snapshots.get(call.step);
    check(!!snapshot && snapshot.state_hash === call.state_hash && snapshot.required_actors.includes(call.actor), 'call_snapshot_scope_mismatch', where);
    check(!!snapshot && call.phase === snapshot.phase && call.round === snapshot.round && call.stage === snapshot.stage, 'call_phase_scope_mismatch', where);
    const agent = snapshot?.agents.find(a=>a.actor===call.actor);
    let prompt = null;
    try { prompt = JSON.parse(call.messages?.[1]?.content); } catch {}
    check(prompt?.player === call.actor && prompt?.identity === perAgent[call.actor].identity && prompt?.round === call.round
      && prompt?.phase === call.phase && prompt?.stage === call.stage, 'prompt_actor_scope_mismatch', where);
    check(!!agent && equal(prompt?.allowed_actions,agent.allowed_actions) && equal(prompt?.observation,agent.observation), 'prompt_observation_mismatch', where);
    const p = perAgent[call.actor]; p.calls++; totals.calls++; inc(callStatus,call.status);
    if (call.stage === 'primary') p.primary_calls++; else if (call.stage === 'post_vote') p.post_vote_calls++;
    const r = call.status === 'completed' ? call.result : null;
    if (r) {
      p.completed_call_records++;
      check(object(r) && (r.usage === null || !!usage(r.usage)), 'call_usage_invalid', where);
      if (uint(r.http_status) && r.http_status >= 100 && r.http_status <= 599) inc(httpStatus,String(r.http_status)); else inc(httpStatus,'unreported');
      if (r.http_status >= 200 && r.http_status < 300) p.http_2xx_responses++;
      inc(models,modelLabel(r.returned_model));
      inc(finishReasons,['stop','length','tool_calls','content_filter'].includes(r.finish_reason)?r.finish_reason:r.finish_reason==null?'unreported':'other');
      if (r.action) {
        p.parsed_action_responses++;
        if (agent?.allowed_actions.some(a=>equal(a,r.action)) && !r.error) p.menu_valid_action_responses++;
        if (config.profile.source === 'provider_reported') check(equal(parsedAction(r.content),r.action), 'parsed_content_action_mismatch', where);
      }
      if (r.error) { p.provider_error_calls++; inc(providerErrors,errorCategory(r.error)); }
      if (uint(r.elapsed_ms)) {
        latency.push(r.elapsed_ms);
        const start = Date.parse(call.started_at);
        check(Number.isFinite(start), 'call_started_at_invalid', where);
        if (Number.isFinite(start)) intervals.push({start,end:start+r.elapsed_ms,duration:r.elapsed_ms});
      }
    } else check(call.status !== 'completed', 'completed_call_result_missing', where);
    const u = r && usage(r.usage);
    if (u) {
      totals.known_prompt_tokens += u.prompt_tokens; totals.known_completion_tokens += u.completion_tokens; totals.known_tokens += u.total_tokens;
      p.known_prompt_tokens += u.prompt_tokens; p.known_completion_tokens += u.completion_tokens; p.known_total_tokens += u.total_tokens;
      if (u.total_tokens > call.reserved_tokens) totals.budget_overrun = true;
    } else {
      totals.reserved_or_unknown_tokens += call.reserved_tokens; p.held_tokens += call.reserved_tokens;
      if (call.status === 'inflight') { totals.inflight_calls++; p.inflight_calls++; }
      else { totals.unknown_usage_calls++; p.unknown_usage_calls++; }
    }
    check(!calls.has(cid(call.step,call.actor)), 'duplicate_call_identity', where);
    calls.set(cid(call.step,call.actor),call);
  }

  const receipts = [], appliedCalls = new Set(), phaseReport = [], acceptedActions = {}, rejectedActions = {}, skippedReasons = {};
  let current = snapshots.get(0), primaryPhases = 0, postVotePhases = 0, preparedPhases = 0;
  for (const phase of phases) {
    const req = phase.request, res = phase.response, step = current.step;
    require(object(req) && Array.isArray(req.decisions), 'invalid_phase_request');
    check(phase.name === `${String(step).padStart(4,'0')}.json` && phase.digest === frozen.digest, 'phase_journal_scope_mismatch', {step});
    check(req.op === 'apply' && req.expected_step === step && req.expected_state_hash === current.state_hash, 'phase_state_chain_broken', {step});
    check(equal(req.decisions.map(d=>d.actor),current.required_actors), 'phase_decision_actor_order_mismatch', {step});
    for (const d of req.decisions) {
      require(uint(d.actor) && d.actor < n, 'invalid_phase_actor_identity');
      const call = calls.get(cid(step,d.actor)), a = current.agents.find(x=>x.actor===d.actor);
      check(!!call && !!a, 'phase_decision_call_missing', {step,actor:d.actor});
      if (!call || !a) continue;
      let expected;
      if (call.status !== 'completed') expected = {actor:d.actor,action:null,params:{},error:'provider_outcome_unknown'};
      else if (call.result.error || !call.result.action) expected = {actor:d.actor,action:null,params:{},error:call.result.error??'invalid_action_json'};
      else if (!a.allowed_actions.some(x=>equal(x,call.result.action))) expected = {actor:d.actor,action:null,params:{},error:'action_not_in_snapshot_menu'};
      else expected = {actor:d.actor,...call.result.action};
      check(equal(d,expected), 'decision_not_derived_from_call', {step,actor:d.actor});
    }
    if (res === null) { preparedPhases++; check(phase === phases.at(-1),'unresolved_phase_not_last',{step}); break; }
    require(object(res) && Array.isArray(res.receipts) && object(res.snapshot), 'invalid_phase_response');
    check(res.ok === true && res.type === 'applied' && res.step === step, 'phase_response_scope_mismatch', {step});
    check(res.state_hash_before === current.state_hash && res.state_hash_after === res.snapshot.state_hash, 'phase_response_hash_mismatch', {step});
    check(equal(res.receipts.map(r=>r.actor),current.required_actors), 'receipt_actor_order_mismatch', {step});
    const phaseCounts = {accepted:0,rejected:0,skipped:0};
    for (const r of res.receipts) {
      require(uint(r.actor) && r.actor < n, 'invalid_receipt_actor_identity');
      const d = req.decisions.find(x=>x.actor===r.actor), p = perAgent[r.actor];
      require(!!p,'receipt_actor_invalid');
      check(!!d && r.action === d.action && equal(r.params,d.params), 'receipt_decision_mismatch', {step,actor:r.actor});
      check(typeof r.accepted === 'boolean' && typeof r.skipped === 'boolean' && !(r.accepted && r.skipped), 'receipt_flags_invalid', {step,actor:r.actor});
      check(r.skipped === (r.action === null), 'receipt_skip_semantics_mismatch', {step,actor:r.actor});
      if (r.accepted) {
        p.accepted_actions++; phaseCounts.accepted++; inc(p.accepted_by_action,actionLabel(r.action)); inc(acceptedActions,actionLabel(r.action));
        check(r.canonical?.ok === true && r.canonical.actor === r.actor, 'accepted_receipt_not_canonical', {step,actor:r.actor});
      } else if (r.skipped) { p.skipped_decisions++; phaseCounts.skipped++; inc(skippedReasons,errorCategory(r.error)); }
      else { p.rejected_actions++; phaseCounts.rejected++; inc(rejectedActions,actionLabel(r.action)); }
      receipts.push(r); appliedCalls.add(cid(step,r.actor));
    }
    if (current.stage === 'primary') primaryPhases++; else postVotePhases++;
    phaseReport.push({step,round:current.round,phase:current.phase,stage:current.stage,required_actors:current.required_actors.length,
      ...phaseCounts,state_hash_before:current.state_hash,state_hash_after:res.snapshot.state_hash,
      laws_passed_after:res.snapshot.world.laws_passed,last_law_passed_after:res.snapshot.world.last_law_passed});
    validateSnapshot(res.snapshot,step+1);
    if (snapshots.has(step+1)) check(equal(snapshots.get(step+1),res.snapshot), 'saved_snapshot_replay_mismatch', {step:step+1});
    check(res.snapshot.world.logical_time >= current.world.logical_time, 'logical_time_regressed', {step});
    if (res.snapshot.stage === 'post_vote') check(res.snapshot.world.logical_time === current.world.logical_time && res.snapshot.round === current.round && current.phase === 'law', 'post_vote_time_or_round_changed', {step});
    current = res.snapshot;
  }
  check(equal(current,final), 'final_snapshot_chain_mismatch');
  for (const [step] of snapshots) check(step <= final.step, 'snapshot_after_final_state', {step});
  const completed = final.phase === 'finished';
  check(summary.game_completed === completed && summary.step === final.step && summary.round === final.round
    && summary.phase === final.phase && summary.state_hash === final.state_hash, 'summary_final_state_mismatch');
  check(summary.game_id === config.init.game_id && summary.agents === n && equal(summary.profile,config.profile), 'summary_config_mismatch');
  for (const key of ['calls','known_tokens','reserved_or_unknown_tokens','unknown_usage_calls','budget_overrun']) check(summary[key] === totals[key], `summary_${key}_mismatch`);
  const accepted = sum(perAgent.map(a=>a.accepted_actions)), rejected = sum(perAgent.map(a=>a.rejected_actions)), skipped = sum(perAgent.map(a=>a.skipped_decisions));
  check(summary.accepted_actions === accepted && summary.rejected_actions === rejected && summary.skipped_decisions === skipped, 'summary_receipt_counts_mismatch');
  check(summary.agents_with_model_response === perAgent.filter(a=>a.parsed_action_responses>0).length
    && summary.agents_with_accepted_model_action === perAgent.filter(a=>a.accepted_actions>0).length, 'summary_actor_counts_mismatch');
  const hasErrors = totals.unknown_usage_calls > 0 || totals.budget_overrun || rejected > 0 || skipped > 0;
  check(!completed || summary.status === (hasErrors?'completed_with_errors':'completed'), 'summary_completion_status_mismatch');
  check(totals.calls <= config.max_calls, 'configured_call_cap_exceeded');
  check(totals.known_tokens + totals.reserved_or_unknown_tokens <= config.max_tokens || totals.budget_overrun, 'unreported_token_cap_exceeded');
  check(totals.inflight_calls === 0, 'unresolved_inflight_calls');
  if (completed) {
    check(primaryPhases === 18 && postVotePhases <= 6 && preparedPhases === 0, 'six_round_phase_count_mismatch');
    check(perAgent.every(a=>a.primary_calls === 18), 'not_every_actor_has_18_primary_calls');
    check(calls.size === receipts.length && appliedCalls.size === calls.size, 'completed_game_unapplied_calls');
    check(equal(summary.result,final.result), 'summary_settlement_mismatch');
    for (let round=1;round<=6;round++) for (const phase of ['market','action','law'])
      check(phaseReport.filter(p=>p.round===round&&p.phase===phase&&p.stage==='primary').length===1,'round_phase_missing_or_duplicate',{round,phase});
  }

  let settlement = null;
  const finalBalances = [...final.world.public_state].sort((a,b)=>a.actor-b.actor).map(r=>({actor:r.actor,cash:r.cash,hard:r.hard,goods:r.goods,influence:r.influence,alive:r.alive}));
  if (completed) {
    const r = final.result;
    require(object(r) && Array.isArray(r.leaderboard) && Array.isArray(r.order) && Array.isArray(r.payouts),'invalid_settlement_shape');
    check(r.settlement_mode === 'simulated' && r.order.length === n && unique(r.order) && r.order.every(i=>uint(i)&&i<n), 'settlement_order_invalid');
    require(r.payouts.every(uint) && uint(r.bank) && uint(r.rake), 'invalid_settlement_amounts');
    check(r.leaderboard.length === n && r.payouts.length === n, 'settlement_amounts_invalid');
    const leaderboard = r.leaderboard.map((row,i) => {
      require(['actor','rank','cash','hard','goods','influence','payout'].every(k=>uint(row[k])), 'invalid_public_leaderboard');
      const actor = row.actor, balance = finalBalances.find(b=>b.actor===actor);
      check(actor === r.order[i] && row.rank === i+1 && row.payout === r.payouts[actor], 'leaderboard_order_or_payout_mismatch', {rank:i+1});
      check(!!balance && ['cash','hard','goods','influence'].every(k=>row[k]===balance[k]), 'leaderboard_balance_mismatch', {rank:i+1});
      return {rank:i+1,actor,cash:row.cash,hard:row.hard,goods:row.goods,influence:row.influence,payout:row.payout};
    });
    const totalPayout = sum(r.payouts), topFour = leaderboard.slice(0,4), topFourPayout = sum(topFour.map(r=>r.payout));
    const conservationApplies = config.init.epoch === 0;
    if (conservationApplies) {
      check(r.bank === config.init.entry_fee*n, 'epoch0_bank_not_entry_fees');
      check(totalPayout+r.rake === r.bank && topFourPayout+r.rake === r.bank, 'epoch0_top_four_plus_rake_not_bank');
      check(leaderboard.slice(4).every(r=>r.payout===0), 'epoch0_payout_outside_top_four');
      check(BigInt(r.rake) === BigInt(r.bank)*BigInt(final.world.rake_bps)/10000n, 'rake_calculation_mismatch');
    }
    settlement = {mode:'simulated',unit:'micro_peso',peso:final.world.peso,bank:r.bank,rake:r.rake,total_payout:totalPayout,
      top_four_payout:topFourPayout,top_four_plus_rake_equals_bank:topFourPayout+r.rake===r.bank,
      conservation_assertion_applicable:conservationApplies,payout_recipient_count:r.payouts.filter(x=>x>0).length,top_four:topFour,leaderboard};
  }
  const events = intervals.filter(x=>x.duration>1).flatMap(x=>[[x.start,1],[x.end-1,-1]]).sort((a,b)=>a[0]-b[0]||a[1]-b[1]);
  let active=0, observedPeak=0; for (const [,delta] of events) { active+=delta; observedPeak=Math.max(active,observedPeak); }
  const firstStart = intervals.length ? Math.min(...intervals.map(x=>x.start)) : null, lastEnd = intervals.length ? Math.max(...intervals.map(x=>x.end)) : null;
  manifest.sort((a,b)=>a.name.localeCompare(b.name));
  const errors = issues.slice(0,200);
  const report = {
    schema:'alashi-world-public-audit-v1',generated_at:new Date().toISOString(),source_commit:sourceCommit,
    evidence:{kind:config.profile.source==='test_provider'?'deterministic_test':'locally_recorded_peer_reported',
      frozen_digest:frozen.digest,worker_sha256:config.worker_sha256,driver_sha256:config.driver_sha256,
      auditor_sha256:sha(readFileSync(fileURLToPath(import.meta.url))),private_artifact_manifest_sha256:sha(canonical(manifest)),
      artifact_file_count:manifest.length,summary_sha256:manifest.find(x=>x.name==='summary.json').sha256,
      final_snapshot_sha256:manifest.find(x=>x.name==='final-snapshot.json').sha256,runtime_file_checks:runtimeChecks,
      checks,issues_count:issues.length,issues:errors,issues_truncated:issues.length>errors.length,
      journal_consistent:issues.length===0,state_hash_verification:'stored_hash_chain_and_full_snapshot_equality_only',
      canonical_worker_reexecution_performed:false},
    experiment:{game_id:config.init.game_id,seed:config.init.seed,epoch:config.init.epoch,agents:n,rounds_planned:6,
      game_completed:completed,driver_status:['completed','completed_with_errors','stopped','running'].includes(summary.status)?summary.status:'unrecognized',
      stop_reason:reasonLabel(summary.reason),round_reached:final.round,phase_reached:final.phase,final_state_hash:final.state_hash,
      primary_phases_applied:primaryPhases,post_vote_phases_applied:postVotePhases,prepared_unapplied_phases:preparedPhases,
      configuration:{parallelism:config.parallelism,max_calls:config.max_calls,max_tokens:config.max_tokens,call_timeout_ms:config.call_timeout_ms,max_seconds:config.max_seconds,max_consecutive_provider_failures:config.max_consecutive_provider_failures}},
    provider:{requested_model:modelLabel(config.profile.model),returned_models:models,model_identity_basis:'peer_reported',temperature:0,max_completion_tokens:256,enable_thinking:false,
      call_status_counts:callStatus,http_status_counts:httpStatus,finish_reason_counts:finishReasons,error_categories:providerErrors,
      ...totals,usage_basis:'peer_reported',unknown_usage_is_not_zero:true,provider_cost_usd:null},
    participation:{actors_with_any_call:perAgent.filter(a=>a.calls>0).length,actors_with_http_2xx:perAgent.filter(a=>a.http_2xx_responses>0).length,
      actors_with_parsed_action:perAgent.filter(a=>a.parsed_action_responses>0).length,actors_with_menu_valid_action:perAgent.filter(a=>a.menu_valid_action_responses>0).length,
      actors_with_accepted_action:perAgent.filter(a=>a.accepted_actions>0).length,all_actors_have_18_primary_calls:perAgent.every(a=>a.primary_calls===18),
      per_agent:perAgent},
    actions:{accepted,rejected,skipped,unapplied_call_records:calls.size-appliedCalls.size,accepted_by_action:acceptedActions,rejected_by_action:rejectedActions,skipped_reason_categories:skippedReasons},
    timing:{completed_call_latency_ms:distribution(latency),first_measured_request_at:firstStart===null?null:new Date(firstStart).toISOString(),
      last_estimated_response_at:lastEnd===null?null:new Date(lastEnd).toISOString(),measured_provider_window_ms:firstStart===null?null:lastEnd-firstStart,
      run_start_to_last_estimated_response_ms:lastEnd===null||!runMeta?null:lastEnd-runMeta.started_at_ms,
      peak_measured_overlapping_requests:observedPeak,overlap_basis:'completed calls only; timestamp plus rounded elapsed_ms minus 1ms; unknown calls excluded'},
    phases:phaseReport,settlement,final_balances:finalBalances,
    spending:{nvidia_spend_usd:summary.nvidia_spend_usd===0?0:null,nvidia_basis:'driver configuration and supplied journal; no external billing audit',provider_cost_usd:null,
      blockchain_transactions:summary.blockchain_transactions===0?0:null,alashi_http_requests:summary.alashi_http_requests===0?0:null},
    limitations:['Token usage and returned model names are supplied by BitLLM peers, not independently attested.',
      'One shared model serves distinct stateful player identities; this does not establish diverse independent intelligence.',
      'Action menus and opponent observations are bounded. A single run does not establish strategy strength or economic balance.',
      'This audit checks durable journal consistency and settlement arithmetic. It does not replay canonical Rust state transitions.',
      'Game cash need not be conserved: the canonical market, production and laws include external economic flows.',
      'No endpoint URLs, local paths, peer identifiers, response IDs, prompts, raw responses or raw errors are copied into this allowlisted report.']
  };
  return report;
}

function main(args) {
  const options = {};
  for (let i=0;i<args.length;i+=2) {
    if (!['--run','--out','--worker','--driver','--source-commit'].includes(args[i]) || !args[i+1] || options[args[i]]) throw new Error('usage_run_out_optional_worker_driver_source_commit');
    options[args[i]]=args[i+1];
  }
  if (!options['--run'] || !options['--out']) throw new Error('usage_requires_run_and_out');
  const output = resolve(options['--out']), run = resolve(options['--run']);
  if (output === run || output.startsWith(run+'/')) throw new Error('output_must_be_outside_read_only_run_directory');
  if (existsSync(output) && (!lstatSync(output).isFile() || lstatSync(output).isSymbolicLink())) throw new Error('unsafe_output_file');
  const report = auditWorld({run,worker:options['--worker'],driver:options['--driver'],sourceCommit:options['--source-commit']});
  writeFileSync(output,JSON.stringify(report,null,2)+'\n',{mode:0o600});
  console.log(JSON.stringify({journal_consistent:report.evidence.journal_consistent,issues:report.evidence.issues_count,game_completed:report.experiment.game_completed,
    calls:report.provider.calls,known_tokens:report.provider.known_tokens,unknown_usage_calls:report.provider.unknown_usage_calls,accepted:report.actions.accepted,rejected:report.actions.rejected,skipped:report.actions.skipped}));
  if (!report.evidence.journal_consistent) process.exitCode=2;
}
if (process.argv[1] && import.meta.url===pathToFileURL(resolve(process.argv[1])).href) {
  try { main(process.argv.slice(2)); }
  catch (error) { console.error(/^[a-z0-9_]+$/.test(error.message)?error.message:'audit_failed_invalid_or_unreadable_input'); process.exitCode=1; }
}
