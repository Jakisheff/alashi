#!/usr/bin/env node
// Replay an exported classic party. Resource values come from cash_after/goods_after.
const {readFileSync,writeFileSync}=require('node:fs');
const {resolve}=require('node:path');
const {createHash}=require('node:crypto');
const assert=require('node:assert/strict');
const {chromium}=require(process.argv[2]||'playwright');
const source=process.argv[3],destination=process.argv[4];
if(!source||!destination)throw Error('Usage: node tools/board_video.cjs PLAYWRIGHT EXPORT OUTPUT.mp4');
const result=JSON.parse(readFileSync(source)),html=readFileSync(resolve(__dirname,'../app/arena.html'),'utf8');
const prices=[12,10,9,8,7,6,5,4,3,3,2,2,2,1,1,1];
const wallets=result.agents.map(a=>{const id=Buffer.alloc(8);id.writeBigUInt64LE(BigInt(result.game_id));return createHash('sha256').update(a.agent_id).update(id).digest()});
const state={game_id:result.game_id,party_no:result.party_no,round:1,phase:'market',epoch:'classic',entry_fee:result.entry_fee,now:0,phase_ends_at:0,grace_until:0,sold_counter:0,price_now:12,price_table:prices,tax_bps:0,price_shift:0,boom:0,laws_passed:0,president_idx:null,law_card_name:null,veto_pending:false,factions:result.agents.map((a,idx)=>({idx,name:a.name,cash:0,goods:0,influence:1,alive:true,acted:false,voted:false})),recent_actions:[]};
const money=n=>(n/1e6).toLocaleString('ru-RU',{maximumFractionDigits:3})+'M';
(async()=>{
 const browser=await chromium.launch({executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true});
 try{
 if(process.argv.includes('--verify')){
  const verify=await browser.newPage({viewport:{width:1600,height:1000}});
  await verify.route('http://video.test/**',route=>route.fulfill({contentType:'video/mp4',body:readFileSync(destination)}));
  await verify.setContent('<video controls style="width:100%;height:100%" src="data:video/mp4;base64,'+readFileSync(destination).toString('base64')+'"></video>',{waitUntil:'domcontentloaded'});
  await verify.waitForFunction(()=>document.querySelector('video').readyState>=1,null,{timeout:15000});
  const metadata=await verify.evaluate(()=>{const v=document.querySelector('video');return {duration:v.duration,width:v.videoWidth,height:v.videoHeight}});
  console.log(JSON.stringify(metadata));
  assert.equal(metadata.width,1600);assert.equal(metadata.height,1000);assert.ok(metadata.duration>70&&metadata.duration<300);
  for(const [name,seconds] of [['opening',3],['middle',metadata.duration/2],['ending',metadata.duration-2]]){
   await verify.evaluate(t=>{document.querySelector('video').currentTime=t},seconds);
   await verify.waitForFunction(t=>{const v=document.querySelector('video');return !v.seeking&&v.readyState>=2&&Math.abs(v.currentTime-t)<0.5},seconds,{timeout:15000});
   await verify.screenshot({path:destination.replace(/\.mp4$/,'.'+name+'.png')});
  }
  console.log(JSON.stringify({verified:true,...metadata}));return;
 }
 const page=await browser.newPage({viewport:{width:1600,height:1000},deviceScaleFactor:1});
 const recorder=await browser.newPage({viewport:{width:1600,height:1000}});let complete=false;const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.route('http://replay.test/**',async route=>{const url=new URL(route.request().url());const data=url.pathname==='/games'?{ok:true,games:[{game_id:result.game_id,party_no:result.party_no,phase:state.phase,round:state.round,factions:state.factions.length}]}:url.pathname==='/export'?[]:complete?{ok:true,finished:true,result}:{ok:true,state};await route.fulfill({contentType:url.pathname==='/ui'?'text/html':'application/json',body:url.pathname==='/ui'?html:JSON.stringify(data)})});
 await page.goto('http://replay.test/ui?game='+result.game_id);await page.waitForFunction(()=>window.AlashiBoard?.snapshot().state);
 await page.addStyleTag({content:'.top-right,.clock,.map-tools{visibility:hidden}#result:not([hidden]){position:fixed;z-index:900;left:180px;right:180px;top:260px;padding:38px;background:#172a23fa;box-shadow:0 20px 120px #000b;border:1px solid #e5bf7388}#result h2{font-size:30px}#result td,#result th{font-size:19px;padding:16px}.shell{padding-bottom:76px}.board-wrap{height:560px;min-height:560px}.map-legend{display:none}#replay-caption{position:fixed;z-index:1000;bottom:0;left:0;right:0;min-height:66px;padding:16px 28px;background:#101e19f5;border-top:1px solid #e5bf7366;color:#eeeedd;font:500 21px system-ui}#replay-label{position:fixed;z-index:1001;top:25px;right:28px;color:#e5bf73;font:600 14px system-ui;letter-spacing:1px}'});
 await page.evaluate(()=>{for(const [id,text] of [['replay-caption',''],['replay-label','ПОВТОР ПО ЖУРНАЛУ · ПАУЗЫ СОКРАЩЕНЫ']]){const el=document.createElement('div');el.id=id;el.textContent=text;document.body.append(el)}});
 await recorder.setContent('<canvas width="1600" height="1000"></canvas>');
 const mime=await recorder.evaluate(()=>{
 const mime=['video/mp4;codecs=avc1.42001E','video/mp4','video/webm;codecs=vp9'].find(x=>MediaRecorder.isTypeSupported(x));
 const canvas=document.querySelector('canvas');window.ctx=canvas.getContext('2d');window.parts=[];window.rec=new MediaRecorder(canvas.captureStream(12),{mimeType:mime,videoBitsPerSecond:4500000});rec.ondataavailable=e=>{if(e.data.size)parts.push(e.data)};rec.start(1000);return mime;
 });
 if(!mime.startsWith('video/mp4'))throw Error('MP4 encoder unavailable: '+mime);
 async function show(caption,seconds=2,actor=null){
  await page.evaluate(()=>AlashiBoard.refresh());
  if(actor!==null)await page.locator('[data-faction="'+actor+'"]').click();
  await page.evaluate(t=>document.getElementById('replay-caption').textContent=t,caption);
  const until=Date.now()+seconds*1000;
  while(Date.now()<until){const frame=(await page.screenshot({type:'jpeg',quality:85})).toString('base64');await recorder.evaluate(async b=>{const img=new Image();img.src='data:image/jpeg;base64,'+b;await img.decode();ctx.drawImage(img,0,0)},frame);await new Promise(r=>setTimeout(r,100))}
 }
 await show('ALASHI · Партия №'+result.party_no+' · Три LLM-фракции · Банк '+money(result.bank),4);
 let count=0;
 for(let round=1;round<=6;round++){
  state.round=round;state.sold_counter=0;state.boom=round===3?2:0;
  for(const phase of ['market','action','law']){
   state.phase=phase;state.law_card_name=null;state.factions.forEach(f=>{f.acted=false;f.voted=false});
   if(phase==='action')state.president_idx=state.factions.slice().sort((a,b)=>b.influence-a.influence||Buffer.compare(wallets[a.idx],wallets[b.idx]))[0].idx;
   const law=result.phases.find(p=>p.round===round&&p.phase==='law');
   if(phase==='law')state.law_card_name=law.card_name;
   state.price_now=Math.max(1,prices[Math.min(15,state.sold_counter)]+state.boom);
   const actions=result.actions.filter(a=>a.round===round&&a.phase===phase);
   await show('Раунд '+round+' / 6 · '+({market:'Рынок',action:'Действие',law:'Закон'})[phase]+(actions.length?'':' · Принятых ходов нет'),1.5);
   for(const a of actions){
    assert.equal(a.ok,true,'This recorder expects accepted actions');const f=state.factions[a.actor],before=f.cash;
    if(a.cash_after!==null)f.cash=a.cash_after;if(a.goods_after!==null)f.goods=a.goods_after;
    let caption=f.name+' · ';
    if(a.action==='sell'){state.sold_counter+=a.params.units;caption+='Продажа '+a.params.units+' товара · +'+money(f.cash-before)+' · Касса '+money(f.cash)}
    else if(a.action==='buy'){state.sold_counter=Math.max(0,state.sold_counter-a.params.units);caption+='Покупка '+a.params.units+' товара · −'+money(before-f.cash)}
    else if(a.action==='produce')caption+='Производство · Товар: '+f.goods;
    else if(a.action==='vote'){f.voted=true;caption+='Голос подан · Решение будет раскрыто с итогом'}
    else throw Error('Unsupported replay action: '+a.action);
    if(phase!=='law')f.acted=true;
    state.price_now=Math.max(1,prices[Math.min(15,state.sold_counter)]+state.boom);
    state.recent_actions.push(a);state.recent_actions=state.recent_actions.slice(-12);count++;
    await show(caption,a.action==='vote'?1.5:2.6,a.actor);
   }
   if(phase==='law'){
    state.laws_passed=law.laws_passed_total;
    if(law.passed&&law.card_name==='tax_10')state.tax_bps=1000;
    if(law.passed&&law.card_name==='tax_20')state.tax_bps=2000;
    if(law.passed&&law.card_name==='subsidy_poor')state.factions.slice().sort((a,b)=>a.cash-b.cash||a.idx-b.idx)[0].influence++;
    const title=await page.locator('#law-name').innerText();
    await show(title+' · '+(law.passed?'Принят':'Отклонён')+' · За '+law.yes+', против '+law.no+(law.veto_pending?' · Вето':''),2.6);
   }
  }
  console.log('Rendered round '+round+'/6; actions '+count);
 }
 assert.deepEqual(state.factions.map(f=>f.cash),result.final_cash);assert.deepEqual(state.factions.map(f=>f.goods),result.final_goods);assert.deepEqual(state.factions.map(f=>f.influence),result.final_influence);assert.equal(count,result.actions.length);
 complete=true;await show('Финал · Банк '+money(result.bank)+' = выплаты '+money(result.bank-result.rake)+' + рейк '+money(result.rake),7);
 await page.screenshot({path:destination.replace(/\.mp4$/,'.png')});
 const encoded=await recorder.evaluate(()=>new Promise(resolve=>{rec.onstop=()=>{const reader=new FileReader();reader.onload=()=>resolve(reader.result.split(',')[1]);reader.readAsDataURL(new Blob(parts,{type:rec.mimeType}))};rec.stop()}));
 writeFileSync(destination,Buffer.from(encoded,'base64'));assert.deepEqual(errors,[]);
 console.log(JSON.stringify({output:destination,bytes:Buffer.byteLength(encoded,'base64'),actions:count,finalResourcesVerified:true,mime,errors}));
 }finally{await browser.close()}
})().catch(e=>{console.error('[ERROR]',e);process.exitCode=1});
