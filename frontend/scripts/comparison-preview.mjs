// Offline visual review only. Serves built assets and synthetic owner responses on loopback.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, extname, sep } from 'node:path';
import { randomBytes } from 'node:crypto';
import { comparisonFixture } from './comparison-fixture.mjs';
import { researchPreview } from './research-preview-fixture.mjs';
const fixture=comparisonFixture(), items=[fixture.comparison], versions=fixture.versions, root=resolve(import.meta.dirname,'../dist'), port=Number(process.env.ALASHI_PREVIEW_PORT??4178);
const research=researchPreview(fixture,items);
const json=(res,value,status=200)=>{res.writeHead(status,{'Content-Type':'application/json','Cache-Control':'no-store'});res.end(JSON.stringify(value))};
const hash=()=>randomBytes(32).toString('hex');
const server=createServer(async(req,res)=>{
  const path=new URL(req.url,'http://127.0.0.1').pathname, prefix=`/agents/${fixture.record}/owner`;
  let body;try{if(req.method==='POST'){let raw='';for await(const chunk of req){raw+=chunk;if(raw.length>50000)throw new Error('large')}body=JSON.parse(raw)}}catch{return json(res,{ok:false,error:'bad_comparison'},400)}
  if(path===prefix+'/browser/session')return json(res,{ok:true,wallet:fixture.owner,expires_at:Math.floor(Date.now()/1000)+3600});
  if(path==='/research-library')return json(res,research.public());
  if(path===prefix+'/research')return json(res,research.owned());
  if(path.startsWith(prefix+'/research/')&&req.method==='POST')return json(res,research.act(path.slice((prefix+'/research/').length),body));
  if(path==='/strategy-library')return json(res,{ok:true,versions:[],runs:[],challenges:[fixture.challenge],standings:[],server_now:Math.floor(Date.now()/1000)});
  if(path===prefix+'/strategies'&&req.method==='GET')return json(res,{ok:true,versions,runs:[],challenges:[fixture.challenge],standings:[],server_now:Math.floor(Date.now()/1000)});
  if(path===prefix+'/strategies'&&req.method==='POST'){const v={id:hash(),published:!!body.published,reuse_owners:0,content:{...body,owner:fixture.owner}};delete v.content.published;versions.push(v);return json(res,{ok:true,version:v})}
  if(path===prefix+'/comparisons'){
    if(req.method==='POST'){const a=versions.find(v=>v.id===body.baseline_version),b=versions.find(v=>v.id===body.candidate_version),task=body.changed_task;const c={id:hash(),content:{...body,owner:fixture.owner,record:fixture.record},actual_diff:{task,field:'instruction',before:a.content.plan.find(t=>t.id===task).instruction,after:b.content.plan.find(t=>t.id===task).instruction},attempts:[]};items.unshift(c);return json(res,{ok:true,comparison:c})}
    return json(res,{ok:true,comparisons:items});
  }
  const match=path.match(new RegExp(`^${prefix}/comparisons/([0-9a-f]{64})/attempts(?:/([0-9a-f]{64})/decision)?$`));
  if(match&&req.method==='POST'){const c=items.find(c=>c.id===match[1]);if(!c)return json(res,{ok:false,error:'comparison_forbidden'},404);
    if(match[2]){c.attempts[0].decision={...body,at:Math.floor(Date.now()/1000)};return json(res,{ok:true,comparison:c,attempt:c.attempts[0]})}
    if(!c.attempts.length){const a=structuredClone(fixture.comparison.attempts[0]);a.decision=null;a.id=hash();a.client_id=body.client_id;for(const[label,key]of[['baseline','baseline_version'],['candidate','candidate_version']]){a[label].id=hash();a[label].version=c.content[key];a[label].plan=versions.find(v=>v.id===c.content[key]).content.plan}c.attempts=[a]}return json(res,{ok:true,comparison:c,attempt:c.attempts[0]})
  }
  if(path.startsWith('/agents/')||path.startsWith('/strategy-'))return json(res,{ok:false,error:'unavailable'},404);
  const file=resolve(root,'.'+decodeURIComponent(path));if(file!==root&&!file.startsWith(root+sep))return json(res,{ok:false},403);
  try{const data=await readFile(extname(file)?file:resolve(root,'index.html'));const mime={'.js':'text/javascript','.css':'text/css','.png':'image/png','.webp':'image/webp','.svg':'image/svg+xml','.glb':'model/gltf-binary'};res.writeHead(200,{'Content-Type':mime[extname(file)]??'text/html'});res.end(data)}catch{res.writeHead(404);res.end('Not found')}
});
server.listen(port,'127.0.0.1',()=>console.log(`LOCAL SYNTHETIC PREVIEW: http://127.0.0.1:${port}/strategies?agent=${fixture.record}#strategy-owner`));
