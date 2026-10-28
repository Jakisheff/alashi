import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, readFileSync, rmSync, chmodSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { parseCompletion, providerUsage, apiUrl, modelProvider, readPrivate, runCycle, executeTool, toolInfo, solanaTrade, httpTrade, messagesFor } from './strategy-run.mjs';

function killFixture(path) {
  try {
    const pid = Number(readFileSync(path, 'utf8'));
    if (Number.isSafeInteger(pid) && pid > 1) process.kill(pid, 'SIGKILL');
  } catch {}
}

test('a missing trade executor stops before reserving or calling a model',async()=>{
 const id='a'.repeat(64),record='b'.repeat(64),dir=mkdtempSync(join(tmpdir(),'alashi-trade-preflight-'));let calls=0;
 const api={record,get:async()=>({runs:[{id,record,status:'running',brief:{trading:{mode:'http'}}}]}),request:async()=>{calls++;},post:async()=>{calls++;}};
 try {await assert.rejects(runCycle({api,id,provider:{complete:async()=>{calls++;}},journalPath:join(dir,'journal')}),/trade_executor_required/);assert.equal(calls,0);}
 finally {rmSync(dir,{recursive:true,force:true});}
});
test('trading tasks receive owner context and a real local state tool, without ad fields',async()=>{
 const run={owner:'owner-a',brief:{trading:{mode:'http',game:'1',phase:'action',round:1,participants:[{wallet:'owner-a',goods:0,cash:'0'}],observation:{price_table:[12,10],tax_bps:0}}}};
 const spec={id:'state',kind:'tool',tool:'trade_state',depends_on:[]};
 const result=await executeTool(spec,run,{});
 assert.equal(result.usage.total_tokens,0);assert.equal(result.report.owner.wallet,'owner-a');assert.deepEqual(result.report.allowed_actions,['produce']);
 const messages=messagesFor(run,{content:{plan:[{id:'deliver',instruction:'Produce',depends_on:['state']}]}},'deliver',{state:result.report});
 assert.equal(JSON.parse(messages[1].content).owner,'owner-a');assert.ok(!messages[0].content.includes('headline'));
});
test('an ambiguous Solana submission never starts another signer; a supplied signature only reconciles',async()=>{
 let posts=0,saves=0;const request={api:{post:async(path,body)=>{posts++;assert.ok(path.endsWith('/trade-receipt'));assert.equal(body.signature,'known-signature');}},run:{id:'a'.repeat(64),status:'running',brief:{trading:{mode:'solana'}}},journal:{trade:{phase:'inflight'}},save:()=>saves++};
 await assert.rejects(solanaTrade({binary:'/does-not-exist'})(request),/trade_outcome_unknown/);assert.equal(posts,0);
 await solanaTrade({signature:'known-signature'})(request);assert.equal(posts,1);assert.equal(saves,1);
});
test('HTTP execution refuses another owner or another pending run before any request',async()=>{
 const dir=mkdtempSync(join(tmpdir(),'alashi-trade-scope-')),path=join(dir,'session');
 try{
  writeFileSync(path,JSON.stringify({game_id:2,agent_record_id:'owner-a',next_op_id:1}),{mode:0o600});
  const input={api:{record:'owner-a'},run:{id:'a'.repeat(64),brief:{trading:{mode:'http',game:'1'}}}};
  await assert.rejects(httpTrade(path)(input),/trade_session_mismatch/);
  writeFileSync(path,JSON.stringify({game_id:1,agent_record_id:'owner-a',next_op_id:1,pending_act:{action:{strategy_run:'b'.repeat(64)}}}));
  await assert.rejects(httpTrade(path)(input),/other_trade_pending/);
 }finally{rmSync(dir,{recursive:true,force:true});}
});
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
  for (const url of ['https://user@provider.example/api','https://provider.example/api?key=x','https://provider.example/api#key','ftp://127.0.0.1/api']) {
    assert.throws(()=>modelProvider({url,model:'test'}), /invalid_provider_config/);
  }
  assert.throws(()=>modelProvider({url:'https://provider.example/api',model:''}), /invalid_provider_config/);
  assert.throws(()=>modelProvider({url:'https://provider.example/api',model:'x'.repeat(129)}), /invalid_provider_config/);
  const dir=mkdtempSync(join(tmpdir(),'alashi-private-check-'));
  try { const path=join(dir,'profile');writeFileSync(path,'{}',{mode:0o600});assert.deepEqual(readPrivate(path),{});chmodSync(path,0o644);assert.throws(()=>readPrivate(path));chmodSync(path,0o600);symlinkSync(path,join(dir,'link'));assert.throws(()=>readPrivate(join(dir,'link'))); }
  finally {rmSync(dir,{recursive:true,force:true});}
});
test('owner tools report missing executables, nonzero exits and invalid output', async () => {
  const spec={tool:'owned',depends_on:[]}, run={brief:{}};
  for (const [argv, expected] of [
    [['/nonexistent-alashi-tool'], /owner_tool_failed/],
    [[process.execPath, '-e', 'process.exit(2)'], /owner_tool_failed/],
    [[process.execPath, '-e', 'process.stdout.write("bad JSON")'], /invalid_tool_response/],
  ]) await assert.rejects(executeTool(spec,run,{}, {owned:{argv,no_model_calls:true}}), expected);
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
    await assert.rejects(runCycle({api,id,provider:{source:'test_provider',provider:'local',model:'test',complete:async()=>{calls++;}},journalPath}),/report_or_usage_not_verified/);
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
test('report limits count UTF-8 bytes so malformed output cannot lose known usage',async()=>{
 const content=JSON.stringify({text:'🙂'.repeat(3000)});
 assert.equal(parseCompletion(content)===null,true,'oversized UTF-8 report must become a rejected report, not a rejected receipt');
 const registry={owned:{argv:[process.execPath,'-e',"console.log(JSON.stringify({report:{text:'🙂'.repeat(3000)},response_id:'🙂'.repeat(60),usage:{prompt_tokens:10,completion_tokens:5,total_tokens:15}}))"],no_model_calls:false,max_total_tokens:5000,provider:'local-fixture',model:'fixture'}};
 const result=await executeTool({tool:'owned',depends_on:[]},{brief:{}},{},registry);
 assert.equal(result.report,null);assert.equal(result.response_id,null);assert.equal(result.usage.total_tokens,15);
});

test('oversized chunked provider response is cancelled before the entire body is read', async t => {
  let received = 0, cancelled = false;
  t.mock.method(globalThis, 'fetch', async () => new Response(new ReadableStream({
    pull(controller) {
      if (received === 4 * 1024 * 1024) return controller.close();
      const chunk = new Uint8Array(65536).fill(65);
      received += chunk.byteLength;
      controller.enqueue(chunk);
    },
    cancel() { cancelled = true; },
  }), { headers: { 'content-length': '1' } }));
  const provider = modelProvider({url:'http://127.0.0.1:8799/completions', model:'test', test:true});
  await assert.rejects(provider.complete([]), /provider_response_too_large/);
  assert.equal(cancelled, true);
  assert.ok(received < 2_000_000, `read ${received} bytes before rejecting`);
});

test('provider limit counts bytes and rejects an oversized declared body', async t => {
  let cancelled = false;
  t.mock.method(globalThis, 'fetch', async () => new Response(new ReadableStream({
    cancel() { cancelled = true; },
  }), { headers: { 'content-length': '1000001' } }));
  const provider = modelProvider({url:'http://127.0.0.1:8799/completions', model:'test', test:true});
  await assert.rejects(provider.complete([]), /provider_response_too_large/);
  assert.equal(cancelled, true);
  globalThis.fetch = async () => new Response('é'.repeat(500001));
  await assert.rejects(provider.complete([]), /provider_response_too_large/);
});

test('a provider response exactly at the byte limit is accepted', async t => {
  const prefix = '{"id":"within-limit","padding":"', suffix = '"}';
  const body = prefix + 'x'.repeat(1_000_000 - Buffer.byteLength(prefix + suffix)) + suffix;
  assert.equal(Buffer.byteLength(body), 1_000_000);
  t.mock.method(globalThis, 'fetch', async () => new Response(body, {headers:{'content-length':'1000000'}}));
  const provider = modelProvider({url:'http://127.0.0.1:8799/completions', model:'test', test:true});
  assert.equal((await provider.complete([])).response_id, 'within-limit');
});

test('an owner tool response exactly at the byte limit is accepted', async () => {
  const prefix = '{"report":{"text":"', suffix = '"}}';
  const body = prefix + 'x'.repeat(65536 - Buffer.byteLength(prefix + suffix)) + suffix;
  assert.equal(Buffer.byteLength(body), 65536);
  const code = `process.stdout.write(${JSON.stringify(body)})`;
  const registry = {owned:{argv:[process.execPath,'-e',code],no_model_calls:true}};
  const result = await executeTool({tool:'owned',depends_on:[]},{brief:{}},{},registry);
  assert.equal(result.report, null);
  assert.equal(result.usage.total_tokens, 0);
});

test('an oversized owner tool that ignores SIGTERM is terminated', async () => {
  if (process.platform === 'win32') return;
  const dir = mkdtempSync(join(tmpdir(), 'alashi-hung-tool-')), pidPath = join(dir, 'pid');
  const code = `require('fs').writeFileSync(${JSON.stringify(pidPath)}, String(process.pid)); process.on('SIGTERM', () => {}); process.stdout.write(Buffer.alloc(100000)); setInterval(() => {}, 1000)`;
  const registry = {owned:{argv:[process.execPath,'-e',code],no_model_calls:true}};
  const work = executeTool({tool:'owned',depends_on:[]},{brief:{}},{},registry).then(() => 'accepted', error => error.message);
  let timer;
  try {
    const outcome = await Promise.race([work, new Promise(resolve => {timer = setTimeout(() => resolve('timed out'), 2500);})]);
    assert.equal(outcome, 'owner_tool_failed');
  } finally {
    clearTimeout(timer);
    killFixture(pidPath);
    await work;
    rmSync(dir,{recursive:true,force:true});
  }
});

test('an owner tool ignoring SIGTERM is stopped at the deadline', async t => {
  if (process.platform === 'win32') return;
  const dir = mkdtempSync(join(tmpdir(), 'alashi-tool-timeout-')), pidPath = join(dir, 'pid');
  const code = `require('fs').writeFileSync(${JSON.stringify(pidPath)}, String(process.pid)); process.on('SIGTERM', () => {}); setInterval(() => {}, 1000)`;
  const registry = {owned:{argv:[process.execPath,'-e',code],no_model_calls:true}};
  const realSetTimeout = setTimeout, realClearTimeout = clearTimeout;
  t.mock.timers.enable({apis:['setTimeout']});
  const work = executeTool({tool:'owned',depends_on:[]},{brief:{}},{},registry).then(() => 'accepted', error => error.message);
  try {
    let pid;
    for (let i = 0; i < 100; i++) {
      try {
        const value = readFileSync(pidPath, 'utf8');
        if (/^[1-9][0-9]*$/.test(value)) { pid = Number(value); break; }
      } catch {}
      await new Promise(resolve => realSetTimeout(resolve, 20));
    }
    assert.ok(pid, 'tool process did not start');
    t.mock.timers.tick(60_000);
    let timer;
    try {
      assert.equal(await Promise.race([work, new Promise(resolve => {timer = realSetTimeout(() => resolve('timed out'), 2500);})]), 'owner_tool_failed');
    } finally { realClearTimeout(timer); }
  } finally {
    t.mock.timers.tick(60_000);
    killFixture(pidPath);
    await work;
    rmSync(dir,{recursive:true,force:true});
  }
});

test('oversized owner tool also stops a descendant holding its output pipe', async () => {
  if (process.platform === 'win32') return;
  const dir = mkdtempSync(join(tmpdir(), 'alashi-tool-group-'));
  const parentPath = join(dir, 'parent'), childPath = join(dir, 'child');
  const descendant = `require('fs').writeFileSync(${JSON.stringify(childPath)}, String(process.pid)); setInterval(() => {}, 1000)`;
  const code = `const fs=require('fs'); fs.writeFileSync(${JSON.stringify(parentPath)},String(process.pid)); require('child_process').spawn(process.execPath,['-e',${JSON.stringify(descendant)}],{stdio:['ignore','inherit','ignore']}); const timer=setInterval(()=>{if(fs.existsSync(${JSON.stringify(childPath)})){clearInterval(timer);process.stdout.write(Buffer.alloc(100000));}},10); setInterval(()=>{},1000)`;
  const registry = {owned:{argv:[process.execPath,'-e',code],no_model_calls:true}};
  const work = executeTool({tool:'owned',depends_on:[]},{brief:{}},{},registry).then(() => 'accepted', error => error.message);
  let timer;
  try {
    const outcome = await Promise.race([work, new Promise(resolve => {timer = setTimeout(() => resolve('timed out'), 4000);})]);
    assert.equal(outcome, 'owner_tool_failed');
  } finally {
    clearTimeout(timer);
    for (const path of [parentPath, childPath]) killFixture(path);
    await work;
    rmSync(dir,{recursive:true,force:true});
  }
});
