#!/usr/bin/env node
const {chromium}=require(process.argv[2]||'playwright');
const {readFileSync,mkdtempSync}=require('node:fs');
const {resolve,join}=require('node:path');
const {tmpdir}=require('node:os');
const assert=require('node:assert/strict');
(async()=>{
 const html=readFileSync(resolve(__dirname,'../app/arena.html'),'utf8'),dir=mkdtempSync(join(tmpdir(),'alashi-board-'));
 const browser=await chromium.launch({executablePath:'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true});
 try{
  if(process.argv[3]==='--live'){
   const url=process.argv[4],pages=await Promise.all([browser.newPage({viewport:{width:1512,height:1050}}),browser.newPage({viewport:{width:1280,height:900}})]),errors=[];
   pages.forEach(p=>p.on('pageerror',e=>errors.push(e.message)));
   await Promise.all(pages.map(p=>p.goto(url)));
   await Promise.all(pages.map(p=>p.waitForFunction(()=>window.AlashiBoard?.snapshot().state?.factions.length>=3,{},{timeout:20000})));
   let equal=false,left,right;
   for(let i=0;i<8;i++){
    await Promise.all(pages.map(p=>p.evaluate(()=>AlashiBoard.refresh())));
    [left,right]=await Promise.all(pages.map(p=>p.evaluate(()=>{const s=AlashiBoard.snapshot();return {gameId:s.gameId,round:s.state.round,phase:s.state.phase,factions:s.state.factions.map(f=>({idx:f.idx,cash:f.cash,goods:f.goods,influence:f.influence}))}})));
    if(JSON.stringify(left)===JSON.stringify(right)){equal=true;break}
   }
   assert.equal(equal,true,'two independent spectators must converge on the same arena state');
   await pages[0].screenshot({path:join(dir,'live-board.png'),fullPage:true});
   assert.deepEqual(errors,[]);console.log(JSON.stringify({ok:true,sharedState:left,artifacts:dir,errors}));return;
  }
  const page=await browser.newPage({viewport:{width:1512,height:1050},reducedMotion:'reduce'}),errors=[];page.on('pageerror',e=>errors.push(e.message));page.setDefaultTimeout(8000);
  const names=['Север','Юг','Восток','Запад','Центр','Порт'];let fail=false,complete=false;
  const s={game_id:99,party_no:42,round:2,phase:'market',epoch:'classic',entry_fee:1e7,now:200,phase_ends_at:240,grace_until:245,sold_counter:0,price_now:12,price_table:[12,10,9,8,7,6,5,4,3,3,2,2,2,1,1,1],tax_bps:1000,price_shift:0,boom:0,laws_passed:1,president_idx:2,law_card_name:null,veto_pending:false,factions:names.map((name,idx)=>({idx,name,cash:(22-idx)*1e6,goods:idx+2,influence:idx===2?3:1,alive:true,acted:false,voted:false})),recent_actions:[]};
  const result={game_id:99,party_no:42,entry_fee:1e7,bank:6e7,rake:3e6,agents:names.map(name=>({name})),ranks:[2,0,1,3,4,5],final_cash:[20e6,19e6,30e6,10e6,8e6,7e6],final_goods:[0,1,2,0,0,0],final_influence:[1,1,3,1,1,1],payouts:[17.1e6,8.55e6,28.5e6,2.85e6,0,0]};
  await page.route('http://board.test/**',async route=>{const url=new URL(route.request().url());if(url.pathname==='/ui')return route.fulfill({contentType:'text/html',body:html});if(fail)return route.fulfill({status:503,body:'offline'});const data=url.pathname==='/games'?{ok:true,games:complete?[]:[{game_id:99,party_no:42,phase:s.phase,round:s.round,factions:6}]}:url.pathname==='/export'?[]:complete?{ok:true,finished:true,result}:{ok:true,state:s};await route.fulfill({contentType:'application/json',body:JSON.stringify(data)})});
  await page.goto('http://board.test/ui?game=99');await page.waitForFunction(()=>window.AlashiBoard?.snapshot().state?.factions.length===6);
  assert.equal(await page.locator('#price').innerText(),'12M');assert.equal(await page.locator('#bank').innerText(),'60M');assert.equal(await page.locator('.faction').count(),6);
  await page.screenshot({path:join(dir,'six-factions.png'),fullPage:true});
  await page.locator('[data-faction="2"]').click();assert.equal(await page.locator('#selected-name').innerText(),'Восток');assert.equal(await page.locator('#selected-badge').innerText(),'Президент');
  await page.locator('#zoom-in').click();assert.ok(await page.evaluate(()=>AlashiBoard.snapshot().zoom>1));await page.locator('#fit').click();assert.equal(await page.evaluate(()=>AlashiBoard.snapshot().zoom),1);
  s.sold_counter=2;s.price_now=9;s.factions[0].cash+=19.8e6;s.factions[0].goods=0;s.factions[0].acted=true;
  s.recent_actions=[{ts:201,round:2,phase:'market',actor:0,action:'sell',by:'llm',ok:true}];
  await page.evaluate(()=>AlashiBoard.refresh());assert.equal(await page.locator('#price').innerText(),'9M');assert.equal(await page.locator('.event').count(),1);await page.evaluate(()=>AlashiBoard.refresh());assert.equal(await page.locator('.event').count(),1);
  s.phase='law';s.law_card_name='tax_20';s.factions[0].voted=true;s.yes_influence=1;s.no_influence=0;
  await page.evaluate(()=>AlashiBoard.refresh());assert.equal(await page.locator('#law-name').innerText(),'Налог 20%');assert.match(await page.locator('#vote-status').innerText(),/1 \/ 6/);assert.doesNotMatch(await page.locator('#vote-status').innerText(),/вес.*за/);
  await page.locator('#join').click();assert.match(await page.locator('#join-info').innerText(),/game\/99\/join/);assert.match(await page.locator('#copy-status').innerText(),/уже началась/);await page.locator('[data-close]').click();
  fail=true;await page.evaluate(()=>AlashiBoard.refresh());assert.equal(await page.locator('#notice').isVisible(),true);assert.equal(await page.locator('.faction').count(),6);fail=false;await page.evaluate(()=>AlashiBoard.refresh());assert.equal(await page.locator('#notice').isVisible(),false);
  await page.setViewportSize({width:390,height:844});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);await page.screenshot({path:join(dir,'mobile.png'),fullPage:true});await page.setViewportSize({width:1512,height:1050});
  // Treat names as text, including in result and event records.
  s.factions[0].name='<img src=x onerror=alert(1)>';await page.evaluate(()=>AlashiBoard.refresh());assert.equal(await page.locator('img').count(),0);
  complete=true;await page.evaluate(()=>AlashiBoard.refresh());assert.equal(await page.locator('#result').isVisible(),true);assert.match(await page.locator('#result h2').innerText(),/Восток.*58,5M/);assert.equal(await page.locator('#result tbody tr').count(),6);assert.equal(await page.evaluate(()=>AlashiBoard.snapshot().finished),true);await page.screenshot({path:join(dir,'settlement.png'),fullPage:true});
  // A new spectator must see the same final resources, not empty placeholder statistics.
  await page.reload();await page.locator('#result').waitFor({state:'visible'});assert.equal(await page.evaluate(()=>AlashiBoard.snapshot().state.factions.find(f=>f.idx===2).influence),3);
  assert.deepEqual(errors,[]);console.log(JSON.stringify({ok:true,checks:13,artifacts:dir,errors}));
 }finally{await browser.close()}
})().catch(e=>{console.error('[ERROR]',e);process.exitCode=1});
