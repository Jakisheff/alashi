import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile, stat, mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { chromium } from "/Users/amir/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright/index.mjs";
const output="/tmp/alashi-pr67-visual/resume";
const site="/tmp/alashi-pr67-visual/site";
const types={".html":"text/html",".js":"application/javascript",".css":"text/css",".json":"application/json",".png":"image/png",".jpg":"image/jpeg",".jpeg":"image/jpeg",".svg":"image/svg+xml",".glb":"model/gltf-binary",".woff2":"font/woff2",".ico":"image/x-icon",".webmanifest":"application/manifest+json"};
const observations={head:"5a80a6ff37de1eba7b238ebf91ff61fa49c327e5",node:process.version,media:"prefers-reduced-motion: reduce",source:"actual production /live local demo; isolated build without app config/env/proxies",cases:[],pages:[]};
const server=createServer(async(req,res)=>{try{const url=new URL(req.url,"http://local");let file=path.join(site,decodeURIComponent(url.pathname));if(!file.startsWith(site+path.sep))file=path.join(site,"index.html");try{if(!(await stat(file)).isFile())throw Error();}catch{if(path.extname(url.pathname)){res.writeHead(404);res.end("missing local asset");return;}file=path.join(site,"index.html");}let body=await readFile(file);if(file.endsWith("index.html"))body=Buffer.from(body.toString().replaceAll("https://alashi.network/","/"));res.setHeader("Content-Type",types[path.extname(file)]??"application/octet-stream");res.end(body);}catch{res.writeHead(500);res.end("local fixture error");}});
await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
const origin=`http://127.0.0.1:${server.address().port}`;
let browser;
const hash=b=>createHash("sha256").update(b).digest("hex");
async function shot(page,prefix,label){await page.locator(".live-hero").scrollIntoViewIfNeeded();await page.waitForTimeout(150);const filename=`${prefix}-${label}.png`;const bytes=await page.locator(".live-hero").screenshot({path:path.join(output,filename)});return {filename,sha256:hash(bytes)};}
async function seek(page,label,value){await page.getByLabel(label,{exact:true}).evaluate((el,v)=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,"value").set.call(el,String(v));el.dispatchEvent(new Event("input",{bubbles:true}));el.dispatchEvent(new Event("change",{bubbles:true}));},value);await page.waitForFunction(({label,value})=>Number(document.querySelector(`input[aria-label="${label}"]`)?.value)===value,{label,value});await page.waitForTimeout(150);}
function pixels(a,b){return JSON.parse(execFileSync("python3",["-c",`from PIL import Image,ImageChops
import sys,json
a=Image.open(sys.argv[1]).convert("RGB");b=Image.open(sys.argv[2]).convert("RGB")
assert a.size==b.size
px=list(ImageChops.difference(a,b).get_flattened_data());mx=[max(p) for p in px]
print(json.dumps({"max_channel_delta":max(mx),"changed_pixels":sum(v>0 for v in mx),"pixels_above_8":sum(v>8 for v in mx),"pixel_count":len(mx)}))`,path.join(output,a.filename),path.join(output,b.filename)],{encoding:"utf8"}));}
async function testCase(page,width,item){
 const {kind,label,frame}=item, reaction=kind==="reaction", prefix=`${width}-${label.toLowerCase().replaceAll(" ","-")}`;
 const e={width,kind,label,canonicalReducedFrame:frame,frames:[]};observations.cases.push(e);
 const launch=reaction?page.getByRole("button",{name:label,exact:true}):page.locator('[aria-label="Market animation preview"] .live-sale-launch button').filter({hasText:new RegExp(label+"$")});
 await launch.click();const timerLabel=reaction?"Reaction frame":"Trade frame";await page.getByLabel(timerLabel,{exact:true}).waitFor();
 await seek(page,timerLabel,frame);e.frames.push(await shot(page,prefix,"canonical"));
 await seek(page,timerLabel,1);await page.getByLabel(reaction?"Resume reaction":"Resume trade animation",{exact:true}).click();await page.locator(".live-hero").scrollIntoViewIfNeeded();
 await page.waitForFunction(label=>Number(document.querySelector(`input[aria-label="${label}"]`)?.value)>1.1,timerLabel);
 e.actualTimeAfterResume=Number(await page.getByLabel(timerLabel,{exact:true}).inputValue());assert(e.actualTimeAfterResume<5,"Before completion threshold");
 await page.getByLabel(reaction?"Pause reaction":"Pause trade animation",{exact:true}).click();e.frames.push(await shot(page,prefix,"resumed"));
 e.difference=pixels(e.frames[0],e.frames[1]);assert(e.difference.pixels_above_8<2000,"Rendered pose comparison: only small speech differences allowed");
 if(label==="Buy")assert.equal(await page.locator(".live-purchase-receipt").count(),1);
 e.stage=await page.locator(".live-trade-stage").allTextContents();e.result="PASS observed timeline advance and resumed rendered pose comparison";
 await writeFile(path.join(output,"observations.json"),JSON.stringify(observations,null,2)+"\n");console.log(`PASS ${width} ${label}: actual ${e.actualTimeAfterResume}, changed pixels above8: ${e.difference.pixels_above_8}`);
 await page.getByRole("button",{name:reaction?"Return to living idle":"Close preview",exact:true}).click();
}
try{browser=await chromium.launch({channel:"chrome",headless:true});observations.chrome=browser.version();observations.origin=origin;for(const width of [1366,390]){const context=await browser.newContext({viewport:{width,height:1000},reducedMotion:"reduce"});const page=await context.newPage();page.setDefaultTimeout(20000);const meta={width,errors:[],consoleErrors:[],blocked:[]};observations.pages.push(meta);page.on("pageerror",e=>meta.errors.push(e.message));page.on("console",m=>{if(m.type()==="error")meta.consoleErrors.push(m.text());});await page.route("**/*",route=>{const url=new URL(route.request().url());if(url.origin===origin)return route.continue();meta.blocked.push(url.href);return route.abort("blockedbyclient");});await page.goto(origin+"/live");await page.locator("canvas").waitFor();await page.waitForTimeout(1500);assert(await page.evaluate(()=>matchMedia("(prefers-reduced-motion: reduce)").matches));meta.gpu=await page.evaluate(()=>{const g=document.querySelector("canvas")?.getContext("webgl2");const info=g?.getExtension("WEBGL_debug_renderer_info");return g?{version:g.getParameter(g.VERSION),renderer:info?g.getParameter(info.UNMASKED_RENDERER_WEBGL):g.getParameter(g.RENDERER)}:null;});assert(meta.gpu,"Real WebGL is required");await page.getByLabel("Pause demo",{exact:true}).click();for(const item of [{kind:"trade",label:"Buy",frame:4.25},{kind:"trade",label:"Sell",frame:4.25},{kind:"scenario",label:"Mule",frame:3.65},{kind:"scenario",label:"Bribe",frame:3.7},{kind:"scenario",label:"Vote",frame:3.3},{kind:"reaction",label:"Thumbs up",frame:2.65},{kind:"reaction",label:"Realization",frame:2.3},{kind:"reaction",label:"Facepalm",frame:2.25},{kind:"reaction",label:"Thinking",frame:2.65},{kind:"reaction",label:"Shrug",frame:2.65},{kind:"reaction",label:"Look around",frame:2.8},{kind:"reaction",label:"Victory",frame:2.3}])await testCase(page,width,item);await context.close();assert.deepEqual(meta.errors,[],"No browser runtime errors");}console.log("PASS reduced-motion real renderer checks; frames require visual review.");}finally{await writeFile(path.join(output,"observations.json"),JSON.stringify(observations,null,2)+"\n");await browser?.close();await new Promise(resolve=>server.close(resolve));}
