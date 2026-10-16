import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, rmSync, chmodSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { parseCompletion, providerUsage, apiUrl, modelProvider, readPrivate, runCycle, executeTool, toolInfo } from './strategy-run.mjs';
test('strict completion parsing preserves usage independently of JSON quality', () => {
  assert.deepEqual(parseCompletion('```json\n{"units":5}\n```'),{units:5});
  assert.deepEqual(parseCompletion('{"units":5}'),{units:5});
  assert.equal(parseCompletion('prefix ```json\n{}\n```'),null);
  assert.equal(parseCompletion('x'.repeat(8193)),null);
  assert.equal(providerUsage({prompt_tokens:10,completion_tokens:4,total_tokens:15}),null);
  assert.equal(providerUsage({prompt_tokens:10,completion_tokens:5,total_tokens:15,prompt_tokens_details:{cached_tokens:11}}),null);
  assert.deepEqual(providerUsage({prompt_tokens:10,completion_tokens:5,total_tokens:15}),{prompt_tokens:10,completion_tokens:5,total_tokens:15,cached_tokens:0});
  assert.equal(providerUsage(null),null);
});
test('profile and endpoint boundaries reject secrets in URLs and insecure remote providers', () => {
  assert.equal(apiUrl('http://127.0.0.1:8790'),'http://127.0.0.1:8790');
  for(const url of ['http://remote.example','https://evil.example','https://key@alashi.network','https://alashi.network?key=x']) assert.throws(()=>apiUrl(url));
  assert.throws(()=>modelProvider({url:'https://remote.example/api',model:'test',test:true}));
  assert.throws(()=>modelProvider({url:'http://remote.example/api',model:'test'}));
  const dir=mkdtempSync(join(tmpdir(),'alashi-private-check-'));
  try { const path=join(dir,'profile');writeFileSync(path,'{}',{mode:0o600});assert.deepEqual(readPrivate(path),{});chmodSync(path,0o644);assert.throws(()=>readPrivate(path));chmodSync(path,0o600);symlinkSync(path,join(dir,'link'));assert.throws(()=>readPrivate(join(dir,'link'))); }
  finally {rmSync(dir,{recursive:true,force:true});}
});
test('ambiguous inflight journal is recorded as unknown without a second provider call', async () => {
  const id='a'.repeat(64), record='b'.repeat(64), version='c'.repeat(64), dir=mkdtempSync(join(tmpdir(),'alashi-ambiguous-check-')), journalPath=join(dir,'journal');
  const run={id,record,version,status:'running',deadline_at:Math.floor(Date.now()/1000)+180,calls:{deliver:{reserved:5000,status:'reserved',usage:null,quality_passed:null,source:'test_provider',provider:'local',model:'test'}},brief:{facts:[],required_terms:[],cta:'Explore ALASHI'},known_tokens:0,reserved_tokens:5000};
  let calls=0;
  const data=()=>({versions:[{id:version,content:{plan:[{id:'deliver',kind:'model',depends_on:[],instruction:'Write the ad.',tool:null}]}}],runs:[structuredClone(run)]});
  const api={record,get:async()=>data(),request:async()=>data(),post:async(path,body)=>{
    if(path.endsWith('/reserve')) return {run};
    assert.ok(path.endsWith('/report'));assert.equal(body.usage,null);assert.equal(body.request_failed,true);
    run.status='usage_unknown';return {run:{...run,calls:{'deliver':{...run.calls['deliver'],quality_passed:false}}}};
  }};
  writeFileSync(journalPath,JSON.stringify({run:id,record,calls:{'deliver':{phase:'inflight'}}}),{mode:0o600});
  try {
    await assert.rejects(runCycle({api,id,provider:{source:'test_provider',provider:'local',model:'test',complete:async()=>{calls++;}},journalPath}),/stage_stopped/);
    assert.equal(calls,0);assert.equal(run.status,'usage_unknown');assert.equal(run.reserved_tokens,5000);
    assert.equal(JSON.parse(readFileSync(journalPath)).calls['deliver'].result.usage,null);
  } finally {rmSync(dir,{recursive:true,force:true});}
});
test('builtin tools report zero model usage explicitly; public plans cannot select executables',async()=>{
 const run={brief:{facts:['Crystals represent tokens.'],audience:'Agent owners',required_terms:['ALASHI'],cta:'Explore ALASHI'}};
 const result=await executeTool({tool:'brief_extract'},run,{});
 assert.deepEqual(result.report.facts,run.brief.facts);assert.equal(result.usage.total_tokens,0);
 const check=await executeTool({tool:'copy_check',depends_on:['draft']},run,{draft:{headline:'ALASHI',body:'Bring your agents.',cta:'Explore ALASHI'}});
 assert.equal(check.report.cta_matches,true);assert.equal(check.usage.total_tokens,0);
 assert.throws(()=>toolInfo({tool:'untrusted-command'}),/owner_tool_not_configured/);
 assert.throws(()=>toolInfo({tool:'my-agent'},{'my-agent':{argv:['owned-command'],no_model_calls:false}}),/tool_usage_budget_required/);
});
test('an oversized owner-tool report keeps its known usage for the failed quality gate',async()=>{
 const registry={owned:{argv:[process.execPath,'-e',"console.log(JSON.stringify({report:{text:'x'.repeat(9000)},response_id:'',usage:{prompt_tokens:10,completion_tokens:5,total_tokens:15}}))"],no_model_calls:false,max_total_tokens:5000,provider:'local-fixture',model:'fixture'}};
 const result=await executeTool({tool:'owned',depends_on:[]},{brief:{}},{},registry);
 assert.equal(result.report,null);assert.equal(result.response_id,null);assert.equal(result.usage.total_tokens,15);
});
