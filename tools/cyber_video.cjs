#!/usr/bin/env node
const {chromium}=require(process.argv[2]||'playwright');
const {readFileSync,writeFileSync,mkdirSync}=require('node:fs');const {resolve}=require('node:path');const assert=require('node:assert/strict');
const output=process.argv[3]||'/Users/amir/Desktop/alashi-cyberpunk-2min.mp4';
(async()=>{const browser=await chromium.launch({executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true,args:['--autoplay-policy=no-user-gesture-required']});try{
 const page=await browser.newPage({viewport:{width:1920,height:1080},deviceScaleFactor:1}),errors=[];page.on('pageerror',e=>errors.push(e.message));const dir='/private/tmp/alashi-cyber-review';mkdirSync(dir,{recursive:true});
 if(process.argv.includes('--verify')){
  await page.setContent('<style>body{margin:0;background:#000}video{width:100vw;height:100vh}</style><video src="data:video/mp4;base64,'+readFileSync(output).toString('base64')+'"></video>',{waitUntil:'domcontentloaded'});
  await page.waitForFunction(()=>document.querySelector('video').readyState>=1,null,{timeout:20000});const metadata=await page.evaluate(()=>{const v=document.querySelector('video');return {duration:v.duration,width:v.videoWidth,height:v.videoHeight,audioBytes:v.webkitAudioDecodedByteCount}});
  assert.equal(metadata.width,1920);assert.equal(metadata.height,1080);assert.ok(Math.abs(metadata.duration-120)<1,JSON.stringify(metadata));
  for(const [name,t] of [['tx',30],['game',60],['payout',100]]){await page.evaluate(t=>document.querySelector('video').currentTime=t,t);await page.waitForFunction(t=>{const v=document.querySelector('video');return !v.seeking&&v.readyState>=2&&Math.abs(v.currentTime-t)<.5},t,{timeout:20000});await page.screenshot({path:dir+'/decoded-'+name+'.png'})}
  await page.evaluate(()=>document.querySelector('video').play());await page.waitForTimeout(1200);metadata.audioBytes=await page.evaluate(()=>document.querySelector('video').webkitAudioDecodedByteCount);assert.ok(metadata.audioBytes>0,'Audio must decode');console.log(JSON.stringify({verified:true,...metadata}));return;
 }
 await page.goto('file://'+resolve(__dirname,'../app/alashi-cyber-demo.html'));await page.evaluate(()=>document.fonts.ready);
 const proof=JSON.parse(readFileSync(resolve(__dirname,'../docs/ops/CYBER_LOCAL_PROOF_20260906.json')));
 const embedded=await page.evaluate(()=>Cyber.proof);
 assert.deepEqual(embedded.transactions,proof.transactions.filter(t=>['join','settle'].includes(t.instruction)));
 assert.ok(embedded.transactions.every(t=>t.receipt.meta.err===null));assert.equal(embedded.transactions[1].receipt.transaction.message.header.numRequiredSignatures,1);
 const names=await page.evaluate(()=>[...new Set(Cyber.picks.map(s=>archive.agents[s.a.actor].name))]);assert.equal(names.length,6);
 for(const [name,t] of [['hook',5],['solana',17],['join',28],['game',57],['payout',100]]){await page.evaluate(t=>Cyber.render(t),t);await page.screenshot({path:dir+'/'+name+'.png'})}
 if(process.argv.includes('--preview')){assert.deepEqual(errors,[]);console.log(JSON.stringify({preview:true,names,review:dir}));return}
 const audio=await page.evaluate(async()=>{window.music=await Cyber.prepareAudio();return cyberAudioStats});assert.ok(audio.peak<.95&&audio.rms>.005);
 await page.evaluate(async()=>{window.audioContext=new AudioContext({sampleRate:44100});await audioContext.resume();const dest=audioContext.createMediaStreamDestination(),src=audioContext.createBufferSource();src.buffer=music;src.connect(dest);const stream=document.getElementById('screen').captureStream(24);for(const track of dest.stream.getAudioTracks())stream.addTrack(track);window.chunks=[];window.rec=new MediaRecorder(stream,{mimeType:'video/mp4;codecs=avc1.42001E,mp4a.40.2',videoBitsPerSecond:5000000,audioBitsPerSecond:128000});rec.ondataavailable=e=>{if(e.data.size)chunks.push(e.data)};window.recordDone=false;rec.start(1000);const start=audioContext.currentTime;src.start(start);function render(){const t=audioContext.currentTime-start;Cyber.render(Math.min(120,t));window.recordTime=t;if(t>=120){window.recordDone=true;return}requestAnimationFrame(render)}render()});
 let last=-1;while(true){await page.waitForTimeout(1000);const s=await page.evaluate(()=>({done:recordDone,t:recordTime}));if(Math.floor(s.t/20)!==last){last=Math.floor(s.t/20);console.log('Recorded '+Math.floor(s.t)+' / 120 seconds')}if(s.done)break}
 const b64=await page.evaluate(()=>new Promise(resolve=>{rec.onstop=()=>{const reader=new FileReader();reader.onload=()=>resolve(reader.result.split(';base64,')[1]);reader.readAsDataURL(new Blob(chunks,{type:rec.mimeType}))};rec.stop()}));const bytes=Buffer.from(b64,'base64');assert.ok(bytes.length>1000000&&bytes.subarray(0,64).includes(Buffer.from('ftyp')),'MP4 header and payload must exist');writeFileSync(output,bytes);assert.deepEqual(errors,[]);console.log(JSON.stringify({output,bytes:Buffer.byteLength(b64,'base64'),audio,errors}));
 }finally{await browser.close()}})().catch(e=>{console.error('[ERROR]',e);process.exitCode=1});
