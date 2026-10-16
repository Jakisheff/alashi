#!/usr/bin/env node
/** Local owner runner. No provider call occurs until a server reservation exists. */
import { createPrivateKey, createPublicKey, sign, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync, renameSync, lstatSync, openSync, closeSync, fsyncSync, unlinkSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawn } from 'node:child_process';
import { act as gameAct, lock as gameLock } from './alashi.mjs';

const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
export function base58(bytes) {
  let n = BigInt('0x' + Buffer.from(bytes).toString('hex')), out = '';
  while (n) { out = alphabet[Number(n % 58n)] + out; n /= 58n; }
  for (const b of bytes) { if (b) break; out = '1' + out; }
  return out;
}
function fail(code) { throw new Error(code); }
export function apiUrl(value) {
  const u = new URL(value);
  if (u.username || u.password || u.search || u.hash || u.pathname !== '/' ||
      !(u.protocol === 'https:' && u.hostname === 'alashi.network' || u.protocol === 'http:' && ['127.0.0.1','localhost','[::1]'].includes(u.hostname))) fail('invalid_api_url');
  return u.origin;
}
export function readPrivate(path) {
  const s = lstatSync(path);
  if (!s.isFile() || s.isSymbolicLink() || process.platform !== 'win32' && (s.mode & 0o077)) fail('private_file_requires_mode_600');
  return JSON.parse(readFileSync(path, 'utf8'));
}
function save(path, value) {
  const tmp = path + '.tmp-' + randomUUID(), fd = openSync(tmp, 'wx', 0o600);
  try { writeFileSync(fd, JSON.stringify(value)); fsyncSync(fd); } finally { closeSync(fd); }
  renameSync(tmp, path);
}
export async function ownerApi(base, profile, origin = 'https://alashi.network') {
  base = apiUrl(base);
  if (!/^[0-9a-f]{64}$/.test(profile.agent_record_id) || !Array.isArray(profile.secret_key) || profile.secret_key.length !== 64 || profile.secret_key.some(n => !Number.isInteger(n) || n < 0 || n > 255)) fail('invalid_profile');
  const key = createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b657004220420','hex'),Buffer.from(profile.secret_key.slice(0,32))]),format:'der',type:'pkcs8'});
  const pub = createPublicKey(key).export({format:'der',type:'spki'}).subarray(-32), wallet = base58(pub);
  if (profile.wallet !== wallet || !Buffer.from(profile.secret_key.slice(32)).equals(pub)) fail('profile_wallet_mismatch');
  let bearer = '';
  const prefix = `/agents/${profile.agent_record_id}/owner`;
  async function request(path, body) {
    const r = await fetch(base + path, {method:body === undefined ? 'GET':'POST',redirect:'error',signal:AbortSignal.timeout(15_000),headers:{Origin:origin,Accept:'application/json',...(bearer?{Authorization:'Bearer '+bearer}:{}),...(body===undefined?{}:{'Content-Type':'application/json'})},...(body===undefined?{}:{body:JSON.stringify(body)})});
    const v = await r.json();
    if (!r.ok || v.ok !== true) fail(typeof v.error === 'string' && /^[a-z0-9_]{1,80}$/.test(v.error) ? v.error : 'arena_request_failed');
    return v;
  }
  const c = await request(prefix+'/challenge', {});
  const lines = typeof c.message === 'string' ? c.message.split('\n') : [];
  if (c.wallet !== wallet || lines.length !== 8 || lines[0] !== 'alashi-owner-auth-v1' || lines[1] !== `origin:${origin}` || lines[2] !== `agent_record_id:${profile.agent_record_id}` || lines[3] !== `wallet:${wallet}` || !/^nonce:[0-9a-f]{64}$/.test(lines[4]) || lines[5] !== `issued_at:${c.issued_at}` || lines[6] !== `expires_at:${c.expires_at}` || lines[7] !== '' || c.expires_at*1000 <= Date.now() || Math.abs(c.issued_at*1000-Date.now()) > 300_000) fail('invalid_owner_challenge');
  const s = await request(prefix+'/session',{challenge_id:c.challenge_id,signature:base58(sign(null,Buffer.from(c.message),key))});
  bearer = s.owner_session;
  if (!/^[0-9a-f]{64}$/.test(bearer)) fail('invalid_owner_session');
  return {request, get:() => request(prefix+'/strategies'), post:(suffix,body)=>request(prefix+suffix,body), record:profile.agent_record_id, base, wallet};
}
const safeReport = report => Buffer.byteLength(JSON.stringify(report ?? null), 'utf8') <= 8192 ? report ?? null : null;
const receiptId = id => typeof id === 'string' && id.length > 0 && Buffer.byteLength(id, 'utf8') <= 200 ? id : null;
export function parseCompletion(content) {
  if (typeof content !== 'string' || content.length > 8192) return null;
  const match = content.trim().match(/^```(?:json)?\s*\n([\s\S]*?)\n```$/i);
  try { return safeReport(JSON.parse(match ? match[1] : content)); } catch { return null; }
}
export function providerUsage(raw) {
  if (!raw || !['prompt_tokens','completion_tokens','total_tokens'].every(k => Number.isSafeInteger(raw[k]) && raw[k]>=0) || raw.prompt_tokens+raw.completion_tokens !== raw.total_tokens) return null;
  const cached = raw.prompt_tokens_details?.cached_tokens ?? 0;
  if (!Number.isSafeInteger(cached) || cached < 0 || cached > raw.prompt_tokens) return null;
  return {prompt_tokens:raw.prompt_tokens,completion_tokens:raw.completion_tokens,total_tokens:raw.total_tokens,cached_tokens:cached};
}
export function modelProvider({url, key, model, test = false}) {
  const u = new URL(url), local = ['localhost','127.0.0.1','[::1]'].includes(u.hostname);
  if (u.username || u.password || u.hash || u.search || !(u.protocol === 'https:' || u.protocol === 'http:' && local) || test && !local || !model || model.length > 128) fail('invalid_provider_config');
  return {source:test?'test_provider':'provider_reported',provider:u.origin,model,async complete(messages) {
    // No retry, fallback model, or tools. Keep provider credentials out of arena receipts.
    const response = await fetch(url,{method:'POST',redirect:'error',signal:AbortSignal.timeout(60_000),headers:{'Content-Type':'application/json',...(key?{Authorization:'Bearer '+key}:{})},body:JSON.stringify({model,messages,max_tokens:256,temperature:0})});
    if (Number(response.headers.get('content-length'))>1_000_000) fail('provider_response_too_large');
    const text = await response.text(); if (text.length>1_000_000) fail('provider_response_too_large');
    let raw; try { raw = JSON.parse(text); } catch { raw = {}; }
    return {response_id:receiptId(raw.id),usage:providerUsage(raw.usage),report:parseCompletion(raw.choices?.[0]?.message?.content),request_failed:!response.ok};
  }};
}
export function messagesFor(run, version, task, reports) {
  const spec = version.content.plan.find(t => t.id === task);
  if (run.brief.trading) return [{role:'system',content:'You control one ALASHI trading faction. Use the shared game snapshot and your own task reports. Cash is in game units (1000000 = 1 alashi); price_table is whole alashi per good. Return a JSON object. The deliver task must return EXACTLY {"action":"sell","params":{"units":1}}, or buy with units, or {"action":"produce","params":{}} in the action phase. The server checks the current game rules. Do not invent a trade receipt, token count or profit. Other tasks return a useful JSON report. Team instruction: '+spec.instruction}, {role:'user',content:JSON.stringify({task,owner:run.owner,brief:run.brief,dependency_reports:Object.fromEntries(spec.depends_on.map(id=>[id,reports[id]]))})}];
  return [{role:'system',content:'You are an AI marketing specialist in an ALASHI duel. Work only from the shared brief facts. Do not invent savings, audience results or guarantees. Return one JSON object, no commentary. Use English with ASCII punctuation. For the final deliver task return EXACTLY {"headline":"...","body":"...","cta":"..."}. Stay within max_words across all three fields; headline at most 12 words. Include required_terms and copy cta exactly. Intermediate tasks return a useful JSON report. Team instruction: '+spec.instruction},
    {role:'user',content:JSON.stringify({task,brief:run.brief,dependency_reports:Object.fromEntries(spec.depends_on.map(id=>[id,reports[id]]))})}];
}
const zeroUsage = {prompt_tokens:0,completion_tokens:0,total_tokens:0,cached_tokens:0};
export function toolInfo(spec, registry = {}) {
  if (['brief_extract','copy_check','trade_state'].includes(spec.tool)) return {source:'local_tool',provider:'alashi-local-tools',model:'none',cap:0};
  const config = registry[spec.tool];
  if (!config || !Array.isArray(config.argv) || !config.argv.length || config.argv.some(s=>typeof s!=='string') || typeof config.no_model_calls!=='boolean') fail('owner_tool_not_configured');
  if (config.no_model_calls) return {source:'local_tool',provider:spec.tool,model:'none',cap:0};
  if (!Number.isSafeInteger(config.max_total_tokens) || config.max_total_tokens<1 || !config.provider || !config.model) fail('tool_usage_budget_required');
  return {source:'provider_reported',provider:config.provider,model:config.model,cap:config.max_total_tokens};
}
export async function executeTool(spec, run, reports, registry = {}) {
  if (spec.tool === 'trade_state') {
    const c=run.brief.trading;if(!c)fail('trading_challenge_required');
    return {report:{phase:c.phase,round:c.round,owner:c.participants.find(p=>p.wallet===run.owner),market:c.observation.market??Object.fromEntries(['sold_counter','price_table','price_now','tax_bps','price_shift','boom'].map(k=>[k,c.observation[k]])),allowed_actions:c.phase==='market'?['sell','buy']:['produce']},usage:zeroUsage,response_id:null,request_failed:false};
  }
  if (spec.tool === 'brief_extract') return {report:{facts:run.brief.facts,audience:run.brief.audience,required_terms:run.brief.required_terms,cta:run.brief.cta},usage:zeroUsage,response_id:null,request_failed:false};
  if (spec.tool === 'copy_check') {
    const draft = spec.depends_on.map(id=>reports[id]).find(r=>r && typeof r.headline==='string' && typeof r.body==='string');
    const text = draft ? [draft.headline,draft.body,draft.cta].join(' ') : '';
    return {report:{draft:draft??null,words:text.trim().split(/\s+/).filter(Boolean).length,required_terms:run.brief.required_terms.every(t=>text.toLowerCase().includes(t.toLowerCase())),cta_matches:draft?.cta===run.brief.cta},usage:zeroUsage,response_id:null,request_failed:false};
  }
  const config=registry[spec.tool]; toolInfo(spec,registry);
  // A public strategy selects only a name. The owner supplies the executable locally.
  const raw=await new Promise((yes,no)=>{
    const child=spawn(config.argv[0],config.argv.slice(1),{stdio:['pipe','pipe','pipe'],timeout:60_000});
    let text='',size=0;child.on('error',()=>no(new Error('owner_tool_failed')));
    child.stdout.on('data',b=>{size+=b.length;if(size>65536)child.kill();else text+=b;});child.stderr.resume();
    child.on('close',code=>{if(code!==0 || size>65536)no(new Error('owner_tool_failed'));else {try{yes(JSON.parse(text));}catch{no(new Error('invalid_tool_response'));}}});
    child.stdin.on('error',()=>{});child.stdin.end(JSON.stringify({task:spec,brief:run.brief,reports:Object.fromEntries(spec.depends_on.map(id=>[id,reports[id]]))}));
  });
  return {report:safeReport(raw.report),usage:config.no_model_calls?zeroUsage:providerUsage(raw.usage),response_id:receiptId(raw.response_id),request_failed:raw.request_failed===true};
}
export async function runCycle({api,id,provider,journalPath,tools={},trade,onProgress=()=>{}}) {
  if (!/^[0-9a-f]{64}$/.test(id)) fail('invalid_run');
  const lockPath=journalPath+'.lock',fd=openSync(lockPath,'wx',0o600);closeSync(fd);
  try {
    let initial=await api.get(),run=initial.runs.find(r=>r.id===id);
    if (!run || run.record!==api.record) fail('run_not_owned');
    if(run.brief.trading && run.status!=='completed' && !trade)fail('trade_executor_required');
    const pub=await api.request('/strategy-library'),version=[...initial.versions,...pub.versions].find(v=>v.id===run.version);
    if (!version) fail('strategy_unavailable');
    const journal=existsSync(journalPath)?readPrivate(journalPath):{run:id,record:api.record,calls:{}};
    if(journal.run!==id || journal.record!==api.record)fail('journal_scope_mismatch');
    if(run.status==='completed'){if(trade&&run.brief.trading)await trade({api,run,journal,save:()=>save(journalPath,journal)});return run;}
    const waitingUntil=Date.now()+15*60_000;
    while(!run.deadline_at) {
      if(Date.now()>waitingUntil)fail('opponent_wait_ended');
      onProgress({status:'waiting_for_opponent'});await new Promise(resolve=>setTimeout(resolve,1000));
      initial=await api.get();run=initial.runs.find(r=>r.id===id);
    }
    const reports={};
    const identity=spec=>spec.kind==='tool'?toolInfo(spec,tools):{source:provider.source,provider:provider.provider,model:provider.model,cap:null};
    async function prepare(spec) {
      const old=run.calls[spec.id];if(old?.quality_passed&&old.usage){reports[spec.id]=old.report;return;}
      if(run.status!=='running')fail('run_stopped');
      if(!journal.calls[spec.id]){journal.calls[spec.id]={phase:old?'inflight':'prepared'};save(journalPath,journal);}
      const who=identity(spec),messages=messagesFor(run,version,spec.id,reports),cap=who.cap??(Buffer.byteLength(JSON.stringify(messages),'utf8')+2048+256);
      // Recover a paid response even after the round ends; never request another completion.
      if(!old)await api.post(`/strategy-runs/${id}/reserve`,{task:spec.id,max_total_tokens:cap,source:who.source,provider:who.provider,model:who.model});
      else if(old.source!==who.source||old.provider!==who.provider||old.model!==who.model)fail('journal_provider_mismatch');
    }
    async function call(spec) {
      const old=run.calls[spec.id];if(old?.quality_passed&&old.usage)return;
      const entry=journal.calls[spec.id];
      if(!entry.result) {
        if(entry.phase==='inflight')entry.result={response_id:null,usage:null,report:null,request_failed:true};
        else {
          if(Date.now()>=run.deadline_at*1000)fail('round_ended');
          entry.phase='inflight';save(journalPath,journal);onProgress({task:spec.id,status:'started'});
          try {entry.result=spec.kind==='tool'?await executeTool(spec,run,reports,tools):await provider.complete(messagesFor(run,version,spec.id,reports));}
          catch {entry.result={response_id:null,usage:null,report:null,request_failed:true};}
        }
        save(journalPath,journal);
      }
      const receipt=await api.post(`/strategy-runs/${id}/report`,{task:spec.id,...entry.result});
      reports[spec.id]=receipt.run.calls[spec.id].report;onProgress({task:spec.id,status:receipt.run.status,known_tokens:receipt.run.known_tokens});
      if(!receipt.run.calls[spec.id].quality_passed||!receipt.run.calls[spec.id].usage)fail('report_or_usage_not_verified');
    }
    while(version.content.plan.some(t=>!run.calls[t.id]?.quality_passed)) {
      if(run.status!=='running')fail('run_stopped');
      const ready=version.content.plan.filter(t=>!run.calls[t.id]?.quality_passed&&t.depends_on.every(d=>run.calls[d]?.quality_passed&&run.calls[d]?.usage));
      if(!ready.length)fail('dependencies_not_complete');
      const reservations=await Promise.allSettled(ready.map(prepare));if(reservations.some(r=>r.status==='rejected'))fail('stage_reservation_stopped');
      const results=await Promise.allSettled(ready.map(call));if(results.some(r=>r.status==='rejected'))fail('stage_stopped');
      run=(await api.get()).runs.find(r=>r.id===id);
      for(const [task,c]of Object.entries(run.calls))if(c.quality_passed&&c.usage)reports[task]=c.report;
    }
    if(run.brief.trading) {
      if(!trade)fail('trade_executor_required');
      await trade({api,run,journal,save:()=>save(journalPath,journal)});
    }
    return(await api.post(`/strategy-runs/${id}/finish`,{})).run;
  } finally {unlinkSync(lockPath);}
}
export function httpTrade(sessionPath, fetcher=fetch) {
  return async({api,run})=>{
    const release=gameLock(sessionPath);
    try {
      const session=readPrivate(sessionPath), context=run.brief.trading;
      if(context.mode!=='http'||String(session.game_id)!==context.game||session.agent_record_id!==api.record||!Number.isSafeInteger(session.next_op_id)||session.next_op_id<1)fail('trade_session_mismatch');
      if(session.pending_act&&session.pending_act.action.strategy_run!==run.id)fail('other_trade_pending');
      if(run.status==='completed'&&!session.pending_act)return;
      if(!session.pending_act)await api.post(`/strategy-runs/${run.id}/prepare-trade`,{});
      const raw=JSON.stringify({...run.calls.deliver.report,by:'llm',strategy_run:run.id});
      const result=await gameAct(api.base,session,sessionPath,raw,fetcher);
      if(result.ok!==true)fail('trade_not_accepted');
    } finally {release();}
  };
}
export function solanaTrade({binary,keyPath,signature}) {
  return async({api,run,journal,save})=>{
    if(run.status==='completed')return;
    const context=run.brief.trading;
    if(context.mode!=='solana')fail('trade_session_mismatch');
    if(signature) {journal.trade={signature};save();}
    if(!journal.trade) {
      if(!binary||!keyPath)fail('trade_signer_required');
      const key=readPrivate(keyPath);
      if(!Array.isArray(key)||key.length!==64||base58(Buffer.from(key.slice(32)))!==api.wallet)fail('trade_wallet_mismatch');
      await api.post(`/strategy-runs/${run.id}/prepare-trade`,{});
      journal.trade={phase:'inflight'};save();
      const result=await new Promise((yes,no)=>{
        const child=spawn(binary,['agent','act','--game',context.game,'--key',keyPath,'--expected-round',String(context.round),'--expected-phase',context.phase,'--deadline',String(Math.min(context.phase_ends_at,run.deadline_at))],{stdio:['pipe','pipe','pipe'],timeout:90_000});
        let text='',size=0;child.on('error',()=>no(new Error('trade_outcome_unknown')));child.stderr.resume();
        child.stdout.on('data',b=>{size+=b.length;if(size>65536)child.kill();else text+=b;});
        child.on('close',()=>{try{yes(JSON.parse(text));}catch{no(new Error('trade_outcome_unknown'));}});
        child.stdin.on('error',()=>{});child.stdin.end(JSON.stringify(run.calls.deliver.report));
      });
      if(typeof result.receipt?.signature!=='string')fail('trade_outcome_unknown');
      journal.trade={signature:result.receipt.signature};save();
    }
    if(!journal.trade.signature)fail('trade_outcome_unknown');
    await api.post(`/strategy-runs/${run.id}/trade-receipt`,{signature:journal.trade.signature});
  };
}
async function main() {
  const args = process.argv.slice(2), opts = {};
  for (let i=0;i<args.length;i+=2) { if (!/^--[a-z-]+$/.test(args[i]) || !args[i+1] || args[i+1].startsWith('--')) fail('expected_flag_value_pairs'); opts[args[i].slice(2)] = args[i+1]; }
  if (!opts.profile || !opts.run || !opts.journal || opts['allow-provider']!=='yes') fail('required: --profile FILE --run ID --journal PRIVATE_FILE --allow-provider yes');
  const api = await ownerApi(opts.api ?? 'https://alashi.network',readPrivate(resolve(opts.profile)));
  const provider = modelProvider({url:process.env.ALASHI_MODEL_URL,key:process.env.ALASHI_MODEL_KEY,model:process.env.ALASHI_MODEL,test:opts['test-provider']==='yes'});
  if(opts['trade-session']&&(opts['trade-key']||opts['trade-signature']))fail('choose_one_trade_executor');
  const trade=opts['trade-session']?httpTrade(resolve(opts['trade-session'])):opts['trade-key']||opts['trade-signature']?solanaTrade({binary:opts['bots-binary']?resolve(opts['bots-binary']):undefined,keyPath:opts['trade-key']?resolve(opts['trade-key']):undefined,signature:opts['trade-signature']}):undefined;
  const run = await runCycle({api,id:opts.run,provider,journalPath:resolve(opts.journal),tools:opts.tools?readPrivate(resolve(opts.tools)):{},trade,onProgress:v=>process.stdout.write(JSON.stringify(v)+'\n')});
  process.stdout.write(JSON.stringify({run:run.id,status:run.status,known_tokens:run.known_tokens,unknown_usage:run.unknown_usage,test_provider:run.test_provider})+'\n');
}
if (process.argv[1] && import.meta.url===pathToFileURL(resolve(process.argv[1])).href) main().catch(e=>{console.error(/^[a-z0-9_: --]{1,200}$/i.test(e.message)?e.message:'strategy_runner_failed');process.exitCode=1;});
