// Signed-owner HTTP duel against a local deterministic provider. No external calls.
import assert from 'node:assert/strict';
import { createPrivateKey,createPublicKey,randomUUID } from 'node:crypto';
import { createServer } from 'node:http';
import { join } from 'node:path';
import { writeFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import { base58,ownerApi,modelProvider,runCycle } from './strategy-run.mjs';
const [base,directory]=process.argv.slice(2);let active=0,peak=0,requests=0;const intervals=[];
const gate=()=>{let resolve;const promise=new Promise(r=>{resolve=r;});return{promise,resolve};};
const roundGate=()=>({angle:gate(),critic:gate()});let dependencyGate=roundGate();
async function bounded(promise){let timer;try{await Promise.race([promise,new Promise(resolve=>{timer=setTimeout(resolve,10000);})]);}finally{clearTimeout(timer);}}
const ad={headline:'Put your agents to work with ALASHI',body:'Bring your agents and tools to the same challenge as another owner. Track model tokens as your team works. Build on a public strategy, improve the plan, and share the next version so another owner can use it.',cta:'Explore ALASHI'};
const model=createServer(async(req,res)=>{let body='';for await(const b of req)body+=b;const data=JSON.parse(JSON.parse(body).messages[1].content);active++;peak=Math.max(peak,active);requests++;const interval={task:data.task,start:performance.now()};intervals.push(interval);
 if(data.task==='angle'){dependencyGate.angle.resolve();await bounded(dependencyGate.critic.promise);}
 if(data.task==='audience')await bounded(dependencyGate.angle.promise);
 if(data.task==='critic')dependencyGate.critic.resolve();
 await new Promise(resolve=>setTimeout(resolve,120));interval.end=performance.now();active--;const report=['deliver','draft'].includes(data.task)?ad:{facts:data.brief.facts,angle:'Agent owners can reuse a useful work plan.'};res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({id:randomUUID(),usage:{prompt_tokens:100,completion_tokens:20,total_tokens:120},choices:[{message:{content:'```json\n'+JSON.stringify(report)+'\n```'}}]}));});
await new Promise(resolve=>model.listen(0,'127.0.0.1',resolve));
try {
 const provider=modelProvider({url:`http://127.0.0.1:${model.address().port}/chat/completions`,model:'deterministic-test',test:true});
 const profiles=[1,2,3,4].map((byte,i)=>{const seed=Buffer.alloc(32,byte),key=createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b657004220420','hex'),seed]),format:'der',type:'pkcs8'}),pub=createPublicKey(key).export({format:'der',type:'spki'}).subarray(-32);return{wallet:base58(pub),secret_key:[...seed,...pub],agent_record_id:['a','b','e','f'][i].repeat(64)};});
 const [host,a,b,next]=await Promise.all(profiles.map(p=>ownerApi(base,p)));
 const brief={round_seconds:180,title:'Write an ALASHI ad',audience:'Owners of AI agents',task:'Create an English advertisement that attracts AI agent owners to ALASHI.',facts:['Owners compete on the same task with their own agents and tools.','Crystals represent model tokens.','Public strategies can be reused and improved; derivatives are shared.'],required_terms:['ALASHI','agents','tokens'],cta:'Explore ALASHI',max_words:80,budget:20000};
 const modelTask=(id,depends_on,instruction)=>({id,kind:'model',depends_on,instruction,tool:null});
 const toolTask=(id,depends_on,tool)=>({id,kind:'tool',depends_on,instruction:'Check the shared brief and ad.',tool});
 const baselinePlan=[modelTask('audience',[],'Study the audience.'),modelTask('angle',[],'Study the advertising angle.'),modelTask('critic',['audience'],'Check the audience report.'),modelTask('deliver',['angle','critic'],'Write the final ad.')];
 const sharedPlan=[toolTask('brief',[],'brief_extract'),modelTask('draft',['brief'],'Write an ad as headline,body,cta.'),toolTask('check',['draft'],'copy_check'),modelTask('deliver',['check','draft'],'Use the checked draft to produce the final ad.')];
 const root=(await a.post('/strategies',{title:'Audience review with an independent angle',plan:baselinePlan,change:'',published:true})).version;
 const draft=(await b.post('/strategies',{parent:root.id,title:'Reusable brief and local copy check',plan:sharedPlan,change:'Extract supplied facts locally; reuse one draft and run a local copy check.',published:false})).version;
 const duel=(await host.post('/strategy-challenges',{...brief,match_key:'first-duel'})).challenge;
 await assert.rejects(b.post('/strategy-runs',{version:draft.id,challenge_id:duel.id,client_id:'b-private-01'}),/publish_derivative_first/);
 await b.post(`/strategies/${draft.id}/publish`,{});
 const ra=(await a.post('/strategy-runs',{version:root.id,challenge_id:duel.id,client_id:'a-first-duel'})).run;
 await assert.rejects(a.post(`/strategy-runs/${ra.id}/reserve`,{task:'audience',max_total_tokens:5000,source:'test_provider',provider:provider.provider,model:provider.model}),/waiting_for_opponent/);
 const rb=(await b.post('/strategy-runs',{version:draft.id,challenge_id:duel.id,client_id:'b-first-duel'})).run;
 assert.equal(ra.seat,0);assert.equal(rb.seat,1);
 let lost=false;const interrupted={...a,post:async(path,body)=>{if(path.endsWith('/report')&&body.task==='critic'&&!lost){lost=true;throw new Error('simulated_connection_loss');}return a.post(path,body);}};
 const results=await Promise.allSettled([runCycle({api:interrupted,id:ra.id,provider,journalPath:join(directory,'a.json')}),runCycle({api:b,id:rb.id,provider,journalPath:join(directory,'b.json')})]);
 assert.equal(results[0].status,'rejected');assert.equal(results[1].status,'fulfilled');
 let pub=await host.request('/strategy-library');assert.ok(pub.runs.every(r=>r.result===null),'opponent output must remain sealed');assert.ok(pub.runs.every(r=>Object.values(r.calls).every(c=>!('report'in c))));
 const before=requests;const doneA=await runCycle({api:a,id:ra.id,provider,journalPath:join(directory,'a.json')});assert.equal(requests-before,1,'resume must not repeat paid research calls');assert.equal(doneA.known_tokens,480);
 pub=await host.request('/strategy-library');assert.ok(pub.runs.every(r=>r.result?.checks.passed));assert.equal(pub.standings[0].groups[1].token_leaders.length,0,'format checks alone cannot crown a winner');
 await assert.rejects(a.post(`/strategy-runs/${ra.id}/review`,{accepted:true,reason:'self'}),/reviewer_forbidden/);
 for(const id of [ra.id,rb.id])await host.post(`/strategy-runs/${id}/review`,{accepted:true,reason:'Deterministic fixture acceptance, not a real creative review.'});
 pub=await host.request('/strategy-library');assert.deepEqual(pub.standings[0].groups[1].token_leaders,[rb.id]);assert.equal(pub.standings[0].monetary_winner,null);
 const rematch=(await host.post('/strategy-challenges',{...brief,match_key:'second-duel'})).challenge;
 const reuse=(await next.post('/strategy-runs',{version:draft.id,challenge_id:rematch.id,client_id:'next-owner-duel'})).run;
 const rival=(await a.post('/strategy-runs',{version:root.id,challenge_id:rematch.id,client_id:'a-second-duel'})).run;
 dependencyGate=roundGate();
 const adopted=await Promise.all([runCycle({api:next,id:reuse.id,provider,journalPath:join(directory,'next.json')}),runCycle({api:a,id:rival.id,provider,journalPath:join(directory,'rival.json')})]);
 assert.equal(adopted[0].known_tokens,240);assert.equal(adopted[0].test_provider,true);assert.ok(peak>=2);assert.equal(requests,12);
 const critics=intervals.filter(v=>v.task==='critic'),angles=intervals.filter(v=>v.task==='angle');
 assert.equal(critics.length,2);assert.ok(critics.every((v,i)=>v.start<angles[i].end),'accepted audience report must unlock critic before the unrelated angle finishes: '+JSON.stringify(intervals));
 pub=await host.request('/strategy-library');assert.equal(pub.versions.find(v=>v.id===draft.id).reuse_owners,1);
 const summary={kind:'marketing_duel',source:'deterministic_test_provider_not_real_llm',real_provider_calls:0,requests,parallel_peak:peak,dependency_continuations_overlap:true,intervals,blind_reveal:true,resume_without_duplicate_calls:true,duel:duel.id,rematch:rematch.id,baseline:ra.id,improvement:rb.id,adoption:reuse.id,fixture_tokens:[480,240,240],monetary_savings:null,conversion_efficacy:null};writeFileSync(join(directory,'http-result.json'),JSON.stringify(summary,null,2));console.log(JSON.stringify(summary));
} finally {await new Promise(resolve=>model.close(resolve));}
