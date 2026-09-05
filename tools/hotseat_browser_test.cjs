#!/usr/bin/env node
// Usage: node tools/hotseat_browser_test.cjs /path/to/playwright
const {chromium}=require(process.argv[2]||'playwright');
const assert=require('node:assert/strict');
const {resolve,join}=require('node:path');
const {pathToFileURL}=require('node:url');
const {mkdtempSync}=require('node:fs');
const {tmpdir}=require('node:os');
(async()=>{
 const artifacts=mkdtempSync(join(tmpdir(),'alashi-classic-'));
 const browser=await chromium.launch({executablePath:process.env.CHROME_PATH||'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true});
 try {
  const context=await browser.newContext({viewport:{width:1440,height:1000},reducedMotion:'reduce',offline:true});
  const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));page.setDefaultTimeout(7000);
  const url=pathToFileURL(resolve(__dirname,'../app/alashi-hotseat.html')).href;
  for(const n of [3,6]) {
   await page.goto(url);await page.locator('#player-count').selectOption(String(n));
   assert.equal(await page.locator('.player-input').count(),n);
   assert.ok((await page.locator('.mini-facts').innerText()).includes(n*10+'M'));
   if(n===3) {
    await page.locator('#name-0').fill('СЕВЕР');await page.locator('#name-1').fill('север');
    await page.locator('#setup-form button[type=submit]').click();assert.match(await page.locator('#setup-error').innerText(),/разные/);
    await page.locator('#name-1').fill('Юг');
    await page.locator('#player-count').selectOption('6');await page.locator('#name-5').fill('Гавань');
    await page.locator('#player-count').selectOption('2');await page.locator('#player-count').selectOption('6');
    assert.equal(await page.locator('#name-5').inputValue(),'Гавань');
    await page.locator('#player-count').selectOption('3');
    await page.locator('#rules-button').click();assert.match(await page.locator('#rules').innerText(),/Эмбарго/);await page.keyboard.press('Escape');
    await page.screenshot({path:join(artifacts,'setup-three.png'),fullPage:true});
   }
   await page.locator('#setup-form button[type=submit]').click();
   for(let round=1;round<=6;round++)for(const phase of ['market','action','law']) {
    for(let player=0;player<n;player++) {
     assert.equal(await page.locator('#choice-form').count(),0);
     assert.equal(await page.locator('.turn-dots').getAttribute('aria-label'),`Ход ${player+1} из ${n}`);
     await page.locator('#open-turn').click();
     assert.equal(await page.locator('.faction').count(),n);
     let choice=phase==='market'?(await page.locator('[data-choice=sell]').isEnabled()?'sell':'pass'):phase==='action'?'produce':'abstain';
     if(n===3&&round===2&&phase==='action')choice=player===0?'donkey':player===1?'bribe':'produce';
     if(n===3&&round===3&&phase==='market'&&player===0)choice='buy';
     if(n===3&&round===2&&phase==='law')choice='yes';
     await page.locator(`[data-choice=${choice}]`).click();
     if(choice==='buy'){await page.locator('#units').fill('1');assert.match(await page.locator('#buy-preview').innerText(),/покупать первым/);}
     if(choice==='bribe'){assert.equal(await page.locator('#target option').count(),n-1);await page.locator('#target').selectOption('2');}
     if(n===3&&round===2&&phase==='law') {
      assert.equal(await page.locator('#veto').count(),player===1?1:0);
      if(player===1)await page.locator('#veto').check();
      assert.ok(!(await page.locator('#app').innerText()).includes('Президент наложил вето'));
     }
     if(round===2&&phase==='market'&&player===0) {
      await page.screenshot({path:join(artifacts,`market-${n}.png`),fullPage:true});
      if(n===6){await page.setViewportSize({width:390,height:844});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);await page.screenshot({path:join(artifacts,'six-mobile.png'),fullPage:true});await page.setViewportSize({width:1440,height:1000});}
     }
     await page.locator('#seal-choice').click();
    }
    assert.equal(await page.locator('.queue-row').count(),0);
    await page.locator('#reveal').click();await page.locator('#next-phase').waitFor();assert.equal(await page.locator('.queue-row').count(),n);
    if(n===3&&round===2&&phase==='law')assert.match(await page.locator('.law-verdict').innerText(),/отклонён: вето/);
    if(n===3&&round===3&&phase==='market')assert.match(await page.locator('.queue').innerText(),/Куплено 1 шт/);
    await page.locator('#next-phase').click();
   }
   assert.equal(await page.locator('tbody tr').count(),n);
   const math=await page.locator('.bank-math').innerText();assert.match(math,new RegExp('знаменатель '+(n===3?95:100)));
   if(n===6)for(const index of [4,5])assert.equal(await page.locator('tbody tr').nth(index).locator('td').nth(3).innerText(),'-');
   await page.screenshot({path:join(artifacts,`final-${n}.png`),fullPage:true});
   console.log(JSON.stringify({players:n,rounds:6,decisions:n*18,final:await page.locator('tbody').innerText()}));
   await page.locator('#play-again').click();await page.locator('#open-turn').click();assert.equal(await page.locator('.faction').count(),n);assert.equal(await page.locator('[data-choice=sell]').isDisabled(),true);
  }
  assert.deepEqual(errors,[]);console.log(JSON.stringify({ok:true,offline:true,artifacts,pageErrors:errors}));
 }finally{await browser.close()}
})().catch(e=>{console.error('[ERROR]',e);process.exitCode=1});
