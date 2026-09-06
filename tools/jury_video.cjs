#!/usr/bin/env node
const {chromium}=require(process.argv[2]||'playwright');
const {resolve}=require('node:path');
const {writeFileSync,readFileSync,mkdirSync}=require('node:fs');
const assert=require('node:assert/strict');
const output=process.argv[3]||'/Users/amir/Desktop/alashi-jury-party21.mp4';
(async()=>{
 const browser=await chromium.launch({executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true});
 try{
 const page=await browser.newPage({viewport:{width:1920,height:1152},deviceScaleFactor:1}),errors=[];page.on('pageerror',e=>errors.push(e.message));
 if(process.argv.includes('--verify')){
  await page.setContent('<video controls style="width:100%;height:100%" src="data:video/mp4;base64,'+readFileSync(output).toString('base64')+'"></video>',{waitUntil:'domcontentloaded'});
  await page.waitForFunction(()=>document.querySelector('video').readyState>=1,null,{timeout:15000});
  const metadata=await page.evaluate(()=>{const v=document.querySelector('video');return {duration:v.duration,width:v.videoWidth,height:v.videoHeight}});
  assert.equal(metadata.width,1920);assert.equal(metadata.height,1080);assert.ok(metadata.duration>360&&metadata.duration<420);
  for(const [name,t] of [['opening',12],['middle',metadata.duration/2],['final',metadata.duration-4]]){
   await page.evaluate(t=>document.querySelector('video').currentTime=t,t);
   await page.waitForFunction(t=>{const v=document.querySelector('video');return !v.seeking&&v.readyState>=2&&Math.abs(v.currentTime-t)<.5},t,{timeout:15000});
   await page.screenshot({path:'/private/tmp/alashi-jury-review/decoded-'+name+'.png'});
  }
  console.log(JSON.stringify({verified:true,...metadata}));return;
 }
 await page.goto('file://'+resolve(__dirname,'../app/alashi-party21-replay.html'));
 await page.evaluate(()=>Director.pause());
 const summary=await page.evaluate(()=>({names:Director.archive.agents.map(a=>a.name),moves:Director.scenes.filter(s=>s.kind==='move').length,total:Director.total,accepted:Director.archive.actions.filter(a=>a.ok).length}));
 assert.deepEqual(summary.names.slice().sort(),['Aitore','Aikorkem','Aisultan','Botagul','Aibot','Zhambyl'].sort());assert.equal(summary.moves,101);assert.equal(summary.accepted,97);
 const fixture=JSON.parse(readFileSync(resolve(__dirname,'../data/live/party21_90s_export.json')));
 assert.deepEqual(await page.evaluate(()=>Director.archive),fixture,'Replay must embed the original export unchanged');
 assert.deepEqual(await page.evaluate(()=>Director.scenes.filter(s=>s.kind==='move').map(s=>s.a)),fixture.actions,'Every attempt must appear once in original order');
 const review='/private/tmp/alashi-jury-review';mkdirSync(review,{recursive:true});
 for(const [name,criteria] of [['sale','sell'],['vote','vote'],['rejected','rejected'],['final','final']]){
  await page.evaluate(key=>{const i=Director.scenes.findIndex(s=>key==='final'?s.kind==='final':key==='rejected'?s.a?.ok===false:s.a?.action===key);Director.go(i)},criteria);await page.waitForTimeout(100);await page.locator('#screen').screenshot({path:review+'/'+name+'.png'});
 }
 await page.setViewportSize({width:960,height:620});await page.evaluate(()=>Director.go(Director.scenes.findIndex(s=>s.a?.action==='sell')));await page.waitForTimeout(100);await page.locator('#screen').screenshot({path:review+'/projector-half-size.png'});
 if(process.argv.includes('--preview')){assert.deepEqual(errors,[]);console.log(JSON.stringify({preview:true,...summary,review}));return}
 await page.setViewportSize({width:1920,height:1152});
 await page.evaluate(()=>{
  Director.start();const canvas=document.getElementById('screen');window.parts=[];window.recorder=new MediaRecorder(canvas.captureStream(24),{mimeType:'video/mp4;codecs=avc1.42001E',videoBitsPerSecond:6000000});recorder.ondataavailable=e=>{if(e.data.size)parts.push(e.data)};recorder.start(1000);
 });
 let last=-1;
 while(true){await page.waitForTimeout(1000);const s=await page.evaluate(()=>Director.snapshot());if(Math.floor(s.index/12)!==last){last=Math.floor(s.index/12);console.log('Recorded scene '+(s.index+1)+' / 109')}if(s.done)break}
 const b64=await page.evaluate(()=>new Promise(resolve=>{recorder.onstop=()=>{const reader=new FileReader();reader.onload=()=>resolve(reader.result.split(',')[1]);reader.readAsDataURL(new Blob(parts,{type:recorder.mimeType}))};recorder.stop()}));
 writeFileSync(output,Buffer.from(b64,'base64'));assert.deepEqual(errors,[]);console.log(JSON.stringify({output,bytes:Buffer.byteLength(b64,'base64'),...summary,errors}));
 }finally{await browser.close()}
})().catch(e=>{console.error('[ERROR]',e);process.exitCode=1});
