#!/usr/bin/env node
// Optional UI regression: node tools/city_ballot_browser_test.cjs [playwright-module-path]
// Test-only hooks go into a temporary HTML copy, never into the shipped game.
const {chromium}=require(process.argv[2]||'playwright');
const {readFileSync,writeFileSync,mkdtempSync}=require('node:fs');
const {tmpdir}=require('node:os');
const {resolve,join}=require('node:path');
const {pathToFileURL}=require('node:url');
const assert=require('node:assert/strict');
const directory=mkdtempSync(join(tmpdir(),'alashi-ballot-'));
const fixture=join(directory,'fixture.html');
const html=readFileSync(resolve(__dirname,'../app/alashi-city.html'),'utf8');
const marker='// Read-only diagnostics for browser smoke tests and performance measurement.';
assert.ok(html.includes(marker));
const hooks=`let testDraws=0;
window.CityBallotFixture={
 prepare(options={}){closeModal();Object.assign(state,{started:true,mission:4,round:options.round||1,cash:10000000,goods:0,influence:options.playerWeight||1,tax:0,nextBoom:0,boom:0,complete:false,law:'tax10'});Object.assign(rival,{cash:20000000,influence:options.rivalWeight||3});sealedBallot=null;const values=[...(options.values||[.95,.1])];testDraws=0;ballotRandom=()=>{testDraws++;if(!values.length)throw Error('Ballot rerolled');return values.shift()};},
 open(){stationDialog(stations[3])},union(){stationDialog(stations[2])},resolve:resolveVote,
 inspect(){return {draws:testDraws,ballot:sealedBallot&&{...sealedBallot},round:state.round,tax:state.tax,cash:state.cash,influence:state.influence,rivalCash:rival.cash,complete:state.complete}}
};\n`;
writeFileSync(fixture,html.replace(marker,hooks+marker));
(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CHROME_PATH||'/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',headless:true,args:['--enable-webgl','--enable-unsafe-swiftshader']});
 try{
  const page=await browser.newPage({viewport:{width:1280,height:900},offline:true});
  const errors=[];page.on('pageerror',e=>errors.push(e.message));page.setDefaultTimeout(10000);
  await page.goto(pathToFileURL(fixture).href);await page.waitForFunction(()=>!!window.CityBallotFixture);await page.click('#start');
  const prepare=async options=>page.evaluate(o=>{CityBallotFixture.prepare(o);CityBallotFixture.open()},options);
  const inspect=()=>page.evaluate(()=>CityBallotFixture.inspect());
  await prepare();
  const secretHTML=await page.locator('#modal-content').innerHTML(),sealed=await inspect();
  assert.equal(await page.locator('[data-vote=veto]').count(),0);
  assert.equal(await page.locator('#ballot-reveal').count(),0);
  await page.screenshot({path:join(directory,'before.png')});
  await page.click('#leave');await page.evaluate(()=>CityBallotFixture.union());
  assert.equal(await page.locator('#bribe').count(),0);assert.equal((await inspect()).influence,1);
  await page.evaluate(()=>CityBallotFixture.open());assert.deepEqual(await inspect(),sealed);
  assert.equal(await page.locator('#modal-content').innerHTML(),secretHTML);
  await prepare({values:[.1]});
  assert.equal(await page.locator('#modal-content').innerHTML(),secretHTML,'secret yes and secret veto must render identically');
  await prepare();await page.click('[data-vote=yes]');
  let s=await inspect();assert.equal(s.tax,0);assert.equal(s.round,2);
  assert.match(await page.locator('#modal-content h1').innerText(),/Вето президента «Союза»/);
  assert.match(await page.locator('#ballot-reveal').innerText(),/За: 1, против: 0/);
  assert.match(await page.locator('#ballot-reveal').innerText(),/Голос «Союза»: воздержание/);
  await page.screenshot({path:join(directory,'npc-veto.png')});
  await page.evaluate(()=>CityBallotFixture.resolve('yes'));assert.deepEqual(await inspect(),s);
  await prepare({values:[.95,.9]});await page.click('[data-vote=yes]');
  assert.equal((await inspect()).tax,10);assert.match(await page.locator('#modal-content h1').innerText(),/Закон принят/);
  await prepare({playerWeight:3,rivalWeight:1,values:[.1]});await page.click('[data-vote=veto]');
  assert.equal((await inspect()).tax,0);assert.match(await page.locator('#ballot-reveal').innerText(),/Вето: игрок/);
  await prepare({round:6});await page.click('[data-vote=yes]');s=await inspect();
  assert.equal(s.complete,true);assert.equal(s.cash,17125000);assert.equal(s.rivalCash,31875000);
  assert.match(await page.locator('#ballot-reveal').innerText(),/Вето: «Союз»/);
  await page.click('#continue');await page.evaluate(()=>{CityBallotFixture.resolve('yes');CityBallotFixture.open()});
  assert.deepEqual(await inspect(),s);assert.deepEqual(errors,[]);
  console.log(JSON.stringify({ok:true,checks:['hidden-ballot-dom','no-reroll','influence-locked','npc-veto-overrides-majority','npc-can-decline-veto','player-veto','sixth-round-reveal','no-double-resolution'],artifacts:directory}));
 }finally{await browser.close()}
})().catch(e=>{console.error('[ERROR]',e);process.exitCode=1});
