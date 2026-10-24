// Exercise the real Vite proxy configuration against a loopback-only mock.
import assert from 'node:assert/strict';
import { createServer as createHttpServer } from 'node:http';
import { once } from 'node:events';
import { resolve } from 'node:path';
import { createServer as createViteServer } from 'vite';

let hits=0, vite;
const mock=createHttpServer((req,res)=>{hits++;res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify({ok:true,path:req.url,method:req.method}))});
try {
  mock.listen(0,'127.0.0.1');await once(mock,'listening');
  const target=`http://127.0.0.1:${mock.address().port}`;
  process.env.ALASHI_STRATEGY_API_TARGET=target;process.env.ALASHI_ARENA_API_TARGET=target;process.env.ALASHI_LIVE_API_TARGET=target;
  vite=await createViteServer({root:resolve(import.meta.dirname,'..'),configFile:resolve(import.meta.dirname,'../vite.config.ts'),server:{host:'127.0.0.1',port:0,strictPort:false},optimizeDeps:{noDiscovery:true,include:[]},logLevel:'error'});
  await vite.listen();const base=`http://127.0.0.1:${vite.httpServer.address().port}`, owner=`/agents/${'a'.repeat(64)}/owner`, comparison=`${owner}/comparisons`, attempt=`${comparison}/${'b'.repeat(64)}/attempts`, decision=`${attempt}/${'c'.repeat(64)}/decision`;
  for(const[method,path]of[['GET',comparison],['POST',comparison],['POST',attempt],['POST',decision],['POST',`${attempt}/${'c'.repeat(64)}/baseline/report`]]){
    const response=await fetch(base+'/strategy-api'+path,{method,headers:{'Content-Type':'application/json'},...(method==='POST'?{body:'{}'}:{})});
    assert.equal(response.status,200,`${method} ${path} must reach the local mock`);
    const result=await response.json();assert.equal(result.path,path,'only the proxy prefix is removed');assert.equal(result.method,method);
  }
  const allowedHits=hits;
  for(const[method,path]of[['DELETE',comparison],['PUT',comparison],['GET',attempt],['GET',decision],['POST',comparison+'/extra'],['POST',decision+'/extra'],['POST',comparison.replace('a'.repeat(64),'short')],['POST',attempt+'/'+ 'c'.repeat(64)+'/candidate/unknown']]){
    const response=await fetch(base+'/strategy-api'+path,{method});await response.text();assert.equal(response.status,403,`${method} ${path} must be blocked`);
  }
  assert.equal(hits,allowedHits,'blocked requests never reach the upstream');
  console.log('PASS: real Vite comparison GET/POST proxy routes and rewrites; wrong methods, malformed IDs and extra paths blocked before upstream');
} finally {await vite?.close();mock.close();mock.closeAllConnections()}
